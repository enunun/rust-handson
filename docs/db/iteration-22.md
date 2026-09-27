# Iteration 22：分離レベルと書き込みの競合

## 分離レベル

ACIDの分離性(Isolation)は，同時に動くトランザクションが互いの途中の状態をどこまで見るかである．
標準SQLは，同時に動くことで起こる現象と，それを許す4つの分離レベルを定める．

| 現象 | 内容 |
| --- | --- |
| ダーティリード | ほかのトランザクションがコミットしていない変更が見える |
| ノンリピータブルリード | 同じ行を2回読むと，間にほかのトランザクションがコミットした変更で値が違う |
| ファントム | 同じ条件で2回問い合わせると，間にほかのトランザクションが加えた行が現れる |

| 分離レベル | ダーティリード | ノンリピータブルリード | ファントム |
| --- | --- | --- | --- |
| `READ UNCOMMITTED` | 起こりうる | 起こりうる | 起こりうる |
| `READ COMMITTED` | 起こらない | 起こりうる | 起こりうる |
| `REPEATABLE READ` | 起こらない | 起こらない | 起こりうる |
| `SERIALIZABLE` | 起こらない | 起こらない | 起こらない |

`SERIALIZABLE`は，3つの現象がないことに加えて，結果がトランザクションを1つずつ順に実行したときのどれかと同じになることを求める．
`START TRANSACTION ISOLATION LEVEL ...`で選ぶ．PostgreSQLの既定は`READ COMMITTED`である．

## スナップショットと分離レベル

MVCC(Iteration 18)では，どの版を見るかをスナップショットで決める．分離レベルの違いは，スナップショットを取る時点の違いになる．

- `READ COMMITTED`：文ごとに新しいスナップショットを取る．文を実行する前にコミットした変更は，次の文から見える．
- `REPEATABLE READ`：トランザクションの最初の文でスナップショットを取り，最後まで使う．ほかのトランザクションがあとでコミットした変更は見えない．

どちらも，コミットしていない変更は見えない．`REPEATABLE READ`のスナップショットは，最初の文の時点のデータベース全体を見るので，ファントムも起こらない(スナップショット分離)．
PostgreSQLも同じ方法で分離レベルを実現する．`READ UNCOMMITTED`を選んでも`READ COMMITTED`として動く．

## 更新の競合

2つのトランザクションが同じ行を書き換えると，スナップショットだけでは決まらない．Iteration 21では，あとから書いた`xmax`が先の`xmax`を上書きし，1行が2行になった．
PostgreSQLと`ferrodb`は，行を書き換える前に，その版の`xmax`を調べる．

| 版の`xmax`を書いたトランザクション | すること |
| --- | --- |
| なし，または中止した | そのまま書き換える |
| 進行中 | そのトランザクションが終わるまで待ってから，もう一度調べる |
| スナップショットのあとにコミットした | `REPEATABLE READ`は`40001`のエラー．`READ COMMITTED`は最新の版で条件を評価し直して書き換える |

- 先に書き換えたトランザクションが勝ち，あとのトランザクションは待つか失敗する(first-updater-wins)．
- `READ COMMITTED`は文ごとにスナップショットを取るので，待ったあとに新しいスナップショットで文をやり直せば，最新の版で条件を評価し直せる．PostgreSQLは，文をやり直さず，待った行の最新の版だけを評価し直す．
- `REPEATABLE READ`で最新の版を書き換えると，トランザクションの中で見ていた値と違う値を書き換えることになり，更新の消失が起こる．そのため`40001`(`could not serialize access due to concurrent update`)にする．

`40001`はトランザクションの誤りでなく，同時に動いたことによる失敗である．アプリケーションは，トランザクションを中止して最初からやり直す．

## 待つ時間の上限とデッドロック

AがBの書き換えた行を待ち，BがAの書き換えた行を待つと，どちらも進めない(デッドロック)．

- PostgreSQLは，一定時間(`deadlock_timeout`，既定は1秒)待ったら，誰が誰を待っているかのグラフを調べ，循環があれば片方を`40P01`(`deadlock detected`)で中止する．
- 待つ時間の上限`lock_timeout`を設定すれば，上限を超えた文は`55P03`(`canceling statement due to lock timeout`)で失敗する．PostgreSQLの既定は上限なしである．
- `ferrodb`は，待つ時間の上限(既定は60秒)でデッドロックを解く．循環を調べる方法は，発展課題で作る．

## スナップショット分離と`SERIALIZABLE`

スナップショット分離は3つの現象を起こさないが，`SERIALIZABLE`ではない．2つのトランザクションが同じデータを読み，互いに違う行を書き換えると，どちらの書き換えも衝突しないが，順に実行したときには起こりえない結果になる(書き込みスキュー，write skew)．

```text
-- 当番の医師が2人いて，1人は必ず当番に残る決まりがある
A: SELECT COUNT(*) FROM doctors WHERE on_call   -- 2
B: SELECT COUNT(*) FROM doctors WHERE on_call   -- 2
A: UPDATE doctors SET on_call = FALSE WHERE name = 'alice'
B: UPDATE doctors SET on_call = FALSE WHERE name = 'bob'
A: COMMIT
B: COMMIT                                       -- 当番が0人になる
```

PostgreSQLの`SERIALIZABLE`は，トランザクションが読んだ範囲と書いた範囲の依存を追いかけ，順に実行したときに起こりえない組み合わせを`40001`で中止する(Serializable Snapshot Isolation)．
`ferrodb`は`SERIALIZABLE`を選ぶと`0A000`(`feature_not_supported`)を返す．
