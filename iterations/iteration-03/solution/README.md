# Iteration 3：文字列(模範解答)

演習を終えた状態の`ferrodb`と，テストリストと設計ドキュメントの模範解答である．
`execute`が，文字列リテラル，連結，文字列の比較を含む式を評価する．

## 実行の仕方

このディレクトリで`cargo test`を実行すると，単体テストと結合テストが実行される．

## ディレクトリの構成

```text
README.md             このファイル
Cargo.toml            パッケージferrodb-03-solution(ライブラリの名前はferrodb)
TESTLIST.md           テストリストの模範解答
docs/iteration-03.md  演習の各手順の解説
design/               設計ドキュメントの模範解答
src/lib.rs            execute，Error，QueryResult
src/token.rs          トークンの型
src/lexer.rs          字句解析
src/ast.rs            構文木の型
src/parser.rs         構文解析
src/value.rs          値の型
src/eval.rs           式の評価
tests/tokenize.rs     字句解析の結合テスト
tests/values.rs       VALUESの結合テスト
```
