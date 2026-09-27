# Iteration 16：B+木

ヒープファイルから`WHERE id = 42`の行を探すには，すべてのページを読むしかない．
このIterationでは，キーから行の位置(`RowId`)を引くB+木を，Iteration 15のバッファプールのページの上に作る．Iteration 17で，これを表のインデックスとして使う．
Rustでは，トレイトの関連関数，ライフタイムと型の引数を持つ構造体，`Iterator`の実装，範囲を受け取る`RangeBounds`を学ぶ．

## 16-1 準備

`cargo test`を実行し，引き継いだテストがすべて通ることを確かめる．次は結果の要約である．

```text
test result: ok. 257 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

## 16-2 文法と概念

次の2つのノートを読む．

- [Rustのノート](../../../../docs/rust/iteration-16.md)：トレイトの関連関数と`Sized`，`PhantomData`，`Iterator`の実装，`RangeBounds`と`Bound`，`partition_point`と`binary_search`
- [データベースのノート](../../../../docs/db/iteration-16.md)：B+木の形，検索と範囲の走査，挿入と分割，順序を保つキーの符号化，同じキーの項目

読み終えたら，一時的なモジュールにテストを書いて，次の課題を確かめる．

1. `i16`を2バイトに符号化する関数を書く．バイト列を辞書式に比べた順序が，`-300`，`-1`，`0`，`1`，`300`の順序と同じになることを確かめる．
2. `1`から`n`までの2乗を順に返すイテレーター`Squares`を実装し，`Squares::new(4).collect::<Vec<u64>>()`が`[1, 4, 9, 16]`になることを確かめる．
3. 整列した`Vec<i32>`と`impl RangeBounds<i32>`を受け取り，範囲にある値を返す関数を，`partition_point`で始まりの位置を求めて書く．`3..7`，`3..=7`，`..`で確かめる．

## 16-3 テストリスト

### 要件

- キーから`RowId`を引くB+木を，バッファプールのページ上に作る．
- 挿入，完全一致の検索，範囲の走査，削除を行える．
- 葉と内部ノードは，いっぱいになったら分割する．削除では併合しない．
- キーは`INTEGER`，`BIGINT`，`BOOLEAN`，`VARCHAR`(Rustの`i32`，`i64`，`bool`，`String`)とし，バイト列の比較で順序が保たれるように符号化する．
- 同じキーを複数の行に使える．項目はキーと行の位置の組で並べる．
- 符号化したキーが2000バイトを超えたらエラーにする．
- ページ0に根のページ番号を書き，木の状態をすべてページに置く．
- SQLとしての振る舞いは変えない．

### 使用例

```rust
let mut tree = BTree::<i32>::create(&pool)?;
tree.insert(42, row_id)?;
assert_eq!(tree.get(&42)?, Some(row_id));
let entries: Vec<(i32, RowId)> = tree.range(10..50)?.collect::<Result<_, _>>()?;
```

`pool`は`BufferPool<Box<dyn DiskManager>>`である．

### 作るもの

| モジュール | 作るもの |
| --- | --- |
| `index::key` | `pub trait IndexKey: Sized { fn encode(&self) -> Vec<u8>; fn decode(bytes: &[u8]) -> Option<Self>; }`と，`i32`，`i64`，`bool`，`String`への実装 |
| `index::btree` | `pub struct BTree<'a, K: IndexKey>`と`create(pool: &'a BufferPool<Box<dyn DiskManager>>) -> Result<BTree<'a, K>, BTreeError>` |
| `index::btree` | `insert(&mut self, key: K, id: RowId) -> Result<(), BTreeError>`，`get(&self, key: &K) -> Result<Option<RowId>, BTreeError>`，`delete(&mut self, key: &K, id: RowId) -> Result<bool, BTreeError>`，`range(&self, bounds: impl RangeBounds<K>) -> Result<RangeIter<'a, K>, BTreeError>` |
| `index::btree` | `pub struct RangeIter<'a, K: IndexKey>`．`Iterator<Item = Result<(K, RowId), BTreeError>>`を実装する |
| `index::btree` | `enum BTreeError { KeyTooLarge { size }, Corrupted, Buffer(BufferError) }` |
| `storage::heap` | `RowId`に`PartialOrd`と`Ord`を導出する |
| `lib.rs` | `pub mod index;`と`pub mod storage;` |

`get`は，同じキーの項目が複数あれば，`RowId`が最も小さい項目を返す．

Iteration 17で，`BTree`をデータベースから使う．このIterationでは，`index`と`storage`をクレートの外に公開し，`tests/`の結合テストから使う．公開しないと，クレートの中で使われていない関数として`cargo clippy`がエラーにする．

### 書くときに考えること

- キーの符号化は，B+木と切り離して単体テストできる．並べた値を符号化したバイト列も，同じ順に並ぶか．最小値，最大値，0の前後はどうか．
- 項目が少ないうちは，木は葉1つである．葉の分割，根の分割，内部ノードの分割をテストで起こすには，何個の項目が要るか．長いキーを使うと，少ない項目で分割できる．
- 範囲の始まりと終わりには「含む」「含まない」「限りがない」の3つがある．その組み合わせと，範囲の中に項目がない場合を考える．
- 同じキーの項目がいくつもあるとき，`get`，`range`，`delete`はどうふるまうか．
- 枠の少ないバッファプールでも木が動くか．下りる途中で，いくつのページを同時にピン留めしているか．
- 結合テストは`tests/`に新しいファイルを作り，ファイルのバッファプールで使用例を確かめる．

## 16-4 設計ドキュメント

- `c4-component.md`：`index`の境界と，`index::btree`，`index::key`を加える．`index::btree`はどのモジュールに依存するか．
- `code-types.md`：インデックスの型の図を新しく加える．`IndexKey`，`BTree`，`RangeIter`，`BTreeError`と，ノードや項目を表す型を描く．
- `layout.md`：メタページ，ノードのヘッダー，葉の項目，内部ノードの項目のバイト配置を加える．
- `code-sequence.md`：`insert`で葉を分割し，区切りを親に加える流れを加える．根を分割する場合も描く．

更新したら，リポジトリのルートでMermaidの構文を検査し，照合スクリプトも実行する．

## 16-5 テスト駆動の実装

### 実装のヒント

- ノードは`enum Node { Leaf { entries, next }, Internal { first, children } }`で表し，ページのバイト列と相互に変換すると扱いやすい．操作のたびにページからノードを読み，変えたノードをページへ書く．
- ページのバイト列は`Page::bytes`で読む．書くときは，`Page::from_bytes`で作ったページに置き換える(`*guard.write() = Page::from_bytes(data);`)．
- 項目は，符号化したキーと`RowId`の構造体にして`Ord`を導出する．フィールドの順に比べるので，キーが同じなら`RowId`で並ぶ．
- 内部ノードの区切りも項目にする．子を選ぶには，`partition_point`で「区切りが探す項目以下」の境目を求める．
- `insert`は，下りる途中の内部ノードのページ番号を`Vec`に積んでおく．分割した区切りを親に加えるとき，`pop`で親を取り出す．
- 分割する位置を項目の数の半分にすると，キーの長さが偏ったときに片方がページに入らないことがある．バイト数がほぼ半分になる位置で分ける．
- 範囲の走査は，範囲の始まりのキーで葉まで下り，葉の項目を`Vec`に読み出して1つずつ返す．読み終えたら右隣の葉を読む．`PageGuard`を持ち続けないので，イテレーターはプールを借りるだけで済む．
- `storage`を公開すると，`Page::new`に`Default`を実装するよう`cargo clippy`が求める．

## 16-6 振り返り

1. 自分の`TESTLIST.md`を[模範解答のテストリスト](../../solution/TESTLIST.md)と比べる．
2. ノードを，ページの中のバイトを直接書き換えるのでなく，`Node`に読み出して書き戻した．ページの中で直接書き換える設計と比べて，何が簡単になり，何が遅くなるか．
3. キーを`IndexKey`の型引数`K`で受け取った．`Value`で受け取る設計と比べて，`BTree<i32>`に`String`のキーを入れる誤りはいつ見つかるか．
4. 範囲の走査で，ピン留めしたまま葉を読み進める設計と比べる．ピン留めしたままだと，`RangeIter`を使っている間に何ができなくなるか．
5. 削除で併合しない木に，多くの挿入と削除を繰り返すと，どうなるか．
6. 設計ドキュメントと実装を見比べ，違うところがあれば設計ドキュメントを直す．

## 16-7 発展課題

`create`で作った木のファイルを，プロセスを終えたあとで開き直せるようにする．
`BTree::open(pool)`は，ページ0から根のページ番号を読む木を返す．空のプールなら`BTreeError::Corrupted`を返す．

```rust
let pool = BufferPool::new(Box::new(FileDiskManager::open(&path)?) as Box<dyn DiskManager>, 4);
let tree = BTree::<i32>::open(&pool)?;
assert_eq!(tree.get(&999)?, Some(row_id));
```

これまでと同じく，テストリスト，設計ドキュメント，実装の順に進める．
