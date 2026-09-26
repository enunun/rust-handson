# テストリスト

## 単体テスト

### sql::parser

- [x] `Select`の期待値に，空の`group_by`と`having: None`を加える
- [x] `COUNT(*)`，`COUNT(a)`，`SUM(DISTINCT a + 1)`，`AVG`，`MIN`，`MAX`を集約関数の呼び出しとして読む
- [x] 引数のない`SUM()`は構文エラーになる
- [x] `GROUP BY`の式の並びと`HAVING`の条件を読む

### plan::binder

- [x] 集約する問い合わせの選択項目は，集約した行(キーの値のあとに集約関数の結果)の列を指す．同じ集約関数は1つにまとめる
- [x] `GROUP BY`がなくても，集約関数があれば集約する問い合わせになる
- [x] `GROUP BY`にない列を集約せずに使うと`NotGrouped`になる．`*`も同じである
- [x] `HAVING`と`ORDER BY`の集約関数も，集約した行の列になる
- [x] `WHERE`の集約関数は`AggregateNotAllowed`，入れ子の集約関数は`NestedAggregate`になる

### plan::planner

- [x] `HashAggregate`と`HAVING`の`Filter`は，`SeqScan`と`Project`の間に入る

### plan::explain

- [x] `HashAggregate`は，集約関数の呼び出しと`GROUP BY`のキーを表示する．キーがなければ`GROUP BY`を省く

### exec::filter

- [x] `Filter::new`に，条件を書いた句の名前`WHERE`を渡す

### exec::aggregate

- [x] 値のない`COUNT`は`0`，ほかの集約関数は`NULL`になる
- [x] 集約関数は`NULL`を数えない
- [x] `AVG`は0の方向に切り捨てる(`-3`と`-4`の平均は`-3`)
- [x] `BIGINT`の範囲を超える`SUM`は`NumericOutOfRange`になる
- [x] 文字列の`SUM`は`UndefinedFunction`になる
- [x] `MIN`と`MAX`は文字列も比べる
- [x] `DISTINCT`付きの集約は，同じ値を1度だけ使う
- [x] `HashAggregate`は，グループを最初に現れた順で返す．キーの`NULL`は1つのグループになる
- [x] キーがなければ，行がなくてもグループを1つ返す．キーがあれば，行がないとグループもない

## 結合テスト

### aggregate

- [x] `GROUP BY dept HAVING COUNT(*) > 1`は，2人以上いる部署だけを返す
- [x] `dept`が`NULL`の行は，1つのグループになる
- [x] `GROUP BY`のない`COUNT(*)`，`COUNT(dept)`，`AVG`，`MIN`，`MAX`は1行を返す
- [x] 空の表では，`COUNT(*)`は`0`，`SUM`と`MAX`は`NULL`になる．`GROUP BY`があれば0行になる
- [x] `COUNT(DISTINCT dept)`と`SUM(DISTINCT ...)`
- [x] キーの式と集約関数を組み合わせた式を選び，別名で並べ替えられる
- [x] `GROUP BY`のない`HAVING`は，1つのグループを絞り込む
- [x] `GROUP BY`にない列は`42803`と`column "NAME" must appear in the GROUP BY clause or be used in an aggregate function`になる
- [x] `WHERE`，入れ子，`VALUES`の集約関数は`42803`になる
- [x] 文字列の`SUM`は`42883`と`function sum(character varying) does not exist`になる
- [x] 真偽値でない`HAVING`は`argument of HAVING must be type boolean, not type bigint`になる
