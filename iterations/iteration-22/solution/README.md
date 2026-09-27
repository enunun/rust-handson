# Iteration 22：分離レベルと書き込みの競合(模範解答)

演習を終えた状態の`ferrodb`と，テストリストと設計ドキュメントの模範解答である．
分離レベルを選べるようにし，2つのトランザクションが同じ行を書き換えるときは，先に書き換えたほうの終わりを待つ．

## 実行の仕方

このディレクトリで`cargo test`を実行すると，単体テストと結合テストが実行される．
`cargo run -- repl`でREPLを起動する．`cargo run -- repl --data-dir ./data`なら，表を`./data`に保存する．
`cargo run --release -- serve --data-dir ./data`でサーバーを起動し，`psql -h 127.0.0.1 -p 5433 -U alice ferro`で接続する．複数の`psql`から同時に接続できる．

## ディレクトリの構成

```text
README.md             このファイル
Cargo.toml            パッケージferrodb-22-solution(ライブラリの名前はferrodb)
TESTLIST.md           テストリストの模範解答
docs/iteration-22.md  演習の各手順の解説
design/               設計ドキュメントの模範解答
src/main.rs           サブコマンドreplとserveのバイナリ(clap)
src/lib.rs            公開する名前をまとめる．index，server，storageを公開する
src/repl.rs           文を区切って実行し，結果を書く
src/format.rs         結果の表とコマンドタグの表示
src/database.rs       共有するDatabaseと接続ごとのSession，StatementResult，QueryResult，データディレクトリ
src/txn.rs            TxnId，Isolation，Transaction，TransactionManager，Snapshot，可視性と書き換えの衝突の判定，EndSignal
src/wal.rs            WalWriter，WalRecord，Lsn，ログの読み戻しとリカバリ
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
src/exec/dml.rs       行とインデックスの追加，書き換え，削除と制約の検査．書き換えの衝突ではOutcome::WaitForを返す
src/storage.rs        ストレージのモジュールをまとめる
src/storage/tuple.rs  タプルのヘッダーと，行とタプルのバイト列の変換
src/storage/page.rs   スロット付きページ
src/storage/heap.rs   HeapFileとRowId
src/storage/buffer.rs BufferPool，PageGuard，ClockReplacer．枠ごとのページのラッチ
src/index.rs          AnyIndex，ColumnIndex．列の型ごとのB+木をSQLの値で使う
src/index/key.rs      IndexKeyと，順序を保つキーの符号化
src/index/btree.rs    BTree，RangeIter，ノードの符号化
src/storage/disk.rs   DiskManagerと，ファイル用とメモリー用の実装
src/server.rs         サーバーのモジュールをまとめる
src/server/message.rs FrontendMessage，BackendMessage，メッセージの読み書き
src/server/connection.rs 接続ごとのスレッド(serve)と，1つの接続の起動，Simple Query，終了
tests/                結合テスト(tokenize，values，tables，select，dml，constraints，order_by，explain，join，aggregate，repl，persistence，btree，index，transaction，recovery，server，session，isolation)
```
