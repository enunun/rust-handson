# テストリスト

## 単体テスト

### lexer

- [x] 既存のテストは，トークンの位置を除いて比べる(`kinds`)．`LexError`の期待値に，読めなかった文字`found`を加える
- [x] `VALUES ('あ', 1)`の各トークンは，文字で数えた位置1，8，9，12，14，15を持つ

### parser

- [x] 構文エラーの期待値を，読めなかったトークンと位置(`UnexpectedToken`)に変える
  - `VALUES (1 +)`は`)`の位置12，`VALUES ()`は`)`の位置9，`VALUES 1`は`1`の位置8
  - `VALUES (1) 2`は`2`の位置12，`VALUES (1 < 2 < 3)`は2つ目の`<`の位置15
- [x] `VALUES ((1 +))`は，内側の`)`の位置13を報告する
- [x] `VALUES (1, )`は，`,`ではなく`)`の位置12を報告する
- [x] `VALUES (1 +`は，文が途中で終わった`UnexpectedEnd`になる

### error

- [x] 各`SqlState`は，5文字のコードを持つ
- [x] `LexError`は，読めなかった文字を含むメッセージと位置を持つ構文エラーになる
- [x] `UnexpectedToken`は，トークンを表示したメッセージと位置を持つ構文エラーになる(文字列のトークンは引用符を付けて表示する)
- [x] `UnexpectedEnd`は，位置のない`syntax error at end of input`になる
- [x] 評価のエラーは，それぞれのSQLSTATEになる
- [x] `Display`は，メッセージを表示する

## 結合テスト

### tokenize

- [x] トークンの期待値に位置を加える
- [x] `LexError`の期待値に`found`を加える

### values

- [x] エラーの期待値を，`Error`のSQLSTATE，位置，メッセージの比較に変える
- [x] `VALUES (1 +)`は，SQLSTATE`42601`，位置12，メッセージ`syntax error at or near ")"`になる
- [x] `VALUES (1 +`は，位置のない`syntax error at end of input`になる
- [x] `VALUES ('日本語' ? 1)`は，位置15の構文エラーになる
- [x] `VALUES (2147483647 + 1)`は，`22003`と`integer out of range`になる
- [x] `VALUES (1 / 0)`は，`22012`と`division by zero`になる
- [x] `VALUES (1 + TRUE)`は，位置のない`42804`になる
