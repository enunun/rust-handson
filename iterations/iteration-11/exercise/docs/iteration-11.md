# Iteration 11：結合

このIterationでは，`FROM`に複数の表を書き，`CROSS JOIN`，`INNER JOIN`，`LEFT JOIN`で結合する．
表に別名を付け，列を`e.name`のように表で修飾できるようにする．名前解決は，複数の表の列から名前を探すようになる．
Rustでは，子の演算子を2つ持ち，途中の状態を覚えながら1行ずつ返す構造体を書く．

## 11-1 準備

`cargo test`を実行し，引き継いだテストがすべて通ることを確かめる．次は結果の要約である．

```text
test result: ok. 185 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

## 11-2 文法と概念

次の2つのノートを読む．

- [Rustのノート](../../../../docs/rust/iteration-11.md)：トレイトオブジェクトを組み合わせる構造体，`Option::take`と`replace`，スライスのパターン，`fold`と`reduce`，`self`を受け取るメソッド
- [データベースのノート](../../../../docs/db/iteration-11.md)：直積，内部結合，外部結合，表の別名と修飾した列名，入れ子ループ結合

読み終えたら，一時的なモジュールにテストを書いて，次の課題を確かめる．

1. `Option<String>`のフィールド`current`を持つ構造体に，`current`の中身を取り出して返し，`current`を`None`にするメソッド`fn finish(&mut self) -> Option<String>`を書く．最初は`take`を使わずに`self.current`を返してみて，コンパイラーのエラーを読む．
2. `&[&str]`のうち`"x"`に等しい要素の添字を集め，0個なら`"none"`，1個なら`"one"`，2個以上なら`"many"`を返す関数を，スライスのパターンで書く．
3. `["a", "b", "c"]`を`fold`で`"((a+b)+c)"`という文字列にする．

## 11-3 テストリスト

### 要件

- `FROM a, b`，`CROSS JOIN`，`INNER JOIN ... ON`，`LEFT [OUTER] JOIN ... ON`を扱う．`INNER`は省略できる．
- 表の別名(`FROM emp e`，`FROM emp AS e`)と，修飾した列名(`e.name`)を扱う．`EXPLAIN`では列名を修飾して表示する．
- どちらの表の列か決まらない列名は`42702`とする．`FROM`にない表で修飾した列名は`42P01`，同じ名前(別名)の表が2つあれば`42712`とする．
- `LEFT JOIN`で相手のない行は，右側の列を`NULL`にする．
- 結合した行は，左の表の列のあとに右の表の列を並べる．`*`もこの順に展開する．修飾した列の結果の列名は，修飾を除いた列名とする．

エラーのメッセージは，PostgreSQLに合わせて次のようにする．

| SQLSTATE | メッセージ |
| --- | --- |
| `42702` | `column reference "ID" is ambiguous` |
| `42P01` | `missing FROM-clause entry for table "X"` |
| `42703` | `column E.CODE does not exist`(修飾した列がないとき) |
| `42712` | `table name "EMP" specified more than once` |
| `42804` | `argument of JOIN/ON must be type boolean, not type integer` |

### 使用例

```console
ferrodb> SELECT e.name, d.title FROM emp e LEFT JOIN dept d ON e.dept = d.code;
  NAME  |    TITLE
--------+-------------
 Sato   | Development
 Suzuki | Development
 Tanaka | Operations
 Ito    |
(4 rows)

ferrodb> EXPLAIN SELECT e.name, d.title FROM emp e LEFT JOIN dept d ON e.dept = d.code;
               QUERY PLAN
-----------------------------------------
 Project [E.NAME, D.TITLE]
   NestedLoopJoin LEFT (E.DEPT = D.CODE)
     SeqScan EMP E
     SeqScan DEPT D
(4 rows)
```

表`emp`と`dept`は，[ロードマップ](../../../../docs/ROADMAP.md)の完成形の例と同じものである．

### 作るもの

| モジュール | 作るもの |
| --- | --- |
| `sql::token`，`sql::lexer` | トークン`Dot`，キーワード`JOIN`，`CROSS`，`INNER`，`LEFT`，`OUTER`，`ON` |
| `sql::ast` | `enum TableRef { Table { name, alias }, Join(Box<Join>) }`，`struct Join { left, right, kind, condition: Option<Expr> }`，`enum JoinKind { Cross, Inner, Left }`，`Expr::QualifiedColumn { table, column }`．`Select::from`を`TableRef`にする |
| `plan::binder` | `struct Scope`と`struct ScopeColumn { table, name }`，`enum BoundFrom`．`bind(expr: &Expr, scope: &Scope)`，`bind_select(select: &Select, catalog: &Catalog)`．`BindError`の新しい列挙子 |
| `plan::planner` | `PlanNode::NestedLoopJoin { left, right, kind, condition, columns }`．`PlanNode::SeqScan`に`alias` |
| `plan::explain` | `NestedLoopJoin 種類 条件`と`SeqScan 表 別名`の表示 |
| `exec::join` | `struct NestedLoopJoin`と`new(left, right, kind, condition, right_width)` |
| `error` | `SqlState::AmbiguousColumn`(`42702`)，`DuplicateAlias`(`42712`) |

### 書くときに考えること

- 名前解決の単体テストでは，2つの表の列を並べた`Scope`を直接作ると，あいまいな名前や修飾した名前を短く確かめられる．
- `NestedLoopJoin`の単体テストでは，左右に`SeqScan`を置く．相手が2つある行，相手のない行，右が空の場合を考える．
- `UNKNOWN`になる結合の条件(`NULL`の列との比較)は，内部結合と左外部結合でそれぞれどうなるか．
- 既存のテストのうち，`Select::from`，`bind`の引数，`EXPLAIN`の表示を使っているものはどう変わるか．

## 11-4 設計ドキュメント

- `c4-component.md`：`exec::join`を加える．名前解決がカタログから表を引くようになることも反映する．
- `code-types.md`：`TableRef`，`Join`，`JoinKind`，`Scope`，`ScopeColumn`，`BoundFrom`，演算子の`NestedLoopJoin`を加え，`Select`，`Expr`，`BoundSelect`，`PlanNode`を更新する．
- `code-sequence.md`：`NestedLoopJoin`が内側の行を繰り返し読む流れを加える．内側の行をいつ読み，外側の行ごとにどこから照らし合わせるかを示す．

更新したら，リポジトリのルートでMermaidの構文を検査する．

## 11-5 テスト駆動の実装

### 実装のヒント

- `FROM`は，`,`で区切った「表と，続く`JOIN`の並び」である．`JOIN`を左から順に結合するには`fold`が，`,`の並びをつなぐには`reduce`が使える．
- 表の別名の`AS`は省略できる．`LEFT`や`JOIN`はキーワードなので，別名と取り違えない．
- 修飾した列名は，識別子のあとに`.`と識別子が続く形である．式の最小の部品(`operand`)で，列の名前と一緒に読む．
- `Scope`は，`FROM`の表の列を「表の別名(なければ名前)と列名」の組で並べる．結合した`Scope`は，左の並びのあとに右の並びをつなぐ．この順は，結合した行の値の順と同じにする．
- 名前に一致する列の番号を集め，数で分ける．0個ならない列，1個なら解決，2個以上ならあいまいである．
- `ON`の条件は，左右の表の列を並べた`Scope`で名前を解決する．
- `NestedLoopJoin`は，最初の`next`で内側の行をすべて読んで手元に置く．外側の今の行と，内側のどこまで照らし合わせたかを覚えておき，条件を満たす組を1つ返すたびに戻る．
- 外側の今の行を照らし合わせ終えたら，`take`で手放す．`LEFT JOIN`で相手がなければ，その行に`NULL`を加えて返す．
- `EXPLAIN`では，`SeqScan`の列名を`表.列`にする．`Project`や`Filter`の式は，子の列名で表示されるので，修飾名になる．

## 11-6 振り返り

1. 自分の`TESTLIST.md`を[模範解答のテストリスト](../../solution/TESTLIST.md)と比べる．
2. `NestedLoopJoin`は内側の行を最初に1度だけ読んで手元に置いた．外側の行ごとに内側の演算子を作り直して読む設計と比べる．内側が`Sort`のとき，それぞれ何回並べ替えるか．
3. 修飾した列名を，`Expr::Column`とは別の列挙子`Expr::QualifiedColumn`にした．`Expr::Column { table: Option<String>, name: String }`の1つにまとめる設計と比べる．既存のコードとテストのどこが変わるか．
4. `ON`の条件と`WHERE`の条件は，内部結合では同じ結果になり，左外部結合では違う結果になる．違いを確かめる結合テストを考える．
5. 設計ドキュメントと実装を見比べ，違うところがあれば設計ドキュメントを直す．

## 11-7 発展課題

標準SQLの`RIGHT [OUTER] JOIN`に対応する．`a RIGHT JOIN b ON 条件`は，内部結合の結果に，相手が1つもなかった右の行を加える．加えた行の左の表の列は`NULL`にする．
結合した行の列の順は，ほかの結合と同じく左の表の列のあとに右の表の列とする．
これまでと同じく，テストリスト，設計ドキュメント，実装の順に進める．
