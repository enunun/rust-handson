# Iteration 15：バッファプール(解説)

演習の各手順の模範解答と，その考え方を説明する．

## 15-1 準備

引き継いだ349個のテストがすべて通れば準備は終わりである．
Iteration 14の`HeapFile::insert`は，最後のページを1回読み，タプルを置いたページを1回書く．ページに入らなければ，新しいページを加える書き込みがさらに2回増える．

## 15-2 文法と概念

課題の解答例である．`tests/`に置いた結合テストで確かめた．

```rust
use std::cell::RefCell;

pub fn first_digits(text: &str) -> &str {
    let start = text.find(|c: char| c.is_ascii_digit()).unwrap_or(text.len());
    let rest = &text[start..];
    let end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
    &rest[..end]
}

pub struct Cursor<'a> {
    values: &'a [i32],
    position: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(values: &'a [i32]) -> Cursor<'a> {
        Cursor { values, position: 0 }
    }

    pub fn next_value(&mut self) -> Option<i32> {
        let value = self.values.get(self.position).copied();
        self.position += 1;
        value
    }
}

struct Named<'a> {
    name: &'static str,
    log: &'a RefCell<Vec<String>>,
}

impl Drop for Named<'_> {
    fn drop(&mut self) {
        self.log.borrow_mut().push(self.name.to_string());
    }
}

#[test]
fn tasks() {
    assert_eq!(first_digits("ab12cd"), "12");
    assert_eq!(first_digits("abc"), "");

    let values = vec![1, 2];
    let mut cursor = Cursor::new(&values);
    assert_eq!(cursor.next_value(), Some(1));
    assert_eq!(cursor.next_value(), Some(2));
    assert_eq!(cursor.next_value(), None);

    let log = RefCell::new(Vec::new());
    {
        let _x = Named { name: "x", log: &log };
        let _y = Named { name: "y", log: &log };
        let _z = Named { name: "z", log: &log };
    }
    assert_eq!(*log.borrow(), vec!["z", "y", "x"]);

    let cell = RefCell::new(1);
    let reader = cell.borrow();
    assert!(cell.try_borrow_mut().is_err());
    drop(reader);
    assert!(cell.try_borrow_mut().is_ok());
}
```

- `first_digits`は参照の引数が1つなので，ライフタイム注釈は要らない．戻り値は`text`を借りる．
- `find`には，文字を受け取って`bool`を返すクロージャを渡せる．
- 変数は宣言と逆の順に捨てられるので，記録は`z`，`y`，`x`の順になる．
- `borrow()`の`Ref`を持ったまま`borrow_mut()`を呼ぶと`RefCell already borrowed`のパニックになる．`try_borrow_mut()`は`Err`を返す．

## 15-3 テストリスト

模範解答は[TESTLIST.md](../TESTLIST.md)である．

- `ClockReplacer`の項目はバッファプールと切り離し，参照ビットとピン留めの組み合わせだけを確かめた．
- ディスクを読み書きした回数は，テストの中の`CountingDisk`で数えた．`MemoryDiskManager`を包み，`read_page`(`&self`)の回数は`Cell<usize>`で，`write_page`(`&mut self`)の回数は`usize`で数える．
- ディスクの`i`番目のページにタプル`[i]`を置いておけば，取得したページが正しいかを1バイトで確かめられる．
- 枠の数を1や2にして，追い出しと`AllPinned`の境界を作った．
- 既存テストへの影響は，`storage::heap`の単体テストでページの数を`heap.disk`でなく`heap.pool`から読むことである．SQLの結合テストは1つも変えていない．
- 結合テストには，1000文字の値を持つ200行の表(25ページ)を作り，閉じて開き直す項目を加えた．枠(16)より多いページがあるので，追い出しと，プールを捨てるときの書き戻しの両方が要る．

## 15-4 設計ドキュメント

| ファイル | 変更 | 理由 |
| --- | --- | --- |
| `c4-component.md` | `storage::buffer`を加え，`storage::heap`から`storage::buffer`への依存，`storage::buffer`から`storage::disk`と`storage::page`への依存，`error`から`storage::buffer`への依存を加えた | ページの読み書きをバッファプールに任せた |
| `code-types.md` | `BufferPool`，`Frame`，`PageGuard`，`ClockReplacer`，`BufferError`を加え，`HeapFile`のフィールドと`HeapError`を更新した | バッファプールの型ができた |
| `code-sequence.md` | `fetch_page`でページを取得する流れを加えた | 枠にある場合，空いた枠を使う場合，追い出す場合で動きが違う |

- `storage::heap`は，`HeapFile::new`の引数で`DiskManager`を受け取るので，`storage::disk`への依存も残る．
- `Frame`は`storage::buffer`の中だけで使う非公開の型だが，枠の状態をどこに持つかを示すために型の図に描いた．
- `PageGuard`は`Frame`を所有せず借りるので，関連(`-->`)で描いた．
- `BufferPool`は`Box<dyn DiskManager>`でなく型引数`D`で`DiskManager`を持つ．テストでは`CountingDisk`を，`HeapFile`では`Box<dyn DiskManager>`を`D`にする．

## 15-5 テスト駆動の実装

### `ClockReplacer`

```rust
#[test]
fn clock_skips_referenced_frames_once() {
    let mut clock = ClockReplacer::new(3);
    clock.access(0);
    clock.access(2);
    assert_eq!(clock.victim(|_| false), Some(1));
    assert_eq!(clock.victim(|_| false), Some(0));
    assert_eq!(clock.victim(|_| false), Some(1));
}
```

1回目は，枠0のビットを下ろして枠1を選ぶ．2回目は，針が枠2のビットを下ろし，ビットの下りた枠0を選ぶ．

```rust
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
```

1周目ですべてのビットを下ろしても，2周目にはピン留めされていない枠が見つかる．2周しても見つからなければ，すべての枠がピン留めされている．

### `BufferPool`と`PageGuard`

最初のテストは，ディスクのページを取得して読む項目である．

```rust
#[test]
fn fetched_page_has_the_contents_on_disk() {
    let pool = BufferPool::new(disk_with(2), 3);
    assert_eq!(first_tuple(&pool.fetch_page(1).unwrap()), vec![1]);
    assert_eq!(first_tuple(&pool.fetch_page(0).unwrap()), vec![0]);
}
```

枠は，ページ番号，ピンの数，変更の印，ページを持つ構造体`Frame`にした．どれも`&self`のメソッドから書き換えるので，`Cell`と`RefCell`に入れる．

```rust
struct Frame {
    page_id: Cell<Option<PageId>>,
    pin_count: Cell<usize>,
    dirty: Cell<bool>,
    page: RefCell<Page>,
}

pub struct BufferPool<D: DiskManager> {
    disk: RefCell<D>,
    frames: Vec<Frame>,
    page_table: RefCell<HashMap<PageId, usize>>,
    replacer: RefCell<ClockReplacer>,
}

pub struct PageGuard<'a> {
    frame: &'a Frame,
}
```

`fetch_page`は，ページテーブルにあればピン留めするだけで，なければ空いた枠にディスクから読む．

```rust
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

    fn pin(&self, index: usize) -> PageGuard<'_> {
        let frame = &self.frames[index];
        frame.pin_count.set(frame.pin_count.get() + 1);
        self.replacer.borrow_mut().access(index);
        PageGuard { frame }
    }
```

戻り値の型を`Result<PageGuard, BufferError>`と書くと，`PageGuard`が`self`を借りていることが見えないという警告になる．`PageGuard<'_>`と書く．

`page_in_a_frame_is_not_read_again`と`pinned_pages_are_not_evicted`を通すために，空いた枠を探す`free_frame`を作った．空いた枠がなければ，`ClockReplacer`に選ばせる．

```rust
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
```

`dropping_the_guard_unpins_the_page`は，`PageGuard`の`Drop`で通る．

```rust
impl Drop for PageGuard<'_> {
    fn drop(&mut self) {
        self.frame.pin_count.set(self.frame.pin_count.get() - 1);
    }
}
```

`PageGuard`は`BufferPool`を借りているので，プールより長く使えない．プールを先に捨てようとすると，借用検査のエラーになる．

```text
error[E0505]: cannot move out of `pool` because it is borrowed
   --> src/storage/buffer.rs:385:14
    |
383 |         let pool = BufferPool::new(MemoryDiskManager::default(), 2);
    |             ---- binding `pool` declared here
384 |         let guard = pool.new_page().unwrap();
    |                     ---- borrow of `pool` occurs here
385 |         drop(pool);
    |              ^^^^ move out of `pool` occurs here
386 |         guard.write().insert(b"x").unwrap();
    |         ----- borrow later used here
```

書き戻しの項目は，`PageGuard::write`で変更の印を付け，追い出すときと`flush_all`で印のあるページだけを書くことで通る．

```rust
    pub fn write(&self) -> RefMut<'_, Page> {
        self.frame.dirty.set(true);
        self.frame.page.borrow_mut()
    }
```

```rust
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
```

`dropping_the_pool_writes_modified_pages`は，`BufferPool`の`Drop`で通る．`drop`はエラーを返せないので，`let _ =`で`flush_all`の結果を捨てる．

```rust
impl<D: DiskManager> Drop for BufferPool<D> {
    fn drop(&mut self) {
        let _ = self.flush_all();
    }
}
```

`new_page`は，`allocate_page`でディスクに0のページを加えてから，枠に`Page::new()`を置く．0のページはヘッダーが正しくないので，枠のページに変更の印を付けて，書き戻すときに上書きする．

### `HeapFile`

`HeapFile`は`BufferPool<Box<dyn DiskManager>>`を持つ．`BufferPool<D: DiskManager>`の`D`にするため，`Box<dyn DiskManager>`に`DiskManager`を実装した．

```rust
impl DiskManager for Box<dyn DiskManager> {
    fn read_page(&self, page_id: PageId, buffer: &mut PageBytes) -> io::Result<()> {
        (**self).read_page(page_id, buffer)
    }
    // write_page，allocate_page，page_countも同じく中身に任せる
}
```

`HeapFile`のメソッドは，Iteration 14の`read`と`write`の代わりに`PageGuard`を使う．

```rust
    pub fn insert(&mut self, tuple: &[u8]) -> Result<RowId, HeapError> {
        if let Some(last) = self.pool.page_count().checked_sub(1) {
            let guard = self.pool.fetch_page(last)?;
            let result = guard.write().insert(tuple);
            match result {
                Ok(slot) => return Ok(RowId { page: last, slot }),
                Err(PageError::PageFull) => {}
                Err(error) => return Err(error.into()),
            }
        }
        if tuple.len() > MAX_TUPLE_SIZE {
            return Err(PageError::TupleTooLarge { size: tuple.len() }.into());
        }
        let guard = self.pool.new_page()?;
        let slot = guard.write().insert(tuple)?;
        Ok(RowId {
            page: guard.page_id(),
            slot,
        })
    }
```

- Iteration 14は，新しい`Page`にタプルを置いてからページ番号をもらった．`new_page`は先にディスクの末尾へページを加えるので，大きすぎるタプルを先に調べる．調べないと`tuple_larger_than_a_page_is_rejected`が失敗する．
- `guard.write()`の`RefMut`は，`let result = ...;`の文の終わりで捨てられる．

`update`で，`PageGuard`を変数に入れたまま`self.insert`を呼ぶと，次のエラーになった．

```text
error[E0502]: cannot borrow `*self` as mutable because it is also borrowed as immutable
   --> src/storage/heap.rs:99:30
    |
 94 |         let guard = self.pool.fetch_page(id.page)?;
    |                     --------- immutable borrow occurs here
...
 99 |                 let new_id = self.insert(tuple)?;
    |                              ^^^^^^^^^^^^^^^^^^ mutable borrow occurs here
...
105 |     }
    |     - immutable borrow might be used here, when `guard` is dropped and runs the `Drop` code for type `PageGuard`
```

`guard`は関数の終わりで捨てられ，そのとき`Drop`がプールを使う．`insert`は`&mut self`を受け取るので，プールを借りたままの`guard`とぶつかる．
ページの`update`の結果だけを変数に入れ，`PageGuard`はその文の終わりで捨てる．

```rust
    pub fn update(&mut self, id: RowId, tuple: &[u8]) -> Result<RowId, HeapError> {
        let result = self
            .pool
            .fetch_page(id.page)?
            .write()
            .update(id.slot, tuple);
        match result {
            Ok(()) => Ok(id),
            Err(PageError::PageFull) => {
                let new_id = self.insert(tuple)?;
                self.delete(id)?;
                Ok(new_id)
            }
            Err(error) => Err(error.into()),
        }
    }
```

`HeapError::Io`は`HeapError::Buffer(BufferError)`にした．`error`は`BufferError::AllPinned`を`XX000`の`no unpinned buffers available`に，`BufferError::Io`をIteration 14と同じ`58030`に変換する．

結合テストの`table_larger_than_the_buffer_pool_remains_after_reopening`は，ここまでの実装で通る．

## 15-6 振り返り

1. 追い出しの境界(ピン留め，変更の印，すべての枠がピン留め)と，ディスクを読み書きした回数を確かめる項目があるかを比べる．
2. `&mut self`にすると，`PageGuard`がプールを書き換えられる借用で借り続けるので，1つ目の`PageGuard`を捨てるまで2つ目を取得できない．B+木でノードを分割するときは，親と子の2つのページを同時に書き換える．`&self`のまま枠の状態を`Cell`と`RefCell`で書き換えるので，複数のページを同時にピン留めできる．
3. `unpin`を呼ぶ設計では，`?`でエラーを返す道のすべてで`unpin`を呼ぶ必要があり，1つでも忘れるとその枠はいつまでも追い出せない．`Drop`でピンを外せば，どの道で関数を抜けても`PageGuard`が捨てられるときに外れる．
4. 枠に残っていて変更の印のあるページが失われる．Iteration 14は操作のたびにページを書いたので，文が終わった変更はファイルにあった．Iteration 19では，変更を先にWALへ書き，コミットした変更を強制終了のあとでも取り戻せるようにする．
5. ファイルごとのプールは，表の数だけ枠が増え，よく使う表と使わない表に同じ数の枠を割り当てる．1つのプールなら枠の総数を決められ，よく使うページに枠が集まる．その代わり，ページテーブルのキーにファイルの区別が要る．PostgreSQLは1つの共有のプールを持ち，キーに表のファイルとページ番号を使う．
6. 模範解答の設計ドキュメントは，照合スクリプトで問題がないことを確かめてある．

## 15-7 発展課題

解答例である．`BufferPool`に`Cell<usize>`の`hits`と`misses`を加え，`fetch_page`で数える．

```rust
    pub fn fetch_page(&self, page_id: PageId) -> Result<PageGuard<'_>, BufferError> {
        if let Some(&index) = self.page_table.borrow().get(&page_id) {
            self.hits.set(self.hits.get() + 1);
            return Ok(self.pin(index));
        }
        self.misses.set(self.misses.get() + 1);
        // ここから先は変わらない
    }

    pub fn stats(&self) -> (usize, usize) {
        (self.hits.get(), self.misses.get())
    }
```

`HeapFile::buffer_stats`はプールの`stats`を返し，`Database::buffer_stats`はすべての表の回数を足す．

```rust
    pub fn buffer_stats(&self) -> (usize, usize) {
        let mut total = (0, 0);
        for heap in self.tables.values() {
            let (hits, misses) = heap.buffer_stats();
            total.0 += hits;
            total.1 += misses;
        }
        total
    }
```

`storage`はクレートの外に公開していないので，`BufferPool::stats`だけを作ると，使われないメソッドとして`cargo clippy`の`dead_code`の警告になる．`Database`まで回数を届けると，警告は消える．

```rust
#[test]
fn second_scan_hits_the_buffer_pool() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Database::open(dir.path()).unwrap();
    db.execute("CREATE TABLE t (a INTEGER)").unwrap();
    db.execute("INSERT INTO t VALUES (1), (2)").unwrap();
    drop(db);
    let mut db = Database::open(dir.path()).unwrap();
    db.execute("SELECT * FROM t").unwrap();
    assert_eq!(db.buffer_stats(), (0, 1));
    db.execute("SELECT * FROM t").unwrap();
    assert_eq!(db.buffer_stats(), (1, 1));
}
```

開き直した直後はどの枠も空なので，1回目の`SELECT`は1ページをディスクから読み，2回目は枠のページを使う．
