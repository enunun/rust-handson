//! 対話的にSQLを受け付けて実行するREPL．

use std::io::{self, BufRead, Write};

use crate::database::{Database, StatementResult};

const PROMPT: &str = "ferrodb> ";
const CONTINUATION_PROMPT: &str = "      -> ";

/// `input`から行を読み，`;`で終わる文ごとに実行して，結果を`output`に書く．
/// `interactive`が真なら，プロンプトを表示する．
/// 入力が終わったとき，`;`で終わっていない文が残っていれば，それも実行する．
pub fn run(input: impl BufRead, output: impl Write, interactive: bool) -> io::Result<()> {
    run_with(Database::new(), input, output, interactive)
}

/// `database`で文を実行する`run`．データディレクトリを開いたデータベースを渡す．
pub fn run_with(
    mut database: Database,
    input: impl BufRead,
    mut output: impl Write,
    interactive: bool,
) -> io::Result<()> {
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

/// `buffer`から`;`で終わる文を取り出す．引用符の中の`;`は文の終わりとみなさない．
/// 取り出した文と，まだ終わっていない残りを返す．
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

/// 空でない文を実行し，結果かエラーを書く．問い合わせの結果のあとには空行を書く．
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

fn write_prompt(output: &mut impl Write, pending: &str) -> io::Result<()> {
    if pending.trim().is_empty() {
        write!(output, "{PROMPT}")?;
    } else {
        write!(output, "{CONTINUATION_PROMPT}")?;
    }
    output.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        let mut strings = Vec::new();
        for item in items {
            strings.push(item.to_string());
        }
        strings
    }

    #[test]
    fn statement_ends_at_a_semicolon() {
        assert_eq!(
            split_statements("VALUES (1);"),
            (strings(&["VALUES (1)"]), String::new())
        );
    }

    #[test]
    fn text_after_the_last_semicolon_is_left_over() {
        assert_eq!(
            split_statements("VALUES (1); VALUES"),
            (strings(&["VALUES (1)"]), " VALUES".to_string())
        );
    }

    #[test]
    fn several_statements_on_one_line() {
        assert_eq!(
            split_statements("VALUES (1);VALUES (2);"),
            (strings(&["VALUES (1)", "VALUES (2)"]), String::new())
        );
    }

    #[test]
    fn semicolon_in_quotes_does_not_end_a_statement() {
        assert_eq!(
            split_statements("VALUES ('a;''b', \"c;\");"),
            (strings(&["VALUES ('a;''b', \"c;\")"]), String::new())
        );
    }

    #[test]
    fn statement_may_span_lines() {
        assert_eq!(
            split_statements("VALUES (1,\n2);\n"),
            (strings(&["VALUES (1,\n2)"]), "\n".to_string())
        );
    }
}
