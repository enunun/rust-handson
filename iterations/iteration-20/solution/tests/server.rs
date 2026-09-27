use std::net::TcpListener;
use std::thread::{self, JoinHandle};

use ferrodb::Database;
use ferrodb::server::connection::handle;
use ferrodb::server::message::ProtocolError;
use postgres::error::{ErrorPosition, SqlState};
use postgres::{Client, NoTls, SimpleQueryMessage};

/// 空いているポートでサーバーを起動し，1つの接続を受け付けて扱うスレッドを作る．
/// そのポートに接続したクライアントと，スレッドを返す．
fn connect() -> (Client, JoinHandle<Result<(), ProtocolError>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        handle(stream, &mut Database::new())
    });
    let config = format!("host=127.0.0.1 port={port} user=alice dbname=ferro");
    let client = Client::connect(&config, NoTls).unwrap();
    (client, server)
}

/// 問い合わせの結果の行を，列の値の`Vec`にする．`NULL`は`None`である．
fn rows(messages: &[SimpleQueryMessage]) -> Vec<Vec<Option<String>>> {
    let mut rows = Vec::new();
    for message in messages {
        if let SimpleQueryMessage::Row(row) = message {
            let values = (0..row.len())
                .map(|index| row.get(index).map(|text| text.to_string()))
                .collect();
            rows.push(values);
        }
    }
    rows
}

#[test]
fn client_receives_rows_as_text() {
    let (mut client, _) = connect();
    client
        .batch_execute(
            "CREATE TABLE t (a INTEGER, b VARCHAR(10)); INSERT INTO t VALUES (1, 'x'), (2, NULL)",
        )
        .unwrap();
    let messages = client.simple_query("SELECT * FROM t").unwrap();
    assert_eq!(
        rows(&messages),
        vec![
            vec![Some("1".to_string()), Some("x".to_string())],
            vec![Some("2".to_string()), None],
        ]
    );
    let SimpleQueryMessage::RowDescription(columns) = &messages[0] else {
        panic!("expected a row description: {messages:?}");
    };
    let names: Vec<&str> = columns.iter().map(|column| column.name()).collect();
    assert_eq!(names, ["A", "B"]);
    assert!(matches!(
        messages.last(),
        Some(SimpleQueryMessage::CommandComplete(2))
    ));
}

#[test]
fn client_receives_the_sqlstate_and_position_of_an_error() {
    let (mut client, _) = connect();
    let error = client.simple_query("VALUES (1 +)").unwrap_err();
    assert_eq!(error.code(), Some(&SqlState::SYNTAX_ERROR));
    let error = error.as_db_error().unwrap();
    assert_eq!(error.message(), "syntax error at or near \")\"");
    assert_eq!(error.position(), Some(&ErrorPosition::Original(12)));
}

#[test]
fn transaction_spans_several_queries() {
    let (mut client, _) = connect();
    client
        .batch_execute("CREATE TABLE t (a INTEGER); START TRANSACTION")
        .unwrap();
    client.batch_execute("INSERT INTO t VALUES (1)").unwrap();
    client.batch_execute("ROLLBACK").unwrap();
    let messages = client.simple_query("SELECT * FROM t").unwrap();
    assert_eq!(rows(&messages), Vec::<Vec<Option<String>>>::new());
}

#[test]
fn server_finishes_when_the_client_terminates() {
    let (client, server) = connect();
    client.close().unwrap();
    assert!(server.join().unwrap().is_ok());
}
