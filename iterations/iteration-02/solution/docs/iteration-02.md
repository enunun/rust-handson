# Iteration 2：真偽値，比較，NULLと3値論理(解説)

演習の各手順の模範解答と，その考え方を説明する．

## 2-1 準備

引き継いだ39個のテストがすべて通れば準備は終わりである．

## 2-2 文法と概念

課題の解答例である．

```rust
use std::cmp::Ordering;

#[derive(Debug, PartialEq)]
pub struct Rectangle {
    width: i64,
    height: i64,
}

impl Rectangle {
    pub fn new(width: i64, height: i64) -> Rectangle {
        Rectangle { width, height }
    }

    pub fn area(&self) -> i64 {
        self.width * self.height
    }

    pub fn scale(&mut self, factor: i64) {
        self.width *= factor;
        self.height *= factor;
    }
}

pub fn either(a: Option<bool>, b: Option<bool>) -> Option<bool> {
    match (a, b) {
        (Some(true), _) | (_, Some(true)) => Some(true),
        (Some(false), Some(false)) => Some(false),
        _ => None,
    }
}

pub enum Animal {
    Dog,
    Cat,
    Bird,
}

pub fn legs(animal: &Animal) -> i64 {
    match animal {
        Animal::Dog | Animal::Cat => 4,
        Animal::Bird => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle() {
        let mut r = Rectangle::new(2, 3);
        assert_eq!(r.area(), 6);
        r.scale(2);
        assert_eq!(r.area(), 24);
    }

    #[test]
    fn three_valued_or() {
        assert_eq!(either(Some(false), None), None);
        assert_eq!(either(Some(true), None), Some(true));
        assert_eq!(either(None, None), None);
        assert_eq!(either(Some(false), Some(false)), Some(false));
    }

    #[test]
    fn animals() {
        assert_eq!(legs(&Animal::Cat), 4);
        assert_eq!(legs(&Animal::Bird), 2);
    }

    #[test]
    fn ordering() {
        assert_eq!(3.cmp(&5), Ordering::Less);
        assert_eq!(true.cmp(&false), Ordering::Greater);
    }
}
```

`Rectangle { width, height }`は，変数名とフィールド名が同じときの省略記法である．

## 2-3 テストリスト

模範解答は[TESTLIST.md](../TESTLIST.md)である．

- 最初の項目は，`BinaryOp`を分けることによる既存テストの変更である．型を先に変えてから，新しい振る舞いを加える．
- 字句解析は，比較演算子，2文字の演算子，キーワードの3項目にした．`<>1`は，`<`と`>`に分かれていないことを確かめる入力である．
- 構文解析は，定数，比較演算子の種類，隣り合う強さの組(算術と比較，比較どうし，`AND`と`OR`，`NOT`と比較，`NOT`と`AND`，`IS NULL`と算術)の順に並べた．
- 評価は，定数，比較，`NULL`との比較，`AND`，`OR`，`NOT`，`IS NULL`，`NULL`を含む算術，型の不一致の順に並べた．
- `NULL / 0`は，0での割り算より`NULL`の伝播が先であることを確かめる．PostgreSQLも`NULL`を返す．

## 2-4 設計ドキュメント

| ファイル | 変更 | 理由 |
| --- | --- | --- |
| `c4-component.md` | 変更なし | 新しいモジュールはなく，依存も変わらない |
| `code-types.md` | `Token`，`Keyword`，`Expr`，`UnaryOp`，`Value`，`EvalError`に列挙子を加えた．`BinaryOp`を`ArithmeticOp`と`ComparisonOp`に分けた．`Value`のメソッドと関連関数を書いた | 演算子を種類ごとの型に分けると，算術演算を評価する関数が算術演算子だけを受け取れる |
| `code-sequence.md` | 図の下に，優先順位と3値論理の扱いを書いた | 呼び出しの順序は変わらない．評価の規則が増えた |

## 2-5 テスト駆動の実装

項目ごとに，追加したテストと，テストを通すために加えたコードを示す．

### `BinaryOp`を分ける

`BinaryOp`を種類ごとの型に分けた．

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    Arithmetic(ArithmeticOp),
    Comparison(ComparisonOp),
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ArithmeticOp {
    Add,
    Sub,
    Mul,
    Div,
}
```

コンパイラーが，`BinaryOp::Add`を使っている場所をすべてエラーにする．構文解析器と`eval`を直し，構文解析のテストの期待値も直した．
テストでは，構文木を作る補助関数`arithmetic`を使う．

```rust
fn arithmetic(op: ArithmeticOp, left: Expr, right: Expr) -> Expr {
    binary(BinaryOp::Arithmetic(op), left, right)
}
```

```rust
// 変更前
binary(BinaryOp::Add, int(1), int(2)),
// 変更後
arithmetic(ArithmeticOp::Add, int(1), int(2)),
```

### 字句解析の3項目

```rust
#[test]
fn comparison_operators() {
    assert_eq!(
        tokenize("= <> < <= > >="),
        Ok(vec![
            Token::Eq,
            Token::NotEq,
            Token::Lt,
            Token::LtEq,
            Token::Gt,
            Token::GtEq
        ])
    );
}

#[test]
fn two_character_operators_are_read_as_one_token() {
    assert_eq!(tokenize("<>1"), Ok(vec![Token::NotEq, Token::Integer(1)]));
    assert_eq!(tokenize("<=1"), Ok(vec![Token::LtEq, Token::Integer(1)]));
}
```

比較演算子を`symbol`の`alt`に加えると，選択肢が13個になり，次のエラーになった．

```text
error[E0277]: the trait bound `(..., ..., ..., ..., ..., ..., ..., ..., ..., ..., ..., ..., ...): Alt<_, _, _>` is not satisfied
```

比較演算子を別の関数`comparison`に分けた．2文字の演算子を先に並べる．

```rust
fn token(input: &mut &str) -> winnow::Result<Token> {
    alt((keyword, integer, comparison, symbol)).parse_next(input)
}

fn comparison(input: &mut &str) -> winnow::Result<Token> {
    alt((
        '='.value(Token::Eq),
        "<>".value(Token::NotEq),
        "<=".value(Token::LtEq),
        '<'.value(Token::Lt),
        ">=".value(Token::GtEq),
        '>'.value(Token::Gt),
    ))
    .parse_next(input)
}
```

キーワードの項目では，`to_keyword`をキーワードの表に書き換えた．

```rust
fn to_keyword(word: &str) -> Option<Keyword> {
    match word.to_ascii_uppercase().as_str() {
        "VALUES" => Some(Keyword::Values),
        "TRUE" => Some(Keyword::True),
        "FALSE" => Some(Keyword::False),
        "UNKNOWN" => Some(Keyword::Unknown),
        "NULL" => Some(Keyword::Null),
        "AND" => Some(Keyword::And),
        "OR" => Some(Keyword::Or),
        "NOT" => Some(Keyword::Not),
        "IS" => Some(Keyword::Is),
        _ => None,
    }
}
```

### 定数

```rust
#[test]
fn boolean_and_null_literals() {
    assert_eq!(
        parse_sql("VALUES (TRUE, FALSE, NULL, UNKNOWN)"),
        Ok(Values {
            exprs: vec![
                Expr::Boolean(true),
                Expr::Boolean(false),
                Expr::Null,
                Expr::Null
            ]
        })
    );
}
```

`Expr`に`Boolean(bool)`と`Null`を加え，整数を読む関数を，定数を読む関数`constant`に広げた．
`Keyword::Unknown | Keyword::Null`のように，`|`は列挙子の中のパターンにも書ける．

```rust
fn constant(token: &Token) -> Option<Expr> {
    match token {
        Token::Integer(n) => Some(Expr::Integer(*n)),
        Token::Keyword(Keyword::True) => Some(Expr::Boolean(true)),
        Token::Keyword(Keyword::False) => Some(Expr::Boolean(false)),
        Token::Keyword(Keyword::Unknown | Keyword::Null) => Some(Expr::Null),
        _ => None,
    }
}
```

### 比較演算子と優先順位

```rust
#[test]
fn arithmetic_binds_tighter_than_comparison() {
    assert_eq!(
        parse_sql("VALUES (1 + 2 < 4)"),
        Ok(Values {
            exprs: vec![comparison(
                ComparisonOp::Lt,
                arithmetic(ArithmeticOp::Add, int(1), int(2)),
                int(4)
            )]
        })
    );
}

#[test]
fn comparisons_cannot_be_chained() {
    assert_eq!(parse_sql("VALUES (1 < 2 < 3)"), Err(ParseError));
}
```

比較演算子を，算術より弱い結合性なしの中置演算子として加えた．
構文木を作る関数(`equal`，`less`など)は，演算子ごとに用意した．選択肢が多いので，`alt`を種類ごとの入れ子にした．

```rust
const COMPARISON: i64 = 5;

        .infix(alt((
            alt((
                literal(Token::Plus).value(Infix::Left(ADDITIVE, add)),
                // 算術演算子の残り
            )),
            alt((
                literal(Token::Eq).value(Infix::Neither(COMPARISON, equal)),
                literal(Token::NotEq).value(Infix::Neither(COMPARISON, not_equal)),
                // 比較演算子の残り
            )),
            // AND と OR
        )))

fn equal(_: &mut Tokens<'_>, left: Expr, right: Expr) -> winnow::Result<Expr> {
    Ok(comparison(ComparisonOp::Eq, left, right))
}
```

### `AND`，`OR`，`NOT`の優先順位

```rust
#[test]
fn and_binds_tighter_than_or() {
    assert_eq!(
        parse_sql("VALUES (TRUE OR FALSE AND FALSE)"),
        Ok(Values {
            exprs: vec![binary(
                BinaryOp::Or,
                Expr::Boolean(true),
                binary(BinaryOp::And, Expr::Boolean(false), Expr::Boolean(false))
            )]
        })
    );
}

#[test]
fn not_binds_looser_than_comparison() {
    assert_eq!(
        parse_sql("VALUES (NOT 1 = 2)"),
        Ok(Values {
            exprs: vec![unary(
                UnaryOp::Not,
                comparison(ComparisonOp::Eq, int(1), int(2))
            )]
        })
    );
}
```

優先順位の表を定数で書き，`AND`と`OR`を中置演算子，`NOT`を前置演算子に加えた．

```rust
const OR: i64 = 1;
const AND: i64 = 2;
const NOT: i64 = 3;
const IS: i64 = 4;
const COMPARISON: i64 = 5;
const ADDITIVE: i64 = 10;
const MULTIPLICATIVE: i64 = 20;
const UNARY: i64 = 30;

        .prefix(alt((
            literal(Token::Minus).value(Prefix(UNARY, negate)),
            literal(Token::Keyword(Keyword::Not)).value(Prefix(NOT, not)),
        )))
```

`NOT TRUE AND FALSE`の項目は，`NOT`の強さが`AND`より大きいので，書いた時点で通る．

### `IS NULL`と`IS NOT NULL`

```rust
#[test]
fn is_null_applies_to_the_whole_arithmetic_expression() {
    assert_eq!(
        parse_sql("VALUES (1 + 2 IS NULL)"),
        Ok(Values {
            exprs: vec![Expr::IsNull {
                operand: Box::new(arithmetic(ArithmeticOp::Add, int(1), int(2))),
                negated: false
            }]
        })
    );
}
```

`Expr::IsNull`を加え，後置演算子として読む．2つか3つのトークンを，パーサーの組で読む．

```rust
        .postfix(alt((
            (
                literal(Token::Keyword(Keyword::Is)),
                literal(Token::Keyword(Keyword::Null)),
            )
                .value(Postfix(IS, is_null)),
            (
                literal(Token::Keyword(Keyword::Is)),
                literal(Token::Keyword(Keyword::Not)),
                literal(Token::Keyword(Keyword::Null)),
            )
                .value(Postfix(IS, is_not_null)),
        )))
```

### 真偽値，`NULL`，比較の評価

```rust
#[test]
fn comparison_with_null_is_unknown() {
    assert_eq!(eval_sql("1 = NULL"), Ok(Value::Null));
    assert_eq!(eval_sql("NULL = NULL"), Ok(Value::Null));
}
```

`Value`に`Boolean`と`Null`を加えると，`let Value::Integer(a) = left;`がコンパイルエラーになる．列挙子が2つ以上ある型は`let`で分解できない．
比較は，2つの値のタプルに対する`match`で型の組み合わせを分け，`cmp`で`Ordering`を得る．

```rust
fn comparison(op: &ComparisonOp, left: Value, right: Value) -> Result<Value, EvalError> {
    let ordering = match (left, right) {
        (Value::Null, _) | (_, Value::Null) => return Ok(Value::Null),
        (Value::Integer(a), Value::Integer(b)) => a.cmp(&b),
        (Value::Boolean(a), Value::Boolean(b)) => a.cmp(&b),
        _ => return Err(EvalError::DatatypeMismatch),
    };
    let result = match op {
        ComparisonOp::Eq => ordering == Ordering::Equal,
        ComparisonOp::NotEq => ordering != Ordering::Equal,
        ComparisonOp::Lt => ordering == Ordering::Less,
        ComparisonOp::LtEq => ordering != Ordering::Greater,
        ComparisonOp::Gt => ordering == Ordering::Greater,
        ComparisonOp::GtEq => ordering != Ordering::Less,
    };
    Ok(Value::Boolean(result))
}
```

`comparison`は`ComparisonOp`だけを受け取るので，`match op`は6つの比較演算子だけを扱えばよい．

### 3値論理

```rust
#[test]
fn and_follows_three_valued_logic() {
    assert_eq!(eval_sql("TRUE AND TRUE"), Ok(Value::Boolean(true)));
    assert_eq!(eval_sql("TRUE AND FALSE"), Ok(Value::Boolean(false)));
    assert_eq!(eval_sql("TRUE AND NULL"), Ok(Value::Null));
    assert_eq!(eval_sql("FALSE AND NULL"), Ok(Value::Boolean(false)));
    assert_eq!(eval_sql("NULL AND NULL"), Ok(Value::Null));
}
```

値を`Option<bool>`に変換する`truth`と，真理値表を書いた`and`，`or`，`not`を作った．
`Value::from_truth`は，`Option<bool>`を値に戻す関連関数である．

```rust
fn truth(value: &Value) -> Result<Option<bool>, EvalError> {
    match value {
        Value::Boolean(b) => Ok(Some(*b)),
        Value::Null => Ok(None),
        Value::Integer(_) => Err(EvalError::DatatypeMismatch),
    }
}

fn and(a: Option<bool>, b: Option<bool>) -> Option<bool> {
    match (a, b) {
        (Some(false), _) | (_, Some(false)) => Some(false),
        (Some(true), Some(true)) => Some(true),
        _ => None,
    }
}
```

```rust
        BinaryOp::And => Ok(Value::from_truth(and(truth(&left)?, truth(&right)?))),
        BinaryOp::Or => Ok(Value::from_truth(or(truth(&left)?, truth(&right)?))),
```

`not`は，`cargo clippy`が`Option::map`で書けると指摘する形を避け，真理値表の形で書いた．

```rust
fn not(a: Option<bool>) -> Option<bool> {
    match a {
        Some(true) => Some(false),
        Some(false) => Some(true),
        None => None,
    }
}
```

### `IS NULL`の評価

```rust
#[test]
fn is_null_and_is_not_null() {
    assert_eq!(eval_sql("NULL IS NULL"), Ok(Value::Boolean(true)));
    assert_eq!(eval_sql("1 IS NULL"), Ok(Value::Boolean(false)));
    assert_eq!(eval_sql("NULL IS NOT NULL"), Ok(Value::Boolean(false)));
    assert_eq!(eval_sql("1 IS NOT NULL"), Ok(Value::Boolean(true)));
}
```

`Value::is_null`メソッドを定義した．`is_null != negated`は，`negated`が真のときだけ結果を反転する．

```rust
impl Value {
    pub fn is_null(&self) -> bool {
        match self {
            Value::Null => true,
            Value::Integer(_) | Value::Boolean(_) => false,
        }
    }
}
```

```rust
        Expr::IsNull { operand, negated } => {
            let is_null = eval(operand)?.is_null();
            Ok(Value::Boolean(is_null != *negated))
        }
```

### `NULL`を含む算術と型の不一致

```rust
#[test]
fn arithmetic_with_null_is_null() {
    assert_eq!(eval_sql("1 + NULL"), Ok(Value::Null));
    assert_eq!(eval_sql("-NULL"), Ok(Value::Null));
    assert_eq!(eval_sql("NULL / 0"), Ok(Value::Null));
}

#[test]
fn arithmetic_on_booleans_is_a_type_mismatch() {
    assert_eq!(eval_sql("1 + TRUE"), Err(EvalError::DatatypeMismatch));
    assert_eq!(eval_sql("-TRUE"), Err(EvalError::DatatypeMismatch));
    assert_eq!(eval_sql("TRUE + NULL"), Err(EvalError::DatatypeMismatch));
}
```

算術は，整数と`NULL`の組み合わせだけを受け付ける．真偽値を含む組み合わせは，`NULL`との組み合わせでも型の不一致にした．
`NULL`の判定を割り算の前に置くので，`NULL / 0`は`NULL`になる．

```rust
fn arithmetic(op: &ArithmeticOp, left: Value, right: Value) -> Result<Value, EvalError> {
    let (a, b) = match (left, right) {
        (Value::Integer(a), Value::Integer(b)) => (a, b),
        (Value::Null, Value::Integer(_) | Value::Null) | (Value::Integer(_), Value::Null) => {
            return Ok(Value::Null);
        }
        _ => return Err(EvalError::DatatypeMismatch),
    };
    // 以降は Iteration 1 と同じ検査付き演算
}
```

### 結合テスト

使用例と型の不一致を`tests/values.rs`に加えた．どちらも書いた時点で通る．

```rust
#[test]
fn evaluates_three_valued_logic() {
    let result = execute("VALUES (1 < 2 AND NULL, NULL IS NULL, 1 + NULL)").unwrap();
    assert_eq!(
        result.rows,
        vec![vec![Value::Null, Value::Boolean(true), Value::Null]]
    );
}

#[test]
fn reports_a_type_mismatch() {
    assert_eq!(
        execute("VALUES (1 + TRUE)"),
        Err(Error::Eval(EvalError::DatatypeMismatch))
    );
}
```

## 2-6 振り返り

1. 3値論理は，`AND`と`OR`のそれぞれで，真，偽，不明の組み合わせのうち結果が異なるものを挙げる．優先順位は，隣り合う強さの組ごとに1項目あると，強さの定数を誤ったときにどの組が壊れたか分かる．
2. `Option<Value>`で包む設計との比較である．
   - `Option<Value>`なら，`NULL`でない値だけを受け取る関数は`Value`を受け取ればよく，型で`NULL`を除ける．一方，式の評価のほとんどは`NULL`を受け取りうるので，至るところで`Option`を外す処理が要る．
   - 3値論理の`Option<bool>`は，`Option<Value>`のうち真偽値の場合にあたる．
   - Iteration 5では列の型をカタログが持つので，値そのものに「`INTEGER`型の`NULL`」を持たせる必要はない．`Value::Null`のまま進める．
3. 12個の列挙子を持つ`BinaryOp`では，算術演算を評価する関数の`match`にも比較演算子や`AND`の腕が要る．実際には呼ばれない腕に`_`や`unreachable!`を書くことになり，列挙子を加えたときにコンパイラーの指摘が届かなくなる．
4. `is_null`は，ある値について問い合わせるのでメソッドにした．`from_truth`は，まだ値がない状態から値を作るので，`self`を受け取らない関連関数にした．
5. 模範解答の設計ドキュメントは，照合スクリプトで問題がないことを確かめてある．

## 2-7 発展課題

解答例である．`Expr`に`IsTruth`を加え，`IS TRUE`と`IS FALSE`を後置演算子として読む．`IS NOT TRUE`と`IS NOT FALSE`も，`negated`を`true`にした同じ形で加えられる．

```rust
    IsTruth {
        operand: Box<Expr>,
        truth: bool,
        negated: bool,
    },
```

```rust
            (
                literal(Token::Keyword(Keyword::Is)),
                literal(Token::Keyword(Keyword::True)),
            )
                .value(Postfix(IS, is_true)),

fn is_true(_: &mut Tokens<'_>, operand: Expr) -> winnow::Result<Expr> {
    Ok(Expr::IsTruth {
        operand: Box::new(operand),
        truth: true,
        negated: false,
    })
}
```

```rust
        Expr::IsTruth {
            operand,
            truth: expected,
            negated,
        } => {
            let matches = truth(&eval(operand)?)? == Some(*expected);
            Ok(Value::Boolean(matches != *negated))
        }
```

```rust
#[test]
fn is_true_and_is_false() {
    assert_eq!(eval_sql("NULL IS TRUE"), Ok(Value::Boolean(false)));
    assert_eq!(eval_sql("1 < 2 IS TRUE"), Ok(Value::Boolean(true)));
    assert_eq!(eval_sql("NULL IS FALSE"), Ok(Value::Boolean(false)));
    assert_eq!(eval_sql("FALSE IS FALSE"), Ok(Value::Boolean(true)));
}
```

`truth: expected`は，フィールド`truth`を`expected`という名前で取り出すパターンである．関数`truth`と名前が重ならないようにした．
