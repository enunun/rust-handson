# Iteration 5：表の作成と挿入(模範解答)

演習を終えた状態の`ferrodb`と，テストリストと設計ドキュメントの模範解答である．
`Database::execute`が，`CREATE TABLE`，`INSERT`，`SELECT * FROM`，`VALUES`を実行する．

## 実行の仕方

このディレクトリで`cargo test`を実行すると，単体テストと結合テストが実行される．

## ディレクトリの構成

```text
README.md             このファイル
Cargo.toml            パッケージferrodb-05-solution(ライブラリの名前はferrodb)
TESTLIST.md           テストリストの模範解答
docs/iteration-05.md  演習の各手順の解説
design/               設計ドキュメントの模範解答
src/lib.rs            公開する名前をまとめる
src/database.rs       Database，StatementResult，QueryResult
src/catalog.rs        表の定義と，定義との照合
src/error.rs          Error，SqlState，各段階のエラーからの変換
src/token.rs          トークンの型
src/lexer.rs          字句解析
src/ast.rs            構文木の型
src/parser.rs         構文解析
src/value.rs          値とデータ型
src/eval.rs           式の評価
tests/tokenize.rs     字句解析の結合テスト
tests/values.rs       VALUESの結合テスト
tests/tables.rs       表の作成，挿入，問い合わせの結合テスト
```
