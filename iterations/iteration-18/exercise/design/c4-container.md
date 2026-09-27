# コンテナ

`ferrodb`を構成する，実行される単位と保存される単位を示す．
REPLのバイナリは，引数`--data-dir`でデータディレクトリを開き，ライブラリの`repl::run_with`に標準入出力を渡す．

```mermaid
C4Container
  title ferrodbのコンテナ
  Person(user, "利用者")
  Person(developer, "開発者")
  System_Boundary(system, "ferrodb") {
    Container(repl, "ferrodb", "Rust binary", "サブコマンド repl．標準入力からSQLを読み，結果を標準出力に書く")
    Container(library, "ferrodb", "Rust library", "SQLの文を実行し，表の行をページに置く")
    ContainerDb(catalog, "catalog", "File", "表とインデックスの定義")
    ContainerDb(heap, "*.heap", "File", "表ごとのページの列")
    ContainerDb(index, "*.index", "File", "インデックスごとの B+木のページ")
  }
  Rel(user, repl, "SQLを入力する")
  Rel(repl, library, "Database::open と repl::run_with を呼ぶ")
  Rel(developer, library, "Database::execute を呼ぶ")
  Rel(library, catalog, "起動時に読み，表とインデックスを作るか消すときに書く")
  Rel(library, heap, "ページを読み書きする")
  Rel(library, index, "ノードのページを読み書きする")
```

- データディレクトリには，カタログのファイル`catalog`と，表ごとのヒープファイルと，インデックスごとのファイルを置く．ファイルの名前は，表やインデックスの名前のUTF-8のバイトを16進数で書いたものに，`.heap`か`.index`を付けたもの(表`T`なら`54.heap`)である．
- `--data-dir`を省略すると，ページをメモリーに置く．REPLを終了すると消える．
- ファイルの中のバイト配置は[layout.md](layout.md)にある．
