# コンポーネント

ライブラリクレート`ferrodb`のモジュールと，モジュールの間の依存を示す．
`database`の`Database::execute`が入口で，字句解析，構文解析のあと，文の種類ごとに評価，カタログとの照合，行を読み書きする．

```mermaid
C4Component
  title ferrodbライブラリのコンポーネント
  Container_Boundary(library, "ferrodb ライブラリ") {
    Component(root, "crate", "Rust module", "公開する名前をまとめる")
    Component(database, "database", "Rust module", "Database，StatementResult，QueryResult")
    Component(lexer, "lexer", "Rust module", "SQLの文字列をトークンの列に分ける")
    Component(token, "token", "Rust module", "トークンの型")
    Component(parser, "parser", "Rust module", "トークンの列を文の構文木にする")
    Component(ast, "ast", "Rust module", "構文木の型")
    Component(eval, "eval", "Rust module", "式を評価する")
    Component(catalog, "catalog", "Rust module", "表の定義と，定義との照合")
    Component(value, "value", "Rust module", "値とデータ型")
    Component(error, "error", "Rust module", "Error，SqlState，各段階のエラーからの変換")
  }
  Rel(root, database, "Database を公開する")
  Rel(root, error, "Error を公開する")
  Rel(root, lexer, "tokenize を公開する")
  Rel(root, token, "Token を公開する")
  Rel(root, value, "Value と DataType を公開する")
  Rel(database, lexer, "tokenize を呼ぶ")
  Rel(database, parser, "parse を呼ぶ")
  Rel(database, ast, "Statement を読む")
  Rel(database, eval, "eval を呼ぶ")
  Rel(database, catalog, "表を作り，引き，値を列に合わせる")
  Rel(database, value, "行の値を持つ")
  Rel(database, error, "Error を返す")
  Rel(lexer, token, "Token を作る")
  Rel(parser, token, "Token を読む")
  Rel(parser, ast, "Statement を作る")
  Rel(parser, value, "DataType を作る")
  Rel(ast, value, "DataType を持つ")
  Rel(eval, ast, "Expr を読む")
  Rel(eval, value, "Value を作る")
  Rel(catalog, value, "DataType と Value を読む")
  Rel(error, lexer, "LexError を変換する")
  Rel(error, parser, "ParseError を変換する")
  Rel(error, eval, "EvalError を変換する")
  Rel(error, catalog, "SchemaError を変換する")
```

- 表の定義は`catalog`の`Catalog`が，表の行は`Database`が持つ．定義と行を分けておくと，行の置き場所を変えても定義の扱いは変わらない．
- `lexer`と`parser`は，外部のクレート`winnow`のパーサーを組み合わせる．
- 単体テストの依存は図に含めない．
