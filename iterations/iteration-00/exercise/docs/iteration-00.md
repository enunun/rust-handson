# Iteration 0：プロジェクトの作成と字句解析

このIterationでは，Cargoで`ferrodb`のパッケージを作り，SQLの文字列をトークンの列に分ける字句解析器を作る．
テストリスト → 設計ドキュメント → テスト駆動の実装 → 設計レビューという，全Iterationに共通する流れを初めて1周する．

## 0-1 準備

### パッケージを作る

ターミナルで，このディレクトリ(`iterations/iteration-00/exercise`)に移動する．

```console
cd iterations/iteration-00/exercise
```

`cargo init`で，このディレクトリをライブラリクレート`ferrodb`のパッケージにする．

```console
$ cargo init --lib --name ferrodb
    Creating library package
note: see more `Cargo.toml` keys and their definitions at https://doc.rust-lang.org/cargo/reference/manifest.html
```

`Cargo.toml`と`src/lib.rs`ができる．`src/lib.rs`には，サンプルの関数`add`とそのテストが入っている．

```rust
pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
```

このディレクトリは，リポジトリのルートの`Cargo.toml`(模範解答のワークスペース)から除外してある．
そのため，このIterationの`cargo`のコマンドはすべて，このディレクトリの中で実行する．

### ビルドとテスト

`cargo build`でビルドし，`cargo test`でサンプルのテストを実行する．

```console
$ cargo build
   Compiling ferrodb v0.1.0 (/workspaces/iterations/iteration-00/exercise)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.08s
$ cargo test
   Compiling ferrodb v0.1.0 (/workspaces/iterations/iteration-00/exercise)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.11s
     Running unittests src/lib.rs (target/debug/deps/ferrodb-2e30a97474e6398f)

running 1 test
test tests::it_works ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests ferrodb

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

### 依存を追加する

字句解析には，解析ライブラリwinnowを使う．`cargo add`で依存に追加する．

```console
$ cargo add winnow
    Updating crates.io index
      Adding winnow v1.0.4 to dependencies
             Features:
             + alloc
             + ascii
             + binary
             + parser
             + std
             - debug
             - simd
             - unstable-doc
             - unstable-recover
    Updating crates.io index
     Locking 2 packages to latest Rust 1.98.1 compatible versions
      Adding memchr v2.8.3
      Adding winnow v1.0.4
```

`Cargo.toml`の`[dependencies]`に`winnow = "1.0.4"`が加わったことを確かめる．

## 0-2 文法と概念

次の2つのノートを読む．

- [Rustのノート](../../../../docs/rust/iteration-00.md)：Cargo，関数，`enum`，`struct`，`Option`と`Result`，`match`，テスト，モジュール，winnow
- [データベースのノート](../../../../docs/db/iteration-00.md)：SQLの処理の流れ，トークン，`VALUES`

読み終えたら，`src/lib.rs`の`mod tests`の中にテストを書いて，次の課題を確かめる．
RustにはREPLがないので，このように小さなテストを書いて`cargo test`で動かす．

1. `"42"`，`"-7"`，`"4 2"`を`parse::<i64>()`で整数に変換すると，それぞれどうなるか．`assert_eq!`と`assert!`で確かめる．
2. 列挙子`Red`とデータを持つ列挙子`Rgb(u8, u8, u8)`からなる`enum Color`を定義する．`Debug`と`PartialEq`を導出し，`Color::Rgb(255, 0, 0)`を`{:?}`で表示した文字列を確かめる．
3. `'+'`を読んだら`1`，`'-'`を読んだら`-1`を返すwinnowのパーサー関数`sign`を書く．`"+-"`に続けて2回適用したときの結果と，残りの入力を確かめる．
4. `sign`を0回以上くり返して`Vec<i64>`を返すパーサー関数`signs`を書く．`signs.parse("+-+")`の結果と，`signs.parse("+x")`のエラーの`offset()`を確かめる．

確かめ終えたら，`src/lib.rs`の中身をすべて消す．0-5で`ferrodb`のコードを書き始める．

## 0-3 テストリスト

作るものの要件と使用例を読み，確かめるべき振る舞いを`TESTLIST.md`に書き出す．
テストリストの書き方は[テスト駆動開発とテストリスト](../../../../docs/tdd.md)を読む．

### 要件

- SQLの文字列を，トークンの列に分ける．
- 対象は`VALUES (1, 2 + 3)`の形の文である．キーワード`VALUES`，整数，`(`，`)`，`,`，`+`，`-`，`*`，`/`を扱う．
- キーワードは大文字と小文字を区別しない．
- 空白と改行は読み飛ばす．
- 整数は64ビット符号付き整数(`i64`)として読む．
- 解釈できない文字，`VALUES`以外の単語，`i64`に収まらない整数があれば，その位置(先頭の文字を1として数えた番号)を含むエラーを返す．

### 使用例

```rust
use ferrodb::{tokenize, Keyword, LexError, Token};

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
assert_eq!(tokenize("VALUES (1 ? 2)"), Err(LexError { position: 11 }));
```

### 作るもの

| モジュール | 作るもの |
| --- | --- |
| `token` | `enum Token`(上の使用例の列挙子)，`enum Keyword`(`Values`) |
| `lexer` | `pub fn tokenize(sql: &str) -> Result<Vec<Token>, LexError>`，`pub struct LexError { pub position: usize }` |
| ルート(`lib.rs`) | `tokenize`，`LexError`，`Token`，`Keyword`を`pub use`で公開する |

### 書くときに考えること

- 1つの項目で1つの振る舞いを確かめる．例えば，演算子と括弧は別の項目にできる．
- 境界を考える．入力が空のとき，`i64`の最大値と，それを1つ超える値はどうなるか．
- エラーの位置は，どの文字を指すべきか．単語や整数の途中ではなく，先頭を指す．
- 単体テスト(`lexer`の中)と結合テスト(`tests/`から公開APIを呼ぶ)に分ける．使用例は結合テストにする．

## 0-4 設計ドキュメント

[設計ドキュメントの書き方](../../../../docs/design.md)を読み，`design/`の5つのファイルに，このIterationの終わりの状態を描く．
各ファイルには，描くものを説明するコメントが入っている．コメントは描き終えたら消す．

- `c4-context.md`：このIterationの`ferrodb`を使うのは誰か．プログラムから呼ぶのか，人が直接操作するのか．
- `c4-container.md`：`ferrodb`は，いくつの実行される単位からなるか．ライブラリか，プログラムか．
- `c4-component.md`：「作るもの」の3つのモジュールのうち，どれがどれの項目を使うか．`pub use`も依存に数える．
- `code-types.md`：`Token`の列挙子のうち，別の型を持つものはどれか．`LexError`のフィールドは何か．
- `code-sequence.md`：`tokenize`は，入力の終わりまで何をくり返すか．成功と失敗で何を返すか．

描き終えたら，Mermaidの構文を検査する．

```console
$ node ../../../scripts/check-mermaid.mjs design/*.md
mermaid: 5 blocks, 0 errors
```

## 0-5 テスト駆動の実装

`TESTLIST.md`の項目を上から1つずつ，Red → Green → Refactorで実装する．
テストを書いたら，実装の前に`cargo test`を実行して，失敗することを確かめる．

### モジュールを用意する

`src/lib.rs`に2つのモジュールを宣言し，`src/token.rs`と`src/lexer.rs`を作る．
`lexer`の単体テストは，`src/lexer.rs`の末尾の`#[cfg(test)] mod tests`に書く．

```rust
mod lexer;
mod token;
```

### 実装のヒント

- 最初のテストは，`tokenize`がまだないのでコンパイルエラーになる．これもRedである．空の`Vec`を返すだけの関数で通す．
- 使わない引数はコンパイラーが警告する．仮実装の間は，引数の名前を`_sql`のように`_`で始める．
- 整数は`digit1`で数字を読み，`parse_to`で`i64`に変換する．`map(Token::Integer)`で`Token`にする．
- トークンを0個以上読むには`repeat(0.., パーサー)`を使い，入力全体を読むには`parse`を使う．`parse`の結果は`match`で`Ok`と`Err`に分け，`Err`を`LexError`に変える．
- 記号は`'('.value(Token::LParen)`のように書き，`alt`で並べる．`value`を使うには，`Token`と`Keyword`に`Clone`を導出する．
- `"VALUES"`というパーサーは大文字と小文字を区別する．大文字と小文字を区別しないためには，英字の並びを`alpha1`で読んでから，`verify_map`でキーワードかどうかを判定する．
- 空白は，各トークンの前で`preceded(multispace0, ...)`として読み飛ばす．最後のトークンのあとの空白は，`terminated(..., multispace0)`で読み飛ばす．
- `ParseError`の`offset()`は，先頭を0とするバイトの位置である．要件の位置は先頭を1と数える．
- `tokenize`の中の式が長くなったら，パーサー関数に切り出す(Refactor)．

### ツールの操作

- 単体テストだけを実行するには`cargo test --lib`，結合テストの1つのファイルだけを実行するには`cargo test --test tokenize`を使う．
- `cargo test keyword`のように名前の一部を渡すと，その文字列を名前に含むテストだけを実行する．
- 結合テストは`tests/tokenize.rs`に書く．`lib.rs`で`pub use`する前は，`use ferrodb::tokenize;`がコンパイルエラーになる．
- 項目を1つ通すたびに`cargo fmt`で整形する．最後に`cargo clippy`を実行し，警告がないことを確かめる．
- `cargo build`は，どこからも使われていない関数を警告する．`pub use`で公開すると，警告は消える．

## 0-6 振り返り

1. 自分の`TESTLIST.md`を[模範解答のテストリスト](../../solution/TESTLIST.md)と比べる．自分にない項目，自分にだけある項目はどれか．その項目は必要か．
2. `i64`の最大値を1つ超える整数の項目は，テストを書いただけで通った可能性が高い．なぜ実装を変えずに通るのか．それでもテストリストに残す理由は何か．
3. 単体テストと結合テストの両方で，エラーの位置を確かめている．それぞれのテストは何を確かめているか．片方だけでは何を見落とすか．
4. `lexer`と`token`を非公開のモジュールにして，必要な名前だけを`pub use`で公開した．結合テストから使える名前はどれか．
5. 設計ドキュメントと実装を見比べ，違うところがあれば設計ドキュメントを直す．Component図と型の図は，次のコマンドでコードと照合できる．

```console
node ../../../scripts/check-design.mjs .
```

## 0-7 発展課題

標準SQLでは，`--`から行末までがコメント(区切り)である．
`tokenize`が，空白と同じようにコメントを読み飛ばすようにする．

```rust
assert_eq!(
    tokenize("VALUES -- first row\n(1)"),
    Ok(vec![
        Token::Keyword(Keyword::Values),
        Token::LParen,
        Token::Integer(1),
        Token::RParen,
    ])
);
```

これまでと同じく，テストリスト，設計ドキュメント，実装の順に進める．
`1 - -2`の`- -`は，コメントではなく2つの`-`である．
