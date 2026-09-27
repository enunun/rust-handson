# Iteration 22：分離レベルと書き込みの競合

Iteration 21の`ferrodb`では，2つのトランザクションが同じ行を書き換えてコミットすると，1行だった行が2行になる．
このIterationでは，`START TRANSACTION ISOLATION LEVEL`で分離レベルを選べるようにし，ほかのトランザクションが書き換えている行を書き換えるときは，その終わりを待つ．
Rustでは，`Condvar`，タイムアウト付きの待ち合わせ，`Barrier`を使った並行処理のテストを学ぶ．これが最後のIterationである．

## 22-1 準備

`cargo test`を実行し，引き継いだテストがすべて通ることを確かめる．次は結果の要約である．

```text
test result: ok. 340 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.14s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.57s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.71s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.23s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

2つの`psql`でトランザクションを始め，同じ行を`UPDATE`してから両方でコミットし，表がどうなるかを見ておく．

## 22-2 文法と概念

次の2つのノートを読む．

- [Rustのノート](../../../../docs/rust/iteration-22.md)：`Duration`と`thread::sleep`，`Condvar`，知らせを逃さない待ち方，`wait_timeout_while`，`Barrier`，`is_finished`，構造体の更新構文，`get_or_insert_with`
- [データベースのノート](../../../../docs/db/iteration-22.md)：分離レベルと現象，スナップショットと分離レベル，更新の競合，待つ時間の上限とデッドロック，スナップショット分離と`SERIALIZABLE`

読み終えたら，一時的なモジュールにテストを書いて，次の課題を確かめる．

1. `Mutex<bool>`と`Condvar`を`thread::scope`の2つのスレッドで使い，一方が`true`にして`notify_all`するまで，もう一方を`wait_while`で待たせる．
2. `wait_timeout_while`で50ミリ秒待ち，条件が成り立たないまま時間切れになることを，`timed_out()`と`Instant::elapsed`で確かめる．
3. `Barrier::new(3)`を3つのスレッドで使い，どのスレッドの「前」の記録も，どのスレッドの「後」の記録より先になることを確かめる．
4. `Mutex`を持ったまま，同じ`Mutex`を`lock`するスレッドを作る．50ミリ秒後に`is_finished`は何を返すか．

## 22-3 テストリスト

### 要件

- `START TRANSACTION ISOLATION LEVEL READ COMMITTED | REPEATABLE READ`で分離レベルを選ぶ．既定は`READ COMMITTED`とする．
- `READ COMMITTED`は文ごとに，`REPEATABLE READ`はトランザクションの最初の文でスナップショットを取る．
- 別のトランザクションが更新中の行を更新しようとしたら，そのトランザクションの終了を待つ．
  - 相手がコミットしたら，`REPEATABLE READ`では`40001`とする．`READ COMMITTED`では最新の版に対して条件を評価し直して更新する．
  - 相手が中止したら，そのまま更新する．
- 待つ時間には上限(既定は60秒)を設け，超えたら`55P03`とする．
- `SERIALIZABLE`は`0A000`とする．

### 使用例

2つの端末の`psql`で，`REPEATABLE READ`のトランザクションを始める．セッションAが行を書き換える．

```console
ferro=> START TRANSACTION ISOLATION LEVEL REPEATABLE READ;
START TRANSACTION
ferro=*> UPDATE emp SET salary = salary + 10 WHERE id = 1;
UPDATE 1
```

セッションBが同じ行を書き換えると，Aが終わるまで返事が来ない．

```console
ferro=> START TRANSACTION ISOLATION LEVEL REPEATABLE READ;
START TRANSACTION
ferro=*> UPDATE emp SET salary = salary + 20 WHERE id = 1;
```

Aがコミットすると，Bはエラーになる．

```console
ferro=*> COMMIT;
COMMIT
```

```console
ERROR:  could not serialize access due to concurrent update
ferro=!> ROLLBACK;
ROLLBACK
ferro=> SELECT id, salary FROM emp;
 ID | SALARY
----+--------
  2 |    450
  1 |    510
(2 rows)
```

### 作るもの

| モジュール | 作るもの |
| --- | --- |
| `sql::ast` | `enum IsolationLevel { ReadCommitted, RepeatableRead, Serializable }`，`Statement::StartTransaction(IsolationLevel)` |
| `sql::parser` | `START TRANSACTION [ISOLATION LEVEL 分離レベル]`．キーワード`ISOLATION`，`LEVEL`，`READ`，`COMMITTED`，`REPEATABLE`，`SERIALIZABLE` |
| `txn` | `enum Isolation { ReadCommitted, RepeatableRead }`，`TransactionManager::begin_with(isolation)`，`Transaction::snapshot(&mut self, manager: &TransactionManager) -> Snapshot` |
| `txn` | `enum WriteConflict { InProgress(TxnId), Committed }`，`pub fn write_conflict(header: &TupleHeader, snapshot: &Snapshot, manager: &TransactionManager) -> Option<WriteConflict>` |
| `txn` | `pub struct EndSignal`と`wait_until(&self, timeout: Duration, ended: impl FnMut() -> bool) -> bool`，`notify(&self)`．`TransactionError`の`SerializationFailure`，`LockTimeout`，`SerializableNotSupported` |
| `exec::dml` | `enum Outcome { Done(usize), WaitFor(TxnId) }`．`update`と`delete`は`Result<Outcome, Error>`を返す |
| `database` | `Database::with_lock_timeout(self, timeout: Duration) -> Database`．待ってから文をやり直す |
| `error` | SQLSTATE `40001`，`55P03`，`0A000` |

### 書くときに考えること

- 分離レベルによるスナップショットの違いは，`TransactionManager`を直接使う単体テストで確かめられる．
- 衝突の判定は，`xmin`と`xmax`を手で書いたヘッダーを使って確かめられる．
- 待つことは，片方のセッションを`thread::scope`のスレッドで動かして確かめる．スレッドがまだ終わっていないこと，相手がコミットか中止をしたあとに終わることを確かめる．
- `REPEATABLE READ`で`40001`になるには，Bのスナップショットが，Aのコミットより前に取られている必要がある．スレッドの間で順序をそろえるのに`Barrier`を使える．
- 待つ時間の上限を超えることを確かめるテストが，60秒かからないようにするには，どうすればよいか．
- 引き継いだテストのうち，どれが変わるか．

## 22-4 設計ドキュメント

- `code-types.md`：`IsolationLevel`と待ち合わせの構造体を加える．構文木の分離レベルと，トランザクションの分離レベルを同じ型にするか．
- `c4-component.md`：`database`と`exec::dml`から`txn`への依存の説明を直す．
- `code-sequence.md`：更新の競合で待ち，エラーにする流れを加える．待つ間，どのラッチを持っているか．

更新したら，リポジトリのルートでMermaidの構文を検査し，照合スクリプトも実行する．

## 22-5 テスト駆動の実装

### 実装のヒント

- 新しいキーワードを加えるので，`READ`や`LEVEL`という名前の列は，引用符で囲まなければ使えなくなる．引き継いだテストに，そういう名前がないかを確かめる．
- `Transaction`にスナップショットを`Option<Snapshot>`で持たせ，`REPEATABLE READ`では最初の文で入れる．
- `update`と`delete`は，書き換える前に，対象のすべての版を調べる．書き換え始めてから待つと，文の途中までの変更が残る．
- 待つときは，`Storage`と`TransactionManager`のガードをすべて捨ててから待つ．ガードを持ったまま待つと，相手の`COMMIT`が`TransactionManager`の書き込みのガードを取れず，どちらも進めない．
- `READ COMMITTED`は，待ったあとに新しいスナップショットで文をやり直せば，最新の版に条件を評価し直せる．`REPEATABLE READ`は同じスナップショットでやり直すと，相手がコミットしていれば衝突が`Committed`になる．
- `Condvar`の`Mutex`とトランザクションの状態の`RwLock`は別のものである．コミットと中止では，状態を変えたあとに`Mutex`を取ってから`notify_all`を呼ぶ．
- `Database`の`Default`の導出は，待つ時間の上限を0にしてしまう．`Default`を自分で実装する．

## 22-6 振り返り

1. 自分の`TESTLIST.md`を[模範解答のテストリスト](../../solution/TESTLIST.md)と比べる．
2. 待つことを確かめるテストは，`thread::sleep`で時間をおいてから`is_finished`を調べる．このテストが誤って通るのはどんなときか．誤って失敗するのはどんなときか．
3. `READ COMMITTED`で，待ったあとに文全体をやり直すと，待った行のほかの行も新しいスナップショットで読み直す．PostgreSQLのように待った行だけを評価し直す方法と比べて，結果が違うのはどんな場合か．
4. 2人の当番の医師がどちらも当番でなくなる例(書き込みスキュー)を，`REPEATABLE READ`の2つのセッションで試す．なぜ衝突として見つからないか．
5. `SELECT ... FOR UPDATE`を実装するとしたら，どこに何を加えればよいか．
6. 設計ドキュメントと実装を見比べ，違うところがあれば設計ドキュメントを直す．

## 22-7 発展課題

AがBの書き換えた行を待ち，BがAの書き換えた行を待つと，今の`ferrodb`では，待つ時間の上限を過ぎるまでどちらも進まない．
トランザクションが待ち始める前に，誰が誰を待っているかをたどり，自分を待っているトランザクションを待とうとしていれば，待たずに`40P01`(`deadlock detected`)のエラーにする．

```rust
a.execute("UPDATE t SET n = 1 WHERE id = 1")?;
b.execute("UPDATE t SET n = 2 WHERE id = 2")?;
// 別のスレッドで，a が id = 2 の行を書き換えようとして，b を待つ
let error = b.execute("UPDATE t SET n = 2 WHERE id = 1").unwrap_err();
assert_eq!(error.sqlstate(), SqlState::DeadlockDetected);
// b が ROLLBACK すると，a の UPDATE が終わる
```

これまでと同じく，テストリスト，設計ドキュメント，実装の順に進める．
