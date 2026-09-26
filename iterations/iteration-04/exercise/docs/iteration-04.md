# Iteration 4：SQLSTATEとエラー位置

このIterationでは，字句解析，構文解析，評価のエラーを1つの型`Error`にまとめ，SQLSTATE，メッセージ，文の中の位置を返す．
エラー型をどう設計するかを考え，`From`，`Display`などのトレイトを実装する．

## 4-1 準備

`cargo test`を実行し，引き継いだテストがすべて通ることを確かめる．次は結果の要約である．

```text
test result: ok. 76 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

## 4-2 文法と概念

次の2つのノートを読む．

- [Rustのノート](../../../../docs/rust/iteration-04.md)：エラー型の設計，`Display`，`std::error::Error`，`From`と`?`，非公開のフィールド，ジェネリックな構造体，winnowの`LocatingSlice`と`cut_err`
- [データベースのノート](../../../../docs/db/iteration-04.md)：SQLSTATE，エラーの位置，バックトラック

読み終えたら，一時的なモジュールにテストを書いて，次の課題を確かめる．

1. 設定のエラーを表す`enum ConfigError`を作る．キーがない`Missing { key }`と，値が不正な`Invalid { key, value }`の2つの列挙子を持つ．`Display`を実装し，`missing key: port`，`invalid value for port: x`と表示されることを確かめる．
2. `ConfigError`に`std::error::Error`を実装する．
3. 理由の文字列を持つ`struct PortError { reason: String }`に`From<std::num::ParseIntError>`を実装し，文字列を`u16`に変換する関数`port`を`?`だけで書く．`"70000"`を渡したときの理由の文字列を確かめる．

## 4-3 テストリスト

### 要件

- すべてのエラーを1つの型`Error`にまとめる．エラーはSQLSTATE，メッセージ，SQL文中の位置(分かる場合)を持つ．
- 構文エラーは`42601`，数値の範囲外は`22003`，0での割り算は`22012`，型の不一致は`42804`を返す．
- 構文エラーの位置は，問題のあるトークンの先頭の文字位置(1から数える)とする．文が途中で終わっていれば，位置のない`syntax error at end of input`とする．
- 括弧の中や`,`のあとの誤りでは，括弧や`,`ではなく，誤りのあるトークンの位置を返す．

### 使用例

```rust
let err = ferrodb::execute("VALUES (1 +)").unwrap_err();
assert_eq!(err.sqlstate(), SqlState::SyntaxError);
assert_eq!(err.sqlstate().code(), "42601");
assert_eq!(err.position(), Some(12));
assert_eq!(err.to_string(), "syntax error at or near \")\"");
```

### 作るもの

| モジュール | 作るもの |
| --- | --- |
| `error` | `pub struct Error`(`sqlstate()`，`message()`，`position()`)，`pub enum SqlState`(`code()`)，`Display`と`std::error::Error`の実装，`LexError`，`ParseError`，`EvalError`からの`From` |
| `token` | `pub struct Spanned<T> { pub value: T, pub position: usize }`，`impl PartialEq<Token> for Spanned<Token>`，`impl Display for Token` |
| `lexer` | `tokenize`が`Result<Vec<Spanned<Token>>, LexError>`を返す．`LexError`に読めなかった文字`found: char`を加える |
| `parser` | `pub enum ParseError { UnexpectedToken { token: Token, position: usize }, UnexpectedEnd }` |
| ルート(`lib.rs`) | `execute`が`Result<QueryResult, Error>`を返す．`Error`の列挙型をなくす |

### 書くときに考えること

- このIterationでは，既存のテストの期待値が大きく変わる．どのテストのどの部分が変わるかを，テストリストの最初に書き出す．
- 字句解析の既存のテストは，トークンの位置まで比べる必要があるか．位置は，位置を確かめる項目だけで比べればよい．
- エラーの位置が括弧や`,`の位置にずれる入力はどれか．
- `Error`に変換する規則(SQLSTATE，メッセージ，位置)は，`error`モジュールの単体テストで確かめられる．

## 4-4 設計ドキュメント

- `c4-component.md`：`error`モジュールを加える．`From`の実装はどのモジュールの型を読むか．依存の向きに注意する．
- `code-types.md`：`Error`，`SqlState`，`Spanned`を加え，`ParseError`を列挙型にする．`Error`のフィールドは非公開であることを示す．
- `code-sequence.md`：どこかの段階が失敗したときに，エラーが`Error`に変わる流れを加える．

## 4-5 テスト駆動の実装

### 実装のヒント

- 字句解析の入力を`LocatingSlice<&str>`にすると，`token.with_span()`でトークンのバイトの範囲が得られる．パーサー関数の引数の型は，型の別名にしておくと書き換えが楽になる．
- バイトの範囲の先頭を，Iteration 3と同じ方法で文字の位置に直す．
- 構文解析のエラーの`offset()`は，読めなかったトークンの番号である．`tokens.get(番号)`が`None`なら，文が途中で終わっている．
- winnowの`literal(Token::Comma)`で`Spanned<Token>`の列を読むには，`Spanned<Token>`と`Token`を比べられるようにする．
- 誤りの位置がずれる場所では，`cut_err`でバックトラックを止める．`cut_err`を使うと，パーサー関数の戻り値を`ModalResult`に変える必要がある．
- `Error`のフィールドを非公開にし，読むためのメソッドを用意する．テストは`sqlstate()`，`position()`，`to_string()`で比べる．
- `execute`の`map_err`は，`From`を実装すれば`?`だけにできる．

### ツールの操作

- 期待値の変わるテストを先に直し，すべて通る状態にしてから新しい項目を加える．
- `cargo test`のコンパイルエラーが多いときは，最初のエラーから直す．後ろのエラーは，最初のエラーの影響であることが多い．

## 4-6 振り返り

1. 自分の`TESTLIST.md`を[模範解答のテストリスト](../../solution/TESTLIST.md)と比べる．既存テストの変更を，どこまで挙げられたか．
2. `ParseError`を`struct ParseError { token: Option<Token>, position: Option<usize> }`にした場合と比べる．どんな値が作れてしまうか．
3. `Error`を，Iteration 1の`enum Error { Lex(..), Parse(..), Eval(..) }`のままにして，SQLSTATEを返すメソッドを加える設計と比べる．利用者のコードとテストはどう変わるか．
4. `Error`のフィールドを`pub`にしなかった理由は何か．
5. `From`の実装を`error`モジュールに置いたので，`error`が各段階のモジュールに依存する．逆に，各段階のモジュールが`Error`を直接返す設計と比べる．
6. 設計ドキュメントと実装を見比べ，違うところがあれば設計ドキュメントを直す．

## 4-7 発展課題

`Error`に，位置を行と列(どちらも1から数える)に直すメソッド`line_and_column(&self, sql: &str) -> Option<(usize, usize)>`を加える．
改行を含む`"VALUES (1,\n  1 +)"`の構文エラーは，2行目の6列目になる．位置のないエラーは`None`を返す．
これまでと同じく，テストリスト，設計ドキュメント，実装の順に進める．
