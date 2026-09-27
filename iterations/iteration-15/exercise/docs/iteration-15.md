# Iteration 15：バッファプール

Iteration 14のヒープファイルは，タプルを読み書きするたびにファイルのページを読んだ．
このIterationでは，読んだページをメモリーの枠に残して使い回すバッファプールを作り，ヒープファイルのページをバッファプールで読み書きする．
Rustでは，参照を持つ構造体とライフタイム注釈，値を捨てるときに走る`Drop`，`&self`のメソッドで状態を書き換える`Cell`と`RefCell`を学ぶ．

## 15-1 準備

`cargo test`を実行し，引き継いだテストがすべて通ることを確かめる．次は結果の要約である．

```text
test result: ok. 245 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`src/storage/heap.rs`の`HeapFile::read`と`HeapFile::write`を読み，1回の`insert`でファイルを何回読み書きするかを数えておく．

## 15-2 文法と概念

次の2つのノートを読む．

- [Rustのノート](../../../../docs/rust/iteration-15.md)：ライフタイム注釈と省略の規則，参照を持つ構造体，`Drop`とRAII，`Cell`と`RefCell`，トレイト境界を持つジェネリックな構造体，`impl Fn`の引数
- [データベースのノート](../../../../docs/db/iteration-15.md)：バッファプール，書き戻し，ピン留め，クロック方式

読み終えたら，一時的なモジュールにテストを書いて，次の課題を確かめる．

1. `&str`の中の最初の数字の並び(`"ab12cd"`なら`"12"`)を返す関数を，戻り値を引数の一部の`&str`にして書く．ライフタイム注釈は要るか．
2. `&[i32]`を借りて，先頭から1つずつ値を返す構造体`Cursor<'a>`と，メソッド`next_value(&mut self) -> Option<i32>`を書く．
3. `Drop`を実装した構造体を3つ作り，捨てられる順序を`RefCell<Vec<String>>`に記録して確かめる．
4. `RefCell`で，`borrow()`を持ったまま`borrow_mut()`を呼ぶと何が起きるか．`try_borrow_mut()`ではどうなるか．

## 15-3 テストリスト

### 要件

- ページをバッファプール経由で読み書きする．枠の数はプールを作るときに決める．ヒープファイルは，ファイルごとに16枠のプールを持つ．
- 使われているページ(ピン留めされたページ)は追い出さない．
- 枠が足りなければ，クロック方式で追い出すページを選ぶ．変更されたページは書き戻してから追い出す．
- すべての枠がピン留めされていたら，エラーを返す．SQLでは`XX000`とし，メッセージを`no unpinned buffers available`とする．
- プールを捨てるとき(データベースを閉じるとき)に，変更されたページを書き戻す．
- SQLとしての振る舞いは変えない．

### 使用例

```rust
let pool = BufferPool::new(disk, 3);
let guard = pool.fetch_page(page_id)?;   // ピン留め
let slot = guard.write().insert(&bytes)?; // 変更を記録
drop(guard);                               // ピンを外す
```

### 作るもの

| モジュール | 作るもの |
| --- | --- |
| `storage::buffer` | `struct BufferPool<D: DiskManager>`と`new(disk: D, frame_count: usize)`，`page_count`，`fetch_page(&self, page_id: PageId) -> Result<PageGuard<'_>, BufferError>`，`new_page(&self) -> Result<PageGuard<'_>, BufferError>`，`flush_all(&self) -> io::Result<()>` |
| `storage::buffer` | `struct PageGuard<'a>`と`page_id`，`read(&self) -> Ref<'_, Page>`，`write(&self) -> RefMut<'_, Page>` |
| `storage::buffer` | `struct ClockReplacer`と`new(frame_count: usize)`，`access(&mut self, index: usize)`，`victim(&mut self, is_pinned: impl Fn(usize) -> bool) -> Option<usize>` |
| `storage::buffer` | `enum BufferError { AllPinned, Io(io::Error) }` |
| `storage::disk` | `impl DiskManager for Box<dyn DiskManager>` |
| `storage::heap` | `HeapFile`が`BufferPool<Box<dyn DiskManager>>`を持つ．`HeapError::Io`を`HeapError::Buffer(BufferError)`にする |
| `error` | `BufferError`からの変換 |

`BufferPool`のメソッドは，すべて`&self`を受け取る．プールを共有の参照で渡したまま，複数のページをピン留めできる．

### 書くときに考えること

- ディスクを読んだ回数や書いた回数を確かめるには，数を数える`DiskManager`をテストの中に作る．`read_page`は`&self`を受け取る．
- 枠の数を2や3にすれば，追い出しの境界をテストで作りやすい．
- ピン留めしたままのページと，`PageGuard`を捨てたあとのページで，何が違うか．
- 書き換えたページと読んだだけのページで，追い出すときに何が違うか．
- クロック方式は，バッファプールと切り離して単体テストできる．参照ビットとピン留めの組み合わせを考える．
- 引き継いだテストのうち，`HeapFile`の非公開のフィールドを読むものはどれか．
- 表のページが枠の数より多くなったとき，データベースを閉じて開き直しても行が残ることを，結合テストで確かめる．

## 15-4 設計ドキュメント

- `c4-component.md`：`storage::buffer`を加える．`storage::heap`は，どのモジュールを通してページを読み書きするか．`storage::buffer`は何に依存するか．
- `code-types.md`：`BufferPool`，`PageGuard`，`ClockReplacer`，`BufferError`を加え，`HeapFile`と`HeapError`を更新する．枠の状態(ページ番号，ピンの数，変更の印)をどこに持つか．
- `code-sequence.md`：`fetch_page`でページを取得する流れを加える．枠にある場合，空いた枠がある場合，追い出す場合を分けて描く．

更新したら，リポジトリのルートでMermaidの構文を検査する．

## 15-5 テスト駆動の実装

### 実装のヒント

- 下の層から作る．`ClockReplacer`，`BufferPool`，`HeapFile`の順にすれば，上の層のテストで下の層を信用できる．
- 枠は，ページ番号(`Cell<Option<PageId>>`)，ピンの数(`Cell<usize>`)，変更の印(`Cell<bool>`)，ページ(`RefCell<Page>`)を持つ構造体にする．ページ番号から枠の番号を引く`HashMap`も要る．
- `PageGuard<'a>`は枠への参照`&'a Frame`を持ち，`Drop`でピンの数を1減らす．
- `ClockReplacer::victim`には，枠がピン留めされているかを答えるクロージャを渡す．参照ビットが立った枠を飛ばしながら2周しても見つからなければ，すべての枠がピン留めされている．
- 追い出す枠に変更の印があれば，ページ番号の位置に書き戻してから，`HashMap`から古いページ番号を消す．
- `new_page`は`DiskManager::allocate_page`で番号を得て，枠に`Page::new()`を置く．枠のページはまだディスクに書いていないので，変更の印を付けておく．
- `impl Drop for BufferPool<D>`で`flush_all`を呼ぶ．`drop`はエラーを返せない．
- `HeapFile::insert`で，大きすぎるタプルのために新しいページを加えないよう，先に大きさを調べる．
- `HeapFile::update`で`PageFull`になったら，`insert`と`delete`を呼ぶ．`PageGuard`や`RefMut`を変数に入れたまま`self.insert`を呼ぶと，借用検査のエラーになる．ページの`update`の結果だけを変数に入れ，`PageGuard`はその文の終わりで捨てる．
- `Database`は`#[derive(Debug)]`を持つ．`HeapFile`の`Debug`は，ページの数を`BufferPool`から読む．

## 15-6 振り返り

1. 自分の`TESTLIST.md`を[模範解答のテストリスト](../../solution/TESTLIST.md)と比べる．
2. `BufferPool`のメソッドを`&mut self`にすると，ページを2つ同時にピン留めできるか．Iteration 16のB+木は，親と子のノードのページを同時に使う．
3. `PageGuard`の`Drop`でピンを外す代わりに，`unpin(page_id)`を呼ぶ設計と比べる．`?`によって途中で関数を抜けたとき，どうなるか．
4. プールを捨てるときに書き戻す設計では，プロセスを強制終了すると何が失われるか．Iteration 14と比べる．
5. ヒープファイルごとにプールを持つ設計と，データベースに1つのプールを持つ設計を比べる．枠の数の決め方と，表の大きさの偏りはどう影響するか．
6. 設計ドキュメントと実装を見比べ，違うところがあれば設計ドキュメントを直す．

## 15-7 発展課題

`fetch_page`で，枠にページがあった回数(ヒット)と，ディスクから読んだ回数(ミス)を数える．
`BufferPool::stats`でプールの回数を，`Database::buffer_stats`ですべての表のプールの合計を返す．

```rust
let mut db = Database::open(dir.path())?; // 行のある表 t を開き直す
db.execute("SELECT * FROM t")?;
assert_eq!(db.buffer_stats(), (0, 1)); // (ヒット，ミス)
db.execute("SELECT * FROM t")?;
assert_eq!(db.buffer_stats(), (1, 1));
```

これまでと同じく，テストリスト，設計ドキュメント，実装の順に進める．
