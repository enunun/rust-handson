# Iteration 22：分離レベルと書き込みの競合(解説)

演習の各手順の模範解答と，その考え方を説明する．

## 22-1 準備

引き継いだ479個のテストがすべて通れば準備は終わりである．
Iteration 21の`ferrodb`で2つのトランザクションが同じ行を書き換えてコミットすると，どちらの`UPDATE`も成功し，1行だった行が2行になる．

## 22-2 文法と概念

課題1〜4の解答例は，[Rustのノート](../../../../docs/rust/iteration-22.md)の`Condvar`，`wait_timeout_while`，`Barrier`，`is_finished`の例である．`tests/`に置いた結合テストで確かめた．

- 課題1：`wait_while`は，条件`!*ready`が偽になるまで眠る．もう一方のスレッドが`true`にして`notify_all`すると起きる．
- 課題2：`timed_out()`は真で，`elapsed()`は50ミリ秒以上である．
- 課題3：3つ目のスレッドが`wait`を呼ぶまで，どのスレッドも「後」を記録しない．
- 課題4：`is_finished`は`false`を返す．`Mutex`を外すと，スレッドは`lock`を終えて終わる．

## 22-3 テストリスト

模範解答は[TESTLIST.md](../TESTLIST.md)である．

- スナップショットの取り方と衝突の判定は`txn`の単体テストで，衝突のときに何も変えないことは`exec::dml`の単体テストで，待ち合わせは結合テスト`tests/isolation.rs`で確かめた．
- 結合テストでは，待つ側のセッションを`thread::scope`のスレッドで動かし，100ミリ秒待っても終わっていないことを`is_finished`で確かめてから，相手をコミットか中止させた．
- `REPEATABLE READ`で`40001`になるテストは，Bのスナップショットを取ってから`Barrier`で待ち合わせ，Aのコミットより前にBのスナップショットがあるようにした．
- 待つ時間の上限は`Database::with_lock_timeout`で50ミリ秒にして確かめた．
- 引き継いだテストのうち，`START TRANSACTION`を構文解析するテストの期待値と，`exec::dml`の`update`と`delete`を呼ぶ単体テストが変わる．

## 22-4 設計ドキュメント

| ファイル | 変更 | 理由 |
| --- | --- | --- |
| `code-types.md` | `IsolationLevel`，`Isolation`，`WriteConflict`，`EndSignal`，`Outcome`を加え，`Statement`，`Transaction`，`TransactionManager`，`TransactionError`，`Database`を更新した | 分離レベルと待ち合わせの型ができた |
| `c4-component.md` | `database`と`exec::dml`から`txn`への依存の説明を直した | 衝突を調べ，終わりを待つようになった |
| `code-sequence.md` | 書き込みの競合の流れを加えた | 待つ間にラッチを持たないことを示す |

- 分離レベルの型を2つに分けた．構文木の`IsolationLevel`は，書かれた3つの分離レベルを表す．`txn`の`Isolation`は，`ferrodb`が実装する2つだけを持つ．`Session`が`Serializable`を`0A000`にするので，`SERIALIZABLE`で動くトランザクションは作れない．
- `exec::dml`の`update`と`delete`は，待つかどうかを`Outcome`で返す．待つことはSQLのエラーでないので，`Error`に入れなかった．

## 22-5 テスト駆動の実装

### 分離レベルの構文

```rust
fn isolation_level(input: &mut Tokens<'_>) -> ModalResult<IsolationLevel> {
    preceded(
        (
            literal(keyword(Keyword::Isolation)),
            cut_err(literal(keyword(Keyword::Level))),
        ),
        cut_err(alt((
            (
                literal(keyword(Keyword::Read)),
                literal(keyword(Keyword::Committed)),
            )
                .value(IsolationLevel::ReadCommitted),
            (
                literal(keyword(Keyword::Repeatable)),
                literal(keyword(Keyword::Read)),
            )
                .value(IsolationLevel::RepeatableRead),
            literal(keyword(Keyword::Serializable)).value(IsolationLevel::Serializable),
        ))),
    )
    .parse_next(input)
}
```

`START TRANSACTION`のあとに`opt(isolation_level)`を置き，なければ`ReadCommitted`にする．引き継いだテストに，新しいキーワードと同じ名前の識別子はなかった．

### スナップショット

```rust
    pub fn snapshot(&mut self, manager: &TransactionManager) -> Snapshot {
        match self.isolation {
            Isolation::ReadCommitted => manager.snapshot(self.xid),
            Isolation::RepeatableRead => self
                .snapshot
                .get_or_insert_with(|| manager.snapshot(self.xid))
                .clone(),
        }
    }
```

`Database::run`は，Iteration 21の`manager.snapshot(txn.xid())`の代わりに`txn.snapshot(&manager)`を呼ぶ．そのため`run`は`&mut Transaction`を受け取る．

### 衝突の判定

```rust
pub fn write_conflict(
    header: &TupleHeader,
    snapshot: &Snapshot,
    manager: &TransactionManager,
) -> Option<WriteConflict> {
    if header.xmax == TxnId::INVALID || header.xmax == snapshot.xid {
        return None;
    }
    match manager.status(header.xmax) {
        TxnStatus::Aborted => None,
        TxnStatus::InProgress => Some(WriteConflict::InProgress(header.xmax)),
        TxnStatus::Committed => Some(WriteConflict::Committed),
    }
}
```

`write_conflict`は，スナップショットから見える版だけに使う．見える版の`xmax`がコミット済みなら，そのトランザクションはスナップショットのあとにコミットしている．

`exec::dml`の`update`と`delete`は，対象の版を集めたあと，書き換える前に`first_conflict`で調べる．

```rust
    if let Some(xid) = first_conflict(heap, targets.iter().map(|(id, _)| *id), snapshot, manager)? {
        return Ok(Outcome::WaitFor(xid));
    }
```

### 待ってやり直す

```rust
    fn change_rows(
        &self,
        txn: &mut Transaction,
        change: impl Fn(&mut Storage, &Snapshot, &TransactionManager) -> Result<Outcome, Error>,
    ) -> Result<usize, Error> {
        loop {
            let outcome = {
                let mut storage = self.write_storage();
                let manager = self.read_transactions();
                let snapshot = txn.snapshot(&manager);
                change(&mut storage, &snapshot, &manager)?
            };
            match outcome {
                Outcome::Done(count) => return Ok(count),
                Outcome::WaitFor(xid) => self.wait_for(xid)?,
            }
        }
    }
```

- ブロックの終わりで`storage`と`manager`のガードが捨てられてから，`wait_for`で待つ．
- `wait_for`は，`EndSignal::wait_until`に，`xid`の状態が進行中でなくなったかを調べるクロージャを渡す．時間切れなら`LockTimeout`のエラーにする．
- `commit`と中止は，状態を変えたあとに`EndSignal::notify`を呼ぶ．ログへ書けずに中止する道も含めて，中止を`abort`にまとめた．

衝突を調べる前は，待つはずの`UPDATE`がすぐに終わり，テストは次のように失敗した．

```text
thread 'read_committed_update_waits_and_changes_the_latest_version' (424036) panicked at tests/isolation.rs:68:9:
assertion failed: !waiting.is_finished()
```

### 待つ時間の上限

`Database`は`Default`を導出していたが，導出すると`lock_timeout`は`Duration::default()`の0になり，どの衝突もすぐに`55P03`になる．`Default`を実装して，既定の上限を60秒にした．
`with_lock_timeout`は，構造体の更新構文で`lock_timeout`だけを変える．

```rust
    pub fn with_lock_timeout(self, timeout: Duration) -> Database {
        Database {
            lock_timeout: timeout,
            ..self
        }
    }
```

### `psql`での確かめ

演習の手順の使用例は，2つの`psql`で実行した結果である．セッションBの`UPDATE`は，セッションAの`COMMIT`のあとにエラーを返す．
データディレクトリで開いたサーバーを`kill -9`で止め，起動し直しても，コミットした行が残ることも確かめた．

## 22-6 振り返り

1. 分離レベルごとのスナップショット，衝突の3つの場合(進行中，コミット済み，中止)，待つ時間の上限，`SERIALIZABLE`を確かめる項目があるかを比べる．
2. 誤って通るのは，待たずに書き換えてしまう実装で，スレッドが`UPDATE`を始めるまでに100ミリ秒以上かかったときである．そのときも，あとで`N`の値を確かめると誤りが見つかる．正しく待つ実装では，スレッドは相手が終わるまで終わらないので，誤って失敗することはない．
3. 文全体をやり直すと，待った行のほかに，文の始まりには条件を満たさなかった行が，やり直しのあとで条件を満たすことがある．PostgreSQLは待った行だけを評価し直すので，そうした行は書き換えない．
4. AとBの書き換える行は違うので，どちらの`xmax`も衝突しない．`ferrodb`で試すと，どちらの`COMMIT`も成功し，当番が0人になった．

    ```text
     NAME  | ON_CALL
    -------+---------
     alice | f
     bob   | f
    (2 rows)
    ```

    衝突として見つけるには，読んだ行や条件も記録して，ほかのトランザクションの書き換えと突き合わせる必要がある(PostgreSQLの`SERIALIZABLE`)．

5. `SELECT`の結果の行の版に，`UPDATE`と同じく自分の番号を印として書き，ほかのトランザクションが`write_conflict`で待つようにする．`ferrodb`の`xmax`は削除の印も兼ねるので，行をロックしただけの印と区別する方法(PostgreSQLはヘッダーのビットで区別する)も要る．
6. 模範解答の設計ドキュメントは，照合スクリプトで問題がないことを確かめてある．

## 22-7 発展課題

解答例である．`Database`に，待っているトランザクションから待たれているトランザクションへの対応`waits: Mutex<HashMap<TxnId, TxnId>>`を加える．

```rust
    fn wait_for(&self, waiter: TxnId, holder: TxnId) -> Result<(), Error> {
        {
            let mut waits = self.waits.lock().expect(POISONED);
            let mut next = holder;
            while let Some(&other) = waits.get(&next) {
                if other == waiter {
                    return Err(TransactionError::Deadlock.into());
                }
                next = other;
            }
            waits.insert(waiter, holder);
        }
        let ended = self.ends.wait_until(self.lock_timeout, || {
            self.read_transactions().status(holder) != TxnStatus::InProgress
        });
        self.waits.lock().expect(POISONED).remove(&waiter);
        if ended {
            Ok(())
        } else {
            Err(TransactionError::LockTimeout.into())
        }
    }
```

- `holder`が待っているトランザクションを順にたどり，`waiter`にたどり着けば，待つと循環になる．待つ前に調べるので，`waits`の中に循環はできず，たどる処理は必ず終わる．
- `change_rows`は`self.wait_for(txn.xid(), xid)`と呼ぶ．
- `TransactionError::Deadlock`を`40P01`(`deadlock detected`)にする．
- 結合テストでは，AとBがそれぞれ1行を書き換えたあと，Aをスレッドで相手の行へ向かわせて待たせ，Bが相手の行を書き換えようとすると`40P01`になることを確かめた．Bが`ROLLBACK`すると，Aの`UPDATE`が終わる．
