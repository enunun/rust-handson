# テストリスト

## 単体テスト

### lexer

- [x] `1 SELECT`のエラーを確かめていたテストを，`1 users`から整数と識別子`USERS`を返すテストに変える
- [x] 識別子は，数字と`_`を含められる(`_user_2`は`_USER_2`)
- [x] キーワードのあとに英字が続く`VALUESX`は，識別子になる
- [x] `"Users"`は，大文字と小文字をそのまま残した識別子になる
- [x] `"select"`はキーワードではなく識別子になり，`"a""b"`は`a"b`になる
- [x] 文のキーワード`CREATE`，`TABLE`，`INSERT`，`INTO`，`SELECT`，`FROM`
- [x] 型のキーワード`INTEGER`，`INT`，`BIGINT`，`BOOLEAN`，`VARCHAR`

### parser

- [x] テストの補助関数を，`VALUES`の1行目の式を返す形に変える
- [x] `VALUES (1, 2), (3, 4)`は，2行の`Values`になる
- [x] `VALUES (1, 2), (3)`は，`ValuesLengthMismatch`になる
- [x] `CREATE TABLE`は，各データ型の列の定義を持つ構文木になる(`INT`は`INTEGER`)
- [x] `VARCHAR(0)`は，`0`の位置の構文エラーになる
- [x] `CREATE TABLE t ()`は，`)`の位置の構文エラーになる
- [x] 列を指定しない`INSERT`は，`columns`が`None`になる
- [x] 列を指定した`INSERT`は，`columns`が列名の並びになる
- [x] `SELECT * FROM t`は，表の名前を持つ`Select`になる
- [x] `SELECT *`は，文が途中で終わった構文エラーになる

### eval

- [x] `2147483648`を範囲外のエラーとしていたテストを，`BIGINT`の値を返すテストに変える
- [x] `INTEGER`と`BIGINT`の演算は`BIGINT`になる
- [x] `BIGINT`の範囲を超える演算は範囲外のエラーになる
- [x] `INTEGER`と`BIGINT`は値で比べる
- [x] `BIGINT`と`NULL`の演算は`NULL`になる

### catalog

- [x] 作った表は，名前で定義を引ける
- [x] ない表を引くと，`UndefinedTable`になる
- [x] 同じ名前の表を2回作ると，`DuplicateTable`になる
- [x] 1つの表に同じ名前の列があると，`DuplicateColumn`になる
- [x] 列の名前から番号を引ける．ない列は`UndefinedColumn`になる
- [x] `NULL`は，どの型の列にも入る
- [x] `INTEGER`と`BIGINT`は列の型に変換される．`INTEGER`の列に範囲外の`BIGINT`は入らない
- [x] `VARCHAR(n)`の長さは文字で数える
- [x] 列の型と違う型の値は，`TypeMismatch`になる

## 結合テスト

### values

- [x] `execute`を`Database::new().execute`に，結果を`StatementResult::Rows`に変える
- [x] 複数の行の`VALUES`を評価する
- [x] 行によって値の数が違う`VALUES`は，構文エラーになる

### tables

- [x] 表を作り，2行を挿入し，`SELECT *`で列名と行を得る
- [x] 作ったばかりの表は，行を持たない
- [x] 引用符で囲まない名前は，大文字と小文字を区別しない
- [x] 引用符で囲んだ名前は，大文字と小文字を区別する
- [x] 列を指定した`INSERT`では，指定しない列が`NULL`になる
- [x] `BIGINT`の列には`BIGINT`として，`INTEGER`の列には`INTEGER`として格納する
- [x] 既にある表を作ると`42P07`になる
- [x] ない表への`INSERT`は`42P01`になる
- [x] ない列への`INSERT`は`42703`になる
- [x] 型の違う値の`INSERT`は`42804`になる
- [x] 長すぎる文字列の`INSERT`は`22001`になる
- [x] `INTEGER`の列に範囲外の値を入れると`22003`になる
- [x] 値の数と列の数が違う`INSERT`は構文エラーになる
- [x] 途中の行でエラーになった`INSERT`は，1行も挿入しない
