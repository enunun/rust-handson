# Iteration 12：集約

このIterationでは，集約関数`COUNT`，`SUM`，`AVG`，`MIN`，`MAX`と，`GROUP BY`，`HAVING`を扱う．
行をキーの値でグループにまとめ，グループごとに集約関数を計算する演算子`HashAggregate`を作る．
Rustでは，値の並びを`HashMap`のキーにする方法と，集約関数ごとの途中の結果をトレイトオブジェクトで表す方法を学ぶ．

## 12-1 準備

`cargo test`を実行し，引き継いだテストがすべて通ることを確かめる．次は結果の要約である．

```text
test result: ok. 201 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

## 12-2 文法と概念

次の2つのノートを読む．

- [Rustのノート](../../../../docs/rust/iteration-12.md)：`HashMap`のキーになる条件と`Hash`と`Eq`の実装，`entry` API，トレイトオブジェクトを作って返す関数
- [データベースのノート](../../../../docs/db/iteration-12.md)：集約関数と`NULL`，`GROUP BY`，集約する問い合わせで使える式，`HAVING`，ハッシュ集約

読み終えたら，一時的なモジュールにテストを書いて，次の課題を確かめる．

1. `&[(&str, i64)]`(部署と給与)から，部署ごとの給与の合計を，部署が最初に現れた順の`Vec<(String, i64)>`で返す関数を書く．`HashMap`の`entry`と`or_insert_with`で，部署から`Vec`の添字を引く．
2. 課題1の部署を`Option<&str>`にし，`None`の部署も1つのグループにまとまることを確かめる．
3. `fn add(&mut self, n: i64)`と`fn finish(&self) -> Option<i64>`を持つトレイトを定義し，合計と最大値の2つの型に実装する．名前(`"sum"`か`"max"`)から`Box<dyn トレイト>`を作る関数を書き，2つを`Vec`に並べて同じ値を加える．

## 12-3 テストリスト

### 要件

- 集約関数`COUNT(*)`，`COUNT(x)`，`SUM`，`AVG`，`MIN`，`MAX`と，`DISTINCT`付きの集約を扱う．
- `GROUP BY`と`HAVING`を扱う．`GROUP BY`がなく集約関数だけがある場合は，全体で1つのグループとする．
- 集約関数は`NULL`を無視する．空の集合の`COUNT`は`0`，それ以外は`NULL`とする．
- `AVG`は整数の平均を0の方向に切り捨てた`BIGINT`とする．
- `COUNT`は`BIGINT`，`SUM`は整数の合計を`BIGINT`，`MIN`と`MAX`は引数と同じ型で返す．`SUM`と`AVG`の引数が整数でなければ`42883`とする．
- `GROUP BY`にない列を集約せずに選んだら`42803`とする．`WHERE`などの集約関数と，入れ子の集約関数も`42803`とする．
- グループは，最初に現れた順で返す．`GROUP BY`のキーの`NULL`どうしは同じグループにする．
- 集約関数を書いた選択項目の列名は，関数の名前(`COUNT`など)とする．

エラーのメッセージは，PostgreSQLに合わせて次のようにする．

| SQLSTATE | メッセージ |
| --- | --- |
| `42803` | `column "NAME" must appear in the GROUP BY clause or be used in an aggregate function` |
| `42803` | `aggregate functions are not allowed in WHERE`(`JOIN conditions`，`GROUP BY`，`VALUES`，`UPDATE`も同じ形) |
| `42803` | `aggregate function calls cannot be nested` |
| `42883` | `function sum(character varying) does not exist` |
| `42804` | `argument of HAVING must be type boolean, not type bigint` |

### 使用例

```console
ferrodb> SELECT dept, COUNT(*) AS n, SUM(salary) AS total FROM emp GROUP BY dept HAVING COUNT(*) > 1;
 DEPT | N | TOTAL
------+---+-------
 dev  | 2 |   950
(1 row)

ferrodb> SELECT COUNT(*), COUNT(dept), AVG(salary), MIN(name), MAX(salary) FROM emp;
 COUNT | COUNT | AVG | MIN | MAX
-------+-------+-----+-----+-----
     4 |     3 | 412 | Ito | 500
(1 row)

ferrodb> EXPLAIN SELECT dept, COUNT(*) AS n, SUM(salary) AS total FROM emp GROUP BY dept HAVING COUNT(*) > 1;
                            QUERY PLAN
-------------------------------------------------------------------
 Project [EMP.DEPT, COUNT(*), SUM(EMP.SALARY)]
   Filter (COUNT(*) > 1)
     HashAggregate [COUNT(*), SUM(EMP.SALARY)] GROUP BY [EMP.DEPT]
       SeqScan EMP
(4 rows)
```

表`emp`は，Iteration 11の`emp`と同じものである．

### 作るもの

| モジュール | 作るもの |
| --- | --- |
| `sql::token`，`sql::lexer` | キーワード`COUNT`，`SUM`，`AVG`，`MIN`，`MAX`，`GROUP`，`HAVING` |
| `sql::ast` | `enum AggregateFunc`と`fn name(self) -> &'static str`，`Expr::Aggregate { func, arg: Option<Box<Expr>>, distinct }`，`Expr::contains_aggregate(&self) -> bool`．`Select`に`group_by: Vec<Expr>`，`having: Option<Expr>` |
| `plan::binder` | `struct AggregateCall { func, arg: Option<BoundExpr>, distinct }`，`struct BoundAggregate { keys, calls, having }`，`BoundSelect::aggregate`．`pub fn bind_in(expr, scope, clause)`．`BindError`の新しい列挙子 |
| `plan::planner` | `PlanNode::HashAggregate { input, keys, calls, columns }`．`PlanNode::Filter`に`clause` |
| `exec::aggregate` | `pub trait Accumulator { fn add(&mut self, value: &Value) -> Result<(), EvalError>; fn finish(&self) -> Value; }`，集約関数ごとの実装，`struct HashAggregate` |
| `exec::filter` | `Filter::new(input, predicate, clause)` |
| `exec::eval` | `EvalError::UndefinedFunction { name, argument }` |
| `error` | `SqlState::GroupingError`(`42803`)，`UndefinedFunction`(`42883`) |

### 書くときに考えること

- `Accumulator`の単体テストは，値を直接`add`して`finish`の結果を確かめられる．値がない場合，`NULL`だけの場合，負の数の`AVG`を考える．
- `HashAggregate`の単体テストでは，子に`SeqScan`を置き，グループの順序，`NULL`のキー，行がない場合を確かめる．
- 集約した行の列の並び(キーのあとに集約関数の結果)を決めておくと，選択項目の名前解決の期待値を書ける．
- 既存のテストのうち，`Select`の構文木や`Filter::new`を使っているものはどう変わるか．

## 12-4 設計ドキュメント

- `c4-component.md`：`exec::aggregate`を加える．
- `code-types.md`：`AggregateFunc`，`AggregateCall`，`BoundAggregate`，`Accumulator`とその実装，`HashAggregate`を加え，`Expr`，`Select`，`BoundSelect`，`PlanNode`，エラーの型を更新する．
- `code-sequence.md`：`HashAggregate`が子の行をグループに振り分け，集約した行を返す流れを加える．

更新したら，リポジトリのルートでMermaidの構文を検査する．

## 12-5 テスト駆動の実装

### 実装のヒント

- 集約関数の呼び出しは，式の最小の部品(`operand`)で読む．`COUNT(*)`の選択肢を先に試し，読めなければ`関数([DISTINCT] 式)`を読む．
- `GROUP BY`か`HAVING`があるか，選択項目か`ORDER BY`に集約関数があれば，集約する問い合わせにする．
- 集約する問い合わせでは，選択項目，`HAVING`，`ORDER BY`の式を，集約した行について解決する．
  - 集約関数の呼び出しは，引数を`FROM`の行について解決し，集約した行の結果の列にする．
  - 集約関数を含まない式は，まず`FROM`の行について解決し，`GROUP BY`のキーと同じなら，キーの列にする．
  - それ以外は，式を分解して部分ごとに解決する．キーでない列にたどり着いたら`42803`である．
- `bind`は，集約関数を見つけたら，句の名前を知らないエラー(`MisplacedAggregate`)を返す．`WHERE`などを解決する側で，句の名前を持つエラーに変える．
- `HashAggregate`は，`HashMap<Row, usize>`でキーの値からグループの添字を引き，グループは最初に現れた順で`Vec`へ置く．
- `NULL`でない値だけを`Accumulator::add`に渡す．`COUNT(*)`には引数がないので，行ごとに`NULL`でない値を渡す．
- `DISTINCT`付きの集約は，受け取った値を`HashSet`で覚え，初めての値だけを中の`Accumulator`に渡す`Accumulator`で表せる．
- `HAVING`は，`HashAggregate`の上の`Filter`になる．`Filter`が句の名前を持てば，エラーのメッセージを`WHERE`と`HAVING`で書き分けられる．

## 12-6 振り返り

1. 自分の`TESTLIST.md`を[模範解答のテストリスト](../../solution/TESTLIST.md)と比べる．
2. 集約関数ごとの途中の結果を`Box<dyn Accumulator>`で持った．`enum AccumulatorState { Count(i64), Sum(Option<i64>), ... }`の列挙型で持つ設計と比べる．集約関数を加えるとき，それぞれ何か所を変えるか．Iteration 10で比べた`PlanNode`と`Executor`の関係と，どこが似ているか．
3. `HashAggregate`は，グループを`HashMap`の値でなく`Vec`に置き，`HashMap`には添字を入れた．`HashMap<Row, Vec<Box<dyn Accumulator>>>`に直接置く設計では，結果の行の順序はどうなるか．
4. 集約した行の列の並びを「キーのあとに集約関数の結果」とした．選択項目，`HAVING`，`ORDER BY`の式は，この並びの番号で列を指す．この並びを知っているのはどのモジュールか．
5. 設計ドキュメントと実装を見比べ，違うところがあれば設計ドキュメントを直す．

## 12-7 発展課題

標準SQLの集約関数の`FILTER (WHERE 条件)`句に対応する．条件が`TRUE`の行だけを，その集約関数に使う．

```sql
SELECT dept, COUNT(*), COUNT(*) FILTER (WHERE salary > 420) AS rich FROM emp GROUP BY dept;
```

`dev`の行は`2`と`2`，`ops`の行は`1`と`0`になる．`EXPLAIN`では`COUNT(*) FILTER (WHERE (EMP.SALARY > 420))`と表示する．
これまでと同じく，テストリスト，設計ドキュメント，実装の順に進める．
