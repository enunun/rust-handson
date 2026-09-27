//! 文の結果を，REPLに表示する文字列にする．

use std::fmt;

use crate::database::{QueryResult, StatementResult};
use crate::value::Value;

impl fmt::Display for StatementResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StatementResult::Rows(result) => write!(f, "{result}"),
            StatementResult::CreateTable => f.write_str("CREATE TABLE"),
            StatementResult::Insert { count } => write!(f, "INSERT 0 {count}"),
            StatementResult::Update { count } => write!(f, "UPDATE {count}"),
            StatementResult::Delete { count } => write!(f, "DELETE {count}"),
            StatementResult::DropTable => f.write_str("DROP TABLE"),
            StatementResult::CreateIndex => f.write_str("CREATE INDEX"),
            StatementResult::DropIndex => f.write_str("DROP INDEX"),
            StatementResult::StartTransaction => f.write_str("START TRANSACTION"),
            StatementResult::Commit => f.write_str("COMMIT"),
            StatementResult::Rollback => f.write_str("ROLLBACK"),
            StatementResult::Checkpoint => f.write_str("CHECKPOINT"),
        }
    }
}

/// 列を縦線で区切った表にする．見出しは中央，数値は右，それ以外は左に寄せる．
/// 最後の行に行の数を書く．各行の末尾の空白は取り除く．
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

/// 表の1つのセルに書く文字列．`NULL`は空，真偽値は`t`と`f`にする．
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

fn is_numeric(value: &Value) -> bool {
    matches!(value, Value::Integer(_) | Value::BigInt(_))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(columns: &[&str], rows: Vec<Vec<Value>>) -> QueryResult {
        let mut names = Vec::new();
        for column in columns {
            names.push(column.to_string());
        }
        QueryResult {
            columns: names,
            rows,
        }
    }

    #[test]
    fn command_tags() {
        assert_eq!(StatementResult::CreateTable.to_string(), "CREATE TABLE");
        assert_eq!(
            StatementResult::Insert { count: 2 }.to_string(),
            "INSERT 0 2"
        );
        assert_eq!(StatementResult::Update { count: 0 }.to_string(), "UPDATE 0");
        assert_eq!(StatementResult::Delete { count: 3 }.to_string(), "DELETE 3");
        assert_eq!(StatementResult::DropTable.to_string(), "DROP TABLE");
        assert_eq!(StatementResult::CreateIndex.to_string(), "CREATE INDEX");
        assert_eq!(StatementResult::DropIndex.to_string(), "DROP INDEX");
        assert_eq!(
            StatementResult::StartTransaction.to_string(),
            "START TRANSACTION"
        );
        assert_eq!(StatementResult::Commit.to_string(), "COMMIT");
        assert_eq!(StatementResult::Rollback.to_string(), "ROLLBACK");
        assert_eq!(StatementResult::Checkpoint.to_string(), "CHECKPOINT");
    }

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

    #[test]
    fn header_is_centered() {
        let table = result(&["A"], vec![vec![Value::Varchar("abc".to_string())]]);
        assert_eq!(table.to_string(), "  A\n-----\n abc\n(1 row)");
    }

    #[test]
    fn null_is_empty_and_booleans_are_t_or_f() {
        let table = result(
            &["A", "B", "C"],
            vec![vec![
                Value::Null,
                Value::Boolean(true),
                Value::Boolean(false),
            ]],
        );
        assert_eq!(
            table.to_string(),
            " A | B | C\n---+---+---\n   | t | f\n(1 row)"
        );
    }

    #[test]
    fn empty_result_has_zero_rows() {
        let table = result(&["ID"], vec![]);
        assert_eq!(table.to_string(), " ID\n----\n(0 rows)");
    }

    #[test]
    fn width_is_counted_in_characters() {
        let table = result(&["N"], vec![vec![Value::Varchar("日本".to_string())]]);
        assert_eq!(table.to_string(), " N\n----\n 日本\n(1 row)");
    }
}
