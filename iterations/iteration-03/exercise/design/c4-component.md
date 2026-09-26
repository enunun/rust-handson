# コンポーネント

ライブラリクレート`ferrodb`のモジュールと，モジュールの間の依存を示す．
`crate`の`execute`が，字句解析，構文解析，評価を順に呼ぶ．

```mermaid
C4Component
  title ferrodbライブラリのコンポーネント
  Container_Boundary(library, "ferrodb ライブラリ") {
    Component(root, "crate", "Rust module", "execute，Error，QueryResult")
    Component(lexer, "lexer", "Rust module", "SQLの文字列をトークンの列に分ける")
    Component(token, "token", "Rust module", "トークンの型")
    Component(parser, "parser", "Rust module", "トークンの列を構文木にする")
    Component(ast, "ast", "Rust module", "構文木の型")
    Component(eval, "eval", "Rust module", "式を評価する")
    Component(value, "value", "Rust module", "SQLの値の型")
  }
  Rel(root, lexer, "tokenize を呼ぶ")
  Rel(root, parser, "parse を呼ぶ")
  Rel(root, eval, "eval を呼ぶ")
  Rel(root, token, "Token と Keyword を公開する")
  Rel(root, value, "Value を公開する")
  Rel(lexer, token, "Token を作る")
  Rel(parser, token, "Token を読む")
  Rel(parser, ast, "Values と Expr を作る")
  Rel(eval, ast, "Expr を読む")
  Rel(eval, value, "Value を作る")
```

- `lexer`と`parser`は，外部のクレート`winnow`のパーサーを組み合わせる．
- `parser`と`eval`の単体テストは，入力を用意するために`lexer`と`parser`を使う．単体テストの依存は図に含めない．
