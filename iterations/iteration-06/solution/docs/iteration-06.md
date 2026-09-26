# Iteration 6：REPL(解説)

演習の各手順の模範解答と，その考え方を説明する．

## 6-1 準備

引き継いだ141個のテストがすべて通れば準備は終わりである．

## 6-2 文法と概念

課題の解答例である．

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t() {
        let mut output = Vec::new();
        number_lines("a\nb\n".as_bytes(), &mut output).unwrap();
        assert_eq!(String::from_utf8(output).unwrap(), "  1: a\n  2: b\n");
        assert_eq!(format!("[{:>5}]", "ab"), "[   ab]");
        assert_eq!(format!("[{:<5}]", "ab"), "[ab   ]");
        assert_eq!(format!("[{:^5}]", "ab"), "[ ab  ]");
        assert_eq!("-".repeat(3), "---");
        assert_eq!(vec!["a", "b"].join("|"), "a|b");
        assert_eq!("x  ".trim_end(), "x");
    }
}
```

中央寄せで余る1文字分の空白は，右側に置かれる．

## 6-3 テストリスト

模範解答は[TESTLIST.md](../TESTLIST.md)である．

- 表の整形(`format`)，文の区切り(`repl`の`split_statements`)，REPL全体(`repl::run`)の3つに分けた．整形と区切りは単体テストで境界を細かく確かめ，REPLの結合テストでは組み合わせを確かめる．
- 表の整形は，コマンドタグ，列の幅と寄せ方，見出しの中央寄せ，`NULL`と真偽値，行の数，文字の幅の順にした．
- REPLの結合テストでは，出力の文字列全体を比べる．文字列の行末に`\`を書くと，期待値を行ごとに分けて書ける．

## 6-4 設計ドキュメント

| ファイル | 変更 | 理由 |
| --- | --- | --- |
| `c4-context.md` | REPLを使う利用者を加えた | 人が直接SQLを入力するようになった |
| `c4-container.md` | REPLのバイナリを加えた | 実行される単位が2つになった |
| `c4-component.md` | `main`，`repl`，`format`を加え，`sql`と`exec`を境界で囲んだ | モジュールの階層を反映した |
| `code-types.md` | `StatementResult`と`QueryResult`の`Display`を説明に加えた | 新しい型はない |
| `code-sequence.md` | REPLの流れの図を加えた | `INSERT`の図とは別の視点なので，見出しを分けた |

- `repl`から`format`への矢印はない．`repl`は`writeln!(output, "{result}")`と書くだけで，`format`の関数を名前で呼ばない．`Display`の実装を通して使う依存は，モジュールの依存には現れない．
- `sql`と`exec`は子モジュールを宣言するだけなので，Componentではなく境界として描いた．

## 6-5 テスト駆動の実装

### モジュールの階層の整理

振る舞いを変えないリファクタリングなので，テストを加えずに行った．
ファイルを移し，`src/sql.rs`と`src/exec.rs`で子モジュールを宣言した．

```rust
//! SQLの文字列を文の構文木にする．

pub mod ast;
pub mod lexer;
pub mod parser;
pub mod token;
```

各ファイルの`use crate::lexer::...`を`use crate::sql::lexer::...`のように直し，`lib.rs`の`pub use`も新しいパスにした．
公開する名前は変わらないので，結合テストの`use ferrodb::...`は変わらない．移したあと，すべてのテストが通ることを確かめた．

### コマンドタグ

```rust
#[test]
fn command_tags() {
    assert_eq!(StatementResult::CreateTable.to_string(), "CREATE TABLE");
    assert_eq!(StatementResult::Insert { count: 2 }.to_string(), "INSERT 0 2");
}
```

`src/format.rs`を作り，`StatementResult`に`Display`を実装した．問い合わせの結果は，`QueryResult`の`Display`に任せる．

```rust
impl fmt::Display for StatementResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StatementResult::Rows(result) => write!(f, "{result}"),
            StatementResult::CreateTable => f.write_str("CREATE TABLE"),
            StatementResult::Insert { count } => write!(f, "INSERT 0 {count}"),
        }
    }
}
```

### 表の整形

```rust
#[test]
fn columns_are_as_wide_as_their_widest_cell() {
    let table = result(
        &["ID", "NAME"],
        vec![
            vec![Value::Integer(1), Value::Varchar("alice".to_string())],
            vec![Value::Integer(20), Value::Varchar("bob".to_string())],
        ],
    );
    assert_eq!(
        table.to_string(),
        " ID | NAME\n----+-------\n  1 | alice\n 20 | bob\n(2 rows)"
    );
}
```

列ごとの幅を求めてから，見出し，区切り線，各行を書く．見出しの中央寄せ，`NULL`と真偽値，行の数，文字の幅の項目も，この実装で通る．

```rust
impl fmt::Display for QueryResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut widths = Vec::new();
        for column in &self.columns {
            widths.push(column.chars().count());
        }
        for row in &self.rows {
            for (index, value) in row.iter().enumerate() {
                let width = cell_text(value).chars().count();
                if width > widths[index] {
                    widths[index] = width;
                }
            }
        }

        let mut header = Vec::new();
        let mut separator = Vec::new();
        for (column, &width) in self.columns.iter().zip(&widths) {
            header.push(format!(" {column:^width$} "));
            separator.push("-".repeat(width + 2));
        }
        writeln!(f, "{}", header.join("|").trim_end())?;
        writeln!(f, "{}", separator.join("+"))?;

        for row in &self.rows {
            let mut cells = Vec::new();
            for (value, &width) in row.iter().zip(&widths) {
                let text = cell_text(value);
                if is_numeric(value) {
                    cells.push(format!(" {text:>width$} "));
                } else {
                    cells.push(format!(" {text:<width$} "));
                }
            }
            writeln!(f, "{}", cells.join("|").trim_end())?;
        }

        match self.rows.len() {
            1 => write!(f, "(1 row)"),
            count => write!(f, "({count} rows)"),
        }
    }
}

fn cell_text(value: &Value) -> String {
    match value {
        Value::Integer(n) => n.to_string(),
        Value::BigInt(n) => n.to_string(),
        Value::Boolean(true) => "t".to_string(),
        Value::Boolean(false) => "f".to_string(),
        Value::Varchar(s) => s.clone(),
        Value::Null => String::new(),
    }
}
```

幅が2の列に1文字の見出しを中央寄せする項目では，最初に期待値を`"  N"`と書いて失敗した．Rustの`{:^}`は余りの空白を右に置くので，`" N"`が正しい．

```text
assertion `left == right` failed
  left: " N\n----\n 日本\n(1 row)"
 right: "  N\n----\n 日本\n(1 row)"
```

### 文の区切り

```rust
#[test]
fn semicolon_in_quotes_does_not_end_a_statement() {
    assert_eq!(
        split_statements("VALUES ('a;''b', \"c;\");"),
        (strings(&["VALUES ('a;''b', \"c;\")"]), String::new())
    );
}
```

引用符の中にいるかを`Option<char>`で覚える．`''`は，引用符を閉じてすぐ開くのと同じ動きになるので，特別に扱わなくてよい．

```rust
pub fn split_statements(buffer: &str) -> (Vec<String>, String) {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for c in buffer.chars() {
        match quote {
            Some(open) => {
                if c == open {
                    quote = None;
                }
                current.push(c);
            }
            None => {
                if c == ';' {
                    statements.push(current.trim().to_string());
                    current.clear();
                } else {
                    if c == '\'' || c == '"' {
                        quote = Some(c);
                    }
                    current.push(c);
                }
            }
        }
    }
    (statements, current)
}
```

### REPL

```rust
fn run(input: &str, interactive: bool) -> String {
    let mut output = Vec::new();
    repl::run(input.as_bytes(), &mut output, interactive).unwrap();
    String::from_utf8(output).unwrap()
}

#[test]
fn shows_an_error_and_continues() {
    assert_eq!(
        run("VALUES (1 +);\nVALUES (2);\n", false),
        "ERROR:  syntax error at or near \")\"\n COLUMN1\n---------\n       2\n(1 row)\n\n"
    );
}
```

`repl::run`は，読んだ行をまだ終わっていない文に足し，終わった文を順に実行する．入力の終わりには，残った文も実行する．

```rust
pub fn run(input: impl BufRead, mut output: impl Write, interactive: bool) -> io::Result<()> {
    let mut database = Database::new();
    let mut pending = String::new();
    if interactive {
        write_prompt(&mut output, &pending)?;
    }
    for line in input.lines() {
        pending.push_str(&line?);
        pending.push('\n');
        let (statements, rest) = split_statements(&pending);
        pending = rest;
        for statement in statements {
            execute(&mut database, &statement, &mut output)?;
        }
        if interactive {
            write_prompt(&mut output, &pending)?;
        }
    }
    execute(&mut database, &pending, &mut output)?;
    if interactive {
        writeln!(output)?;
    }
    Ok(())
}

fn execute(database: &mut Database, statement: &str, output: &mut impl Write) -> io::Result<()> {
    let statement = statement.trim();
    if statement.is_empty() {
        return Ok(());
    }
    match database.execute(statement) {
        Ok(result) => {
            writeln!(output, "{result}")?;
            if matches!(result, StatementResult::Rows(_)) {
                writeln!(output)?;
            }
        }
        Err(error) => writeln!(output, "ERROR:  {error}")?,
    }
    Ok(())
}
```

プロンプトの項目では，文の途中かどうかで2つのプロンプトを書き分ける．

```rust
fn write_prompt(output: &mut impl Write, pending: &str) -> io::Result<()> {
    if pending.trim().is_empty() {
        write!(output, "{PROMPT}")?;
    } else {
        write!(output, "{CONTINUATION_PROMPT}")?;
    }
    output.flush()
}
```

最後に`src/main.rs`を作り，標準入出力をつないだ．`cargo run`で起動した対話的なセッションは，使用例のとおりになる．

## 6-6 振り返り

1. 表の整形，文の区切り，REPL全体の3つに分けて項目を挙げたかを比べる．
2. 関数の中で標準入出力を使うと，テストはバイナリを子プロセスとして起動し，標準入出力をつないで確かめることになる．テストが遅くなり，バイナリの名前にも依存する．入出力を引数に取れば，文字列だけでテストできる．
3. `repl::run`の中で調べると，テストでは常に「端末ではない」になり，プロンプトの表示を確かめられない．判断を呼び出し側(`main`)に移すと，両方の場合をテストできる．
4. `Display`の実装にすると，REPL以外(例えばIteration 20のサーバーのログ)からも`{}`で同じ表示を使える．`repl`の関数にすると，表示の規則がREPLに閉じる．
5. 字句解析器は，閉じていない文字列リテラルをエラーにする．入力の途中で文字列が閉じていないのは誤りではなく「続きがある」ことなので，字句解析のエラーと区別する処理が別に要る．引用符と`;`だけを見る小さな関数のほうが，目的に合っている．
6. 模範解答の設計ドキュメントは，照合スクリプトで問題がないことを確かめてある．

## 6-7 発展課題

解答例である．`Catalog`と`Database`に，表の名前を返すメソッドを加えた．`repl::run`は，`\`で始まる行を文より先に調べる．

```rust
    /// 表の名前を，名前の順に返す．
    pub fn table_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        for name in self.tables.keys() {
            names.push(name.clone());
        }
        names.sort();
        names
    }
```

```rust
    for line in input.lines() {
        let line = line?;
        if pending.trim().is_empty() && line.trim_start().starts_with('\\') {
            match line.trim() {
                "\\q" => return Ok(()),
                "\\dt" => {
                    for name in database.table_names() {
                        writeln!(output, "{name}")?;
                    }
                }
                command => writeln!(output, "invalid command {command}")?,
            }
            if interactive {
                write_prompt(&mut output, &pending)?;
            }
            continue;
        }
        pending.push_str(&line);
        // 以降は同じ
    }
```

```rust
#[test]
fn backslash_commands() {
    assert_eq!(
        run("CREATE TABLE b (x INTEGER);\nCREATE TABLE a (x INTEGER);\n\\dt\n\\x\n\\q\nVALUES (1);\n", false),
        "CREATE TABLE\nCREATE TABLE\nA\nB\ninvalid command \\x\n"
    );
}
```

- `HashMap`の`keys()`は，キーを順不同で返す．名前の順に表示するため，`sort`で並べ替えた．
- `continue`は，ループの残りを飛ばして次の行に進む．
- `match`の最後の腕の`command`は，どの文字列にも一致して，その値に名前を付ける．
