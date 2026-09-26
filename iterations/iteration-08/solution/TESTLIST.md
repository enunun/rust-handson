# テストリスト

## 単体テスト

### sql::parser

- [x] `CREATE TABLE`の期待値の各`ColumnDef`に，空の`constraints`を加える
- [x] 列の定義のあとに`PRIMARY KEY`，`NOT NULL`，`UNIQUE`を書ける．1つの列に複数の制約を書ける
- [x] `UPDATE`は，表，`列 = 式`の並び，`WHERE`の条件を持つ
- [x] `SET`のあとに代入のない`UPDATE`は，構文エラーになる
- [x] `DELETE FROM`は，表と`WHERE`の条件を持つ．`WHERE`は省略できる
- [x] `DROP TABLE`は，表の名前を持つ

### catalog

- [x] テストの補助関数の`Column`に`nullable`を，`TableSchema`に`unique_constraints`を加える
- [x] 消した表は，名前で引けなくなる
- [x] ない表を消すと，`UndefinedTableToDrop`になる

### plan::binder

- [x] テストの補助関数の`Column`に`nullable`を加える

### exec::dml

- [x] `NOT NULL`の列に`NULL`があれば，`ConstraintError::NotNull`になる
- [x] 一意性制約の列に同じ値があれば，`ConstraintError::Unique`になる
- [x] 一意性制約の列に`NULL`がいくつあっても，違反にならない
- [x] `insert`は，行を表の末尾に加え，加えた行の数を返す
- [x] `insert`で制約に違反すれば，1行も加えない
- [x] `update`は，条件を満たす行に代入し，書き換えた行の数を返す
- [x] `update`の代入の式は，書き換える前の行で評価する
- [x] `update`で制約に違反すれば，1行も書き換えない
- [x] `update`の一意性は，すべての行を書き換えたあとに調べる(`ID = ID + 1`)
- [x] `delete`は，条件を満たす行を消し，消した行の数を返す
- [x] 条件の評価が途中の行でエラーになれば，`update`と`delete`は表を変えない

### format

- [x] コマンドタグ`UPDATE n`，`DELETE n`，`DROP TABLE`

## 結合テスト

### tables

- [x] `DROP TABLE`で消した表は引けなくなり，同じ名前で作り直せる
- [x] ない表の`DROP TABLE`は，`42P01`と`table "T" does not exist`になる

### dml

- [x] `UPDATE`は条件を満たす行を書き換え，`UPDATE 2`を返す
- [x] `WHERE`のない`UPDATE`は，すべての行を書き換える
- [x] どの行も条件を満たさなければ，`UPDATE 0`を返す
- [x] `SET a = b, b = a`は，2つの列の値を入れ替える
- [x] 列の型に合わない値の代入は，`42804`になる
- [x] ない列への代入は，`42703`と`column "EMAIL" of relation "USERS" does not exist`になる
- [x] 同じ列への2つの代入は，`42601`と`multiple assignments to same column "AGE"`になる
- [x] `DELETE`は条件を満たす行を消し，`DELETE 2`を返す
- [x] `WHERE`のない`DELETE`は，すべての行を消す
- [x] 途中の行で評価のエラーになった`UPDATE`と`DELETE`は，表を変えない

### constraints

- [x] 主キーの値が重なる`INSERT`は，`23505`と`duplicate key value violates unique constraint "USERS_PKEY"`になり，1行も加えない
- [x] `UNIQUE`の制約の名前は`USERS_EMAIL_KEY`のように表と列から作る
- [x] `UNIQUE`の列には，`NULL`をいくつも入れられる
- [x] `NOT NULL`の列への`NULL`は，`23502`と`null value in column "NAME" of relation "USERS" violates not-null constraint`になる．列を省いた`INSERT`でも同じである
- [x] `PRIMARY KEY`の列には`NULL`を入れられない
- [x] 値を重ねる`UPDATE`は`23505`になり，1行も書き換えない
- [x] `UPDATE users SET id = id + 1`は，文の終わりに一意なので成功する
- [x] 2つの列に`PRIMARY KEY`を書くと，`42P16`と`multiple primary keys for table "T" are not allowed`になる
