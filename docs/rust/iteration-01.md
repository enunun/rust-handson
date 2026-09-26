# Iteration 1：代数的データ型と構文木

Iteration 1では，`VALUES (1 + 2 * 3)`を構文木にして評価する．
このノートでは，構文木を表すための代数的データ型，再帰的な型，`match`による分解，エラーの伝播と，winnowでトークンの列を解析する方法を説明する．

## 代数的データ型

Rustの`struct`と`enum`は，組み合わせて新しい型を作る2つの方法である．この2つで作る型を代数的データ型と呼ぶ．

| 作り方 | Rust | 意味 | 取りうる値の数 |
| --- | --- | --- | --- |
| 直積型 | `struct`，タプル | AとBの両方を持つ | Aの数×Bの数 |
| 直和型 | `enum` | AかBのどちらか一方である | Aの数＋Bの数 |

例えば`struct Point { x: bool, y: bool }`は2×2＝4通り，`enum Choice { Yes, No, Maybe(bool) }`は1＋1＋2＝4通りの値をとる．

### 不正な状態を表現できない型

オブジェクト指向言語では，種類の違うデータを1つのクラスにまとめ，使わないフィールドを`null`にすることがある．
例えば式を次のように表すと，「足し算なのに右辺がない」や「整数なのに左辺がある」という不正な状態も作れてしまう．

```text
class Expr {
    String kind;     // "integer" か "add"
    Long value;      // kind が "integer" のときだけ使う
    Expr left;       // kind が "add" のときだけ使う
    Expr right;      // kind が "add" のときだけ使う
}
```

直和型では，種類ごとに持つデータを列挙子に書く．列挙子に書いていないデータは，そもそも持てない．

```rust
#[derive(Debug, PartialEq)]
pub enum Calc {
    Number(i64),
    Add(Box<Calc>, Box<Calc>),
}
```

`Calc::Add`は必ず左辺と右辺を持ち，`Calc::Number`は必ず整数を持つ．不正な組み合わせは型として書けないので，それを調べるコードも要らない．
型を設計するときは，「この型でどんな値が作れるか」を数え，意味のない値が作れないかを確かめる．

### 列挙子の形

列挙子は，データを持たない形，タプルの形，名前付きのフィールドを持つ形のどれでも書ける．

```rust
#[derive(Debug, PartialEq)]
pub enum Shape {
    Circle { radius: i64 },
    Rectangle { width: i64, height: i64 },
}
```

フィールドを持たない`struct`も書ける(`pub struct ParseError;`)．値は1通りしかなく，「失敗した」ことだけを表すのに使える．

## `match`による分解

`match`は，値の形ごとに処理を分け，列挙子の中のデータに名前を付けて取り出す．

```rust
pub fn area(shape: &Shape) -> i64 {
    match shape {
        Shape::Circle { radius } => 3 * radius * radius,
        Shape::Rectangle { width, height } => width * height,
    }
}

assert_eq!(area(&Shape::Rectangle { width: 2, height: 3 }), 6);
```

`match`はすべての形を扱わなければならない(網羅性)．
`Shape`に`Triangle`を加えると，`Triangle`を扱っていない`match`がすべてコンパイルエラーになる．
列挙子を加えたときに，直すべき場所をコンパイラーが教えてくれる．

```text
error[E0004]: non-exhaustive patterns: `&Shape::Triangle { .. }` not covered
  --> src/lib.rs:8:11
   |
 8 |     match shape {
   |           ^^^^^ pattern `&Shape::Triangle { .. }` not covered
```

残りをまとめて扱うには`_`を使う．ただし`_`を使うと，列挙子を加えたときにコンパイラーが教えてくれなくなる．

列挙子が1つしかない型は，`let`でも分解できる．

```rust
let Value::Integer(n) = value;
```

列挙子が2つ以上になると，この`let`はコンパイルエラーになる．`match`で書き直す．

```text
error[E0005]: refutable pattern in local binding
 --> src/lib.rs:7:9
  |
7 |     let Value::Integer(n) = value;
  |         ^^^^^^^^^^^^^^^^^ pattern `Value::Boolean(_)` not covered
```

## 再帰的な型と`Box`

`Calc::Add`は，自分と同じ型`Calc`を持つ．これを`Box`なしで書くとコンパイルエラーになる．

```text
error[E0072]: recursive type `Calc` has infinite size
 --> src/lib.rs:1:1
  |
1 | pub enum Calc {
  | ^^^^^^^^^^^^^
2 |     Number(i64),
3 |     Add(Calc, Calc),
  |         ---- recursive without indirection
```

Rustは値の大きさをコンパイル時に決める．`Calc`の中に`Calc`をそのまま置くと，大きさが無限になる．
`Box<T>`は，`T`の値をヒープに置き，その場所だけを持つ．`Box`の大きさは`T`によらず一定なので，再帰的な型を作れる．
`Box::new(値)`で作り，中の値は`Box`を通してそのまま使える．

```rust
pub fn calc(c: &Calc) -> i64 {
    match c {
        Calc::Number(n) => *n,
        Calc::Add(a, b) => calc(a) + calc(b),
    }
}

assert_eq!(
    calc(&Calc::Add(Box::new(Calc::Number(1)), Box::new(Calc::Number(2)))),
    3
);
```

## 参照と`*`

`calc(c: &Calc)`の`&Calc`は，`Calc`の値を借りて読む参照である．呼ぶ側は`&値`で参照を渡す．
参照に対して`match`すると，取り出したデータも参照になる．上の例の`n`は`&i64`なので，`*n`で中の`i64`を取り出す．
所有権と借用はIteration 3で詳しく扱う．

## 検査付きの整数演算

`i32`の`+`が範囲を超えると，テストやデバッグビルドではプログラムが止まる(パニック)．

```text
attempt to add with overflow
```

`checked_add`，`checked_sub`，`checked_mul`，`checked_div`，`checked_neg`は，範囲を超えると`None`を返す．`checked_div`は0で割ったときも`None`を返す．

```rust
assert_eq!(1_i32.checked_add(2), Some(3));
assert_eq!(2147483647_i32.checked_add(1), None);
assert_eq!(7_i32.checked_div(0), None);
```

整数の`/`は0の方向に切り捨てる．

```rust
assert_eq!(-7 / 2, -3);
```

`i64`を`i32`に変換するには`i32::try_from`を使う．範囲を超えると`Err`を返す．

```rust
assert_eq!(i32::try_from(42_i64), Ok(42));
assert_eq!(i32::try_from(2147483648_i64).is_err(), true);
```

## エラーの伝播

`?`は，`Result`が`Err`ならその`Err`をすぐに関数から返し，`Ok`なら中の値を取り出す．
関数の戻り値のエラーの型と，`?`を付けた式のエラーの型が同じときに使える．

```rust
pub fn half(n: i64) -> Result<i64, String> {
    if n % 2 == 0 { Ok(n / 2) } else { Err(format!("{n} is odd")) }
}

pub fn quarter(n: i64) -> Result<i64, String> {
    let h = half(n)?;
    half(h)
}

assert_eq!(quarter(8), Ok(2));
assert_eq!(quarter(6), Err("3 is odd".to_string()));
```

エラーの型が違うときは，`map_err`でエラーを変換してから`?`を付ける．
データを持つ列挙子は関数として使えるので，エラーを包む列挙子をそのまま渡せる．

```rust
#[derive(Debug, PartialEq)]
pub enum AppError {
    Parse(String),
    Negative(i64),
}

let e: Result<i64, i64> = Err(-5);
assert_eq!(e.map_err(AppError::Negative), Err(AppError::Negative(-5)));
```

Iteration 4では，`From`を実装して，`map_err`を書かずに`?`でエラーを変換する．

## `mut`と`for`

変数は，既定では書き換えられない．書き換える変数には`let mut`を付ける．
`for x in &v`は，`Vec`の要素を先頭から順に借りて読む．

```rust
pub fn sum_all(numbers: &Vec<i64>) -> i64 {
    let mut total = 0;
    for n in numbers {
        total += n;
    }
    total
}

assert_eq!(sum_all(&vec![1, 2, 3]), 6);
```

## `const`

`const`は，コンパイル時に決まる定数を定義する．名前は大文字で書き，型を必ず書く．

```rust
const LIMIT: i64 = 10;
```

## `PartialEq`と`Eq`

`PartialEq`は`==`で比べられることを表す．`Eq`は，それに加えて「どの値も自分自身と等しい」ことを約束する．
浮動小数点数の`NaN`は自分自身と等しくないので，`f64`は`PartialEq`だけを実装する．
整数や文字列だけからなる型には，`Eq`も導出できる．winnowの`literal`でトークンを比べるには，トークンの型が`Eq`を実装している必要がある．

## winnowでトークンの列を解析する

### `TokenSlice`

`TokenSlice<'t, Token>`は，トークンの列(`&[Token]`)をwinnowの入力にする型である．
`TokenSlice::new(&tokens)`で作る．
`'t`はライフタイムと呼ばれる印で，「どこかにあるトークンの列を借りている」ことを表す．ライフタイムはIteration 15で詳しく扱う．
関数の引数では，`'_`と書けば，借りている先をコンパイラーが推論する．

```rust
use winnow::stream::TokenSlice;

type Tokens<'t> = TokenSlice<'t, Token>;

fn operand(input: &mut Tokens<'_>) -> winnow::Result<Expr> {
    ...
}
```

`type`は，型に別名を付ける．

### トークンを読む部品

| 書き方 | 読むもの | 結果 |
| --- | --- | --- |
| `literal(Token::Comma)` | そのトークン1つ | 読んだトークンの並び |
| `any` | 任意のトークン1つ | トークンへの参照(`&Token`) |
| `any.verify_map(f)` | `f`が`Some`を返すトークン | `Some`の中の値 |
| `delimited(p, q, r)` | `p`，`q`，`r`を順に | `q`の結果 |
| `separated(1.., p, sep)` | `sep`で区切った1個以上の`p` | 結果を集めたもの |

`any`の結果は参照なので，`verify_map`に渡す関数は`&Token`を受け取る．

```rust
fn integer(token: &Token) -> Option<Expr> {
    match token {
        Token::Integer(n) => Some(Expr::Integer(*n)),
        _ => None,
    }
}
```

### 優先順位のある式

`expression(operand)`は，Prattの方法で優先順位のある式を読む．
`operand`は式の最小の部品(整数や括弧で囲んだ式)を読むパーサーである．

- `.prefix(p)`：前置演算子を読むパーサー`p`を渡す．`p`は`Prefix(強さ, 関数)`を返す．
- `.infix(p)`：中置演算子を読むパーサー`p`を渡す．`p`は`Infix::Left(強さ, 関数)`(左結合)などを返す．

強さの値が大きい演算子ほど，強く結び付く．
関数は，読んだ部分式から新しい式を作る．前置演算子の関数は`fn(&mut 入力, 式) -> winnow::Result<式>`，中置演算子の関数は`fn(&mut 入力, 左辺, 右辺) -> winnow::Result<式>`の形の，名前の付いた関数を渡せる．

```rust
expression(operand)
    .prefix(literal(Token::Minus).value(Prefix(UNARY, negate)))
    .infix(alt((
        literal(Token::Plus).value(Infix::Left(ADDITIVE, add)),
        literal(Token::Star).value(Infix::Left(MULTIPLICATIVE, multiply)),
    )))
    .parse_next(input)
```

どの演算子にどの強さを与えるかは，SQLの文法で決まる．
