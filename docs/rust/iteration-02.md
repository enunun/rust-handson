# Iteration 2：メソッド，タプルの`match`，型による分類

Iteration 2では，真偽値と`NULL`を値に加え，比較と3値論理を評価する．
このノートでは，型にメソッドを定義する方法，複数の値をまとめて`match`する方法，演算子を型で分類する方法を説明する．

## `impl`ブロックとメソッド

`impl 型名 { ... }`の中に，その型の関数を定義する．

- 最初の引数が`&self`の関数はメソッドで，`値.メソッド名()`で呼ぶ．`self`はその値への参照である．
- 値を書き換えるメソッドは`&mut self`を受け取る．
- `self`を受け取らない関数は関連関数で，`型名::関数名()`で呼ぶ．値を作る関数によく使う．

```rust
#[derive(Debug, PartialEq)]
pub struct Counter {
    count: i64,
}

impl Counter {
    pub fn new() -> Counter {
        Counter { count: 0 }
    }

    pub fn get(&self) -> i64 {
        self.count
    }

    pub fn increment(&mut self) {
        self.count += 1;
    }
}

let mut c = Counter::new();
c.increment();
c.increment();
assert_eq!(c.get(), 2);
```

`enum`にも`impl`ブロックを書ける．`ferrodb`では，`Value`に`is_null`メソッドと`from_truth`関連関数を定義する．

## `Option<bool>`による3値

SQLの論理演算は，真，偽，不明の3つの値をとる．
Rustでは`Option<bool>`で表せる．`Some(true)`が真，`Some(false)`が偽，`None`が不明である．
`Option`は「値がないこともある」を表す直和型なので，「真偽が分からない」をそのまま表せる．

## タプルに対する`match`

2つの値をタプル`(a, b)`にまとめると，組み合わせで処理を分けられる．
`|`で複数のパターンを1つの腕にまとめられる．`_`は，どんな値にも一致する．

```rust
pub fn both(a: Option<bool>, b: Option<bool>) -> Option<bool> {
    match (a, b) {
        (Some(false), _) | (_, Some(false)) => Some(false),
        (Some(true), Some(true)) => Some(true),
        _ => None,
    }
}

assert_eq!(both(Some(true), None), None);
assert_eq!(both(Some(false), None), Some(false));
```

`match`の腕は上から順に調べられる．上の例では，片方が偽なら，もう片方が不明でも偽になる．

`|`は，列挙子のパターンの中でも使える．

```rust
pub enum Shape {
    Circle(i64),
    Square(i64),
    Point,
}

pub fn has_size(shape: &Shape) -> bool {
    match shape {
        Shape::Circle(_) | Shape::Square(_) => true,
        Shape::Point => false,
    }
}
```

## 型による分類

`enum`の列挙子に別の`enum`を持たせると，値を段階的に分類できる．

```rust
pub enum BinaryOp {
    Arithmetic(ArithmeticOp),
    Comparison(ComparisonOp),
    And,
    Or,
}
```

算術演算を評価する関数は`&ArithmeticOp`を受け取る．
`BinaryOp`をそのまま受け取ると，その関数の中の`match`でも比較演算子や`AND`を扱わなければならない．実際には呼ばれない腕に`_`や`unreachable!`を書くことになり，列挙子を加えたときにコンパイラーが直す場所を教えてくれなくなる．
分類を型にしておけば，関数は自分が扱う演算子だけを受け取る．

## `std::cmp::Ordering`

整数や真偽値の`cmp`メソッドは，2つの値の大小を`Ordering`で返す．
`Ordering`は`Less`，`Equal`，`Greater`の3つの列挙子を持つ`enum`である．真偽値は`false`が`true`より小さい．

```rust
use std::cmp::Ordering;

assert_eq!(1.cmp(&2), Ordering::Less);
assert_eq!(false.cmp(&true), Ordering::Less);
assert_eq!(format!("{:?}", 3.cmp(&3)), "Equal");
```

大小を一度`Ordering`にしておくと，`<`や`>=`などの6つの比較演算子を，型によらず同じ方法で判定できる．

## 文字列の大文字への変換

`to_ascii_uppercase`は，ASCIIの英字を大文字にした新しい文字列(`String`)を返す．
`String`の`as_str()`で`&str`として読めるので，`match`で文字列のリテラルと比べられる．
`String`と`&str`の違いはIteration 3で扱う．

```rust
assert_eq!("values".to_ascii_uppercase(), "VALUES");
```

## winnowの`postfix`と`Infix::Neither`

- `.postfix(p)`：後置演算子を読むパーサー`p`を渡す．`p`は`Postfix(強さ, 関数)`を返す．`IS NULL`のように複数のトークンからなる演算子は，`(literal(..), literal(..))`のようにパーサーを組にして読む．
- `Infix::Neither(強さ, 関数)`：結合性のない中置演算子である．同じ強さの演算子を続けて書くと，式の終わりとみなされる．`1 < 2 < 3`は`1 < 2`で式が終わり，残った`< 3`が構文エラーになる．

`alt`のタプルに並べられる選択肢の数には上限がある．多いときは，`alt((alt((..)), alt((..))))`のように入れ子にする．
