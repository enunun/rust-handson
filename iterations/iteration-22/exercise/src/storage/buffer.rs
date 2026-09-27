//! ディスクのページをメモリーの枠に置いて使い回すバッファプール．
//! 複数のスレッドから使える．ページの中身は枠ごとのラッチで，枠の割り当てはプールのラッチで守る．

use std::cell::Cell;
use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::sync::{Arc, Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::storage::disk::{DiskManager, PageId};
use crate::storage::page::{PAGE_SIZE, Page};
use crate::wal::{Lsn, WalRecord, WalWriter};

/// バッファプールを使うときのエラー．
#[derive(Debug)]
pub enum BufferError {
    /// すべての枠のページがピン留めされていて，新しいページを置けない．
    AllPinned,
    Io(io::Error),
}

impl From<io::Error> for BufferError {
    fn from(error: io::Error) -> BufferError {
        BufferError::Io(error)
    }
}

/// 1つのページを置く枠．ページの中身は`RwLock`(ページのラッチ)で守り，
/// 同じページを複数のスレッドが同時に読めるが，書くスレッドは1つだけにする．
struct Frame {
    page: RwLock<Page>,
    state: Mutex<FrameState>,
}

/// 枠の使われ方．
#[derive(Default)]
struct FrameState {
    pin_count: usize,
    dirty: bool,
    /// ページの内容を最後に記録したログの位置．
    page_lsn: Lsn,
}

/// どの枠にどのページがあるかと，ディスク．プールの`Mutex`で守る．
struct PoolState<D> {
    disk: D,
    page_ids: Vec<Option<PageId>>,
    page_table: HashMap<PageId, usize>,
    replacer: ClockReplacer,
}

/// 書き換えたページをログに書くための，データベースで共有するログと，プールのファイルの名前．
pub struct PageLog {
    pub wal: Arc<Mutex<WalWriter<File>>>,
    pub file: String,
}

/// `DiskManager`のページを，決まった数の枠に置いて使い回す．
/// 枠が足りなければ，ピン留めされていないページをクロック方式で選んで追い出す．
/// ページのピン留めと追い出しは，どちらもプールの`Mutex`を取って行う．
pub struct BufferPool<D: DiskManager> {
    frames: Vec<Frame>,
    state: Mutex<PoolState<D>>,
    log: Option<PageLog>,
}

/// ピン留めしたページ．値を捨てると，ピンが外れる．`write`で書き換えたページは，捨てるときにログに記録する．
pub struct PageGuard<'a> {
    frame: &'a Frame,
    page_id: PageId,
    log: Option<&'a PageLog>,
    written: Cell<bool>,
}

/// クロック方式で，追い出す枠を選ぶ．枠ごとに参照ビットを持ち，
/// 針が指す枠のビットが立っていれば下ろして次へ進み，下りていればその枠を選ぶ．
#[derive(Debug)]
pub struct ClockReplacer {
    referenced: Vec<bool>,
    hand: usize,
}

/// `Mutex`を取る．ほかのスレッドが`Mutex`を持ったままパニックしていれば，パニックする．
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .expect("a thread panicked while holding the latch")
}

impl<D: DiskManager> BufferPool<D> {
    /// `disk`のページを`frame_count`個の枠に置くプールを作る．
    pub fn new(disk: D, frame_count: usize) -> BufferPool<D> {
        let frames = (0..frame_count)
            .map(|_| Frame {
                page: RwLock::new(Page::new()),
                state: Mutex::new(FrameState::default()),
            })
            .collect();
        BufferPool {
            frames,
            state: Mutex::new(PoolState {
                disk,
                page_ids: vec![None; frame_count],
                page_table: HashMap::new(),
                replacer: ClockReplacer::new(frame_count),
            }),
            log: None,
        }
    }

    /// 書き換えたページの内容を`log`に記録するプールを作る．
    pub fn with_log(disk: D, frame_count: usize, log: PageLog) -> BufferPool<D> {
        let mut pool = BufferPool::new(disk, frame_count);
        pool.log = Some(log);
        pool
    }

    /// ディスクのページの数．
    pub fn page_count(&self) -> usize {
        lock(&self.state).disk.page_count()
    }

    /// ページをピン留めして返す．枠になければ，ディスクから読んで枠に置く．
    pub fn fetch_page(&self, page_id: PageId) -> Result<PageGuard<'_>, BufferError> {
        let mut state = lock(&self.state);
        if let Some(&index) = state.page_table.get(&page_id) {
            return Ok(self.pin(&mut state, index, page_id));
        }
        let index = self.free_frame(&mut state)?;
        let mut data = Box::new([0; PAGE_SIZE]);
        state.disk.read_page(page_id, &mut data)?;
        self.place(&mut state, index, page_id, Page::from_bytes(data), false);
        Ok(self.pin(&mut state, index, page_id))
    }

    /// ディスクの末尾に空のページを加え，ピン留めして返す．
    pub fn new_page(&self) -> Result<PageGuard<'_>, BufferError> {
        let mut state = lock(&self.state);
        let index = self.free_frame(&mut state)?;
        let page_id = state.disk.allocate_page()?;
        self.place(&mut state, index, page_id, Page::new(), true);
        let guard = self.pin(&mut state, index, page_id);
        guard.written.set(true);
        Ok(guard)
    }

    /// 変更されたページをすべてディスクに書き戻し，ディスクに届くまで待つ．
    pub fn flush_all(&self) -> io::Result<()> {
        let mut state = lock(&self.state);
        for index in 0..self.frames.len() {
            self.flush(&mut state, index)?;
        }
        state.disk.sync()
    }

    /// 空いている枠を返す．なければ，ページを追い出して枠を空ける．
    fn free_frame(&self, state: &mut PoolState<D>) -> Result<usize, BufferError> {
        if let Some(index) = state.page_ids.iter().position(Option::is_none) {
            return Ok(index);
        }
        let victim = state
            .replacer
            .victim(|index| lock(&self.frames[index].state).pin_count > 0);
        let Some(index) = victim else {
            return Err(BufferError::AllPinned);
        };
        self.flush(state, index)?;
        if let Some(old) = state.page_ids[index].take() {
            state.page_table.remove(&old);
        }
        Ok(index)
    }

    /// ピン留めされていない枠`index`に，ページを置く．
    fn place(
        &self,
        state: &mut PoolState<D>,
        index: usize,
        page_id: PageId,
        page: Page,
        dirty: bool,
    ) {
        let frame = &self.frames[index];
        *frame
            .page
            .write()
            .expect("a thread panicked while holding the latch") = page;
        lock(&frame.state).dirty = dirty;
        state.page_ids[index] = Some(page_id);
        state.page_table.insert(page_id, index);
    }

    fn pin(&self, state: &mut PoolState<D>, index: usize, page_id: PageId) -> PageGuard<'_> {
        let frame = &self.frames[index];
        lock(&frame.state).pin_count += 1;
        state.replacer.access(index);
        PageGuard {
            frame,
            page_id,
            log: self.log.as_ref(),
            written: Cell::new(false),
        }
    }

    /// 枠`index`のページが変更されていれば，ログをそのページの記録まで届けてから，ディスクに書き戻す．
    fn flush(&self, state: &mut PoolState<D>, index: usize) -> io::Result<()> {
        let Some(page_id) = state.page_ids[index] else {
            return Ok(());
        };
        let frame = &self.frames[index];
        let (dirty, page_lsn) = {
            let frame_state = lock(&frame.state);
            (frame_state.dirty, frame_state.page_lsn)
        };
        if dirty {
            if let Some(log) = &self.log {
                lock(&log.wal).flush_to(page_lsn)?;
            }
            let page = frame
                .page
                .read()
                .expect("a thread panicked while holding the latch");
            state.disk.write_page(page_id, page.bytes())?;
            lock(&frame.state).dirty = false;
        }
        Ok(())
    }
}

/// 枠の内容は書かず，枠の数とディスクのページの数だけを書く．
impl<D: DiskManager> std::fmt::Debug for BufferPool<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BufferPool")
            .field("frames", &self.frames.len())
            .field("page_count", &self.page_count())
            .finish()
    }
}

/// プールを捨てるときに，変更されたページを書き戻す．書き戻せなかったエラーは捨てる．
impl<D: DiskManager> Drop for BufferPool<D> {
    fn drop(&mut self) {
        let _ = self.flush_all();
    }
}

impl PageGuard<'_> {
    /// ページの番号．
    pub fn page_id(&self) -> PageId {
        self.page_id
    }

    /// ページを読む．ほかのスレッドがページを書いている間は待つ．
    pub fn read(&self) -> RwLockReadGuard<'_, Page> {
        self.frame
            .page
            .read()
            .expect("a thread panicked while holding the latch")
    }

    /// ページを書き換える．ほかのスレッドがページを読み書きしている間は待つ．
    /// ページは，追い出すときにディスクへ書き戻す．
    pub fn write(&self) -> RwLockWriteGuard<'_, Page> {
        lock(&self.frame.state).dirty = true;
        self.written.set(true);
        self.frame
            .page
            .write()
            .expect("a thread panicked while holding the latch")
    }
}

/// 書き換えたページなら，内容をログに記録し，枠の`page_lsn`をレコードの位置にする．
/// ログに書けなければ，ログが失敗を覚え，次のコミットがエラーになる．
impl Drop for PageGuard<'_> {
    fn drop(&mut self) {
        let mut page_lsn = None;
        if let Some(log) = self.log
            && self.written.get()
        {
            let record = WalRecord::PageImage {
                file: log.file.clone(),
                page: self.page_id,
                bytes: Box::new(*self.read().bytes()),
            };
            page_lsn = lock(&log.wal).append(&record).ok();
        }
        let mut state = lock(&self.frame.state);
        if let Some(lsn) = page_lsn {
            state.page_lsn = lsn;
        }
        state.pin_count -= 1;
    }
}

impl ClockReplacer {
    /// `frame_count`個の枠を見る．
    pub fn new(frame_count: usize) -> ClockReplacer {
        ClockReplacer {
            referenced: vec![false; frame_count],
            hand: 0,
        }
    }

    /// 枠のページが使われたことを記録する．
    pub fn access(&mut self, index: usize) {
        self.referenced[index] = true;
    }

    /// 追い出す枠を選ぶ．`is_pinned`が真を返す枠は選ばない．
    /// どの枠もピン留めされていれば`None`を返す．
    pub fn victim(&mut self, is_pinned: impl Fn(usize) -> bool) -> Option<usize> {
        let count = self.referenced.len();
        for _ in 0..2 * count {
            let index = self.hand;
            self.hand = (self.hand + 1) % count;
            if is_pinned(index) {
                continue;
            }
            if self.referenced[index] {
                self.referenced[index] = false;
                continue;
            }
            return Some(index);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::disk::MemoryDiskManager;

    /// ページを読み書きした回数を数える`DiskManager`．
    #[derive(Default)]
    struct CountingDisk {
        inner: MemoryDiskManager,
        reads: Cell<usize>,
        writes: usize,
    }

    impl DiskManager for CountingDisk {
        fn read_page(&self, page_id: PageId, buffer: &mut [u8; PAGE_SIZE]) -> io::Result<()> {
            self.reads.set(self.reads.get() + 1);
            self.inner.read_page(page_id, buffer)
        }

        fn write_page(&mut self, page_id: PageId, buffer: &[u8; PAGE_SIZE]) -> io::Result<()> {
            self.writes += 1;
            self.inner.write_page(page_id, buffer)
        }

        fn allocate_page(&mut self) -> io::Result<PageId> {
            self.inner.allocate_page()
        }

        fn page_count(&self) -> usize {
            self.inner.page_count()
        }

        fn sync(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// `pages`個のページを持つディスク．`i`番目のページには，タプル`[i]`を1つ置く．
    fn disk_with(pages: u8) -> CountingDisk {
        let mut disk = CountingDisk::default();
        for i in 0..pages {
            let mut page = Page::new();
            page.insert(&[i]).unwrap();
            let id = disk.inner.allocate_page().unwrap();
            disk.inner.write_page(id, page.bytes()).unwrap();
        }
        disk
    }

    fn first_tuple(guard: &PageGuard) -> Vec<u8> {
        guard.read().get(0).unwrap().to_vec()
    }

    #[test]
    fn fetched_page_has_the_contents_on_disk() {
        let pool = BufferPool::new(disk_with(2), 3);
        assert_eq!(first_tuple(&pool.fetch_page(1).unwrap()), vec![1]);
        assert_eq!(first_tuple(&pool.fetch_page(0).unwrap()), vec![0]);
    }

    #[test]
    fn page_in_a_frame_is_not_read_again() {
        let pool = BufferPool::new(disk_with(2), 3);
        pool.fetch_page(1).unwrap();
        pool.fetch_page(1).unwrap();
        assert_eq!(lock(&pool.state).disk.reads.get(), 1);
    }

    #[test]
    fn pinned_pages_are_not_evicted() {
        let pool = BufferPool::new(disk_with(3), 2);
        let _first = pool.fetch_page(0).unwrap();
        let _second = pool.fetch_page(1).unwrap();
        assert!(matches!(pool.fetch_page(2), Err(BufferError::AllPinned)));
    }

    #[test]
    fn dropping_the_guard_unpins_the_page() {
        let pool = BufferPool::new(disk_with(3), 2);
        let first = pool.fetch_page(0).unwrap();
        let _second = pool.fetch_page(1).unwrap();
        drop(first);
        assert_eq!(first_tuple(&pool.fetch_page(2).unwrap()), vec![2]);
        assert_eq!(lock(&pool.state).page_table.get(&0), None);
    }

    #[test]
    fn modified_page_is_written_back_when_evicted() {
        let pool = BufferPool::new(disk_with(2), 1);
        pool.fetch_page(0).unwrap().write().insert(b"new").unwrap();
        assert_eq!(lock(&pool.state).disk.writes, 0);
        pool.fetch_page(1).unwrap();
        assert_eq!(lock(&pool.state).disk.writes, 1);
        let page = pool.fetch_page(0).unwrap();
        assert_eq!(page.read().get(1), Some(&b"new"[..]));
    }

    #[test]
    fn unmodified_page_is_not_written_back() {
        let pool = BufferPool::new(disk_with(2), 1);
        drop(pool.fetch_page(0).unwrap().read());
        pool.fetch_page(1).unwrap();
        assert_eq!(lock(&pool.state).disk.writes, 0);
    }

    #[test]
    fn new_page_is_added_to_the_end_of_the_disk() {
        let pool = BufferPool::new(disk_with(1), 2);
        let page = pool.new_page().unwrap();
        page.write().insert(b"x").unwrap();
        drop(page);
        assert_eq!(pool.page_count(), 2);
        assert_eq!(first_tuple(&pool.fetch_page(1).unwrap()), b"x".to_vec());
    }

    #[test]
    fn flush_all_writes_modified_pages() {
        let pool = BufferPool::new(disk_with(2), 2);
        pool.fetch_page(0).unwrap().write().insert(b"a").unwrap();
        drop(pool.fetch_page(1).unwrap().read());
        pool.flush_all().unwrap();
        assert_eq!(lock(&pool.state).disk.writes, 1);
        pool.flush_all().unwrap();
        assert_eq!(lock(&pool.state).disk.writes, 1);
    }

    #[test]
    fn dropping_the_pool_writes_modified_pages() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.heap");
        let disk = crate::storage::disk::FileDiskManager::open(&path).unwrap();
        let pool = BufferPool::new(disk, 2);
        pool.new_page().unwrap().write().insert(b"kept").unwrap();
        drop(pool);
        let disk = crate::storage::disk::FileDiskManager::open(&path).unwrap();
        let pool = BufferPool::new(disk, 2);
        assert_eq!(first_tuple(&pool.fetch_page(0).unwrap()), b"kept".to_vec());
    }

    #[test]
    fn written_page_goes_to_the_log_before_the_disk() {
        let dir = tempfile::tempdir().unwrap();
        let wal = Arc::new(Mutex::new(
            WalWriter::open(&dir.path().join("wal"), Lsn(0)).unwrap(),
        ));
        let log = PageLog {
            wal: Arc::clone(&wal),
            file: "t.heap".to_string(),
        };
        let pool = BufferPool::with_log(disk_with(2), 1, log);
        pool.fetch_page(0).unwrap().write().insert(b"x").unwrap();
        let lsn = lock(&pool.frames[0].state).page_lsn;
        assert_eq!(lsn, lock(&wal).next_lsn());
        pool.fetch_page(1).unwrap();
        assert_eq!(lock(&pool.state).disk.writes, 1);
        let logged = std::fs::read(dir.path().join("wal")).unwrap();
        let (records, end) = crate::wal::read_records(&logged);
        assert_eq!(end, lsn);
        let WalRecord::PageImage { file, page, bytes } = &records[0].0 else {
            panic!("not a page image");
        };
        assert_eq!((file.as_str(), *page), ("t.heap", 0));
        assert_eq!(Page::from_bytes(bytes.clone()).get(1), Some(&b"x"[..]));
    }

    #[test]
    fn threads_share_the_pool() {
        let pool = BufferPool::new(disk_with(8), 4);
        std::thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| {
                    for _ in 0..50 {
                        for i in 0..8 {
                            assert_eq!(first_tuple(&pool.fetch_page(i).unwrap()), vec![i as u8]);
                        }
                    }
                });
            }
        });
    }

    #[test]
    fn clock_skips_referenced_frames_once() {
        let mut clock = ClockReplacer::new(3);
        clock.access(0);
        clock.access(2);
        assert_eq!(clock.victim(|_| false), Some(1));
        assert_eq!(clock.victim(|_| false), Some(0));
        assert_eq!(clock.victim(|_| false), Some(1));
    }

    #[test]
    fn clock_skips_pinned_frames() {
        let mut clock = ClockReplacer::new(3);
        assert_eq!(clock.victim(|index| index != 2), Some(2));
        assert_eq!(clock.victim(|_| true), None);
    }
}
