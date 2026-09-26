# Iteration 0：Rustの基本とwinnow

Iteration 0では，Cargoでパッケージを作り，SQLの字句解析器を書く．
このノートでは，そのために必要なRustの文法と，解析ライブラリwinnowの使い方を説明する．

## Cargo

CargoはRustのビルドツール兼パッケージマネージャーである．

- パッケージ：`Cargo.toml`を持つディレクトリ．1つ以上のクレートを含む．
- クレート：コンパイルの単位．`src/lib.rs`から始まるものをライブラリクレート，`src/main.rs`から始まるものをバイナリクレートと呼ぶ．

`cargo init --lib --name ferrodb`は，今いるディレクトリに`Cargo.toml`と`src/lib.rs`を作る．
`--lib`はライブラリクレートを作る指定，`--name`はパッケージの名前である．

```toml
[package]
name = "ferrodb"
version = "0.1.0"
edition = "2024"

[dependencies]
```

- `edition`は言語の版である．このハンズオンでは2024を使う．
- `[dependencies]`には，使う外部のクレート(依存)を書く．`cargo add winnow`を実行すると，最新の版がここに追加される．

よく使うコマンドは次のとおりである．

| コマンド | すること |
| --- | --- |
| `cargo build` | ビルドする．結果は`target/`に置かれる |
| `cargo test` | ビルドしてテストを実行する |
| `cargo add クレート名` | 依存を追加する |
| `cargo fmt` | コードを整形する |
| `cargo clippy` | よくある間違いや，より良い書き方を指摘する |

`Cargo.lock`には，実際に使った依存の版が記録される．手で書き換えない．

## 関数

関数は`fn`で定義する．引数と戻り値には型を書く．
関数の本体の最後の式(セミコロンのない式)が戻り値になる．

```rust
fn double(n: i64) -> i64 {
    n * 2
}

assert_eq!(double(21), 42);
```

## 整数型

| 型 | 範囲 |
| --- | --- |
| `i64` | 64ビット符号付き整数(−9223372036854775808〜9223372036854775807) |
| `u64` | 64ビット符号なし整数 |
| `usize` | 符号なし整数．大きさは環境のポインターと同じ．長さや位置に使う |

最大値は`i64::MAX`のように書ける．

```rust
assert_eq!(i64::MAX, 9223372036854775807);
```

文字列を整数に変換するには`parse`を使う．範囲を超えると失敗する(`Err`を返す)．

```rust
assert_eq!("42".parse::<i64>(), Ok(42));
assert!("9223372036854775808".parse::<i64>().is_err());
```

## `&str`

`&str`は文字列スライスで，どこかにある文字列の一部を借りて読むための型である．
`"VALUES"`のような文字列リテラルも`&str`である．
所有権と借用はIteration 3で詳しく扱う．ここでは「文字列を読むための型」と考えればよい．

`eq_ignore_ascii_case`は，ASCIIの大文字と小文字を区別せずに比べる．

```rust
assert_eq!("Values".eq_ignore_ascii_case("VALUES"), true);
```

## `enum`と`#[derive]`

`enum`(列挙型)は，いくつかの形(列挙子)のうちどれか1つをとる値の型である．
列挙子はデータを持てる．

```rust
#[derive(Debug, Clone, PartialEq)]
enum Shape {
    Circle(i64),
    Square(i64),
    Dot,
}
```

`#[derive(...)]`は，よく使うトレイト(型に共通の機能)の実装を自動で作る．トレイトはIteration 10で詳しく扱う．

| 導出するもの | できるようになること |
| --- | --- |
| `Debug` | `{:?}`で表示できる．`assert_eq!`が失敗したときの表示に必要 |
| `PartialEq` | `==`で比べられる．`assert_eq!`に必要 |
| `Clone` | `.clone()`で複製できる |

```rust
assert_eq!(format!("{:?}", Shape::Circle(3)), "Circle(3)");
assert_ne!(Shape::Dot, Shape::Circle(1));
let s = Shape::Square(2);
assert_eq!(s.clone(), Shape::Square(2));
```

データを持つ列挙子`Shape::Circle`は，値を受け取って`Shape`を作る関数としても使える．

```rust
let make: fn(i64) -> Shape = Shape::Circle;
assert_eq!(make(5), Shape::Circle(5));
```

## `struct`の基本

`struct`(構造体)は，名前の付いたフィールドをまとめた型である．
`pub`を付けたフィールドは，モジュールの外から読み書きできる．

```rust
#[derive(Debug, PartialEq)]
struct Point {
    x: i64,
    y: i64,
}

let p = Point { x: 1, y: 2 };
assert_eq!(p.x + p.y, 3);
assert_eq!(format!("{:?}", p), "Point { x: 1, y: 2 }");
```

## `Vec`

`Vec<T>`は，`T`型の値を並べた伸び縮みする配列である．`vec![...]`で作る．

```rust
let v: Vec<i64> = vec![1, 2, 3];
assert_eq!(v.len(), 3);
```

## `Option`と`Result`

どちらも標準ライブラリの`enum`である．

- `Option<T>`は，値がないこともあることを表す．`Some(値)`か`None`をとる．
- `Result<T, E>`は，成功と失敗のどちらかを表す．`Ok(成功の値)`か`Err(失敗の値)`をとる．

Rustには例外がない．失敗しうる関数は`Result`を返し，呼び出し側が成功と失敗の両方を扱う．

```rust
fn first_word(text: &str) -> Option<&str> {
    text.split_whitespace().next()
}

assert_eq!(first_word("hello world"), Some("hello"));
assert_eq!(first_word("   "), None);
```

## `if`と`match`の基本

`if`は式で，値を返せる．このとき`else`は省略できない．

```rust
fn sign(n: i64) -> &'static str {
    if n < 0 { "negative" } else { "non-negative" }
}
```

`match`は，値がどの形かで処理を分ける．列挙子の中のデータは，名前を付けて取り出せる．

```rust
fn describe(result: Result<i64, String>) -> String {
    match result {
        Ok(n) => format!("ok {n}"),
        Err(message) => format!("error {message}"),
    }
}

assert_eq!(describe(Ok(1)), "ok 1");
assert_eq!(describe(Err("bad".to_string())), "error bad");
```

`match`の詳しい使い方は，Iteration 1で扱う．

## テスト

単体テストは，同じファイルの末尾に書く．

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doubles_a_number() {
        assert_eq!(double(21), 42);
    }
}
```

- `#[cfg(test)]`は，テストのときだけコンパイルする指定である．
- `mod tests`は，テストをまとめる子モジュールである．`use super::*;`で親のモジュールの名前を使えるようにする．
- `#[test]`を付けた関数が1つのテストになる．
- `assert_eq!(実際の値, 期待値)`は，2つが等しくなければテストを失敗させる．`assert!(条件)`は条件が偽なら失敗させる．

結合テストは`tests/`の下のファイルに書く．各ファイルは，ライブラリを外から使う別のクレートとしてコンパイルされる．

## モジュール

`src/lib.rs`に`mod lexer;`と書くと，`src/lexer.rs`がモジュール`lexer`になる．

- モジュールの中の項目(関数，型など)は，既定では非公開である．`pub`を付けると公開される．
- 別のモジュールの項目は`use crate::token::Token;`のようにパスで取り込む．`crate`はクレートのルート(`lib.rs`)を指す．
- `pub use lexer::tokenize;`は，`lexer::tokenize`を`crate::tokenize`としても公開する(再公開)．モジュールを非公開のまま，必要な名前だけを外に見せられる．

結合テストから使えるのは，ルートから公開された名前だけである．`mod lexer;`だけで`pub use`がないと，`use ferrodb::tokenize;`は次のエラーになる．

```text
error[E0432]: unresolved imports `ferrodb::Keyword`, `ferrodb::LexError`, `ferrodb::Token`, `ferrodb::tokenize`
 --> tests/tokenize.rs:1:15
  |
1 | use ferrodb::{Keyword, LexError, Token, tokenize};
  |               ^^^^^^^  ^^^^^^^^  ^^^^^  ^^^^^^^^ no `tokenize` in the root
```

## winnow

[winnow](https://docs.rs/winnow/1.0.4/winnow/)は，小さなパーサーを組み合わせて大きなパーサーを作るライブラリ(パーサーコンビネーター)である．

### パーサー関数

winnowのパーサーは，次の形の関数として書ける．

```rust
use winnow::Parser;
use winnow::ascii::digit1;

fn number(input: &mut &str) -> winnow::Result<i64> {
    digit1.parse_to().parse_next(input)
}
```

- `input: &mut &str`は，まだ読んでいない残りの入力である．パーサーは読んだ分だけ`input`を先に進める．`&mut`は，呼び出し側の変数を書き換えられる参照である．
- 戻り値の`winnow::Result<T>`は，成功なら`Ok(読んだ値)`，失敗なら`Err(エラー)`である．
- `parse_next(input)`は，パーサーを実行して1つ読む．

```rust
let mut input = "123abc";
assert_eq!(number(&mut input), Ok(123));
assert_eq!(input, "abc");

let mut input = "abc";
assert!(number(&mut input).is_err());
assert_eq!(input, "abc");
```

`use winnow::Parser;`は，`parse_to`や`parse_next`などのメソッドを使えるようにする．

### 部品と組み合わせ

| 書き方 | 読むもの | 結果 |
| --- | --- | --- |
| `'x'` | 文字`x` | その文字 |
| `"abc"` | 文字列`abc`(大文字と小文字を区別する) | その文字列 |
| `digit1` | 1文字以上の数字 | 読んだ文字列 |
| `alpha1` | 1文字以上の英字 | 読んだ文字列 |
| `multispace0` | 0文字以上の空白と改行 | 読んだ文字列 |
| `p.value(v)` | `p`と同じ | `v`(の複製) |
| `p.map(f)` | `p`と同じ | `p`の結果に関数`f`を適用した値 |
| `p.parse_to()` | `p`と同じ | `p`の結果を`parse`で変換した値．変換できなければ失敗する |
| `p.verify_map(f)` | `p`と同じ | `f`が`Some(v)`を返せば`v`．`None`なら失敗する |
| `alt((p, q, r))` | `p`，`q`，`r`を順に試し，最初に成功したもの | そのパーサーの結果 |
| `repeat(0.., p)` | `p`を0回以上くり返す | 結果を集めたもの(`Vec`など) |
| `preceded(p, q)` | `p`のあとに`q` | `q`の結果 |
| `terminated(p, q)` | `p`のあとに`q` | `p`の結果 |

`alt`の中のパーサーが失敗すると，`input`はそのパーサーを試す前の位置に戻る．
`repeat`も同じで，失敗した回の読みかけを戻してから終わる．

```rust
fn dot(input: &mut &str) -> winnow::Result<Shape> {
    'x'.value(Shape::Dot).parse_next(input)
}

fn circle(input: &mut &str) -> winnow::Result<Shape> {
    number.map(Shape::Circle).parse_next(input)
}

fn shape(input: &mut &str) -> winnow::Result<Shape> {
    alt((circle, dot)).parse_next(input)
}

let mut input = "x7";
assert_eq!(shape(&mut input), Ok(Shape::Dot));
assert_eq!(shape(&mut input), Ok(Shape::Circle(7)));
```

`value`は結果を複製して返すので，値の型は`Clone`を導出している必要がある．

### 入力全体を読む

`p.parse(入力)`は，入力の全体を`p`で読む．
入力が残ったり`p`が失敗したりすると，`Err`で`ParseError`を返す．`ParseError`の`offset()`は，読めなかった位置(先頭を0とするバイトの位置)である．

```rust
use winnow::ascii::multispace0;
use winnow::combinator::{preceded, repeat, terminated};

fn numbers(input: &mut &str) -> winnow::Result<Vec<i64>> {
    terminated(repeat(0.., preceded(multispace0, number)), multispace0).parse_next(input)
}

assert_eq!(numbers.parse(" 1 23 "), Ok(vec![1, 23]));
let error = numbers.parse("1 x").unwrap_err();
assert_eq!(error.offset(), 2);
```

このときの`error`を`{:?}`で表示すると，次のようになる．

```text
ParseError { input: "1 x", offset: 2, inner: ContextError { context: [], cause: None } }
```

### エラー型の推論

コンビネーターを関数の外で直接実行すると，コンパイラーはエラーの型を決められない．
パーサーは，戻り値の型に`winnow::Result<T>`を書いた関数にしてから使う．

```rust
let mut input = "x";
assert_eq!('x'.value(1).parse_next(&mut input), Ok(1));
```

```text
error[E0283]: type annotations needed
 --> src/lib.rs:8:57
  |
8 |         assert_eq!('x'.value(1).parse_next(&mut input), Ok(1));
  |                        -----                            ^^ cannot infer type of the type parameter `E` declared on the enum `Result`
  |                        |
  |                        type must be known at this point
```
