# テストリスト

## 単体テスト

### sql::parser

- [x] `Select`の期待値に，`distinct: false`，空の`order_by`，既定の`limit`を加える
- [x] `ORDER BY`は，式，`ASC`か`DESC`，`NULLS FIRST`か`NULLS LAST`を持つキーの並びである．向きを省くと昇順である
- [x] `OFFSET n ROWS`と`FETCH FIRST n ROWS ONLY`は，飛ばす行の数と返す行の数になる．`0`も書ける
- [x] 行の数に整数以外を書くと，その位置の構文エラーになる
- [x] `SELECT DISTINCT`は`distinct`を真にする

### value

- [x] 同じ型の値は，数，`FALSE`から`TRUE`，文字列の順に並ぶ
- [x] `NULL`はどの値よりも大きく，`NULL`どうしは等しい
- [x] `INTEGER`と`BIGINT`は数として比べる

### exec::sort

- [x] 既定では，`NULL`は昇順で最後，降順で最初になる
- [x] `NULLS FIRST`と`NULLS LAST`は，既定の位置を変える
- [x] 前のキーが等しい行は，次のキーで並べる
- [x] すべてのキーが等しい行は，元の順序を保つ

## 結合テスト

### order_by

- [x] `ORDER BY name DESC OFFSET 1 ROWS FETCH FIRST 2 ROWS ONLY`は，名前の降順の2番目から2行を返す
- [x] `NULL`は昇順で最後，降順で最初になる
- [x] `NULLS FIRST`と`NULLS LAST`で`NULL`の位置を変えられる
- [x] 1つ目のキーが等しい行は，2つ目のキーで並べる
- [x] `ORDER BY`で，選択項目の別名を使える
- [x] `ORDER BY`で，選択していない列を使える
- [x] 行の数より大きい`OFFSET`と，`FETCH FIRST 0 ROWS ONLY`は，0行を返す
- [x] `DISTINCT`は重複する行を除く．`NULL`の行も1つにまとめる
- [x] `DISTINCT`は`FETCH FIRST`より先に適用する
- [x] `SELECT DISTINCT`で選択項目にない式で並べ替えると，`42P10`と`for SELECT DISTINCT, ORDER BY expressions must appear in select list`になる
- [x] `ORDER BY`のない列は`42703`になる
