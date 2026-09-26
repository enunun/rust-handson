# Iteration 0：プロジェクトの作成と字句解析(解説)

演習の各手順の模範解答と，その考え方を説明する．

## 0-1 準備

演習では`cargo init --lib --name ferrodb`でパッケージを作り，`cargo add winnow`で依存を加えた．
受講者の`Cargo.toml`は次のようになる．

```toml
[package]
name = "ferrodb"
version = "0.1.0"
edition = "2024"

[dependencies]
winnow = "1.0.4"
```

模範解答の`Cargo.toml`は，パッケージ名が`ferrodb-00-solution`で，`[lib]`の`name`でライブラリの名前を`ferrodb`にしている．
模範解答はすべてリポジトリのルートのワークスペースに属するので，パッケージ名をIterationごとに変えている．
ライブラリの名前は同じ`ferrodb`なので，テストの`use ferrodb::...`は演習と模範解答で共通である．

```toml
[package]
name = "ferrodb-00-solution"
version = "0.1.0"
edition = "2024"

[lib]
name = "ferrodb"

[dependencies]
winnow = "1.0.4"
```

## 0-2 文法と概念

課題の解答例である．4つのテストはすべて通る．

```rust
use winnow::Parser;
use winnow::combinator::{alt, repeat};

#[derive(Debug, Clone, PartialEq)]
pub enum Color {
    Red,
    Rgb(u8, u8, u8),
}

pub fn sign(input: &mut &str) -> winnow::Result<i64> {
    alt(('+'.value(1), '-'.value(-1))).parse_next(input)
}

pub fn signs(input: &mut &str) -> winnow::Result<Vec<i64>> {
    repeat(0.., sign).parse_next(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_integers() {
        assert_eq!("42".parse::<i64>(), Ok(42));
        assert_eq!("-7".parse::<i64>(), Ok(-7));
        assert!("4 2".parse::<i64>().is_err());
    }

    #[test]
    fn colors() {
        assert_eq!(Color::Rgb(255, 0, 0), Color::Rgb(255, 0, 0));
        assert_ne!(Color::Red, Color::Rgb(255, 0, 0));
        assert_eq!(format!("{:?}", Color::Rgb(255, 0, 0)), "Rgb(255, 0, 0)");
    }

    #[test]
    fn sign_parser() {
        let mut input = "+-";
        assert_eq!(sign(&mut input), Ok(1));
        assert_eq!(input, "-");
        assert_eq!(sign(&mut input), Ok(-1));
        assert_eq!(input, "");
        let mut input = "x";
        assert!(sign(&mut input).is_err());
    }

    #[test]
    fn signs_parser() {
        assert_eq!(signs.parse("+-+"), Ok(vec![1, -1, 1]));
        assert_eq!(signs.parse(""), Ok(vec![]));
        assert_eq!(signs.parse("+x").unwrap_err().offset(), 1);
    }
}
```

- `parse::<i64>()`は先頭の`-`を符号として読む．途中に空白がある`"4 2"`は変換できない．
- `sign`は1回で1文字だけ読み，`input`を1文字進める．
- `signs.parse("+x")`では，`repeat`が`+`を読んだあと`x`で止まり，`parse`が「入力が残った」として失敗する．残った位置はバイトの位置1である．

## 0-3 テストリスト

模範解答は[TESTLIST.md](../TESTLIST.md)である．

### 並べ方

- 空の文字列から始める．`Ok(vec![])`を返すだけで通るので，まず`tokenize`の形(引数と戻り値の型)を決められる．
- 次に整数，演算子，括弧とコンマ，キーワードの順に，トークンの種類を1つずつ増やす．どれも1文字か1語だけの入力にして，空白の処理とは分ける．
- キーワードは，大文字の`VALUES`と，大文字と小文字の混ざった入力を別の項目にした．前者は`"VALUES"`という固定の文字列でも通るので，後者で初めて大文字と小文字の区別をなくす実装が要る．
- 空白は，トークンの種類がそろってから加える．前後と間の空白，改行を1つの入力で確かめる．
- エラーは最後にまとめる．解釈できない文字，キーワードでない単語，`i64`に収まらない整数の3つは，どれも「どこで読めなくなったか」を位置で返す．

### 単体テストと結合テスト

- 単体テストは`lexer`モジュールの`tokenize`を直接呼び，トークンの種類ごとの振る舞いを細かく確かめる．
- 結合テストは，クレートの外から`ferrodb::tokenize`を呼ぶ．使用例のとおりに，公開した名前だけでSQLの文を分けられることを確かめる．

## 0-4 設計ドキュメント

Iteration 0で，5つの設計ドキュメントの最初の版を描いた．

| ファイル | 描いたもの | 理由 |
| --- | --- | --- |
| `c4-context.md` | 開発者と`ferrodb` | このIterationの`ferrodb`は，Rustのプログラムから呼ぶライブラリである．使うのは，そのプログラムを書く開発者である |
| `c4-container.md` | ライブラリクレート`ferrodb`だけ | 実行されるプログラムも，保存されるファイルもまだない |
| `c4-component.md` | `crate`，`lexer`，`token`と3本の依存 | `lib.rs`は`pub use`で`lexer`と`token`の名前を公開するので，両方に依存する．`lexer`は`Token`を作るので`token`に依存する |
| `code-types.md` | `Token`，`Keyword`，`LexError` | `Token::Keyword`が`Keyword`を値として持つので，所有の関係で結んだ |
| `code-sequence.md` | `tokenize`のくり返しと成功と失敗の分岐 | 空白を読み飛ばしてトークンを1つ読むことを，入力の終わりまでくり返す |

- `winnow`は外部のクレートなので，Component図には描かず，図の下の説明に書いた．
- `tokenize`は型ではないので，型の図には描かず，図の下にシグネチャを書いた．

## 0-5 テスト駆動の実装

テストリストの項目ごとに，追加したテストと，テストを通したときのコードを示す．

### モジュールを用意する

`src/lib.rs`でモジュールを宣言し，`src/token.rs`に`Token`を定義した．
`Token`の列挙子は使用例から決まるので，最初にまとめて書いた．

```rust
mod lexer;
mod token;
```

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Keyword(Keyword),
    Integer(i64),
    LParen,
    RParen,
    Comma,
    Plus,
    Minus,
    Star,
    Slash,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Keyword {
    Values,
}
```

### 空の文字列からは，空のトークン列を返す

```rust
#[test]
fn empty_input_gives_no_tokens() {
    assert_eq!(tokenize(""), Ok(vec![]));
}
```

`tokenize`がまだないので，コンパイルエラーになる(Red)．

```text
error[E0425]: cannot find function `tokenize` in this scope
 --> src/lexer.rs:9:20
  |
9 |         assert_eq!(tokenize(""), Ok(vec![]));
  |                    ^^^^^^^^ not found in this scope
```

`LexError`と，空の`Vec`を返すだけの`tokenize`を書いて通す(Green)．

```rust
use crate::token::Token;

#[derive(Debug, PartialEq)]
pub struct LexError {
    pub position: usize,
}

pub fn tokenize(_sql: &str) -> Result<Vec<Token>, LexError> {
    Ok(vec![])
}
```

### `42`からは，整数42を1つ返す

```rust
#[test]
fn digits_become_an_integer() {
    assert_eq!(tokenize("42"), Ok(vec![Token::Integer(42)]));
}
```

仮実装は空の`Vec`を返すので失敗する．

```text
assertion `left == right` failed
  left: Ok([])
 right: Ok([Integer(42)])
```

整数を読むパーサー`integer`を書き，`repeat`で0個以上くり返して入力全体を読む．
失敗したときの位置はまだテストがないので，`0`にしておく．

```rust
use winnow::Parser;
use winnow::ascii::digit1;
use winnow::combinator::repeat;

pub fn tokenize(sql: &str) -> Result<Vec<Token>, LexError> {
    match repeat(0.., integer).parse(sql) {
        Ok(tokens) => Ok(tokens),
        Err(_) => Err(LexError { position: 0 }),
    }
}

fn integer(input: &mut &str) -> winnow::Result<Token> {
    digit1.parse_to().map(Token::Integer).parse_next(input)
}
```

### `+-*/`からは，4つの演算子を順に返す

```rust
#[test]
fn arithmetic_operators() {
    assert_eq!(
        tokenize("+-*/"),
        Ok(vec![Token::Plus, Token::Minus, Token::Star, Token::Slash])
    );
}
```

1つのトークンを読むパーサー`token`を作り，`integer`と，記号を読む`symbol`を`alt`で並べる．

```rust
use winnow::combinator::{alt, repeat};

pub fn tokenize(sql: &str) -> Result<Vec<Token>, LexError> {
    match repeat(0.., token).parse(sql) {
        Ok(tokens) => Ok(tokens),
        Err(_) => Err(LexError { position: 0 }),
    }
}

fn token(input: &mut &str) -> winnow::Result<Token> {
    alt((integer, symbol)).parse_next(input)
}

fn symbol(input: &mut &str) -> winnow::Result<Token> {
    alt((
        '+'.value(Token::Plus),
        '-'.value(Token::Minus),
        '*'.value(Token::Star),
        '/'.value(Token::Slash),
    ))
    .parse_next(input)
}
```

### `(,)`からは，左括弧，コンマ，右括弧を順に返す

```rust
#[test]
fn parentheses_and_comma() {
    assert_eq!(
        tokenize("(,)"),
        Ok(vec![Token::LParen, Token::Comma, Token::RParen])
    );
}
```

`symbol`に3つの記号を加える．

```rust
fn symbol(input: &mut &str) -> winnow::Result<Token> {
    alt((
        '('.value(Token::LParen),
        ')'.value(Token::RParen),
        ','.value(Token::Comma),
        '+'.value(Token::Plus),
        '-'.value(Token::Minus),
        '*'.value(Token::Star),
        '/'.value(Token::Slash),
    ))
    .parse_next(input)
}
```

### `VALUES`からは，キーワードVALUESを返す

```rust
#[test]
fn values_is_a_keyword() {
    assert_eq!(
        tokenize("VALUES"),
        Ok(vec![Token::Keyword(Keyword::Values)])
    );
}
```

固定の文字列`"VALUES"`を読むパーサー`keyword`を`token`に加える．

```rust
use crate::token::{Keyword, Token};

fn token(input: &mut &str) -> winnow::Result<Token> {
    alt((keyword, integer, symbol)).parse_next(input)
}

fn keyword(input: &mut &str) -> winnow::Result<Token> {
    "VALUES".value(Token::Keyword(Keyword::Values)).parse_next(input)
}
```

### `values`と`Values`からも，キーワードVALUESを返す

```rust
#[test]
fn keywords_are_case_insensitive() {
    assert_eq!(
        tokenize("values"),
        Ok(vec![Token::Keyword(Keyword::Values)])
    );
    assert_eq!(
        tokenize("Values"),
        Ok(vec![Token::Keyword(Keyword::Values)])
    );
}
```

`"VALUES"`は大文字と小文字を区別するので失敗する．
英字の並びを`alpha1`で読み，`to_keyword`でキーワードかどうかを判定するように変える．
`to_keyword`が`None`を返すと，`verify_map`は失敗する．

```rust
use winnow::ascii::{alpha1, digit1};

fn keyword(input: &mut &str) -> winnow::Result<Token> {
    alpha1
        .verify_map(to_keyword)
        .map(Token::Keyword)
        .parse_next(input)
}

fn to_keyword(word: &str) -> Option<Keyword> {
    if word.eq_ignore_ascii_case("VALUES") {
        Some(Keyword::Values)
    } else {
        None
    }
}
```

### `" 1 +\n2 "`のように，トークンの前後と間の空白と改行を読み飛ばす

```rust
#[test]
fn whitespace_and_newlines_are_skipped() {
    assert_eq!(
        tokenize(" 1 +\n2 "),
        Ok(vec![Token::Integer(1), Token::Plus, Token::Integer(2)])
    );
}
```

各トークンの前の空白を`preceded`で，最後のトークンのあとの空白を`terminated`で読み飛ばす．

```rust
use winnow::ascii::{alpha1, digit1, multispace0};
use winnow::combinator::{alt, preceded, repeat, terminated};

pub fn tokenize(sql: &str) -> Result<Vec<Token>, LexError> {
    match terminated(repeat(0.., preceded(multispace0, token)), multispace0).parse(sql) {
        Ok(tokens) => Ok(tokens),
        Err(_) => Err(LexError { position: 0 }),
    }
}
```

### `1 ? 2`は，解釈できない文字の位置3のエラーになる

```rust
#[test]
fn unknown_character_reports_its_position() {
    assert_eq!(tokenize("1 ? 2"), Err(LexError { position: 3 }));
}
```

位置が仮の`0`なので失敗する．

```text
assertion `left == right` failed
  left: Err(LexError { position: 0 })
 right: Err(LexError { position: 3 })
```

`ParseError`の`offset()`は先頭を0とするバイトの位置なので，1を足す．

```rust
pub fn tokenize(sql: &str) -> Result<Vec<Token>, LexError> {
    match terminated(repeat(0.., preceded(multispace0, token)), multispace0).parse(sql) {
        Ok(tokens) => Ok(tokens),
        Err(error) => Err(LexError {
            position: error.offset() + 1,
        }),
    }
}
```

このIterationで扱う文字はすべてASCIIで，1文字が1バイトなので，バイトの位置に1を足せば文字の番号になる．

### `1 SELECT`は，VALUES以外の単語の先頭の位置3のエラーになる

```rust
#[test]
fn unknown_word_reports_its_first_character() {
    assert_eq!(tokenize("1 SELECT"), Err(LexError { position: 3 }));
}
```

このテストは，書いた時点で通る．
`keyword`の`alpha1`は`SELECT`を読むが，`verify_map`が失敗すると`alt`は入力を`SELECT`の前に戻す．
`integer`と`symbol`も失敗するので，`repeat`は`1`だけを読んで止まり，`parse`は`S`の位置で失敗する．

### `9223372036854775807`(`i64`の最大値)は，整数になる

```rust
#[test]
fn largest_i64_is_an_integer() {
    assert_eq!(
        tokenize("9223372036854775807"),
        Ok(vec![Token::Integer(i64::MAX)])
    );
}
```

書いた時点で通る．`parse_to`は`i64`の範囲の整数をそのまま変換する．

### `9223372036854775808`は，`i64`に収まらないので位置1のエラーになる

```rust
#[test]
fn integer_beyond_i64_is_an_error() {
    assert_eq!(
        tokenize("9223372036854775808"),
        Err(LexError { position: 1 })
    );
}
```

書いた時点で通る．`parse_to`は変換に失敗するとパーサーとして失敗し，`integer`の前まで入力を戻す．

### Refactor：入力全体を読むパーサーを切り出す

`tokenize`の`match`の中の式が長いので，パーサー関数`tokens`に切り出した．
`tokenize`は「入力全体を読み，エラーを`LexError`に変える」ことだけを受け持つ．

```rust
pub fn tokenize(sql: &str) -> Result<Vec<Token>, LexError> {
    match tokens.parse(sql) {
        Ok(tokens) => Ok(tokens),
        Err(error) => Err(LexError {
            position: error.offset() + 1,
        }),
    }
}

fn tokens(input: &mut &str) -> winnow::Result<Vec<Token>> {
    terminated(repeat(0.., preceded(multispace0, token)), multispace0).parse_next(input)
}
```

### 結合テスト

`tests/tokenize.rs`に，使用例の2つをテストとして書いた．

```rust
use ferrodb::{Keyword, LexError, Token, tokenize};

#[test]
fn tokenizes_a_values_statement() {
    assert_eq!(
        tokenize("values (1, 2 + 3)"),
        Ok(vec![
            Token::Keyword(Keyword::Values),
            Token::LParen,
            Token::Integer(1),
            Token::Comma,
            Token::Integer(2),
            Token::Plus,
            Token::Integer(3),
            Token::RParen,
        ])
    );
}

#[test]
fn reports_the_position_of_an_unknown_character() {
    assert_eq!(tokenize("VALUES (1 ? 2)"), Err(LexError { position: 11 }));
}
```

`lexer`と`token`は非公開のモジュールなので，コンパイルエラーになる．

```text
error[E0432]: unresolved imports `ferrodb::Keyword`, `ferrodb::LexError`, `ferrodb::Token`, `ferrodb::tokenize`
 --> tests/tokenize.rs:1:15
  |
1 | use ferrodb::{Keyword, LexError, Token, tokenize};
  |               ^^^^^^^  ^^^^^^^^  ^^^^^  ^^^^^^^^ no `tokenize` in the root
```

`src/lib.rs`で，4つの名前を`pub use`で公開して通す．

```rust
mod lexer;
mod token;

pub use lexer::{LexError, tokenize};
pub use token::{Keyword, Token};
```

公開する前は，`cargo build`が`tokenize`などを「使われていない」と警告していた．公開すると，ライブラリの利用者が使う名前になるので警告は消える．

## 0-6 振り返り

1. テストリストを比べるときは，項目の数ではなく，どんな振る舞いを確かめているかを比べる．例えば「演算子と括弧を1つの入力で確かめる」項目は，失敗したときにどちらが原因かを切り分けにくい．
2. `integer`は`parse_to`で`i64`に変換し，変換できなければ失敗する．これは`42`の項目を通したときから成り立っていたので，実装を変えずに通る．それでも項目に残すと，`i64`より大きな整数を扱えるように変えたときに，境界の振る舞いが変わったことにテストで気づける．
3. 単体テストの`1 ? 2`は，`tokenize`の位置の計算(0始まりのバイトの位置に1を足すこと)を確かめる．結合テストの`VALUES (1 ? 2)`は，キーワード，括弧，整数を読んだあとの位置を，公開した名前だけで得られることを確かめる．単体テストだけでは`pub use`の漏れを，結合テストだけでは失敗の原因がどのトークンの処理にあるかを見落としやすい．
4. 結合テストから使えるのは，`lib.rs`が`pub use`した`tokenize`，`LexError`，`Token`，`Keyword`だけである．`lexer::tokens`や`lexer::keyword`は使えない．
5. 模範解答の設計ドキュメントは，照合スクリプトで問題がないことを確かめてある．

```console
$ node ../../../scripts/check-design.mjs .
design: 1 packages, 0 problems
```

## 0-7 発展課題

解答例である．`multispace0`の代わりに，空白とコメントを0個以上読む`separators`を使う．

```rust
use winnow::ascii::{alpha1, digit1, multispace1};
use winnow::combinator::{alt, preceded, repeat, terminated};
use winnow::token::take_till;

fn tokens(input: &mut &str) -> winnow::Result<Vec<Token>> {
    terminated(repeat(0.., preceded(separators, token)), separators).parse_next(input)
}

fn separators(input: &mut &str) -> winnow::Result<()> {
    repeat(0.., alt((multispace1.void(), comment))).parse_next(input)
}

fn comment(input: &mut &str) -> winnow::Result<()> {
    ("--", take_till(0.., '\n')).void().parse_next(input)
}
```

- `take_till(0.., '\n')`は，改行の手前までを読む．改行そのものは，次の`multispace1`が読む．
- `void()`は，結果を捨てて`()`を返す．`repeat`は`()`を集めることもできる．
- `(p, q)`のようにパーサーを組にすると，`p`と`q`を順に読むパーサーになる．
- `1 - -2`は，`-`のあとに空白があるので`--`に一致せず，`symbol`が2つの`-`として読む．
