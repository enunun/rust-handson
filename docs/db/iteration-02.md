# Iteration 2：`NULL`と3値論理

## `NULL`

`NULL`は「値がない」ことを表す特別な印である．値が分からない，まだ決まっていない，当てはまらない，といった場面で使う．
`NULL`はどの型の列にも入る．`0`や空の文字列とは違う．

`NULL`を含む計算の結果は，原則として`NULL`になる．

```sql
VALUES (1 + NULL)   -- NULL
```

## 3値論理

`NULL`と比べた結果は，真と偽のどちらでもない「不明」(`UNKNOWN`)になる．
`NULL = NULL`も不明である．2つの分からない値が等しいかどうかは分からない．

そのため，SQLの論理演算は真(`TRUE`)，偽(`FALSE`)，不明(`UNKNOWN`)の3つの値をとる．
標準SQLでは，真偽値型の`NULL`が`UNKNOWN`である．

| `a` | `b` | `a AND b` | `a OR b` |
| --- | --- | --- | --- |
| TRUE | TRUE | TRUE | TRUE |
| TRUE | FALSE | FALSE | TRUE |
| TRUE | UNKNOWN | UNKNOWN | TRUE |
| FALSE | FALSE | FALSE | FALSE |
| FALSE | UNKNOWN | FALSE | UNKNOWN |
| UNKNOWN | UNKNOWN | UNKNOWN | UNKNOWN |

表は`a`と`b`を入れ替えても同じである．`NOT UNKNOWN`は`UNKNOWN`である．

覚え方は，「不明を真と偽のどちらに置き換えても結果が変わらないなら，その結果になる」である．
`FALSE AND UNKNOWN`は，不明を真と偽のどちらに置き換えても偽なので，偽になる．

## `IS NULL`

`NULL`かどうかを調べるには，`= NULL`ではなく`IS NULL`を使う．`= NULL`は常に不明になる．

```sql
VALUES (NULL = NULL, NULL IS NULL)   -- UNKNOWN と TRUE
```

`IS NOT NULL`は，`NULL`でなければ真になる．`IS NULL`と`IS NOT NULL`の結果は，不明にはならない．

## 比較

比較演算子は`=`，`<>`(等しくない)，`<`，`<=`，`>`，`>=`の6つである．
標準SQLでは，比べる2つの値は比べられる型でなければならない．整数と真偽値は比べられない．
真偽値どうしでは，`FALSE`が`TRUE`より小さい．

## 優先順位

`ferrodb`は，PostgreSQLと同じく次の優先順位にする(上ほど弱い)．

| 優先順位 | 演算子 | 結合性 |
| --- | --- | --- |
| 1 | `OR` | 左 |
| 2 | `AND` | 左 |
| 3 | `NOT` | 前置 |
| 4 | `IS NULL`，`IS NOT NULL` | 後置 |
| 5 | `=`，`<>`，`<`，`<=`，`>`，`>=` | なし |
| 6 | `+`，`-` | 左 |
| 7 | `*`，`/` | 左 |
| 8 | 単項の`-` | 前置 |

`NOT a = b`は`NOT (a = b)`，`a OR b AND c`は`a OR (b AND c)`になる．
比較は結合性がないので，`1 < 2 < 3`は構文エラーである．
