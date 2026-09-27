# Iteration 20：PostgreSQL互換プロトコル

これまでの`ferrodb`は，REPLかRustのプログラムから，同じプロセスの中で使ってきた．
このIterationでは，PostgreSQLのフロントエンド/バックエンドプロトコルを話すサーバーを作り，`psql`や`postgres`クレートからTCPで接続して問い合わせられるようにする．
Rustでは，`TcpListener`，`Read`と`Write`を境界にした型引数，`Cursor`と自分の型への`Read`と`Write`の実装によるテスト，ビッグエンディアンのバイト列の読み書きを学ぶ．

## 20-1 準備

`cargo test`を実行し，引き継いだテストがすべて通ることを確かめる．次は結果の要約である．

```text
test result: ok. 322 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.83s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.13s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.81s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.71s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.09s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`psql --version`で，`psql`が使えることを確かめておく．

## 20-2 文法と概念

次の2つのノートを読む．

- [Rustのノート](../../../../docs/rust/iteration-20.md)：`TcpListener`，`Read + Write`の型引数，`Cursor`と`Read`と`Write`の実装，ビッグエンディアン，書き込みをまとめる，`clap`の既定値，テストの`thread::spawn`，`postgres`クレート，`--release`
- [データベースのノート](../../../../docs/db/iteration-20.md)：クライアントとサーバー，メッセージの形式，起動と認証，Simple Query，型OIDとテキスト形式，`ErrorResponse`，`ReadyForQuery`

読み終えたら，一時的なモジュールにテストを書いて，次の課題を確かめる．

1. 4バイトのビッグエンディアンの長さのあとに本文を置く`write_frame`と`read_frame`を作る．`Vec<u8>`に書いたバイト列を`Cursor`で読み戻し，もう1回読むと`None`になることを確かめる．
2. 読む元と書く先を別に持つ型に`Read`と`Write`を実装し，読んだ本文を大文字にして書き返す`echo_upper<S: Read + Write>`に渡す．
3. `TcpListener`をポート0で作り，`thread::spawn`で動かした`echo_upper`に`TcpStream`で接続して，大文字の本文が返ることを確かめる．
4. `echo_upper`に`&[u8]`を渡すと，コンパイラーは何と言うか．

## 20-3 テストリスト

### 要件

- サブコマンド`serve`の`--data-dir DIR --port N`で，TCPの接続を1つずつ受け付ける．`--port`を省略すると5433を使う．
- PostgreSQLのフロントエンド/バックエンドプロトコル3.0の，起動，認証(常に成功)，Simple Query，終了を実装する．SSLの要求には`N`を返す．
- 問い合わせの結果を`RowDescription`，`DataRow`，`CommandComplete`で返す．値はテキスト形式で返す．
- エラーは`ErrorResponse`でSQLSTATEと位置を返す．
- `ReadyForQuery`でトランザクションの状態(`I`，`T`，`E`)を返す．
- 1つの`Query`に`;`で区切った複数の文があれば，順に実行する．

### 使用例

```console
$ cargo run --release -q -- serve --data-dir ./data --port 5433
ferrodb listening on 127.0.0.1:5433
```

別の端末から`psql`で接続する．

```console
$ psql -h 127.0.0.1 -p 5433 -U alice ferro
psql (17.11 (Debian 17.11-0+deb13u1), server 17.0)
Type "help" for help.

ferro=> SELECT * FROM t;
 A | B
---+---
 1 | x
 2 |
(2 rows)

ferro=> START TRANSACTION;
START TRANSACTION
ferro=*> INSERT INTO t VALUES (3, 'y');
INSERT 0 1
ferro=*> VALUES (1 / 0);
ERROR:  division by zero
ferro=!> ROLLBACK;
ROLLBACK
ferro=> \q
```

エラーの位置は，`psql`が`^`で示す．

```text
ERROR:  syntax error at or near "FROM"
LINE 1: SELECT a + FROM t;
                   ^
```

### 作るもの

| モジュール | 作るもの |
| --- | --- |
| `server::message` | `enum FrontendMessage { EncryptionRequest, Startup { parameters }, Query(String), Terminate }` |
| `server::message` | `enum BackendMessage { AuthenticationOk, ParameterStatus { name, value }, ReadyForQuery { status }, RowDescription(Vec<FieldDescription>), DataRow(Vec<Option<String>>), CommandComplete(String), EmptyQueryResponse, ErrorResponse { code, message, position } }` |
| `server::message` | `struct FieldDescription { name, type_oid, type_size }`，`enum ProtocolError` |
| `server::message` | `read_startup(input: &mut impl Read) -> Result<FrontendMessage, ProtocolError>`，`read_message(input: &mut impl Read) -> Result<Option<FrontendMessage>, ProtocolError>`，`write_message(output: &mut impl Write, message: &BackendMessage) -> io::Result<()>` |
| `server::connection` | `fn handle<S: Read + Write>(stream: S, db: &mut Database) -> Result<(), ProtocolError>` |
| `server` | `connection`と`message`をまとめる公開のモジュール |
| `src/main.rs` | サブコマンド`serve` |

### 書くときに考えること

- メッセージの読み書きは，`Vec<u8>`と`Cursor`で，接続を使わずに単体テストできる．バイト列の期待値は，レイアウトから手で書く．
- `handle`の単体テストには，読む元と書く先を別に持つ型を渡す．起動のあとの返事は，メッセージの種類のバイトを並べて確かめると短く書ける．
- 問い合わせに返すメッセージの並びは，`handle`から問い合わせを受け持つ関数を分ければ，`BackendMessage`の`Vec`で比べられる．
- 結合テストでは，テストの中でサーバーを別のスレッドで起動し，`postgres`クレートのクライアントから接続する．
- 引き継いだテストの期待値は変わるか．

## 20-4 設計ドキュメント

- `c4-context.md`：`psql`などのPostgreSQLのクライアントを加える．
- `c4-container.md`：サーバーのプロセスとTCPの接続を加える．
- `c4-component.md`：`server`を加える．`server::connection`は，文を区切るのと値を文字列にするのに，どのモジュールの関数を使えるか．
- `code-types.md`：メッセージの型を加える．
- `layout.md`：メッセージのバイト配置を加える．起動のメッセージと，そのあとのメッセージの違いに注意する．
- `code-sequence.md`：起動からSimple Queryまでのメッセージのやりとりを加える．

更新したら，リポジトリのルートでMermaidの構文を検査し，照合スクリプトも実行する．

## 20-5 テスト駆動の実装

### 受講者が行うツール操作

- `postgres`クレートを，テストだけで使う依存に加える．
- 最適化したビルドでサーバーを起動するときは，`cargo run --release -q -- serve --data-dir ./data --port 5433`を実行する．

### 実装のヒント

- `server::message`から作る．起動のメッセージは種類のバイトを持たないので，`read_startup`と`read_message`を分ける．
- 長さは自分の4バイトを含む．読むときは長さから4を引いた分を読み，4より小さい長さはエラーにする．
- SSLの要求には，メッセージでなく1バイトの`N`を書く．クライアントは同じ接続で起動のメッセージを送り直す．
- `psql`は`ParameterStatus`の`server_version`などを使う．`server_version`，`server_encoding`，`client_encoding`，`DateStyle`，`standard_conforming_strings`を送る．
- 文の区切りは，REPLの`split_statements`を使える．
- 1つの`Query`の中の2つ目からの文のエラーの位置には，その文より前の文字の数を足す．
- 型OIDは，`INTEGER`が23，`BIGINT`が20，`BOOLEAN`が16，`VARCHAR`が1043，`TEXT`が25である．結果の表は列の型を持たないので，値から決める．
- 値のテキスト形式は，REPLの表のセルと同じでよい．`NULL`は`None`にする．
- 1つの問い合わせに返すメッセージは，`Vec<u8>`に並べてから1回で書く．メッセージごとに書くと，`psql`への応答が大きく遅れる．
- `postgres`クレートの`query`と`execute`は拡張クエリプロトコルを使う．テストでは`simple_query`と`batch_execute`を使う．
- 識別子は大文字に正規化されるので，クライアントが受け取る列の名前も大文字である．

## 20-6 振り返り

1. 自分の`TESTLIST.md`を[模範解答のテストリスト](../../solution/TESTLIST.md)と比べる．
2. `handle`の単体テストと，`postgres`クレートを使う結合テストは，それぞれどんな誤りを見つけられるか．単体テストの期待値は，誰がプロトコルをどう読んだかに左右されないか．
3. `CREATE TABLE t (a INTEGER); INSERT INTO t VALUES (1); VALUES (1 / 0)`を1つの`Query`で送ると，`ferrodb`とPostgreSQLで，表`T`の中身はどう違うか．PostgreSQLと同じにするには，どうすればよいか．
4. 型OIDを値から決める方法では，行のない結果の列の型はどうなるか．正しい型を返すには，どこから型を受け取ればよいか．
5. 1つ目の`psql`が接続している間に，2つ目の`psql`で接続すると，何が起きるか．
6. 設計ドキュメントと実装を見比べ，違うところがあれば設計ドキュメントを直す．

## 20-7 発展課題

`postgres`クレートの`query`は，拡張クエリプロトコルの`Parse`，`Describe`，`Sync`などを送る．今の`ferrodb`は，知らない種類のメッセージで接続を切る．
拡張クエリプロトコルのメッセージを受け取ったら，接続を切らずに`Sync`までのメッセージを読み捨て，`ErrorResponse`(SQLSTATE `0A000`，メッセージ`extended query protocol is not supported`)と`ReadyForQuery`を返す．

```rust
let error = client.query("VALUES (1)", &[]).unwrap_err();
assert_eq!(error.code(), Some(&SqlState::FEATURE_NOT_SUPPORTED));
// 同じ接続で，Simple Queryの問い合わせを続けられる
let messages = client.simple_query("VALUES (1)").unwrap();
```

これまでと同じく，テストリスト，設計ドキュメント，実装の順に進める．
