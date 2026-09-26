# コンポーネント

`ferrodb`のモジュールと，モジュールの間の依存を示す．
`main`がREPLを起動し，`repl`は文を`database`で実行して，結果を`format`の表示で書く．

```mermaid
C4Component
  title ferrodbのコンポーネント
  Component(main, "main", "Rust module", "標準入出力をREPLに渡す")
  Container_Boundary(library, "ferrodb ライブラリ") {
    Component(root, "crate", "Rust module", "公開する名前をまとめる")
    Component(repl, "repl", "Rust module", "文を区切って実行し，結果を書く")
    Component(format, "format", "Rust module", "結果の表とコマンドタグの表示")
    Component(database, "database", "Rust module", "Database，StatementResult，QueryResult")
    Container_Boundary(sql, "sql") {
      Component(lexer, "sql::lexer", "Rust module", "SQLの文字列をトークンの列に分ける")
      Component(token, "sql::token", "Rust module", "トークンの型")
      Component(parser, "sql::parser", "Rust module", "トークンの列を文の構文木にする")
      Component(ast, "sql::ast", "Rust module", "構文木の型")
    }
    Container_Boundary(exec, "exec") {
      Component(eval, "exec::eval", "Rust module", "式を評価する")
    }
    Component(catalog, "catalog", "Rust module", "表の定義と，定義との照合")
    Component(value, "value", "Rust module", "値とデータ型")
    Component(error, "error", "Rust module", "Error，SqlState，各段階のエラーからの変換")
  }
  Rel(main, repl, "run を呼ぶ")
  Rel(root, database, "Database を公開する")
  Rel(root, error, "Error を公開する")
  Rel(root, lexer, "tokenize を公開する")
  Rel(root, token, "Token を公開する")
  Rel(root, value, "Value と DataType を公開する")
  Rel(repl, database, "execute を呼ぶ")
  Rel(format, database, "StatementResult を表示する")
  Rel(format, value, "Value をセルの文字列にする")
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

- `sql`と`exec`は，子モジュールを宣言するだけのモジュールなので，境界として描いた．
- `repl`は`format`の関数を直接呼ばない．`format`が`StatementResult`に`Display`を実装するので，`repl`は`{}`で書くだけである．
- `lexer`と`parser`は，外部のクレート`winnow`のパーサーを組み合わせる．
- 単体テストの依存は図に含めない．
