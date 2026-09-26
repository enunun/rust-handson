# Iteration 1：算術式の構文解析と評価(解説)

演習の各手順の模範解答と，その考え方を説明する．

## 1-1 準備

引き継いだ13個のテスト(単体テスト11個，結合テスト2個)がすべて通れば準備は終わりである．

## 1-2 文法と概念

課題の解答例である．

```rust
pub enum Temperature {
    Celsius(i64),
    Fahrenheit(i64),
}

pub fn to_celsius(t: &Temperature) -> i64 {
    match t {
        Temperature::Celsius(c) => *c,
        Temperature::Fahrenheit(f) => (*f - 32) * 5 / 9,
    }
}

pub enum Calc {
    Number(i64),
    Add(Box<Calc>, Box<Calc>),
    Mul(Box<Calc>, Box<Calc>),
}

pub fn calc(c: &Calc) -> i64 {
    match c {
        Calc::Number(n) => *n,
        Calc::Add(a, b) => calc(a) + calc(b),
        Calc::Mul(a, b) => calc(a) * calc(b),
    }
}

pub fn parse_and_double(text: &str) -> Result<i64, std::num::ParseIntError> {
    let n = text.parse::<i64>()?;
    Ok(n * 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temperature() {
        assert_eq!(to_celsius(&Temperature::Celsius(20)), 20);
        assert_eq!(to_celsius(&Temperature::Fahrenheit(212)), 100);
    }

    #[test]
    fn calculator() {
        let c = Calc::Mul(
            Box::new(Calc::Add(Box::new(Calc::Number(1)), Box::new(Calc::Number(2)))),
            Box::new(Calc::Number(3)),
        );
        assert_eq!(calc(&c), 9);
    }

    #[test]
    fn checked() {
        assert_eq!(65536_i32.checked_mul(32768), None);
        assert!(i32::try_from(3_000_000_000_i64).is_err());
    }

    #[test]
    fn question_mark() {
        assert_eq!(parse_and_double("21"), Ok(42));
        assert!(parse_and_double("x").is_err());
    }
}
```

課題2で`Kelvin(i64)`を加えると，`to_celsius`の`match`が次のエラーになる．
列挙子を加えると，その型を扱うすべての`match`をコンパイラーが指摘する．

```text
error[E0004]: non-exhaustive patterns: `&Temperature::Kelvin(_)` not covered
  --> src/lib.rs:8:11
   |
 8 |     match t {
```

## 1-3 テストリスト

模範解答は[TESTLIST.md](../TESTLIST.md)である．

- 構文解析は，1つの整数，式の並び，演算子の種類，左結合，優先順位，括弧，単項の`-`の順に並べた．前の項目のコードを少し広げれば次の項目が通る．
- 構文エラーには4つの項目を挙げた．式が欠けている場合，括弧の中が空の場合，括弧がない場合，文のあとにトークンが残る場合である．
- 評価は，整数，四則演算，単項の`-`，切り捨て，0での割り算，範囲外の順に並べた．範囲外は，リテラル，足し算，引き算，掛け算，単項の`-`，割り算の6つの経路がある．
- 結合テストは，使用例と，3つの段階のエラーが`Error`のどの列挙子になるかを確かめる．

## 1-4 設計ドキュメント

| ファイル | 変更 | 理由 |
| --- | --- | --- |
| `c4-context.md` | 開発者が渡すものを「SQLの文」，受け取るものを「結果の表」にした | `execute`が文を実行して表を返す |
| `c4-container.md` | ライブラリの役割と，呼ぶ関数を`execute`にした | 利用者が呼ぶ入口が`execute`になった |
| `c4-component.md` | `parser`，`ast`，`eval`，`value`と，その依存を加えた | `crate`が`lexer`，`parser`，`eval`を順に呼ぶ．`parser`は`ast`の型を作り，`eval`は`ast`を読んで`value`を作る |
| `code-types.md` | 構文木，値，エラー，結果の型を加えた | `Expr`は自分自身を持つ再帰的な直和型である．`Error`はどの段階のエラーかを列挙子で表す |
| `code-sequence.md` | `execute`が3つの段階を呼び，式ごとに評価する流れにした | 評価は式の数だけくり返し，各式の中では部分式を再帰的に評価する |

- `ast`と`value`は型だけを持つモジュールである．構文木と値を，それを作る処理から分けておくと，`parser`と`eval`が互いに依存しない．
- `parser`と`eval`の単体テストは入力を作るために`lexer`や`parser`を使う．照合スクリプトは単体テストを除いて依存を調べるので，これらの矢印は図に描かない．

## 1-5 テスト駆動の実装

### 準備：`Eq`の導出と空のモジュール

`literal`でトークンを比べるには，`Token`と`Keyword`が`Eq`を実装している必要がある．`src/token.rs`の2つの`derive`に`Eq`を加えた．

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
```

`src/ast.rs`，`src/parser.rs`，`src/value.rs`，`src/eval.rs`を作り，`lib.rs`で`mod`を宣言した．

### `VALUES (1)`からは，整数1を1つ持つ`Values`を返す

```rust
fn parse_sql(sql: &str) -> Result<Values, ParseError> {
    parse(&tokenize(sql).unwrap())
}

fn int(n: i64) -> Expr {
    Expr::Integer(n)
}

#[test]
fn values_with_one_integer() {
    assert_eq!(parse_sql("VALUES (1)"), Ok(Values { exprs: vec![int(1)] }));
}
```

`src/ast.rs`に構文木の型を定義した．このテストで使うのは`Expr::Integer`と`Values`だけである．

```rust
#[derive(Debug, PartialEq)]
pub enum Expr {
    Integer(i64),
}

#[derive(Debug, PartialEq)]
pub struct Values {
    pub exprs: Vec<Expr>,
}
```

構文解析器は，`VALUES`，左括弧，コンマで区切った式，右括弧を順に読む．式はまだ整数だけである．

```rust
use winnow::Parser;
use winnow::combinator::{delimited, preceded, separated};
use winnow::stream::TokenSlice;
use winnow::token::{any, literal};

use crate::ast::{Expr, Values};
use crate::token::{Keyword, Token};

#[derive(Debug, PartialEq)]
pub struct ParseError;

type Tokens<'t> = TokenSlice<'t, Token>;

pub fn parse(tokens: &[Token]) -> Result<Values, ParseError> {
    match values.parse(TokenSlice::new(tokens)) {
        Ok(values) => Ok(values),
        Err(_) => Err(ParseError),
    }
}

fn values(input: &mut Tokens<'_>) -> winnow::Result<Values> {
    preceded(
        literal(Token::Keyword(Keyword::Values)),
        delimited(
            literal(Token::LParen),
            separated(1.., expr, literal(Token::Comma)),
            literal(Token::RParen),
        ),
    )
    .map(to_values)
    .parse_next(input)
}

fn to_values(exprs: Vec<Expr>) -> Values {
    Values { exprs }
}

fn expr(input: &mut Tokens<'_>) -> winnow::Result<Expr> {
    any.verify_map(integer).parse_next(input)
}

fn integer(token: &Token) -> Option<Expr> {
    match token {
        Token::Integer(n) => Some(Expr::Integer(*n)),
        _ => None,
    }
}
```

### `VALUES (1, 2, 3)`からは，3つの式を順に持つ`Values`を返す

```rust
#[test]
fn values_with_several_expressions() {
    assert_eq!(
        parse_sql("VALUES (1, 2, 3)"),
        Ok(Values {
            exprs: vec![int(1), int(2), int(3)]
        })
    );
}
```

`separated(1.., ...)`で並びを読んでいるので，書いた時点で通る．

### `+`，`-`，`*`，`/`は，それぞれの二項演算の構文木になる

```rust
#[test]
fn each_binary_operator() {
    assert_eq!(
        parse_sql("VALUES (1 + 2, 1 - 2, 1 * 2, 1 / 2)"),
        Ok(Values {
            exprs: vec![
                binary(BinaryOp::Add, int(1), int(2)),
                binary(BinaryOp::Sub, int(1), int(2)),
                binary(BinaryOp::Mul, int(1), int(2)),
                binary(BinaryOp::Div, int(1), int(2)),
            ]
        })
    );
}
```

`BinaryOp`と`binary`がないので，コンパイルエラーになる．

```text
error[E0433]: cannot find type `BinaryOp` in this scope
```

`Expr`に`Binary`を加え，`expr`を`expression`で書き直した．
すべての演算子を同じ強さの左結合にしておく．`operand`は整数だけを読む．

```rust
#[derive(Debug, PartialEq)]
pub enum Expr {
    Integer(i64),
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
}
```

```rust
const ADDITIVE: i64 = 10;

fn expr(input: &mut Tokens<'_>) -> winnow::Result<Expr> {
    expression(operand)
        .infix(alt((
            literal(Token::Plus).value(Infix::Left(ADDITIVE, add)),
            literal(Token::Minus).value(Infix::Left(ADDITIVE, subtract)),
            literal(Token::Star).value(Infix::Left(ADDITIVE, multiply)),
            literal(Token::Slash).value(Infix::Left(ADDITIVE, divide)),
        )))
        .parse_next(input)
}

fn operand(input: &mut Tokens<'_>) -> winnow::Result<Expr> {
    any.verify_map(integer).parse_next(input)
}

fn add(_: &mut Tokens<'_>, left: Expr, right: Expr) -> winnow::Result<Expr> {
    Ok(binary(BinaryOp::Add, left, right))
}

// subtract，multiply，divide も同じ形で，BinaryOp の列挙子だけが違う．

fn binary(op: BinaryOp, left: Expr, right: Expr) -> Expr {
    Expr::Binary {
        op,
        left: Box::new(left),
        right: Box::new(right),
    }
}
```

`Infix::Left`に渡す関数は関数ポインター(`fn`の型)なので，演算子ごとに名前の付いた関数を用意した．
共通の部分は`binary`にまとめた．

### `1 - 2 - 3`は`(1 - 2) - 3`になる(左結合)

```rust
#[test]
fn binary_operators_are_left_associative() {
    assert_eq!(
        parse_sql("VALUES (1 - 2 - 3)"),
        Ok(Values {
            exprs: vec![binary(
                BinaryOp::Sub,
                binary(BinaryOp::Sub, int(1), int(2)),
                int(3)
            )]
        })
    );
}
```

`Infix::Left`で左結合にしているので，書いた時点で通る．

### `1 + 2 * 3`は`1 + (2 * 3)`になる(乗算が先)

```rust
#[test]
fn multiplication_binds_tighter_than_addition() {
    assert_eq!(
        parse_sql("VALUES (1 + 2 * 3)"),
        Ok(Values {
            exprs: vec![binary(
                BinaryOp::Add,
                int(1),
                binary(BinaryOp::Mul, int(2), int(3))
            )]
        })
    );
}
```

すべての演算子が同じ強さなので，`(1 + 2) * 3`になって失敗する．
乗除算に，加減算より大きい強さを与えた．

```rust
const ADDITIVE: i64 = 10;
const MULTIPLICATIVE: i64 = 20;

            literal(Token::Star).value(Infix::Left(MULTIPLICATIVE, multiply)),
            literal(Token::Slash).value(Infix::Left(MULTIPLICATIVE, divide)),
```

### `(1 + 2) * 3`は括弧の中が先になる

```rust
#[test]
fn parentheses_group_an_expression() {
    assert_eq!(
        parse_sql("VALUES ((1 + 2) * 3)"),
        Ok(Values {
            exprs: vec![binary(
                BinaryOp::Mul,
                binary(BinaryOp::Add, int(1), int(2)),
                int(3)
            )]
        })
    );
}
```

`operand`に，括弧で囲んだ式を加えた．括弧の中は`expr`自身で読む．

```rust
fn operand(input: &mut Tokens<'_>) -> winnow::Result<Expr> {
    alt((
        any.verify_map(integer),
        delimited(literal(Token::LParen), expr, literal(Token::RParen)),
    ))
    .parse_next(input)
}
```

### `-2 * 3`は`(-2) * 3`になる(単項の`-`が先)

```rust
#[test]
fn unary_minus_binds_tighter_than_multiplication() {
    assert_eq!(
        parse_sql("VALUES (-2 * 3)"),
        Ok(Values {
            exprs: vec![binary(
                BinaryOp::Mul,
                Expr::Unary {
                    op: UnaryOp::Neg,
                    operand: Box::new(int(2))
                },
                int(3)
            )]
        })
    );
}
```

`Expr`に`Unary`と`UnaryOp`を加え，前置演算子を`prefix`で読む．強さは乗除算より大きくした．

```rust
pub enum Expr {
    Integer(i64),
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    Binary { .. },
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Neg,
}
```

```rust
const UNARY: i64 = 30;

    expression(operand)
        .prefix(literal(Token::Minus).value(Prefix(UNARY, negate)))
        .infix(alt((..)))

fn negate(_: &mut Tokens<'_>, operand: Expr) -> winnow::Result<Expr> {
    Ok(Expr::Unary {
        op: UnaryOp::Neg,
        operand: Box::new(operand),
    })
}
```

`UnaryOp`の列挙子は今`Neg`だけで，Iteration 2で`NOT`が加わる．列挙子が1つの`enum`にしておくと，単項演算子の種類が増えても`Expr`の形は変わらない．

### 構文エラーの4項目

```rust
#[test]
fn missing_operand_is_an_error() {
    assert_eq!(parse_sql("VALUES (1 +)"), Err(ParseError));
}

#[test]
fn empty_parentheses_are_an_error() {
    assert_eq!(parse_sql("VALUES ()"), Err(ParseError));
}

#[test]
fn values_without_parentheses_is_an_error() {
    assert_eq!(parse_sql("VALUES 1"), Err(ParseError));
}

#[test]
fn tokens_after_the_statement_are_an_error() {
    assert_eq!(parse_sql("VALUES (1) 2"), Err(ParseError));
}
```

4つとも書いた時点で通る．`separated(1.., ...)`は式を1つ以上要求し，`parse`は入力全体を読めなければ失敗する．
エラーの項目は，実装の変更を必要としなくても，文法のどの誤りを検出するかを示す仕様として残す．

### 整数`7`は，値`7`になる

```rust
fn eval_sql(expr: &str) -> Result<Value, EvalError> {
    let values = parse(&tokenize(&format!("VALUES ({expr})")).unwrap()).unwrap();
    eval(&values.exprs[0])
}

#[test]
fn integer_literal() {
    assert_eq!(eval_sql("7"), Ok(Value::Integer(7)));
}
```

`src/value.rs`に`Value`を，`src/eval.rs`に`EvalError`と`eval`を書いた．
構文木の整数は`i64`，値は`i32`なので，`i32::try_from`で変換する．変換できないときの列挙子`NumericOutOfRange`も，ここで定義した．

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Integer(i32),
}
```

```rust
#[derive(Debug, PartialEq)]
pub enum EvalError {
    NumericOutOfRange,
}

pub fn eval(expr: &Expr) -> Result<Value, EvalError> {
    match expr {
        Expr::Integer(n) => match i32::try_from(*n) {
            Ok(n) => Ok(Value::Integer(n)),
            Err(_) => Err(EvalError::NumericOutOfRange),
        },
        Expr::Unary { .. } => Err(EvalError::NumericOutOfRange),
        Expr::Binary { .. } => Err(EvalError::NumericOutOfRange),
    }
}
```

`match`は`Expr`のすべての列挙子を扱わなければならない．まだテストのない`Unary`と`Binary`は，仮にエラーを返す．`{ .. }`は，フィールドを取り出さずに形だけを調べるパターンである．

### 四則演算

```rust
#[test]
fn four_arithmetic_operations() {
    assert_eq!(eval_sql("6 + 3"), Ok(Value::Integer(9)));
    assert_eq!(eval_sql("6 - 3"), Ok(Value::Integer(3)));
    assert_eq!(eval_sql("6 * 3"), Ok(Value::Integer(18)));
    assert_eq!(eval_sql("6 / 3"), Ok(Value::Integer(2)));
}
```

左辺と右辺を`eval`自身で評価し，`?`で`Err`をそのまま返す．計算は`eval_binary`に分けた．
`Value`の列挙子は`Integer`だけなので，`let`で分解できる．

```rust
        Expr::Binary { op, left, right } => {
            let left = eval(left)?;
            let right = eval(right)?;
            eval_binary(op, left, right)
        }

fn eval_binary(op: &BinaryOp, left: Value, right: Value) -> Result<Value, EvalError> {
    let Value::Integer(a) = left;
    let Value::Integer(b) = right;
    let result = match op {
        BinaryOp::Add => a + b,
        BinaryOp::Sub => a - b,
        BinaryOp::Mul => a * b,
        BinaryOp::Div => a / b,
    };
    Ok(Value::Integer(result))
}
```

### `-(4 - 6)`は`2`になる

```rust
#[test]
fn unary_minus() {
    assert_eq!(eval_sql("-(4 - 6)"), Ok(Value::Integer(2)));
}
```

```rust
        Expr::Unary { op, operand } => {
            let value = eval(operand)?;
            eval_unary(op, value)
        }

fn eval_unary(op: &UnaryOp, value: Value) -> Result<Value, EvalError> {
    let Value::Integer(n) = value;
    let result = match op {
        UnaryOp::Neg => -n,
    };
    Ok(Value::Integer(result))
}
```

### 切り捨て

```rust
#[test]
fn division_truncates_toward_zero() {
    assert_eq!(eval_sql("7 / 2"), Ok(Value::Integer(3)));
    assert_eq!(eval_sql("-7 / 2"), Ok(Value::Integer(-3)));
}
```

Rustの整数の`/`は0の方向に切り捨てるので，書いた時点で通る．

### `1 / 0`は，0での割り算のエラーになる

```rust
#[test]
fn division_by_zero_is_an_error() {
    assert_eq!(eval_sql("1 / 0"), Err(EvalError::DivisionByZero));
}
```

`EvalError`に`DivisionByZero`を加え，割る前に右辺が0かを調べる．

```rust
        BinaryOp::Div => {
            if b == 0 {
                return Err(EvalError::DivisionByZero);
            }
            a / b
        }
```

### 範囲外の6項目

```rust
#[test]
fn literal_beyond_integer_is_out_of_range() {
    assert_eq!(eval_sql("2147483647"), Ok(Value::Integer(i32::MAX)));
    assert_eq!(eval_sql("2147483648"), Err(EvalError::NumericOutOfRange));
}

#[test]
fn overflowing_addition_is_out_of_range() {
    assert_eq!(eval_sql("2147483647 + 1"), Err(EvalError::NumericOutOfRange));
}
```

リテラルの項目は，`try_from`で変換しているので書いた時点で通る．
足し算の項目は，`a + b`が範囲を超えてパニックする．

```text
attempt to add with overflow
```

演算を検査付きの`checked_add`などに変え，`None`を範囲外のエラーにする．
引き算，掛け算，単項の`-`，割り算(`-2147483648 / -1`)の項目も，同じ変更で通る．

```rust
fn eval_binary(op: &BinaryOp, left: Value, right: Value) -> Result<Value, EvalError> {
    let Value::Integer(a) = left;
    let Value::Integer(b) = right;
    let result = match op {
        BinaryOp::Add => a.checked_add(b),
        BinaryOp::Sub => a.checked_sub(b),
        BinaryOp::Mul => a.checked_mul(b),
        BinaryOp::Div => {
            if b == 0 {
                return Err(EvalError::DivisionByZero);
            }
            a.checked_div(b)
        }
    };
    integer_or_out_of_range(result)
}

fn integer_or_out_of_range(result: Option<i32>) -> Result<Value, EvalError> {
    match result {
        Some(n) => Ok(Value::Integer(n)),
        None => Err(EvalError::NumericOutOfRange),
    }
}
```

`eval_unary`も`n.checked_neg()`と`integer_or_out_of_range`を使うように変えた．

### 結合テスト

`tests/values.rs`に，使用例と3つの段階のエラーを書いた．

```rust
use ferrodb::{Error, EvalError, LexError, ParseError, Value, execute};

#[test]
fn evaluates_each_expression_of_a_row() {
    let result = execute("VALUES (1 + 2 * 3, -(4 - 6) / 2)").unwrap();
    assert_eq!(result.columns, vec!["COLUMN1", "COLUMN2"]);
    assert_eq!(
        result.rows,
        vec![vec![Value::Integer(7), Value::Integer(1)]]
    );
}

#[test]
fn reports_a_lexical_error() {
    assert_eq!(
        execute("VALUES (1 ? 2)"),
        Err(Error::Lex(LexError { position: 11 }))
    );
}

#[test]
fn reports_a_syntax_error() {
    assert_eq!(execute("VALUES (1 +)"), Err(Error::Parse(ParseError)));
}

#[test]
fn reports_division_by_zero() {
    assert_eq!(
        execute("VALUES (1 / 0)"),
        Err(Error::Eval(EvalError::DivisionByZero))
    );
}
```

`lib.rs`に`Error`，`QueryResult`，`execute`を書き，テストで使う名前を`pub use`で公開した．
各段階のエラーは，`map_err`で`Error`の列挙子に包んでから`?`で返す．
列名は，列を1つ加えるたびに`columns.len() + 1`で番号を振る．

```rust
#[derive(Debug, PartialEq)]
pub enum Error {
    Lex(LexError),
    Parse(ParseError),
    Eval(EvalError),
}

#[derive(Debug, PartialEq)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

pub fn execute(sql: &str) -> Result<QueryResult, Error> {
    let tokens = tokenize(sql).map_err(Error::Lex)?;
    let values = parser::parse(&tokens).map_err(Error::Parse)?;
    let mut columns = Vec::new();
    let mut row = Vec::new();
    for expr in &values.exprs {
        columns.push(format!("COLUMN{}", columns.len() + 1));
        row.push(eval::eval(expr).map_err(Error::Eval)?);
    }
    Ok(QueryResult {
        columns,
        rows: vec![row],
    })
}
```

## 1-6 振り返り

1. 範囲外になる経路は，リテラル，足し算，引き算，掛け算，単項の`-`，割り算の6つである．`i32`の最小値を`-1`で割ると，結果の`2147483648`が範囲を超える．
2. 種類を表す文字列と`Option`のフィールドを持つ`struct`では，「`kind`が`"add"`なのに`left`が`None`」のような値が作れる．`eval`は`kind`の文字列を比べて分岐し，使うフィールドが`Some`かどうかを毎回調べることになる．未知の`kind`や`None`のときの処理も要る．`enum`では，こうした値がそもそも作れないので，`match`で列挙子ごとに必要なフィールドを取り出すだけで済む．列挙子を加えたときに，直すべき`match`をコンパイラーが教えてくれる．
3. エラーを1つの文字列にすると，テストは文面を比べるしかなく，文面を変えるとテストが壊れる．利用者は，構文の誤りか計算の誤りかを文字列から読み取らなければならない．列挙子なら，`match`で段階ごとに処理を分けられる．
4. `eval`の単体テストは，1つの式の評価の規則(切り捨て，範囲外など)を細かく確かめる．`execute`の結合テストは，字句解析から評価までがつながり，各段階のエラーが正しい列挙子で返ることを確かめる．
5. 模範解答の設計ドキュメントは，照合スクリプトで問題がないことを確かめてある．

## 1-7 発展課題

解答例である．`UnaryOp`に`Plus`を加えると，`eval_unary`の`match`がコンパイルエラーになり，直す場所が分かる．

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Plus,
    Neg,
}
```

```rust
        .prefix(alt((
            literal(Token::Plus).value(Prefix(UNARY, identity)),
            literal(Token::Minus).value(Prefix(UNARY, negate)),
        )))

fn identity(_: &mut Tokens<'_>, operand: Expr) -> winnow::Result<Expr> {
    Ok(Expr::Unary {
        op: UnaryOp::Plus,
        operand: Box::new(operand),
    })
}
```

```rust
    let result = match op {
        UnaryOp::Plus => Some(n),
        UnaryOp::Neg => n.checked_neg(),
    };
```

```rust
#[test]
fn unary_plus() {
    assert_eq!(eval_sql("+5"), Ok(Value::Integer(5)));
    assert_eq!(eval_sql("-+5"), Ok(Value::Integer(-5)));
}
```
