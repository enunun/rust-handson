//! ディスクのページをメモリーの枠に置いて使い回すバッファプール．

use std::cell::{Cell, Ref, RefCell, RefMut};
use std::collections::HashMap;
use std::io;

use crate::storage::disk::{DiskManager, PageId};
use crate::storage::page::{PAGE_SIZE, Page};

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

/// 1つのページを置く枠．
struct Frame {
    page_id: Cell<Option<PageId>>,
    pin_count: Cell<usize>,
    dirty: Cell<bool>,
    page: RefCell<Page>,
}

/// `DiskManager`のページを，決まった数の枠に置いて使い回す．
/// 枠が足りなければ，ピン留めされていないページをクロック方式で選んで追い出す．
pub struct BufferPool<D: DiskManager> {
    disk: RefCell<D>,
    frames: Vec<Frame>,
    page_table: RefCell<HashMap<PageId, usize>>,
    replacer: RefCell<ClockReplacer>,
}

/// ピン留めしたページ．値を捨てると，ピンが外れる．
pub struct PageGuard<'a> {
    frame: &'a Frame,
}

/// クロック方式で，追い出す枠を選ぶ．枠ごとに参照ビットを持ち，
/// 針が指す枠のビットが立っていれば下ろして次へ進み，下りていればその枠を選ぶ．
#[derive(Debug)]
pub struct ClockReplacer {
    referenced: Vec<bool>,
    hand: usize,
}

impl<D: DiskManager> BufferPool<D> {
    /// `disk`のページを`frame_count`個の枠に置くプールを作る．
    pub fn new(disk: D, frame_count: usize) -> BufferPool<D> {
        let frames = (0..frame_count)
            .map(|_| Frame {
                page_id: Cell::new(None),
                pin_count: Cell::new(0),
                dirty: Cell::new(false),
                page: RefCell::new(Page::new()),
            })
            .collect();
        BufferPool {
            disk: RefCell::new(disk),
            frames,
            page_table: RefCell::new(HashMap::new()),
            replacer: RefCell::new(ClockReplacer::new(frame_count)),
        }
    }

    /// ディスクのページの数．
    pub fn page_count(&self) -> usize {
        self.disk.borrow().page_count()
    }

    /// ページをピン留めして返す．枠になければ，ディスクから読んで枠に置く．
    pub fn fetch_page(&self, page_id: PageId) -> Result<PageGuard<'_>, BufferError> {
        if let Some(&index) = self.page_table.borrow().get(&page_id) {
            return Ok(self.pin(index));
        }
        let index = self.free_frame()?;
        let mut data = Box::new([0; PAGE_SIZE]);
        self.disk.borrow().read_page(page_id, &mut data)?;
        self.place(index, page_id, Page::from_bytes(data), false);
        Ok(self.pin(index))
    }

    /// ディスクの末尾に空のページを加え，ピン留めして返す．
    pub fn new_page(&self) -> Result<PageGuard<'_>, BufferError> {
        let index = self.free_frame()?;
        let page_id = self.disk.borrow_mut().allocate_page()?;
        self.place(index, page_id, Page::new(), true);
        Ok(self.pin(index))
    }

    /// 変更されたページをすべてディスクに書き戻す．
    pub fn flush_all(&self) -> io::Result<()> {
        for frame in &self.frames {
            self.flush(frame)?;
        }
        Ok(())
    }

    /// 空いている枠を返す．なければ，ページを追い出して枠を空ける．
    fn free_frame(&self) -> Result<usize, BufferError> {
        if let Some(index) = self.frames.iter().position(|f| f.page_id.get().is_none()) {
            return Ok(index);
        }
        let victim = self
            .replacer
            .borrow_mut()
            .victim(|index| self.frames[index].pin_count.get() > 0);
        let Some(index) = victim else {
            return Err(BufferError::AllPinned);
        };
        let frame = &self.frames[index];
        self.flush(frame)?;
        if let Some(old) = frame.page_id.take() {
            self.page_table.borrow_mut().remove(&old);
        }
        Ok(index)
    }

    fn place(&self, index: usize, page_id: PageId, page: Page, dirty: bool) {
        let frame = &self.frames[index];
        *frame.page.borrow_mut() = page;
        frame.page_id.set(Some(page_id));
        frame.dirty.set(dirty);
        self.page_table.borrow_mut().insert(page_id, index);
    }

    fn pin(&self, index: usize) -> PageGuard<'_> {
        let frame = &self.frames[index];
        frame.pin_count.set(frame.pin_count.get() + 1);
        self.replacer.borrow_mut().access(index);
        PageGuard { frame }
    }

    fn flush(&self, frame: &Frame) -> io::Result<()> {
        let Some(page_id) = frame.page_id.get() else {
            return Ok(());
        };
        if frame.dirty.get() {
            self.disk
                .borrow_mut()
                .write_page(page_id, frame.page.borrow().bytes())?;
            frame.dirty.set(false);
        }
        Ok(())
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
        self.frame
            .page_id
            .get()
            .expect("a pinned frame holds a page")
    }

    /// ページを読む．
    pub fn read(&self) -> Ref<'_, Page> {
        self.frame.page.borrow()
    }

    /// ページを書き換える．ページは，追い出すときにディスクへ書き戻す．
    pub fn write(&self) -> RefMut<'_, Page> {
        self.frame.dirty.set(true);
        self.frame.page.borrow_mut()
    }
}

impl Drop for PageGuard<'_> {
    fn drop(&mut self) {
        self.frame.pin_count.set(self.frame.pin_count.get() - 1);
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
        assert_eq!(pool.disk.borrow().reads.get(), 1);
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
        assert_eq!(pool.page_table.borrow().get(&0), None);
    }

    #[test]
    fn modified_page_is_written_back_when_evicted() {
        let pool = BufferPool::new(disk_with(2), 1);
        pool.fetch_page(0).unwrap().write().insert(b"new").unwrap();
        assert_eq!(pool.disk.borrow().writes, 0);
        pool.fetch_page(1).unwrap();
        assert_eq!(pool.disk.borrow().writes, 1);
        let page = pool.fetch_page(0).unwrap();
        assert_eq!(page.read().get(1), Some(&b"new"[..]));
    }

    #[test]
    fn unmodified_page_is_not_written_back() {
        let pool = BufferPool::new(disk_with(2), 1);
        pool.fetch_page(0).unwrap().read();
        pool.fetch_page(1).unwrap();
        assert_eq!(pool.disk.borrow().writes, 0);
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
        pool.fetch_page(1).unwrap().read();
        pool.flush_all().unwrap();
        assert_eq!(pool.disk.borrow().writes, 1);
        pool.flush_all().unwrap();
        assert_eq!(pool.disk.borrow().writes, 1);
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
