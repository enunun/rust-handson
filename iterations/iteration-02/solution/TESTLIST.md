# テストリスト

## 単体テスト

### lexer

- [x] `= <> < <= > >=`からは，6つの比較演算子を順に返す
- [x] `<>1`と`<=1`では，2文字の演算子を1つのトークンとして読む
- [x] `true false unknown null and or not is`からは，8つのキーワードを返す

### parser

- [x] 構文木の期待値の`BinaryOp::Add`などを，`BinaryOp::Arithmetic(ArithmeticOp::Add)`などに変える
- [x] `TRUE`，`FALSE`，`NULL`，`UNKNOWN`は，真偽値と`NULL`の定数になる(`UNKNOWN`は`NULL`)
- [x] 6つの比較演算子は，それぞれの比較の構文木になる
- [x] `1 + 2 < 4`は`(1 + 2) < 4`になる(算術が先)
- [x] `1 < 2 < 3`は，構文エラーになる(比較は結合しない)
- [x] `TRUE OR FALSE AND FALSE`は`TRUE OR (FALSE AND FALSE)`になる
- [x] `NOT 1 = 2`は`NOT (1 = 2)`になる
- [x] `NOT TRUE AND FALSE`は`(NOT TRUE) AND FALSE`になる
- [x] `1 IS NULL`と`1 IS NOT NULL`は，`IsNull`の構文木になる
- [x] `1 + 2 IS NULL`は`(1 + 2) IS NULL`になる

### eval

- [x] `TRUE`と`FALSE`は，真偽値になる
- [x] `NULL`と`UNKNOWN`は，`NULL`になる
- [x] 整数どうしの6つの比較
- [x] `FALSE < TRUE`は真になる
- [x] `1 = NULL`と`NULL = NULL`は，`NULL`になる
- [x] `AND`は3値論理の真理値表に従う
- [x] `OR`は3値論理の真理値表に従う
- [x] `NOT`は3値論理に従う(`NOT NULL`は`NULL`)
- [x] `IS NULL`と`IS NOT NULL`は，`NULL`かどうかで真か偽になる
- [x] 算術演算の片方が`NULL`なら，結果は`NULL`になる(`NULL / 0`も`NULL`)
- [x] `1 + TRUE`，`-TRUE`，`TRUE + NULL`は，型の不一致のエラーになる
- [x] `1 = TRUE`は，型の不一致のエラーになる
- [x] `1 AND TRUE`と`NOT 1`は，型の不一致のエラーになる

## 結合テスト

### values

- [x] `VALUES (1 < 2 AND NULL, NULL IS NULL, 1 + NULL)`は，`NULL`，真，`NULL`の行になる
- [x] `VALUES (1 + TRUE)`は，型の不一致のエラーになる
