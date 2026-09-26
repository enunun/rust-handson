# Iteration 11：結合

## 直積

`FROM`に表を`,`で並べると，2つの表の行のすべての組(直積)ができる．`CROSS JOIN`も同じである．
`emp`が4行，`dept`が2行なら，`SELECT * FROM emp, dept`は8行を返す．
結果の行は，左の表の列のあとに右の表の列が並ぶ．

## 内部結合

```sql
SELECT e.name, d.title
FROM emp e INNER JOIN dept d ON e.dept = d.code;
```

内部結合(`INNER JOIN`)は，直積のうち`ON`の条件が`TRUE`になる組だけを返す．`INNER`は省略できる．
`WHERE`と同じく，条件が`FALSE`か`UNKNOWN`の組は返さない．
意味は`FROM emp e, dept d WHERE e.dept = d.code`と同じである．`ON`に結合の条件を，`WHERE`に行の絞り込みを書くと読みやすい．

## 外部結合

```sql
SELECT e.name, d.title
FROM emp e LEFT JOIN dept d ON e.dept = d.code;
```

左外部結合(`LEFT [OUTER] JOIN`)は，内部結合の結果に，相手が1つもなかった左の行を加える．加えた行の右の表の列は`NULL`になる．
`dept`が`NULL`の社員は，`e.dept = d.code`が`UNKNOWN`なので相手がなく，部署名が`NULL`の行として残る．

| 結合 | 相手のない左の行 | 相手のない右の行 |
| --- | --- | --- |
| `INNER JOIN` | 返さない | 返さない |
| `LEFT JOIN` | 右を`NULL`にして返す | 返さない |

標準SQLには，右外部結合(`RIGHT JOIN`)と完全外部結合(`FULL JOIN`)もある．`a RIGHT JOIN b`は`b LEFT JOIN a`と同じ行を返す．

外部結合では，条件を`ON`に書くか`WHERE`に書くかで結果が変わる．
`ON`の条件は相手を探すときに使い，相手がなくても左の行は残る．`WHERE`の条件は結合したあとの行を絞り込むので，`NULL`で埋めた行も条件で消える．

## 表の別名と修飾した列名

```sql
SELECT a.name, b.name FROM emp a JOIN emp b ON a.salary > b.salary;
```

- `FROM emp e`や`FROM emp AS e`で，表に別名を付ける．別名を付けた表は，別名で修飾する(`e.name`)．元の表の名前では修飾できない．
- 同じ表を2度結合するときは，別名で区別する．同じ名前の表が`FROM`に2つあると，どちらか決まらないのでエラーになる．
- 修飾しない列名は，`FROM`のすべての表から探す．2つ以上の表にあると，どちらの列か決まらないのでエラー(`42702`)になる．

## 入れ子ループ結合

入れ子ループ結合(nested loop join)は，外側の表の行ごとに，内側の表のすべての行と組にして条件を調べる．

```text
外側の各行 l について:
    内側の各行 r について:
        l と r をつないだ行が条件を満たせば返す
```

外側が`m`行，内側が`n`行なら，`m × n`回条件を調べる．
`ferrodb`は，内側の行を最初に1度だけ読んで手元に置き，外側の行ごとにその並びを先頭からたどる．

どんな条件の結合にも使えるが，行が多いと遅い．
等号の条件(`e.dept = d.code`)なら，内側の行をキーでまとめたハッシュ表を作るハッシュ結合や，キーで並べた2つの表を突き合わせるマージ結合が速い．
PostgreSQLは，行の数の見積もりから3つの方法のどれかを選ぶ．
