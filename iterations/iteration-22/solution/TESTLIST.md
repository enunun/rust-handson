# テストリスト

## 単体テスト

### sql::parser

- [x] `START TRANSACTION ISOLATION LEVEL`のあとの`READ COMMITTED`，`REPEATABLE READ`，`SERIALIZABLE`を読む．`LEVEL`や分離レベルが欠けていればエラーである
- [x] (変更)`START TRANSACTION`だけなら，分離レベルは`READ COMMITTED`である

### txn

- [x] `REPEATABLE READ`のトランザクションは最初のスナップショットを使い続け，`READ COMMITTED`は文ごとに新しいスナップショットを作る
- [x] 見える版の`xmax`を進行中のトランザクションが書いていれば`InProgress`，スナップショットのあとにコミットしたトランザクションが書いていれば`Committed`の衝突である．中止したトランザクションの`xmax`は衝突でない
- [x] 終わりを待つスレッドは，知らせを受けて条件が成り立てば起き，時間の上限を過ぎればあきらめる

### error

- [x] 直列化の失敗は`40001`，待つ時間の超過は`55P03`，`SERIALIZABLE`は`0A000`である

### exec::dml

- [x] 進行中のトランザクションが書き換えている行は，何も変えずに`WaitFor`を返す．そのトランザクションがコミットしたあとは，同じスナップショットでは直列化の失敗になる
- [x] (変更)`update`と`delete`は，書き換えた行の数を`Outcome::Done`で返す

## 結合テスト

### isolation

- [x] `READ COMMITTED`のトランザクションは，ほかのトランザクションがコミットした変更を次の文から見る
- [x] `REPEATABLE READ`のトランザクションは，最初のスナップショットを最後まで見る
- [x] `SERIALIZABLE`は`0A000`で，トランザクションは始まらない
- [x] `READ COMMITTED`の`UPDATE`は，同じ行を書き換えているトランザクションの終わりを待ち，最新の版を書き換える
- [x] `REPEATABLE READ`の`UPDATE`は，待った相手がコミットすると`40001`になり，トランザクションは失敗した状態になる
- [x] 待った相手が中止すると，そのまま書き換える
- [x] 待つ時間が上限を超えると`55P03`になる

### そのほか(既存)

- [x] 引き継いだテストがすべて通る
