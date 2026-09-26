# Iteration 13：ページとタプルのバイト表現

このIterationから，データを保存する層(ストレージ)を作る．
行をバイト列(タプル)に符号化し，8192バイトのスロット付きページに置く．表の行は，`Vec<Row>`からページの列(ヒープファイル)に置き換える．
SQLとしての振る舞いは変えない．Rustでは，バイト列を扱う配列，数とバイト列の変換，失敗しうる型の変換を学ぶ．

## 13-1 準備

`cargo test`を実行し，引き継いだテストがすべて通ることを確かめる．次は結果の要約である．

```text
test result: ok. 220 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

## 13-2 文法と概念

次の2つのノートを読む．

- [Rustのノート](../../../../docs/rust/iteration-13.md)：固定長配列とスライス，`const`，`to_le_bytes`と`from_le_bytes`，`TryFrom`，`split_at`，ビットの操作
- [データベースのノート](../../../../docs/db/iteration-13.md)：ページ，タプルの符号化，スロット付きページ，行の位置，ヒープファイル

読み終えたら，一時的なモジュールにテストを書いて，次の課題を確かめる．

1. `&[i32]`を，値ごとに4バイトのリトルエンディアンで並べた`Vec<u8>`にする関数と，元に戻す関数を書く．戻す関数は`split_at`で4バイトずつ読み，長さが4の倍数でなければ`None`を返す．
2. `u8::try_from`で，`255`と`300`を`u8`にしてみる．
3. `&[bool]`を8個ずつ1バイトにまとめ，ビットマップを作る関数を書く．`i`番目が真なら，`i / 8`バイト目の`i % 8`ビットを1にする．

## 13-3 テストリスト

### 要件

- 行(タプル)をバイト列に符号化し，元に戻せる．`NULL`はビットマップで表す．
- 8192バイトのスロット付きページに，タプルを追加，取得，削除，更新できる．
- ページに入らないタプルは，エラーを返す．SQLでは`54000`とし，メッセージを`row is too big: size 9003, maximum size 8184`の形にする．
- この時点では，表のデータはメモリ上のページの列に置く．SQLとしての振る舞いは変えない．

バイト配置は次のとおりとする．数値はすべてリトルエンディアンで書く．

- タプル：`NULL`のビットマップ(列ごとに1ビット，`(列の数 + 7) / 8`バイト)のあとに，`NULL`でない列の値を列の順に並べる．`INTEGER`は4バイト，`BIGINT`は8バイト，`BOOLEAN`は1バイト(`0`か`1`)，`VARCHAR`は2バイトのバイト数とUTF-8のバイト列である．
- ページ：先頭の4バイトをヘッダーとし，スロットの数(2バイト)と，タプルを置いた領域の始まりの位置(2バイト)を書く．そのあとにスロット(タプルの位置と長さ，それぞれ2バイト)を並べる．タプルはページの末尾から前へ詰める．消したタプルのスロットは位置と長さを0にし，その番号をほかのタプルに使わない．

### 使用例

```rust
let mut page = Page::new();
let slot = page.insert(&encode_tuple(&row, &schema))?;
assert_eq!(decode_tuple(page.get(slot).unwrap(), &schema)?, row);
```

### 作るもの

| モジュール | 作るもの |
| --- | --- |
| `storage::tuple` | `pub fn encode_tuple(row: &[Value], schema: &TableSchema) -> Vec<u8>`，`pub fn decode_tuple(bytes: &[u8], schema: &TableSchema) -> Result<Row, TupleError>`，`enum TupleError` |
| `storage::page` | `pub const PAGE_SIZE: usize = 8192`，`MAX_TUPLE_SIZE`，`pub type SlotId = u16`，`struct Page`と`new`，`insert`，`get`，`update`，`delete`，`slot_count`，`enum PageError { PageFull, TupleTooLarge { size } }` |
| `storage::heap` | `struct RowId { page: usize, slot: SlotId }`，`struct HeapFile`と`new`，`insert`，`update`，`delete`，`tuples`，`rows(schema)` |
| `exec::dml` | `insert`，`update`，`delete`が`&mut HeapFile`を受け取る |
| `exec::build` | `build(plan, tables: &HashMap<String, HeapFile>, catalog: &Catalog) -> Result<Box<dyn Executor>, Error>` |
| `error` | `SqlState::ProgramLimitExceeded`(`54000`)と，`PageError`，`TupleError`からの変換 |

`HeapFile::update`は，新しいタプルが同じページに入らなければ別のページに移し，新しい`RowId`を返す．

### 書くときに考えること

- タプルの単体テストでは，符号化したバイト列そのものを期待値に書く項目と，符号化して復号すると元に戻る項目を分ける．
- ページの単体テストでは，ページの中のバイトを直接比べれば，ヘッダーとスロットの書き方を確かめられる．テストはページのモジュールの中にあるので，非公開のフィールドも読める．
- 空きの境界を考える．2つの4000バイトのタプルを置いたページには，あと何バイトのタプルを置けるか．
- `exec::dml`の単体テストは，表の行の持ち方が変わると，どう変わるか．
- SQLの結合テストは，行の持ち方を置き換えている間の安全網になる．

## 13-4 設計ドキュメント

- `layout.md`：新たに作る．ページの先頭(ヘッダーとスロット)と，タプルのバイト配置をMermaidの`packet`図で描く．`packet`図の目盛りはビットである．
- `c4-component.md`：`storage::page`，`storage::tuple`，`storage::heap`を加える．どのモジュールが`HeapFile`を使うか．
- `code-types.md`：`Page`，`SlotId`，`RowId`，`HeapFile`とエラーの型を加え，`Database`を更新する．
- `code-sequence.md`：`UPDATE`が，行を読み，検査してから，行の位置のタプルを書き換える流れにする．

更新したら，リポジトリのルートでMermaidの構文を検査する．

## 13-5 テスト駆動の実装

### 実装のヒント

- `encode_tuple`は，ビットマップの分の0を並べた`Vec<u8>`から始め，`NULL`ならビットを立て，そうでなければ値のバイトを末尾に加える．
- `decode_tuple`は，列の型に従って，`split_at`で先頭から値のバイトを切り出す．足りなければエラーにする．
- `Page`は`Box<[u8; PAGE_SIZE]>`を持ち，ヘッダーとスロットの2バイトの数を読み書きする小さな関数を作る．
- タプルを置くには，タプルの長さとスロットの4バイトの空きが要る．空きは，スロットの並びの終わりからタプルの領域の始まりまでである．
- `HeapFile::insert`は，最後のページに置けなければ新しいページを加える．
- `exec::dml`は，`HeapFile::rows`で行と位置を読み，Iteration 8と同じく制約を検査してから，位置のタプルを書き換える．ページに入らない行も，書き換える前に調べる．
- `SeqScan`は行の`Vec`を受け取るので，`build`で`HeapFile::rows`の行を渡す．演算子の単体テストは変わらない．

## 13-6 振り返り

1. 自分の`TESTLIST.md`を[模範解答のテストリスト](../../solution/TESTLIST.md)と比べる．
2. タプルに列の型を書かず，復号に`TableSchema`を渡した．値ごとに型の印(1バイト)を書く設計と比べる．タプルの大きさと，`ALTER TABLE`で列の型を変えるときの扱いはどう違うか．
3. `UPDATE`で行が別のページに移ると，`RowId`が変わる．`RowId`を覚えている仕組み(Iteration 16のインデックス)にとって，何が問題になるか．
4. `exec::dml`は，行を読んで検査し終えてから，ページを書き換える．ページを書き換えながら制約を検査する設計では，文の途中で違反を見つけたときに何が必要になるか．
5. 設計ドキュメントと実装を見比べ，違うところがあれば設計ドキュメントを直す．

## 13-7 発展課題

消したタプルと，更新で置き直したタプルの古い領域は，今のページでは再利用されない．
残っているタプルをページの末尾へ詰め直し，空きを取り戻す`Page::compact`を作る．スロットの番号は変えない．
`insert`と`update`は，空きが足りなければ`compact`してから置き直す．それでも足りなければ，これまでどおりエラーにする．

```rust
let mut page = Page::new();
page.insert(&[1; 4000])?;
page.insert(&[2; 4000])?;
page.delete(0);
page.insert(&[3; 3900])?; // 詰めれば入る
```

これまでと同じく，テストリスト，設計ドキュメント，実装の順に進める．
