# Iteration 19：WALとクラッシュリカバリ

Iteration 15のバッファプールは，書き換えたページを枠に残す．プロセスが途中で止まると，コミットした変更も失われる．
このIterationでは，変更を先にログ(WAL)へ書き，コミットではログがディスクに届くまで待つ．プロセスが止まったあとは，開くときにログから変更をやり直す．
Rustでは，書き込み先を`Write`の型引数にする方法，`BufWriter`と`sync_data`，`Rc<RefCell<T>>`による共有，子プロセスを強制終了させるテストを学ぶ．

## 19-1 準備

`cargo test`を実行し，引き継いだテストがすべて通ることを確かめる．次は結果の要約である．

```text
test result: ok. 315 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.83s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.17s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.72s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.15s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

REPLを`--data-dir`で起動して行を加え，別の端末から`kill -9`でプロセスを止める．もう一度起動して，行が残っているかを見ておく．

## 19-2 文法と概念

次の2つのノートを読む．

- [Rustのノート](../../../../docs/rust/iteration-19.md)：`Write`を型引数にする，`BufWriter`と`sync_data`，型引数を決めた`impl`，`Rc<RefCell<T>>`，`if let`をつなぐ条件，`crc32fast`，子プロセスを強制終了させるテスト
- [データベースのノート](../../../../docs/db/iteration-19.md)：WALとその規則，LSN，ページイメージ，REDO，チェックポイント，壊れたログの末尾

ノートの`crc32fast`を試す前に，`crc32fast`を依存に加えておく．
読み終えたら，一時的なモジュールにテストを書いて，次の課題を確かめる．

1. `Write`を型引数に取り，行に番号を付けて書く`Numbered<W: Write>`を作る．`Vec<u8>`に書いて，`1: a\n2: b\n`になることを確かめる．
2. `BufWriter<File>`に5バイトを書き，`flush`の前と後で，`fs::read`で読んだファイルの内容を比べる．
3. `Rc<RefCell<Vec<String>>>`を2つの構造体で共有し，両方から文字列を加える．`Rc::strong_count`はいくつになるか．
4. `crc32fast::hash`で`b"abc"`と`b"abd"`のCRC-32を計算し，違う値になることを確かめる．

## 19-3 テストリスト

### 要件

- ページの変更，コミット，中止は，先にWALファイル(データディレクトリの`wal`)へ記録する．トランザクションの開始も記録する．
- ページの変更は，書き換えたあとのページの内容を丸ごと記録する(`PageImage`)．
- コミットはWALをディスクに書き込んで(`sync_data`)から完了とする．
- ページを書き戻す前に，そのページの変更を記録したWALを書き込む．
- 起動時にWALを読み，最後のチェックポイントからREDOする．コミットの記録がないトランザクションは中止扱いにする．
- WALのレコードにチェックサム(CRC-32)を付け，壊れた末尾のレコードは読み捨てる．
- `CHECKPOINT`文で，変更されたページを書き戻してディスクに届け，トランザクションの状態を`xact`に書き，チェックポイントを記録する．データベースを開くときと，表とインデックスを作る文と消す文のあとにも，チェックポイントを取る．

### 使用例

```console
$ cargo run -q -- repl --data-dir ./data
ferrodb> CREATE TABLE t (a INTEGER);
CREATE TABLE
ferrodb> INSERT INTO t VALUES (1);
INSERT 0 1
$ cargo run -q -- repl --data-dir ./data
ferrodb> INSERT INTO t VALUES (2);
INSERT 0 1
(別の端末で kill -9 を実行し，プロセスを強制終了する)
$ cargo run -q -- repl --data-dir ./data
ferrodb> SELECT * FROM t;
 A
---
 1
 2
(2 rows)
```

### 作るもの

| モジュール | 作るもの |
| --- | --- |
| `wal` | `pub struct Lsn(pub u64)`，`enum WalRecord { Begin { xid }, Commit { xid }, Abort { xid }, PageImage { file, page, bytes }, Checkpoint }` |
| `wal` | `struct WalWriter<W: Write>`と`new(out: W, start: Lsn)`，`append(&mut self, record: &WalRecord) -> io::Result<Lsn>`，`next_lsn`．`WalWriter<File>`の`open(path, end)`と`flush_to(lsn)` |
| `wal` | `pub fn read_records(bytes: &[u8]) -> (Vec<(WalRecord, Lsn)>, Lsn)`，`pub fn recover(dir: &Path, wal: &Path, manager: &mut TransactionManager) -> io::Result<Lsn>` |
| `storage::buffer` | `pub struct PageLog { wal: Rc<RefCell<WalWriter<File>>>, file: String }`，`BufferPool::with_log`．枠の`page_lsn` |
| `storage::disk` | `DiskManager::sync(&mut self) -> io::Result<()>` |
| `storage::heap` | `HeapFile::with_log`，`HeapFile::flush` |
| `txn` | `TransactionManager::set_status`，`abort_unfinished` |
| `sql::parser` | `Statement::Checkpoint`．キーワード`CHECKPOINT` |
| `database` | 開くときのリカバリ，開始，コミット，中止の記録，チェックポイント，`StatementResult::Checkpoint` |

### 書くときに考えること

- レコードの符号化と読み戻しは，`WalWriter<Vec<u8>>`でファイルを使わずに単体テストできる．
- 壊れた末尾は，書いたバイト列の最後を切るか，1バイトを書き換えて作る．
- リカバリの単体テストでは，ログのファイルを直接書いてから`recover`を呼ぶ．チェックポイントの前後の記録と，コミットしていないトランザクションを入れる．
- WALの規則は，枠が1つのバッファプールで，書き換えたページを追い出させて確かめる．
- プロセスを強制終了したあとのことは，テストのバイナリを子プロセスとして起動し，`std::process::abort()`で止めて確かめる．
- データディレクトリのファイルの一覧を確かめる引き継いだテストは，どう変わるか．

## 19-4 設計ドキュメント

- `c4-container.md`：ログのファイルを加える．`xact`を書く時点も見直す．
- `c4-component.md`：`wal`を加える．どのモジュールがログに書き，どのモジュールがログを読むか．
- `code-types.md`：ログの型の図を加え，`BufferPool`，`PageGuard`，`DiskManager`，`HeapFile`，`Database`，`TransactionManager`を更新する．
- `layout.md`：ログのレコードの配置を加える．
- `code-sequence.md`：コミットの流れと，開くときのリカバリの流れを加える．

更新したら，リポジトリのルートでMermaidの構文を検査し，照合スクリプトも実行する．

## 19-5 テスト駆動の実装

### 受講者が行うツール操作

レコードのCRC-32を計算する`crc32fast`を，依存に加える．

### 実装のヒント

- `wal`から作る．レコードは，種類の1バイトのあとに内容を並べ，前に内容のバイト数とCRC-32を置く．
- `append`は`BufWriter`に書くだけで，ディスクに届けるのは`flush_to`である．`flush_to`は，`BufWriter::flush`のあとに`get_ref().sync_data()`を呼ぶ．
- `BufferPool`は`Option<PageLog>`を持ち，`PageGuard`は`write`で書き換えたかを覚える．`PageGuard`を捨てるときに，書き換えていれば`PageImage`を記録して，枠の`page_lsn`をレコードの位置にする．
- `Drop`はエラーを返せない．ログに書けなかったときは，`WalWriter`に失敗を覚えさせ，次の`flush_to`でエラーにする．
- `new_page`の`PageGuard`は，書き換えたものとして扱う．ファイルには0のページしかないからである．
- 枠を書き戻す前に，その枠の`page_lsn`まで`flush_to`する．
- `recover`は，`xact`から読んだトランザクションの状態に，最後のチェックポイントのあとの`Begin`，`Commit`，`Abort`を反映する．
- `Database`は`Option<Rc<RefCell<WalWriter<File>>>>`を持ち，表とインデックスのプールに`Rc::clone`で渡す．データディレクトリのないデータベースは，ログを使わない．
- 文の`alt`は9個なので，`EXPLAIN`と`CHECKPOINT`を1つの`alt`にまとめる．
- 引き継いだ`xact`は，トランザクションを始めるときと終えるときに書いていた．ログに開始，コミット，中止を記録するので，チェックポイントで書けばよい．

## 19-6 振り返り

1. 自分の`TESTLIST.md`を[模範解答のテストリスト](../../solution/TESTLIST.md)と比べる．
2. ページを丸ごと記録する設計と，タプルの追加や`xmax`の書き換えを記録する設計を比べる．ログの大きさと，やり直すときに要る情報はどう違うか．
3. コミットのたびに`sync_data`を呼ぶので，1行ずつ`INSERT`すると遅くなる．多くの行を速く加えるには，どうすればよいか．
4. `abort`で止めたプロセスでは，OSのメモリーに書いた内容は残る．OSや電源が止まる場合も考えると，チェックポイントで何をディスクに届ける必要があるか．
5. 表を消してから同じ名前の表を作ったあとで止まり，チェックポイントなしに古いログからやり直すと，何が起きるか．
6. 設計ドキュメントと実装を見比べ，違うところがあれば設計ドキュメントを直す．

## 19-7 発展課題

ログのファイルは末尾に書き足し続けるので，大きくなり続ける．
チェックポイントで，それより前のレコードは要らなくなる．チェックポイントのレコードを書く前に，ログのファイルを空にして先頭から書き直す`WalWriter<File>::truncate`を作る．

```rust
db.execute("INSERT INTO t VALUES (1)")?;   // ログは8192バイトより大きい
db.execute("CHECKPOINT")?;
// ログは，チェックポイントのレコード(9バイト)と，CHECKPOINTの文のトランザクションのコミット(13バイト)だけになる
assert_eq!(std::fs::metadata(dir.path().join("wal"))?.len(), 9 + 13);
```

これまでと同じく，テストリスト，設計ドキュメント，実装の順に進める．
