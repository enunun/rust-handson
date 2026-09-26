# テストリスト

## 単体テスト

### sql::parser

- [x] `SELECT * FROM t`の期待値を，`items`，`from`，`filter`を持つ`Select`に変える
- [x] 列の名前は，式の中の値として使える(`a + 1`)
- [x] 選択項目は，式と，`AS`を付けた別名，`AS`を省いた別名を持てる
- [x] `WHERE`の条件を持つ`SELECT`
- [x] 選択項目のない`SELECT FROM t`は，`FROM`の位置の構文エラーになる

### plan::binder

- [x] リテラルは値の定数になる(整数は範囲で`INTEGER`か`BIGINT`)
- [x] 列の名前は，列の並びの中の番号になる
- [x] 式の中の列の名前も解決される
- [x] ない列の名前は，`UndefinedColumn`になる

### exec::eval

- [x] 評価のテストの補助関数で，評価の前に`bind`を呼ぶ
- [x] 列は，行の中の値になる
- [x] 条件は，真のときだけ`true`になる．偽と`NULL`は`false`になる
- [x] 真偽値でない条件は，`ArgumentNotBoolean`になる

## 結合テスト

### select

- [x] 列と式を選び，別名を付け，`WHERE`で絞り込む
- [x] `WHERE`は条件が真の行だけを残す
- [x] `WHERE`は条件が`UNKNOWN`の行を残さない(`NOT age > 26`で`age`が`NULL`の行は残らない)
- [x] 別名のない式の列名は`?column?`になる
- [x] `*`と式を混ぜて選べる
- [x] ない列は`42703`と`column "EMAIL" does not exist`になる
- [x] 真偽値でない`WHERE`の条件は`42804`になる
- [x] `VALUES`の式で列を参照すると`42703`になる
