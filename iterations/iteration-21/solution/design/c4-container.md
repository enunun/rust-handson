# コンテナ

`ferrodb`を構成する，実行される単位と保存される単位を示す．
バイナリのサブコマンド`repl`は，ライブラリの`repl::run_with`に標準入出力を渡す．サブコマンド`serve`は，TCPの接続を受け付けるたびにスレッドを作り，そのスレッドで`server::connection::handle`を呼ぶ．
どちらのサブコマンドも，引数`--data-dir`でデータディレクトリを開く．

```mermaid
C4Container
  title ferrodbのコンテナ
  Person(user, "利用者")
  Person(developer, "開発者")
  System_Ext(client, "PostgreSQLのクライアント", "psql や postgres クレートなど")
  System_Boundary(system, "ferrodb") {
    Container(repl, "ferrodb", "Rust binary", "サブコマンド repl．標準入力からSQLを読み，結果を標準出力に書く")
    Container(server, "ferrodb serve", "Rust binary", "サブコマンド serve．127.0.0.1 のポートで接続を待つ")
    Container(connection, "接続のスレッド", "Rust thread", "接続ごとに1つ．自分のセッションで文を実行する")
    Container(library, "ferrodb", "Rust library", "SQLの文を実行し，表の行をページに置く")
    ContainerDb(catalog, "catalog", "File", "表とインデックスの定義")
    ContainerDb(heap, "*.heap", "File", "表ごとのページの列")
    ContainerDb(index, "*.index", "File", "インデックスごとの B+木のページ")
    ContainerDb(xact, "xact", "File", "チェックポイントの時点のトランザクションの状態")
    ContainerDb(wal, "wal", "File", "変更を先に書くログ")
  }
  Rel(user, repl, "SQLを入力する")
  Rel(repl, library, "Database::open と repl::run_with を呼ぶ")
  Rel(client, server, "接続する", "TCP")
  Rel(client, connection, "問い合わせを送る", "TCP，プロトコル3.0")
  Rel(server, library, "Database::open と server::connection::serve を呼ぶ")
  Rel(server, connection, "接続ごとに作る")
  Rel(connection, library, "Session::new と Session::execute を呼ぶ")
  Rel(developer, library, "Database::execute を呼ぶ")
  Rel(library, catalog, "起動時に読み，表とインデックスを作るか消すときに書く")
  Rel(library, heap, "ページを読み書きする")
  Rel(library, index, "ノードのページを読み書きする")
  Rel(library, xact, "チェックポイントで書き，開くときに読む")
  Rel(library, wal, "変更を先に書き，開くときに読んでやり直す")
```

- データディレクトリには，カタログのファイル`catalog`と，表ごとのヒープファイルと，インデックスごとのファイルを置く．ファイルの名前は，表やインデックスの名前のUTF-8のバイトを16進数で書いたものに，`.heap`か`.index`を付けたもの(表`T`なら`54.heap`)である．
- 変更は，まずログのファイル`wal`に書く．開くときは，`xact`のトランザクションの状態から始めて，`wal`の最後のチェックポイントのあとをやり直す．
- `--data-dir`を省略すると，ページをメモリーに置く．プロセスを終了すると消える．
- `serve`のスレッドは，どれも同じ`Database`を`Arc`で共有し，接続ごとの`Session`を持つ．複数のクライアントが同時に問い合わせられる．待ち受けるポートは`--port`で決め，省略すると5433である．
- ファイルの中のバイト配置は[layout.md](layout.md)にある．
