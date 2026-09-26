# Iteration 1：算術式の構文解析と評価

このIterationでは，`VALUES (1 + 2 * 3, -4)`のような文を構文木にし，評価して結果の行を返す．
構文木を代数的データ型で表し，再帰的な`enum`と`match`で評価する．

## 1-1 準備

このディレクトリ(`iterations/iteration-01/exercise`)には，Iteration 0の模範解答と同じコードが入っている．
`cargo test`を実行し，引き継いだテストがすべて通ることを確かめる．

```console
$ cargo test
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.70s
     Running unittests src/lib.rs (target/debug/deps/ferrodb-fcec4bc47a054b29)

running 11 tests
test lexer::tests::arithmetic_operators ... ok
test lexer::tests::digits_become_an_integer ... ok
test lexer::tests::empty_input_gives_no_tokens ... ok
test lexer::tests::integer_beyond_i64_is_an_error ... ok
test lexer::tests::keywords_are_case_insensitive ... ok
test lexer::tests::largest_i64_is_an_integer ... ok
test lexer::tests::parentheses_and_comma ... ok
test lexer::tests::unknown_character_reports_its_position ... ok
test lexer::tests::unknown_word_reports_its_first_character ... ok
test lexer::tests::values_is_a_keyword ... ok
test lexer::tests::whitespace_and_newlines_are_skipped ... ok

test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/tokenize.rs (target/debug/deps/tokenize-8d7c29559361cdcd)

running 2 tests
test reports_the_position_of_an_unknown_character ... ok
test tokenizes_a_values_statement ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests ferrodb

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

## 1-2 文法と概念

次の2つのノートを読む．

- [Rustのノート](../../../../docs/rust/iteration-01.md)：代数的データ型，`match`による分解，再帰的な型と`Box`，検査付きの整数演算，`?`と`map_err`，winnowの`TokenSlice`と`expression`
- [データベースのノート](../../../../docs/db/iteration-01.md)：構文木，優先順位と結合性，`INTEGER`型

読み終えたら，一時的な`src/scratch.rs`を作って`lib.rs`に`mod scratch;`を加え，次の課題をテストで確かめる．確かめ終えたら両方を消す．

1. 摂氏`Celsius(i64)`と華氏`Fahrenheit(i64)`の列挙子を持つ`enum Temperature`と，摂氏の値を返す`fn to_celsius(t: &Temperature) -> i64`を書く．華氏212度が摂氏100度になることを確かめる．
2. 1の`Temperature`に列挙子`Kelvin(i64)`を加え，`cargo build`でどんなエラーが出るかを確かめる．
3. `Number(i64)`，`Add`，`Mul`の列挙子を持つ再帰的な`enum Calc`と，それを計算する`fn calc(c: &Calc) -> i64`を書く．`(1 + 2) * 3`を表す値を作り，`9`になることを確かめる．
4. `65536_i32.checked_mul(32768)`と`i32::try_from(3_000_000_000_i64)`の結果を確かめる．
5. 文字列を`i64`に変換して2倍する`fn parse_and_double(text: &str) -> Result<i64, std::num::ParseIntError>`を，`?`を使って書く．

## 1-3 テストリスト

作るものの要件と使用例を読み，`TESTLIST.md`に確かめるべき振る舞いを書き出す．

### 要件

- `VALUES (1 + 2 * 3, -4)`のように，1行の`VALUES`を構文解析して評価し，結果の行を返す．
- 演算子`+`，`-`，`*`，`/`と単項の`-`，括弧に対応する．優先順位は標準SQLに従う．
- 整数は`INTEGER`(32ビット符号付き)として計算する．範囲を超えたらエラーにする．
- `0`で割ったらエラーにする．整数の割り算は0の方向に切り捨てる．
- 結果の列名は`COLUMN1`，`COLUMN2`，…とする．

### 使用例

```rust
let result = ferrodb::execute("VALUES (1 + 2 * 3, -(4 - 6) / 2)").unwrap();
assert_eq!(result.columns, vec!["COLUMN1", "COLUMN2"]);
assert_eq!(result.rows, vec![vec![Value::Integer(7), Value::Integer(1)]]);
```

### 作るもの

| モジュール | 作るもの |
| --- | --- |
| `token` | `Token`と`Keyword`に`Eq`を導出する |
| `ast` | 式の構文木`enum Expr`，`enum BinaryOp`，`enum UnaryOp`，`VALUES`の構文木`struct Values` |
| `parser` | `pub fn parse(tokens: &[Token]) -> Result<Values, ParseError>`，`pub struct ParseError` |
| `value` | `pub enum Value { Integer(i32) }` |
| `eval` | `pub fn eval(expr: &Expr) -> Result<Value, EvalError>`，`pub enum EvalError` |
| ルート(`lib.rs`) | `pub fn execute(sql: &str) -> Result<QueryResult, Error>`，`pub struct QueryResult { pub columns: Vec<String>, pub rows: Vec<Vec<Value>> }`，`pub enum Error` |

### 書くときに考えること

- 構文解析と評価を分けてテストする．構文解析のテストは構文木を，評価のテストは値を確かめる．
- 優先順位と結合性は，それぞれを確かめる入力を1つずつ用意する．
- 範囲外になるのは足し算だけではない．引き算，掛け算，単項の`-`，割り算で範囲を超える入力をそれぞれ考える．
- どの段階(字句解析，構文解析，評価)のエラーかを，`execute`の結果で区別できるか．
- Iteration 0のテストの期待値は変わらない．

## 1-4 設計ドキュメント

`design/`の図を，このIterationの終わりの状態に更新する．

- `c4-context.md`と`c4-container.md`：`ferrodb`が受け取るもの，返すものが変わる．
- `c4-component.md`：「作るもの」のモジュールを加える．どのモジュールが，どのモジュールの型や関数を使うか．
- `code-types.md`：構文木は，どの型がどの型を持つか．自分自身を持つ型はどれか．エラーの型は，どの段階のエラーを持つか．
- `code-sequence.md`：`execute`が，どのモジュールの関数をどの順に呼ぶか．どこでくり返すか．

描き終えたら，Mermaidの構文を検査する．

## 1-5 テスト駆動の実装

`TESTLIST.md`の項目を上から1つずつ，Red → Green → Refactorで実装する．

### 実装のヒント

- 構文解析の単体テストでは，`tokenize`でトークンの列を作ってから`parse`に渡すと，入力を短く書ける．
- `parse`の中では，`TokenSlice::new(tokens)`で入力を作り，`parse`で入力全体を読む．
- `VALUES`，括弧，コンマは`literal`で読む．コンマで区切った式の並びは`separated(1.., 式, コンマ)`で読む．
- 整数のトークンは`any.verify_map(関数)`で読む．関数は`&Token`を受け取り，`Token::Integer`なら`Some(Expr::Integer(..))`を返す．
- 式は`expression(operand)`で読む．`operand`は整数か，括弧で囲んだ式である．括弧の中の式を読むには，式を読む関数自身を呼ぶ．
- 演算子の強さは`const`で定義すると，優先順位の表として読める．
- 評価は`match`で`Expr`の列挙子ごとに分ける．部分式は`eval`自身を呼んで評価する．
- 範囲外の検出には`checked_add`などを使う．0での割り算は，割る前に調べる．
- `execute`では，各段階のエラーを`map_err`で`Error`の列挙子に包み，`?`で返す．

### ツールの操作

- 構文解析のテストだけを実行するには，テストの名前に含まれる`parser`で絞り込む．
- `cargo clippy`は，`?`で書ける`match`などを指摘する．指摘に従って直す．

## 1-6 振り返り

1. 自分の`TESTLIST.md`を[模範解答のテストリスト](../../solution/TESTLIST.md)と比べる．範囲外になる演算を，いくつ挙げられたか．
2. `Expr`を「種類を表す文字列と，`Option`の左辺，右辺，整数を持つ`struct`」にした場合と比べる．どんな不正な値が作れるようになるか．`eval`はどう変わるか．
3. `Error`は`Lex`，`Parse`，`Eval`の3つの列挙子を持つ．エラーを1つの文字列にした場合と比べて，テストと利用者にとって何が違うか．
4. `eval`の単体テストと，`execute`の結合テストは，それぞれ何を確かめているか．
5. 設計ドキュメントと実装を見比べ，違うところがあれば設計ドキュメントを直す．

## 1-7 発展課題

単項の`+`(`VALUES (+5)`)に対応する．`+5`は`5`と同じ値になる．
これまでと同じく，テストリスト，設計ドキュメント，実装の順に進める．
`UnaryOp`に列挙子を加えたとき，コンパイラーがどこを直すよう求めるかを確かめる．
