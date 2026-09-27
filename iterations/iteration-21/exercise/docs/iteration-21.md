# Iteration 21：複数の同時接続

Iteration 20のサーバーは，接続を1つずつ扱う．1つ目の`psql`が接続している間，2つ目の`psql`は返事を待ち続ける．
このIterationでは，接続ごとにスレッドを作り，複数のクライアントが1つのデータベースを同時に使えるようにする．データベースを，すべての接続で共有する部分(`Database`)と，接続ごとのセッション(`Session`)に分ける．
Rustでは，`Arc`，`Mutex`と`RwLock`，`Send`と`Sync`，`thread::scope`によるテストを学ぶ．

## 21-1 準備

`cargo test`を実行し，引き継いだテストがすべて通ることを確かめる．次は結果の要約である．

```text
test result: ok. 338 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.94s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.23s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.99s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.66s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.23s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.37s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

サーバーを起動して2つの端末から`psql`で接続し，2つ目の`psql`が1つ目の接続が終わるまで待つことを確かめておく．

## 21-2 文法と概念

次の2つのノートを読む．

- [Rustのノート](../../../../docs/rust/iteration-21.md)：スレッドと`move`，`Arc`，`Mutex`とガード，毒された`Mutex`，`RwLock`，ガードを持つ期間とラッチの順序，`Send`と`Sync`，スーパートレイトの`Send`，`thread::scope`
- [データベースのノート](../../../../docs/db/iteration-21.md)：接続とセッション，ラッチとロック，`ferrodb`のラッチ，読み取りがブロックされない理由，書く側どうしの衝突

読み終えたら，一時的なモジュールにテストを書いて，次の課題を確かめる．

1. `Arc<Mutex<i32>>`を4つのスレッドに渡し，それぞれ1000回1を足す．合計と，スレッドが終わったあとの`Arc::strong_count`を確かめる．
2. `Rc`を`thread::spawn`のクロージャに渡すと，コンパイラーは何と言うか．
3. `RwLock`の読み取りのガードを持ったまま`try_write`を呼ぶと，何が返るか．
4. `thread::scope`の中で，スコープの外の`Vec<&str>`を借用する3つのスレッドを作り，文字の数の合計を`Mutex<usize>`に足す．
5. `Cell<i32>`のフィールドを持つ構造体を，`thread::scope`の2つのスレッドから`&`で使うと，コンパイラーは何と言うか．フィールドを`Mutex<i32>`にするとどうなるか．

## 21-3 テストリスト

### 要件

- 接続ごとにスレッドを作り，複数のクライアントが同時に問い合わせられる．
- 各接続はそれぞれのセッション(トランザクションの状態)を持つ．
- 他のセッションの，コミットしていない変更は見えない．
- カタログ，バッファプール，トランザクション管理，WALを，複数のスレッドから安全に使えるようにする．
- 接続が切れるなどして，トランザクションの中でセッションが終わったら，そのトランザクションを中止する．

### 使用例

サーバーを起動し，2つの端末から`psql`で接続する．表`T`には，1と2の2行がある．

```console
$ psql -h 127.0.0.1 -p 5433 -U alice ferro
ferro=> START TRANSACTION;
START TRANSACTION
ferro=*> INSERT INTO t VALUES (3);
INSERT 0 1
```

セッションAがコミットする前に，別の端末のセッションBで数える．Aの行は見えない．

```console
$ psql -h 127.0.0.1 -p 5433 -U alice ferro
ferro=> SELECT COUNT(*) FROM t;
 COUNT
-------
     2
(1 row)
```

Aでコミットすると，Bから見える．

```console
ferro=*> COMMIT;
COMMIT
```

```console
ferro=> SELECT COUNT(*) FROM t;
 COUNT
-------
     3
(1 row)
```

### 作るもの

| モジュール | 作るもの |
| --- | --- |
| `database` | `pub struct Session`と`new(database: Arc<Database>) -> Session`，`execute(&mut self, sql: &str) -> Result<StatementResult, Error>`，`transaction_status(&self) -> TransactionStatus` |
| `database` | `Database`は`execute`と`transaction_status`を持たず，`Send`と`Sync`を実装する．表とインデックスとカタログ，トランザクションの状態，ログを，それぞれ`RwLock`か`Mutex`で守る |
| `storage::buffer` | `BufferPool`を複数のスレッドから使えるようにする．`PageGuard::read`は`RwLockReadGuard<'_, Page>`を，`write`は`RwLockWriteGuard<'_, Page>`を返す．`PageLog::wal`は`Arc<Mutex<WalWriter<File>>>` |
| `storage::disk` | `trait DiskManager: Send` |
| `server::connection` | `pub fn serve(listener: TcpListener, database: Arc<Database>)`，`handle<S: Read + Write>(stream: S, database: Arc<Database>) -> Result<(), ProtocolError>` |
| `repl` | `run_with(database: Database, ...)`は，受け取った`Database`でセッションを作る |
| `src/main.rs` | `serve`は`server::connection::serve`を呼ぶ |

### 書くときに考えること

- `BufferPool`を複数のスレッドから使うテストは，今のコードではコンパイルできない．コンパイルエラーがRedである．
- 2つのセッションで1つのデータベースを使うテストは，`Arc<Database>`を`Arc::clone`して2つの`Session`に渡せば，スレッドなしで書ける．
- トランザクションの中でセッションを捨てたとき，ほかのセッションからSQLで違いが見えるか．見えなければ，どこで何を確かめればよいか．
- 2つのクライアントが同時に接続できることは，`postgres`クレートの2つの`Client`で確かめる．Iteration 20のサーバーでは，このテストはどうなるか．
- 引き継いだテストのうち，どれが変わるか．

## 21-4 設計ドキュメント

- `c4-container.md`：接続ごとのスレッドを加える．
- `code-types.md`：`Session`と，共有する構造体のロックを加える．バッファプールの枠のどの状態を，どのロックで守るか．
- `c4-component.md`：`database`，`server::connection`，`repl`の依存の説明を直す．
- `code-sequence.md`：2つのセッションが並行に動く流れを加える．どこでラッチを取り，どこで外すか．

更新したら，リポジトリのルートでMermaidの構文を検査し，照合スクリプトも実行する．

## 21-5 テスト駆動の実装

### 実装のヒント

- `storage::buffer`から始める．`thread::scope`で複数のスレッドから`fetch_page`を呼ぶテストを書くと，コンパイラーが`Sync`でないフィールドを1つずつ教えてくれる．
- ページの中身は枠ごとの`RwLock<Page>`にする．どの枠にどのページがあるかと，ディスクと置換方式は，プールの1つの`Mutex`にまとめる．ピン留めと追い出しを同じ`Mutex`の中で行えば，ピン留めした枠を追い出すことはない．
- `BufferPool<Box<dyn DiskManager>>`を共有するには，`Box<dyn DiskManager>`は`Send`でなければならない．
- `Rc<RefCell<WalWriter<File>>>`は`Arc<Mutex<...>>`にする．
- `Database`は，表とインデックスとカタログを1つの構造体にまとめて`RwLock`に入れる．問い合わせは読み取りのガードを，ほかの文は書き込みのガードを，文を実行する間だけ持つ．
- 文を実行する間，トランザクションの状態の読み取りのガードは1回だけ取り，`&TransactionManager`を関数に渡す．同じスレッドで2回取ると，間にほかのスレッドのコミットが入ったときに進めなくなる．チェックポイントも，渡された`&TransactionManager`を使う．
- ガードは，表とインデックス，トランザクションの状態，ログの順に取る．コミットは，ログのガードを捨ててから，トランザクションの状態のガードを取る．
- `let _ = pool.fetch_page(0).unwrap().read();`はコンパイルエラーになる．ガードをすぐに捨てるなら`drop`を使う．
- `Session`に`Drop`を実装し，トランザクションの中なら中止する．
- `serve`は，接続ごとに`Arc::clone`した`Database`を`move`クロージャで新しいスレッドに渡す．

## 21-6 振り返り

1. 自分の`TESTLIST.md`を[模範解答のテストリスト](../../solution/TESTLIST.md)と比べる．
2. セッションAのトランザクションが終わる前に，セッションBの`SELECT`は結果を返す．Bは何を待ち，何を待たないか．
3. 2つのセッションがどちらもトランザクションの中で，同じ行を`UPDATE`してからコミットすると，表はどうなるか．
4. 表を変える文は，表とインデックス全体の書き込みのガードを取るので，1つずつ動く．別の表を変える2つの文も同時に動けるようにするには，どうすればよいか．何が難しくなるか．
5. 表とインデックスを`Mutex`でなく`RwLock`で守ると，何がよいか．
6. 設計ドキュメントと実装を見比べ，違うところがあれば設計ドキュメントを直す．

## 21-7 発展課題

同時に受け付ける接続の数に上限を設ける．サブコマンド`serve`に`--max-connections N`(既定は100)を加える．
上限に達しているときの接続には，起動のメッセージを読んでから，`ErrorResponse`(SQLSTATE `53300`，メッセージ`sorry, too many clients already`)を返して閉じる．接続が終わったら，数を1つ減らす．

上限を1にしたサーバーへ，2つ目の`psql`で接続すると，次のようになる．

```console
$ psql -h 127.0.0.1 -p 5441 -U alice ferro -c "VALUES (1)"
psql: error: connection to server at "127.0.0.1", port 5441 failed: ERROR:  sorry, too many clients already
```

これまでと同じく，テストリスト，設計ドキュメント，実装の順に進める．
