# Iteration 18：トランザクションとMVCC

これまでの`ferrodb`は，文を1つずつ確定させてきた．
このIterationでは，`START TRANSACTION`，`COMMIT`，`ROLLBACK`で文をまとめ，すべてを反映するか何も反映しないかのどちらかにする．
行を書き換えても古い版を残し，版に作ったトランザクションと削除したトランザクションの番号を書く．どの版が見えるかは，スナップショットとトランザクションの状態で決める(MVCC)．
Rustでは，ニュータイプ，`Copy`と`Clone`，値を消費するメソッドによるAPIの設計，`std::mem::take`を学ぶ．

## 18-1 準備

`cargo test`を実行し，引き継いだテストがすべて通ることを確かめる．次は結果の要約である．

```text
test result: ok. 299 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.81s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.19s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.54s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

REPLで表を作り，`UPDATE`のあとに`SELECT`した行の順序を見ておく．

## 18-2 文法と概念

次の2つのノートを読む．

- [Rustのノート](../../../../docs/rust/iteration-18.md)：ニュータイプと関連定数，`Copy`と`Clone`，値を消費するメソッド，`std::mem::take`，`map_err`と`and_then`
- [データベースのノート](../../../../docs/db/iteration-18.md)：トランザクションとACID，失敗したトランザクション，MVCC，スナップショットと可視性の規則，トランザクションの状態の記録

読み終えたら，一時的なモジュールにテストを書いて，次の課題を確かめる．

1. `u16`を包むニュータイプ`Port`と，関連定数`Port::HTTP`(80)を作る．`fn connect(port: Port) -> String`に`80u16`をそのまま渡すと，どうなるか．
2. 番号を持つ`Ticket`に，`fn redeem(self, used: &mut Vec<u32>)`を作る．同じ`Ticket`で2回呼ぶと，どのエラーになるか．
3. `enum Light { Off, On(u8) }`に`Default`を導出し(`Off`を既定にする)，`fn toggle(light: &mut Light) -> u8`を`std::mem::take`で書く．`Off`なら`On(100)`にして0を返し，`On(n)`なら`Off`にして`n`を返す．

## 18-3 テストリスト

### 要件

- `START TRANSACTION`，`COMMIT`，`ROLLBACK`を扱う．トランザクションの外の文は，それぞれを1つのトランザクションとして自動でコミットする．
- タプルに作成したトランザクション(`xmin`)と削除したトランザクション(`xmax`)を記録する．`UPDATE`は古い版を削除済みにして新しい版を挿入する．
- 行の可視性は，スナップショットとトランザクションの状態(進行中，コミット済み，中止)で決める．スナップショットは文ごとに取る．
- `ROLLBACK`したトランザクションの変更は見えなくなる．
- トランザクションの中でエラーが起きたら，`ROLLBACK`までの文を`25P02`(`current transaction is aborted, commands ignored until end of transaction block`)で拒否する．失敗したトランザクションの中の`COMMIT`は中止し，`ROLLBACK`を返す．
- トランザクションの中での`START TRANSACTION`は`25001`(`there is already a transaction in progress`)とし，トランザクションは続ける．
- トランザクションの外の`COMMIT`と`ROLLBACK`は何もしない．
- 表とインデックスを作る文と消す文は，トランザクションの外でだけ実行できる．中で実行すると`25001`(`CREATE TABLE cannot run inside a transaction block`など)とする．
- REPLのプロンプトは，トランザクションの中では`ferrodb*>`，失敗したトランザクションの中では`ferrodb!>`とする．
- トランザクションの状態は，データディレクトリのファイル`xact`に置く．開き直したとき，進行中のまま残ったトランザクションは中止したものとみなす．

### 使用例

```console
ferrodb> CREATE TABLE emp (id INTEGER PRIMARY KEY, name VARCHAR(20) NOT NULL);
CREATE TABLE
ferrodb> INSERT INTO emp VALUES (1, 'alice'), (2, 'bob'), (3, 'carol'), (4, 'dave');
INSERT 0 4
ferrodb> START TRANSACTION;
START TRANSACTION
ferrodb*> DELETE FROM emp;
DELETE 4
ferrodb*> SELECT COUNT(*) FROM emp;
 COUNT
-------
     0
(1 row)

ferrodb*> ROLLBACK;
ROLLBACK
ferrodb> SELECT COUNT(*) FROM emp;
 COUNT
-------
     4
(1 row)

ferrodb> START TRANSACTION;
START TRANSACTION
ferrodb*> INSERT INTO emp VALUES (1, 'eve');
ERROR:  duplicate key value violates unique constraint "EMP_PKEY"
ferrodb!> SELECT * FROM emp;
ERROR:  current transaction is aborted, commands ignored until end of transaction block
ferrodb!> COMMIT;
ROLLBACK
ferrodb> START TRANSACTION;
START TRANSACTION
ferrodb*> CREATE TABLE t (a INTEGER);
ERROR:  CREATE TABLE cannot run inside a transaction block
ferrodb!> ROLLBACK;
ROLLBACK
```

### 作るもの

| モジュール | 作るもの |
| --- | --- |
| `txn` | `pub struct TxnId(pub u32)`と`TxnId::INVALID`，`enum TxnStatus`，`struct Transaction`と`xid`，`commit(self, &mut TransactionManager)`，`rollback(self, &mut TransactionManager)` |
| `txn` | `struct TransactionManager`と`begin`，`status`，`snapshot(xid)`，`save`，`load`．`struct Snapshot { xid, xmax, active }`．`pub fn is_visible(header: &TupleHeader, snapshot: &Snapshot, manager: &TransactionManager) -> bool` |
| `txn` | `enum TransactionError { InFailedTransaction, AlreadyInProgress, NotInTransactionBlock { statement } }` |
| `storage::tuple` | `pub struct TupleHeader { pub xmin: TxnId, pub xmax: TxnId }`と`new`，`encode`，`split`．`encode_version(header, row, schema)` |
| `storage::heap` | `rows(&self, schema, visible: impl Fn(&TupleHeader) -> bool)`，`set_xmax(&mut self, id, xmax)` |
| `sql::parser` | `Statement::StartTransaction`，`Statement::Commit`，`Statement::Rollback` |
| `exec::dml` | `insert`，`update`，`delete`がスナップショットとトランザクションの状態を受け取る |
| `exec::build` | 表，インデックス，カタログ，スナップショット，トランザクションの状態をまとめた`BuildContext` |
| `database` | セッションの状態，`transaction_status() -> TransactionStatus`，`StatementResult::StartTransaction`，`Commit`，`Rollback` |
| `repl` | トランザクションの状態に合わせたプロンプト |
| `error` | `SqlState::InFailedSqlTransaction`(`25P02`)，`SqlState::ActiveSqlTransaction`(`25001`)と，`TransactionError`からの変換 |

### 書くときに考えること

- 可視性の規則は，`txn`の単体テストで，`xmin`と`xmax`の組み合わせごとに確かめる．自分，コミット済み，中止，進行中の4つの状態がある．
- 表の版を直接確かめたいときは，`HeapFile::tuples`で版のヘッダーを読む．
- `exec::dml`の単体テストでは，表を作る処理と，操作をトランザクションの中で行う処理を，テストの中の補助関数にまとめる．
- 引き継いだテストのうち，タプルの大きさ，`UPDATE`のあとの行の順序，データディレクトリのファイルの一覧に関わるものは，どう変わるか．
- 開き直したときに，進行中だったトランザクションの変更が見えないことを確かめる．そのあとに始めたトランザクションの変更は見えるか．

## 18-4 設計ドキュメント

- `c4-container.md`：データディレクトリにトランザクションの状態のファイルを加える．
- `c4-component.md`：`txn`を加える．どのモジュールが版の可視性を判定し，どのモジュールが`TxnId`を使うか．
- `code-types.md`：トランザクションの型の図を加える．`TupleHeader`，`BuildContext`，セッションの状態を加え，`Database`と`HeapFile`を更新する．
- `layout.md`：タプルのヘッダーと，トランザクションの状態のファイルを加える．
- `code-sequence.md`：`UPDATE`が新しい版を作る流れと，`START TRANSACTION`から`ROLLBACK`までの状態の変化を加える．

更新したら，リポジトリのルートでMermaidの構文を検査し，照合スクリプトも実行する．

## 18-5 テスト駆動の実装

### 実装のヒント

- 下の層から作る．`txn`の可視性，タプルのヘッダー，`HeapFile`，`exec::dml`，`exec::build`，`database`，`repl`の順にする．
- `TxnId`と`TxnStatus`は小さな値なので`Copy`にする．`Transaction`は複製できないようにし，`commit`と`rollback`で消費する．
- 状態は，番号を添字にした`Vec<TxnStatus>`で持てる．番号0は使わない．
- タプルのヘッダーは，これまでの行の値のバイト列の前に置く．`encode_tuple`と`decode_tuple`は行の値だけを扱うまま残せる．
- `set_xmax`は，ヘッダーを書き換えたタプルを`HeapFile::update`で同じ長さのまま書き戻す．
- `exec::dml`の`update`は，見える版を読み，古い版の`xmax`を書いてから新しい版を加える．インデックスには新しい版の項目だけを加える．
- 一意性の検査では，インデックスで見つかった版が見えるかを，表の版のヘッダーで確かめる．
- `build`の引数が増えるので，引数をまとめる構造体を作ると，子の演算子を作る再帰の呼び出しが短くなる．
- `Database`は`Session`を持ち，`execute`で`std::mem::take`を使って状態を取り出す．取り出した`Transaction`は，文の結果に合わせて`commit`，`rollback`するか，次の状態に入れ直す．
- `winnow`の`value`は`Clone`を求める．`Statement`は`Clone`を導出していないので，`map(|_| Statement::Commit)`と書く．
- 文の`alt`は，トランザクションの3つの文を1つの`alt`にまとめると，9個に収まる．
- データディレクトリでは，トランザクションを終えるときだけでなく，始めるときにも状態をファイルに書く．

## 18-6 振り返り

1. 自分の`TESTLIST.md`を[模範解答のテストリスト](../../solution/TESTLIST.md)と比べる．
2. `Transaction::commit`を`&self`で受け取る設計と比べる．コミットしたあとの`Transaction`を使う誤りは，いつ見つかるか．
3. `ROLLBACK`は状態を書き換えるだけで，版を書き戻さない．この設計で，表とインデックスの大きさはどう変わっていくか．PostgreSQLは，それをどう片付けるか．
4. トランザクションを始めるときに状態をファイルに書かないと，開き直したときに何が起きるか．模範解答の結合テストで確かめる．
5. 表を作る文をトランザクションの中で実行できないようにした．実行できるようにするには，カタログをどう持てばよいか．
6. 設計ドキュメントと実装を見比べ，違うところがあれば設計ドキュメントを直す．

## 18-7 発展課題

`START TRANSACTION READ ONLY`で，読み取り専用のトランザクションを始められるようにする．
読み取り専用のトランザクションの中の`INSERT`，`UPDATE`，`DELETE`は，`25006`とし，メッセージを`cannot execute INSERT in a read-only transaction`の形にする．エラーのあとは，ほかのエラーと同じくトランザクションが失敗する．

```console
ferrodb> CREATE TABLE t (a INTEGER);
CREATE TABLE
ferrodb> START TRANSACTION READ ONLY;
START TRANSACTION
ferrodb*> INSERT INTO t VALUES (1);
ERROR:  cannot execute INSERT in a read-only transaction
ferrodb!> ROLLBACK;
ROLLBACK
```

これまでと同じく，テストリスト，設計ドキュメント，実装の順に進める．
