# Iteration 4：エラー型の設計とトレイトの実装

Iteration 4では，すべての段階のエラーを1つの型にまとめ，SQLSTATEと位置を返す．
このノートでは，エラー型をどう設計するか，標準ライブラリのトレイト(`From`，`Display`，`std::error::Error`)を自分の型に実装する方法，ジェネリックな構造体を説明する．
トレイトそのものの定義と使い方は，Iteration 10で詳しく扱う．

## エラー型の設計

エラーの型は，利用者がエラーをどう扱うかで決める．代数的データ型でよく使う形は2つある．

| 形 | 例 | 向いている場面 |
| --- | --- | --- |
| 列挙子ごとに持つ情報を変える直和型 | `enum ParseError { UnexpectedToken { token, position }, UnexpectedEnd }` | 呼び出し側が種類ごとに処理を変える．種類によって持つ情報が違う |
| 共通の情報を持つ直積型 | `struct Error { sqlstate, message, position }` | 呼び出し側はどの種類でも同じ扱いをする(表示する，コードを返す) |

`ParseError`では，トークンがある場合とない場合で持つ情報が違う．トークンと位置をどちらも`Option`で持つ`struct`では，「トークンはないのに位置がある」といった組み合わせも作れてしまう．
一方，利用者に返す`Error`は，SQLSTATE，メッセージ，位置をどの種類でも同じように使うので，共通の`struct`にまとめた．

## トレイトを実装する

トレイトは，型が持つべきメソッドの集まりである．`impl トレイト名 for 型名 { ... }`で，自分の型にトレイトを実装する．

### `Display`

`std::fmt::Display`を実装すると，`{}`で表示でき，`to_string()`で文字列にできる．
`fmt`メソッドでは，`write!(f, ...)`や`f.write_str(...)`で書き出す．

```rust
use std::fmt;

#[derive(Debug, PartialEq)]
pub struct AppError {
    message: String,
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "app error: {}", self.message)
    }
}
```

`Debug`は開発者向けの表示で導出できる．`Display`は利用者向けの表示なので，自分で書く．

### `std::error::Error`

`std::error::Error`は，エラーを表す型の印になるトレイトである．`Debug`と`Display`を実装した型なら，中身は空でよい．

```rust
impl std::error::Error for AppError {}
```

### `From`と`?`

`From<T>`を実装すると，`T`から自分の型に変換できる．
`?`は，`Err`を返すときに，関数の戻り値のエラーの型へ`From`で変換する．そのため，`map_err`を書かずに異なる型のエラーを返せる．

```rust
use std::num::ParseIntError;

impl From<ParseIntError> for AppError {
    fn from(error: ParseIntError) -> AppError {
        AppError {
            message: error.to_string(),
        }
    }
}

pub fn double(text: &str) -> Result<i64, AppError> {
    let n: i64 = text.parse()?;
    Ok(n * 2)
}

assert_eq!(double("21"), Ok(42));
assert_eq!(
    double("x").unwrap_err().to_string(),
    "app error: invalid digit found in string"
);
```

変換の規則を`From`に1か所だけ書いておけば，`?`を使うすべての場所で同じ変換になる．

### ほかの型と比べる`PartialEq`

`PartialEq`は型引数をとり，`impl PartialEq<B> for A`と書くと，`A == B`で比べられる．

```rust
impl PartialEq<i64> for Tagged<i64> {
    fn eq(&self, other: &i64) -> bool {
        self.value == *other
    }
}
```

`ferrodb`では`impl PartialEq<Token> for Spanned<Token>`を実装し，位置の付いたトークンをトークンそのものと比べる．winnowの`literal(Token::Comma)`は，この比較でトークンを読む．

## 非公開のフィールドとアクセサー

構造体のフィールドは，`pub`を付けなければモジュールの外から読めない．
外からはメソッドで読ませると，フィールドの持ち方を変えても利用者のコードは変わらない．また，外から不正な値に書き換えられることもない．

```rust
impl AppError {
    pub fn message(&self) -> &str {
        &self.message
    }
}
```

## ジェネリックな構造体

型引数`T`を持つ構造体は，いろいろな型の値を同じ形で包める．

```rust
#[derive(Debug, PartialEq)]
pub struct Tagged<T> {
    pub value: T,
    pub tag: String,
}

let words = Tagged { value: "a", tag: "y".to_string() };
```

`Vec<T>`や`Option<T>`も，型引数を持つ型である．ジェネリックな関数やトレイト境界は，Iteration 15と16で扱う．

## `&'static str`と`Copy`

文字列リテラルは，プログラムが動いている間ずっと存在する．その参照の型は`&'static str`である．
`'static`はライフタイム(Iteration 15)の1つで，「いつまでも有効」を表す．固定の文字列を返す関数の戻り値に使える．

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Warning,
    Error,
}

impl Level {
    pub fn name(self) -> &'static str {
        match self {
            Level::Warning => "WARNING",
            Level::Error => "ERROR",
        }
    }
}
```

データを持たない列挙子だけの`enum`は，`Copy`を導出できる．`Copy`な値は，メソッドの`self`で受け取っても，代入しても，元の変数を使い続けられる．

## winnow：位置と`cut_err`

### `LocatingSlice`と`with_span`

`LocatingSlice::new(sql)`で入力を包むと，入力の先頭からの位置を知っているパーサーになる．
`p.with_span()`は，`p`の結果と，読んだ部分のバイトの範囲(`Range<usize>`)の組を返す．

```rust
fn spanned_word(input: &mut LocatingSlice<&str>) -> winnow::Result<(&'static str, Range<usize>)> {
    "ab".value("word").with_span().parse_next(input)
}

assert_eq!(spanned_word.parse(LocatingSlice::new("ab")), Ok(("word", 0..2)));
```

### `ModalResult`と`cut_err`

`alt`は，選択肢が失敗すると入力を戻して次の選択肢を試す(バックトラック)．
そのため，`(`を読んだあとの中身の誤りでも，`alt`が`(`の前まで戻ってしまい，エラーの位置が`(`になる．

`cut_err(p)`は，`p`が失敗したら，バックトラックせずにその場でエラーにする．「ここまで読んだら，もう別の読み方はない」という位置で使う．
`cut_err`を使うには，パーサーの戻り値を`winnow::ModalResult<T>`にする．`ModalResult`のエラーは，バックトラックしてよい失敗と，してはいけない失敗を区別する．

```rust
use winnow::combinator::{alt, cut_err, preceded};
use winnow::{ModalResult, Parser};

fn signed(input: &mut &str) -> ModalResult<char> {
    alt((preceded('+', cut_err('1')), '2')).parse_next(input)
}

assert_eq!(signed.parse("+3").unwrap_err().offset(), 1);
```

`cut_err`がなければ，`+`のあとの`3`で失敗した`alt`は先頭に戻って`2`を試し，エラーの位置は0になる．
