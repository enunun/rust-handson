# テストリスト

## 単体テスト

### sql::lexer

- [x] `.`は，表の名前と列の名前を区切るトークンになる

### sql::parser

- [x] `Select`の期待値の`from`を，表の名前から`TableRef`に変える
- [x] `e.name`は修飾した列名になる
- [x] `FROM a, b AS x, c y`は，別名を持つ表を左から順に`CROSS JOIN`でつなぐ
- [x] `CROSS JOIN`，`JOIN`，`INNER JOIN`，`LEFT JOIN`，`LEFT OUTER JOIN`を読む
- [x] 続けて書いた`JOIN`は，左から順に結合する
- [x] `ON`のない`JOIN`は構文エラーになる

### plan::binder

- [x] テストの`bind`に渡す列の並びを`Scope`に，`bind_select`に渡す表の定義をカタログに変える
- [x] 1つの表にだけある名前は，その列になる
- [x] 2つの表にある名前は`AmbiguousColumn`になる
- [x] 修飾した名前は，その表の列だけから探す．見つからなければ`UndefinedQualifiedColumn`になる．`FROM`にない表の修飾は`MissingFromEntry`とする
- [x] 結合した行の列は，左の表の列のあとに右の表の列が並ぶ．`ON`の条件はその並びで解決する
- [x] `FROM`に同じ名前の表が2つあれば`DuplicateTableName`，ない表なら`UndefinedTable`になる

### plan::planner

- [x] テストの`SeqScan`の期待値に別名を加え，列名を修飾名にする
- [x] 結合は，2つの`SeqScan`を子とする`NestedLoopJoin`になり，左右の列名を並べる

### plan::explain

- [x] `EXPLAIN`の期待値の列名を修飾名にする
- [x] 結合は，種類と条件を表示し，左右の子を同じ深さに並べる．別名のある表は`SeqScan 表 別名`になる

### exec::join

- [x] `CROSS JOIN`は，左のすべての行を右のすべての行と組にする
- [x] 内部結合は，条件を満たす組だけを返す．相手が2つあれば2行を返す
- [x] 左外部結合は，相手のない左の行の右の列を`NULL`にする．右の表が空でも左の行を返す

## 結合テスト

### join

- [x] `emp e LEFT JOIN dept d ON e.dept = d.code`は，部署のない社員を`NULL`の部署名で返す
- [x] 内部結合は，相手のある行だけを返す
- [x] `,`と`CROSS JOIN`は，すべての組を返す．`*`は左の表の列のあとに右の表の列を並べる
- [x] `WHERE`は，結合した行を絞り込む
- [x] 同じ表を別名で2度結合できる
- [x] 2つの表にある列名は`42702`と`column reference "ID" is ambiguous`になる
- [x] `FROM`にない表で修飾すると`42P01`と`missing FROM-clause entry for table "X"`になる．別名を付けた表は元の名前で修飾できない．表にない列は`42703`と`column E.CODE does not exist`になる
- [x] 同じ名前の表を2つ書くと，`42712`と`table name "EMP" specified more than once`になる
- [x] 真偽値でない`ON`の条件は，`42804`と`argument of JOIN/ON must be type boolean, not type integer`になる
- [x] `EXPLAIN`は，結合と修飾した列名を表示する

### explain，repl(既存)

- [x] `EXPLAIN`の期待値の列名を修飾名にする
