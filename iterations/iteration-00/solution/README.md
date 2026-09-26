# Iteration 0：プロジェクトの作成と字句解析(模範解答)

演習を終えた状態の`ferrodb`と，テストリストと設計ドキュメントの模範解答である．
SQLの文字列`VALUES (1, 2 + 3)`をトークンの列に分ける字句解析器`tokenize`を持つ．

## 実行の仕方

このディレクトリで`cargo test`を実行すると，単体テストと結合テストが実行される．

```console
cargo test
```

## ディレクトリの構成

```text
README.md             このファイル
Cargo.toml            パッケージferrodb-00-solution(ライブラリの名前はferrodb)
TESTLIST.md           テストリストの模範解答
docs/iteration-00.md  演習の各手順の解説
design/               設計ドキュメントの模範解答
src/lib.rs            公開する名前をまとめる
src/token.rs          トークンの型
src/lexer.rs          字句解析と，その単体テスト
tests/tokenize.rs     結合テスト
```
