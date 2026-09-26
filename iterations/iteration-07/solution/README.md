# Iteration 7：WHERE，列の選択，別名(模範解答)

演習を終えた状態の`ferrodb`と，テストリストと設計ドキュメントの模範解答である．
`SELECT`で列と式を選び，別名を付け，`WHERE`で行を絞り込める．

## 実行の仕方

このディレクトリで`cargo test`を実行すると，単体テストと結合テストが実行される．
`cargo run`でREPLを起動する．

## ディレクトリの構成

```text
README.md             このファイル
Cargo.toml            パッケージferrodb-07-solution(ライブラリの名前はferrodb)
TESTLIST.md           テストリストの模範解答
docs/iteration-07.md  演習の各手順の解説
design/               設計ドキュメントの模範解答
src/main.rs           REPLのバイナリ
src/lib.rs            公開する名前をまとめる
src/repl.rs           文を区切って実行し，結果を書く
src/format.rs         結果の表とコマンドタグの表示
src/database.rs       Database，StatementResult，QueryResult
src/catalog.rs        表の定義と，定義との照合
src/error.rs          Error，SqlState，各段階のエラーからの変換
src/value.rs          値とデータ型
src/sql.rs            SQLの解析のモジュールをまとめる
src/sql/token.rs      トークンの型
src/sql/lexer.rs      字句解析
src/sql/ast.rs        構文木の型
src/sql/parser.rs     構文解析
src/plan.rs           実行の準備のモジュールをまとめる
src/plan/binder.rs    式の名前解決
src/exec.rs           実行のモジュールをまとめる
src/exec/eval.rs      式の評価
tests/                結合テスト(tokenize，values，tables，select，repl)
```
