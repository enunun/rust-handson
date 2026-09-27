# Iteration 14：ヒープファイルとデータディレクトリ(模範解答)

演習を終えた状態の`ferrodb`と，テストリストと設計ドキュメントの模範解答である．
表ごとのヒープファイルとカタログを，データディレクトリのファイルに保存する．

## 実行の仕方

このディレクトリで`cargo test`を実行すると，単体テストと結合テストが実行される．
`cargo run -- repl`でREPLを起動する．`cargo run -- repl --data-dir ./data`なら，表を`./data`に保存する．

## ディレクトリの構成

```text
README.md             このファイル
Cargo.toml            パッケージferrodb-14-solution(ライブラリの名前はferrodb)
TESTLIST.md           テストリストの模範解答
docs/iteration-14.md  演習の各手順の解説
design/               設計ドキュメントの模範解答
src/main.rs           サブコマンドreplのバイナリ(clap)
src/lib.rs            公開する名前をまとめる
src/repl.rs           文を区切って実行し，結果を書く
src/format.rs         結果の表とコマンドタグの表示
src/database.rs       Database，StatementResult，QueryResult，データディレクトリ
src/catalog.rs        表の定義と，定義との照合，カタログのファイル
src/error.rs          Error，SqlState，各段階のエラーからの変換
src/value.rs          値，データ型，行
src/sql.rs            SQLの解析のモジュールをまとめる
src/sql/token.rs      トークンの型
src/sql/lexer.rs      字句解析
src/sql/ast.rs        構文木の型
src/sql/parser.rs     構文解析
src/plan.rs           実行の準備のモジュールをまとめる
src/plan/binder.rs    FROMの表と式の名前解決(Scope)
src/plan/planner.rs   実行計画を作る
src/plan/explain.rs   実行計画をEXPLAINの行にする
src/exec.rs           トレイトExecutorと，実行のモジュール
src/exec/build.rs     実行計画から演算子の木を作る
src/exec/scan.rs      SeqScan
src/exec/join.rs      NestedLoopJoin
src/exec/aggregate.rs HashAggregateと集約関数のAccumulator
src/exec/filter.rs    Filter
src/exec/project.rs   Project
src/exec/distinct.rs  Distinct
src/exec/limit.rs     Limit
src/exec/sort.rs      Sortと並べ替えの比較
src/exec/eval.rs      式と条件の評価
src/exec/dml.rs       行の追加，書き換え，削除と制約の検査
src/storage.rs        ストレージのモジュールをまとめる
src/storage/tuple.rs  行とタプルのバイト列の変換
src/storage/page.rs   スロット付きページ
src/storage/heap.rs   HeapFileとRowId
src/storage/disk.rs   DiskManagerと，ファイル用とメモリー用の実装
tests/                結合テスト(tokenize，values，tables，select，dml，constraints，order_by，explain，join，aggregate，repl，persistence)
```
