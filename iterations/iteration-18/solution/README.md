# Iteration 18：トランザクションとMVCC(模範解答)

演習を終えた状態の`ferrodb`と，テストリストと設計ドキュメントの模範解答である．
`START TRANSACTION`，`COMMIT`，`ROLLBACK`で文をまとめ，行の版からトランザクションごとに見える版を選ぶ．

## 実行の仕方

このディレクトリで`cargo test`を実行すると，単体テストと結合テストが実行される．
`cargo run -- repl`でREPLを起動する．`cargo run -- repl --data-dir ./data`なら，表を`./data`に保存する．

## ディレクトリの構成

```text
README.md             このファイル
Cargo.toml            パッケージferrodb-18-solution(ライブラリの名前はferrodb)
TESTLIST.md           テストリストの模範解答
docs/iteration-18.md  演習の各手順の解説
design/               設計ドキュメントの模範解答
src/main.rs           サブコマンドreplのバイナリ(clap)
src/lib.rs            公開する名前をまとめる．indexとstorageを公開する
src/repl.rs           文を区切って実行し，結果を書く
src/format.rs         結果の表とコマンドタグの表示
src/database.rs       Database，StatementResult，QueryResult，データディレクトリ，セッションの状態
src/txn.rs            TxnId，Transaction，TransactionManager，Snapshot，可視性の判定
src/catalog.rs        表とインデックスの定義，定義との照合，カタログのファイル
src/error.rs          Error，SqlState，各段階のエラーからの変換
src/value.rs          値，データ型，行
src/sql.rs            SQLの解析のモジュールをまとめる
src/sql/token.rs      トークンの型
src/sql/lexer.rs      字句解析
src/sql/ast.rs        構文木の型
src/sql/parser.rs     構文解析
src/plan.rs           実行の準備のモジュールをまとめる
src/plan/binder.rs    FROMの表と式の名前解決(Scope)
src/plan/planner.rs   実行計画を作る．インデックスを使える条件を探す
src/plan/explain.rs   実行計画をEXPLAINの行にする
src/exec.rs           トレイトExecutorと，実行のモジュール
src/exec/build.rs     実行計画から演算子の木を作る
src/exec/scan.rs      SeqScan
src/exec/index_scan.rs IndexScan
src/exec/join.rs      NestedLoopJoin
src/exec/aggregate.rs HashAggregateと集約関数のAccumulator
src/exec/filter.rs    Filter
src/exec/project.rs   Project
src/exec/distinct.rs  Distinct
src/exec/limit.rs     Limit
src/exec/sort.rs      Sortと並べ替えの比較
src/exec/eval.rs      式と条件の評価
src/exec/dml.rs       行とインデックスの追加，書き換え，削除と制約の検査
src/storage.rs        ストレージのモジュールをまとめる
src/storage/tuple.rs  タプルのヘッダーと，行とタプルのバイト列の変換
src/storage/page.rs   スロット付きページ
src/storage/heap.rs   HeapFileとRowId
src/storage/buffer.rs BufferPool，PageGuard，ClockReplacer
src/index.rs          AnyIndex，ColumnIndex．列の型ごとのB+木をSQLの値で使う
src/index/key.rs      IndexKeyと，順序を保つキーの符号化
src/index/btree.rs    BTree，RangeIter，ノードの符号化
src/storage/disk.rs   DiskManagerと，ファイル用とメモリー用の実装
tests/                結合テスト(tokenize，values，tables，select，dml，constraints，order_by，explain，join，aggregate，repl，persistence，btree，index，transaction)
```
