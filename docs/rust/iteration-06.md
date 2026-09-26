# Iteration 6：バイナリクレートと入出力

Iteration 6では，SQLを対話的に実行するREPLを作る．
このノートでは，実行できるプログラム(バイナリクレート)の作り方，標準入出力の読み書き，入出力をテストできる形に書く方法，書式指定，モジュールの階層を説明する．

## ライブラリクレートとバイナリクレート

1つのパッケージは，ライブラリクレート(`src/lib.rs`)とバイナリクレート(`src/main.rs`)を両方持てる．
`src/main.rs`を作るだけで，Cargoはパッケージ名と同じ名前の実行ファイルを作る．`cargo run`で実行できる．

バイナリクレートからは，同じパッケージのライブラリをクレート名で使う．

```rust
use std::io::{self, IsTerminal};

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let interactive = stdin.is_terminal();
    ferrodb::repl::run(stdin.lock(), io::stdout().lock(), interactive)
}
```

- `main`は`Result`を返せる．`Err`を返すと，エラーを表示して終了コード1で終わる．
- `stdin.lock()`と`stdout.lock()`は，標準入出力を占有して読み書きする値を返す．毎回のロックを省けるので速い．
- `IsTerminal`の`is_terminal()`は，端末につながっているかを返す．ファイルやパイプから読むときは偽である．

処理のほとんどをライブラリに書き，`main`は入出力をつなぐだけにすると，処理をテストで確かめやすい．

## `BufRead`と`Write`

`std::io::BufRead`は行単位で読める入力，`std::io::Write`はバイト列を書ける出力のトレイトである．

- `input.lines()`は，1行ずつ`io::Result<String>`を返すイテレーターである．行末の改行は含まない．
- `write!(output, ...)`と`writeln!(output, ...)`は，書式に従って書く．`writeln!`は最後に改行を書く．どちらも`io::Result<()>`を返す．
- `output.flush()`は，ためている出力を書き出す．改行を書かないプロンプトは，`flush`しないと表示されないことがある．

`io::Result<T>`は`Result<T, io::Error>`の別名で，`?`で伝播できる．

## 引数の`impl Trait`

引数の型に`impl BufRead`と書くと，`BufRead`を実装したどんな型でも受け取れる．

```rust
use std::io::{self, BufRead, Write};

pub fn number_lines(input: impl BufRead, mut output: impl Write) -> io::Result<()> {
    let mut count = 0;
    for line in input.lines() {
        let line = line?;
        count += 1;
        writeln!(output, "{count:>3}: {line}")?;
    }
    Ok(())
}
```

本番では標準入出力を，テストでは文字列のバイト列(`&[u8]`は`BufRead`)と`Vec<u8>`(`Write`)を渡せる．

```rust
let mut output = Vec::new();
number_lines("a\nb\n".as_bytes(), &mut output).unwrap();
assert_eq!(String::from_utf8(output).unwrap(), "  1: a\n  2: b\n");
```

`String::from_utf8`は，バイト列をUTF-8の文字列に変換する．`&mut Vec<u8>`も`Write`を実装しているので，書いたあとも`output`を使える．
型引数を使うジェネリックな関数の書き方は，Iteration 15で扱う．

## 書式指定

`{}`の中に，幅と寄せ方を書ける．

| 書き方 | 意味 | 例 |
| --- | --- | --- |
| `{:>5}` | 幅5で右に寄せる | `[   ab]` |
| `{:<5}` | 幅5で左に寄せる | `[ab   ]` |
| `{:^5}` | 幅5で中央に寄せる．余りは右に置く | `[ ab  ]` |
| `{:>width$}` | 幅を変数`width`で指定する | `[   7]`(`width`が4) |
| `{name:>3}` | 変数`name`を，幅3で右に寄せる | |

幅は文字(`char`)の数で数える．日本語の文字は端末で2文字分の幅に表示されることが多いので，日本語を含む表は端末でそろわないことがある．

文字列を組み立てるメソッドもよく使う．

```rust
assert_eq!("-".repeat(3), "---");
assert_eq!(vec!["a", "b"].join("|"), "a|b");
assert_eq!("x  ".trim_end(), "x");
```

文字列リテラルの行末に`\`を書くと，改行と次の行の先頭の空白を飛ばしてつなげられる．長い期待値を書くときに使える．

```rust
let s = "one \
         two";
assert_eq!(s, "one two");
```

## モジュールの階層

`src/sql.rs`に`pub mod lexer;`と書くと，`src/sql/lexer.rs`がモジュール`sql::lexer`になる．
関連するモジュールを1つの親モジュールにまとめられる．

```text
src/lib.rs          mod sql;
src/sql.rs          pub mod lexer; pub mod parser; ...
src/sql/lexer.rs    crate::sql::lexer
src/sql/parser.rs   crate::sql::parser
```

- 親モジュール`sql`が非公開なら，子モジュールを`pub mod`にしても，クレートの外からは使えない．クレートの中では`crate::sql::lexer::tokenize`のように使える．
- 外に見せる名前は，`lib.rs`で`pub use sql::lexer::tokenize;`のように選ぶ．利用者のコードの`use ferrodb::tokenize`は，モジュールを移しても変わらない．
- `pub mod repl;`のように親を公開すると，`ferrodb::repl::run`のようにモジュールのパスで使える．
