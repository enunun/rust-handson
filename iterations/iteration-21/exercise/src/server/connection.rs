//! 1つの接続で，起動からSimple Queryの問い合わせと終了までのメッセージをやりとりする．

use std::io::{Read, Write};

use crate::database::{Database, QueryResult, StatementResult, TransactionStatus};
use crate::format::cell_text;
use crate::repl::split_statements;
use crate::server::message::{
    BackendMessage, FieldDescription, FrontendMessage, ProtocolError, read_message, read_startup,
    write_message,
};
use crate::value::Value;

/// 起動のあとにクライアントへ知らせるサーバーの設定．
const PARAMETERS: [(&str, &str); 5] = [
    ("server_version", "17.0"),
    ("server_encoding", "UTF8"),
    ("client_encoding", "UTF8"),
    ("DateStyle", "ISO, MDY"),
    ("standard_conforming_strings", "on"),
];

/// 1つの接続を扱う．クライアントが`Terminate`を送るか，接続を閉じたら終わる．
/// 認証は求めず，どの利用者も受け付ける．
pub fn handle<S: Read + Write>(mut stream: S, db: &mut Database) -> Result<(), ProtocolError> {
    loop {
        match read_startup(&mut stream)? {
            FrontendMessage::EncryptionRequest => {
                stream.write_all(b"N")?;
                stream.flush()?;
            }
            FrontendMessage::Startup { .. } => break,
            _ => return Err(ProtocolError::Malformed),
        }
    }
    let mut messages = vec![BackendMessage::AuthenticationOk];
    for (name, value) in PARAMETERS {
        messages.push(BackendMessage::ParameterStatus {
            name: name.to_string(),
            value: value.to_string(),
        });
    }
    messages.push(ready_for_query(db));
    send(&mut stream, &messages)?;
    while let Some(message) = read_message(&mut stream)? {
        match message {
            FrontendMessage::Query(sql) => send(&mut stream, &run_query(db, &sql))?,
            FrontendMessage::Terminate => return Ok(()),
            FrontendMessage::EncryptionRequest | FrontendMessage::Startup { .. } => {
                return Err(ProtocolError::Malformed);
            }
        }
    }
    Ok(())
}

/// メッセージを`Vec<u8>`に並べてから，1回で書く．
/// TCPでは，小さな書き込みを続けると，相手の確認応答を待って遅れることがある．
fn send(stream: &mut impl Write, messages: &[BackendMessage]) -> Result<(), ProtocolError> {
    let mut bytes = Vec::new();
    for message in messages {
        write_message(&mut bytes, message)?;
    }
    stream.write_all(&bytes)?;
    stream.flush()?;
    Ok(())
}

/// Simple Queryの問い合わせを実行し，返すメッセージを並べる．
/// `;`で区切った文を順に実行し，エラーになったら残りの文は実行しない．最後に`ReadyForQuery`を置く．
fn run_query(db: &mut Database, sql: &str) -> Vec<BackendMessage> {
    let mut messages = Vec::new();
    let statements = statements_with_offsets(sql);
    if statements.is_empty() {
        messages.push(BackendMessage::EmptyQueryResponse);
    }
    for (offset, statement) in statements {
        match db.execute(&statement) {
            Ok(result) => messages.extend(result_messages(result)),
            Err(error) => {
                messages.push(BackendMessage::ErrorResponse {
                    code: error.sqlstate().code().to_string(),
                    message: error.message().to_string(),
                    position: error.position().map(|position| position + offset),
                });
                break;
            }
        }
    }
    messages.push(ready_for_query(db));
    messages
}

/// 問い合わせを文に分け，それぞれの文の前にある文字の数と組にする．空の文は除く．
fn statements_with_offsets(sql: &str) -> Vec<(usize, String)> {
    let (mut statements, rest) = split_statements(sql);
    statements.push(rest.trim().to_string());
    let mut result = Vec::new();
    let mut searched = 0;
    for statement in statements {
        if statement.is_empty() {
            continue;
        }
        let start = searched
            + sql[searched..]
                .find(&statement)
                .expect("statement is in the query");
        searched = start + statement.len();
        result.push((sql[..start].chars().count(), statement));
    }
    result
}

/// 文の結果を返すメッセージ．
fn result_messages(result: StatementResult) -> Vec<BackendMessage> {
    match result {
        StatementResult::Rows(result) => rows_messages(result),
        other => vec![BackendMessage::CommandComplete(other.to_string())],
    }
}

/// 問い合わせの結果の列，行，行の数のメッセージ．
fn rows_messages(result: QueryResult) -> Vec<BackendMessage> {
    let fields = result
        .columns
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let (type_oid, type_size) = column_type(&result.rows, index);
            FieldDescription {
                name: name.clone(),
                type_oid,
                type_size,
            }
        })
        .collect();
    let mut messages = vec![BackendMessage::RowDescription(fields)];
    for row in &result.rows {
        let values = row
            .iter()
            .map(|value| match value {
                Value::Null => None,
                other => Some(cell_text(other)),
            })
            .collect();
        messages.push(BackendMessage::DataRow(values));
    }
    messages.push(BackendMessage::CommandComplete(format!(
        "SELECT {}",
        result.rows.len()
    )));
    messages
}

/// 列の型のOIDと大きさ．列の最初の`NULL`でない値の型で決める．すべて`NULL`なら`text`とする．
fn column_type(rows: &[Vec<Value>], column: usize) -> (u32, i16) {
    let value = rows
        .iter()
        .map(|row| &row[column])
        .find(|value| !value.is_null());
    match value {
        Some(Value::Integer(_)) => (23, 4),
        Some(Value::BigInt(_)) => (20, 8),
        Some(Value::Boolean(_)) => (16, 1),
        Some(Value::Varchar(_)) => (1043, -1),
        Some(Value::Null) | None => (25, -1),
    }
}

/// トランザクションの状態を知らせる`ReadyForQuery`．
fn ready_for_query(db: &Database) -> BackendMessage {
    let status = match db.transaction_status() {
        TransactionStatus::Idle => b'I',
        TransactionStatus::InTransaction => b'T',
        TransactionStatus::Failed => b'E',
    };
    BackendMessage::ReadyForQuery { status }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Cursor};

    /// クライアントが送るバイト列を読ませ，サーバーが書いたバイト列をためる接続．
    struct FakeStream {
        input: Cursor<Vec<u8>>,
        output: Vec<u8>,
    }

    impl Read for FakeStream {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.input.read(buf)
        }
    }

    impl Write for FakeStream {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.output.write(buf)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    const USER: &[u8] = b"user\0alice\0\0";

    /// 長さを前に付けた起動のメッセージ．`code`はプロトコルか要求の番号，`rest`は残りの中身である．
    fn startup(code: u32, rest: &[u8]) -> Vec<u8> {
        let len = u32::try_from(8 + rest.len()).unwrap();
        [&len.to_be_bytes()[..], &code.to_be_bytes(), rest].concat()
    }

    /// 種類のバイトと長さを付けたメッセージ．
    fn frontend(kind: u8, body: &[u8]) -> Vec<u8> {
        let len = u32::try_from(4 + body.len()).unwrap();
        [&[kind][..], &len.to_be_bytes(), body].concat()
    }

    /// サーバーが書いたバイト列を，メッセージの種類のバイト列にする．
    fn kinds(mut output: &[u8]) -> Vec<u8> {
        let mut kinds = Vec::new();
        while !output.is_empty() {
            kinds.push(output[0]);
            let len = u32::from_be_bytes(output[1..5].try_into().unwrap());
            output = &output[1 + usize::try_from(len).unwrap()..];
        }
        kinds
    }

    /// `input`を送る接続を扱い，サーバーが書いたバイト列を返す．
    fn run(input: Vec<u8>) -> Vec<u8> {
        let mut stream = FakeStream {
            input: Cursor::new(input),
            output: Vec::new(),
        };
        handle(&mut stream, &mut Database::new()).unwrap();
        stream.output
    }

    fn field(name: &str, type_oid: u32, type_size: i16) -> FieldDescription {
        FieldDescription {
            name: name.to_string(),
            type_oid,
            type_size,
        }
    }

    fn complete(tag: &str) -> BackendMessage {
        BackendMessage::CommandComplete(tag.to_string())
    }

    const IDLE: BackendMessage = BackendMessage::ReadyForQuery { status: b'I' };

    #[test]
    fn startup_is_answered_with_parameters_and_ready_for_query() {
        let output = run(startup(196_608, USER));
        assert_eq!(kinds(&output), b"RSSSSSZ");
        assert!(output.ends_with(&[b'Z', 0, 0, 0, 5, b'I']));
    }

    #[test]
    fn ssl_request_is_refused_with_n() {
        let mut input = startup(80_877_103, b"");
        input.extend(startup(196_608, USER));
        let output = run(input);
        assert_eq!(output[0], b'N');
        assert_eq!(kinds(&output[1..]), b"RSSSSSZ");
    }

    #[test]
    fn queries_are_answered_until_terminate() {
        let mut input = startup(196_608, USER);
        input.extend(frontend(b'Q', b"VALUES (1)\0"));
        input.extend(frontend(b'X', b""));
        input.extend(frontend(b'Q', b"VALUES (2)\0"));
        assert_eq!(kinds(&run(input)), b"RSSSSSZTDCZ");
    }

    #[test]
    fn rows_have_type_oids_and_text_values() {
        let mut db = Database::new();
        assert_eq!(
            run_query(
                &mut db,
                "VALUES (1, 'a', TRUE, NULL), (2, 'bc', FALSE, NULL)"
            ),
            vec![
                BackendMessage::RowDescription(vec![
                    field("COLUMN1", 23, 4),
                    field("COLUMN2", 1043, -1),
                    field("COLUMN3", 16, 1),
                    field("COLUMN4", 25, -1),
                ]),
                BackendMessage::DataRow(vec![
                    Some("1".to_string()),
                    Some("a".to_string()),
                    Some("t".to_string()),
                    None,
                ]),
                BackendMessage::DataRow(vec![
                    Some("2".to_string()),
                    Some("bc".to_string()),
                    Some("f".to_string()),
                    None,
                ]),
                complete("SELECT 2"),
                IDLE,
            ]
        );
    }

    #[test]
    fn statements_separated_by_semicolons_run_in_order() {
        let mut db = Database::new();
        assert_eq!(
            run_query(
                &mut db,
                "CREATE TABLE t (a BIGINT); INSERT INTO t VALUES (1), (2);"
            ),
            vec![complete("CREATE TABLE"), complete("INSERT 0 2"), IDLE]
        );
        assert_eq!(
            run_query(&mut db, "SELECT * FROM t WHERE a > 1"),
            vec![
                BackendMessage::RowDescription(vec![field("A", 20, 8)]),
                BackendMessage::DataRow(vec![Some("2".to_string())]),
                complete("SELECT 1"),
                IDLE,
            ]
        );
    }

    #[test]
    fn empty_query_has_an_empty_query_response() {
        let mut db = Database::new();
        assert_eq!(
            run_query(&mut db, " ; "),
            vec![BackendMessage::EmptyQueryResponse, IDLE]
        );
    }

    #[test]
    fn error_stops_the_query_and_its_position_counts_from_the_query() {
        let mut db = Database::new();
        assert_eq!(
            run_query(&mut db, "VALUES (1); VALUES (1 +); VALUES (3)"),
            vec![
                BackendMessage::RowDescription(vec![field("COLUMN1", 23, 4)]),
                BackendMessage::DataRow(vec![Some("1".to_string())]),
                complete("SELECT 1"),
                BackendMessage::ErrorResponse {
                    code: "42601".to_string(),
                    message: "syntax error at or near \")\"".to_string(),
                    position: Some(24),
                },
                IDLE,
            ]
        );
    }

    #[test]
    fn ready_for_query_reports_the_transaction_status() {
        let mut db = Database::new();
        assert_eq!(
            run_query(&mut db, "START TRANSACTION").last(),
            Some(&BackendMessage::ReadyForQuery { status: b'T' })
        );
        assert_eq!(
            run_query(&mut db, "VALUES (1 / 0)").last(),
            Some(&BackendMessage::ReadyForQuery { status: b'E' })
        );
        assert_eq!(run_query(&mut db, "ROLLBACK").last(), Some(&IDLE));
    }
}
