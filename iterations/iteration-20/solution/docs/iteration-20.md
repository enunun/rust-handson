# Iteration 20：PostgreSQL互換プロトコル(解説)

演習の各手順の模範解答と，その考え方を説明する．

## 20-1 準備

引き継いだ453個のテストがすべて通れば準備は終わりである．
Iteration 19の`ferrodb`にはサブコマンド`repl`しかないので，`psql`から接続する先はまだない．

## 20-2 文法と概念

課題1〜3の解答例は，[Rustのノート](../../../../docs/rust/iteration-20.md)の`write_frame`，`read_frame`，`echo_upper`，`FakeStream`と，`thread::spawn`の例である．`tests/`に置いた結合テストで確かめた．

```rust
#[test]
fn frames_in_memory() {
    let mut bytes = Vec::new();
    write_frame(&mut bytes, "hi").unwrap();
    assert_eq!(bytes, [0, 0, 0, 2, b'h', b'i']);
    let mut input = Cursor::new(bytes);
    assert_eq!(read_frame(&mut input).unwrap(), Some("hi".to_string()));
    assert_eq!(read_frame(&mut input).unwrap(), None);
}

#[test]
fn echo_with_fake_stream() {
    let mut stream = FakeStream {
        input: Cursor::new(b"\0\0\0\x02hi".to_vec()),
        output: Vec::new(),
    };
    echo_upper(&mut stream).unwrap();
    assert_eq!(stream.output, b"\0\0\0\x02HI");
}

#[test]
fn echo_over_tcp() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        echo_upper(stream)
    });
    let mut client = TcpStream::connect(addr).unwrap();
    write_frame(&mut client, "ferro").unwrap();
    assert_eq!(read_frame(&mut client).unwrap(), Some("FERRO".to_string()));
    drop(client);
    assert!(server.join().unwrap().is_ok());
}
```

- `read_frame`は，メッセージの先頭で接続が閉じると`read_exact`の`UnexpectedEof`を`None`にする．
- 課題4では，`&[u8]`が`Write`を実装しないというエラー(E0277)になる．エラーの全文はノートにある．

## 20-3 テストリスト

模範解答は[TESTLIST.md](../TESTLIST.md)である．

- メッセージのバイト配置は`server::message`の単体テストで，起動からの流れと問い合わせへの返事は`server::connection`の単体テストで，実際のクライアントとのやりとりは結合テスト`tests/server.rs`で確かめた．
- `server::connection`の単体テストは2種類にした．起動と終了は，`handle`に`FakeStream`を渡し，書かれたメッセージの種類のバイトを並べて比べる．問い合わせへの返事は，`handle`から分けた`run_query`が返す`BackendMessage`の`Vec`を比べる．
- 引き継いだテストの期待値は変わらない．

## 20-4 設計ドキュメント

| ファイル | 変更 | 理由 |
| --- | --- | --- |
| `c4-context.md` | PostgreSQLのクライアントを外部のシステムとして加えた | 利用者が`psql`から使うようになった |
| `c4-container.md` | サブコマンド`serve`のプロセスと，クライアントからのTCPの接続を加えた | 別のプロセスからネットワークでつながる |
| `c4-component.md` | `server::connection`と`server::message`を加え，`main`から`server::connection`への依存と，`server::connection`から`server::message`，`database`，`repl`，`format`，`value`への依存を加えた | 接続を扱うモジュールができた |
| `code-types.md` | サーバーの図を加えた | メッセージの型ができた |
| `layout.md` | 起動のメッセージ，そのあとのメッセージ，各メッセージの内容を加えた | プロトコルのバイト配置ができた |
| `code-sequence.md` | 接続から`Terminate`までのやりとりを加えた | 起動とSimple Queryの流れができた |

- `server::connection`は，文の区切りに`repl::split_statements`を，値の文字列に`format::cell_text`を使う．REPLと同じ規則で文を区切り，同じ文字列で値を返す．`cell_text`は`format`の中だけで使っていたので，`pub`にした．
- `server::message`は，ほかのモジュールを参照しない．バイト列とメッセージの変換だけを受け持つ．

## 20-5 テスト駆動の実装

### メッセージの読み書き

起動のメッセージは，長さのあとの4バイトの番号で種類を見分ける．

```rust
pub fn read_startup(input: &mut impl Read) -> Result<FrontendMessage, ProtocolError> {
    let body = read_body(input)?;
    let (code, mut rest) = body.split_at_checked(4).ok_or(ProtocolError::Malformed)?;
    match u32::from_be_bytes(code.try_into().expect("4 bytes")) {
        SSL_REQUEST | GSSENC_REQUEST => Ok(FrontendMessage::EncryptionRequest),
        PROTOCOL_3_0 => {
            let mut parameters = Vec::new();
            loop {
                let name = read_cstring(&mut rest)?;
                if name.is_empty() {
                    break;
                }
                let value = read_cstring(&mut rest)?;
                parameters.push((name, value));
            }
            Ok(FrontendMessage::Startup { parameters })
        }
        other => Err(ProtocolError::UnsupportedProtocol(other)),
    }
}
```

- `read_body`は長さを読み，長さから4を引いたバイト数を読む．4より小さい長さは`checked_sub`で見つけて`Malformed`にする．
- `read_cstring`は`&mut &[u8]`(Iteration 14)を受け取り，0のバイトまでを文字列にして，残りのスライスに進める．
- `read_message`は，種類のバイトを読むときに`UnexpectedEof`なら`None`を返す．`Q`は`Query`，`X`は`Terminate`で，ほかの種類は`UnsupportedMessage`にする．

書く側は，`BackendMessage::encode`で種類のバイトと内容を作り，`write_message`で長さを前に付ける．

```rust
pub fn write_message(output: &mut impl Write, message: &BackendMessage) -> io::Result<()> {
    let (kind, body) = message.encode();
    let len = u32::try_from(body.len() + 4).expect("message fits in u32");
    let mut bytes = vec![kind];
    bytes.extend_from_slice(&len.to_be_bytes());
    bytes.extend_from_slice(&body);
    output.write_all(&bytes)
}
```

単体テストの期待値は，レイアウトから手でバイト列を書いた．

```rust
    #[test]
    fn data_row_writes_null_as_minus_one() {
        assert_eq!(
            encoded(&BackendMessage::DataRow(vec![Some("42".to_string()), None])),
            vec![b'D', 0, 0, 0, 16, 0, 2, 0, 0, 0, 2, b'4', b'2', 0xff, 0xff, 0xff, 0xff]
        );
    }
```

### 起動と終了

`handle`は，暗号化の要求に`N`を書き，起動のメッセージを受け取ったら，`AuthenticationOk`，5つの`ParameterStatus`，`ReadyForQuery`を返す．

```rust
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
```

単体テストでは，`FakeStream`に送るバイト列を読ませ，書かれたバイト列をメッセージの種類のバイトの並びにして比べる．

```rust
    #[test]
    fn queries_are_answered_until_terminate() {
        let mut input = startup(196_608, USER);
        input.extend(frontend(b'Q', b"VALUES (1)\0"));
        input.extend(frontend(b'X', b""));
        input.extend(frontend(b'Q', b"VALUES (2)\0"));
        assert_eq!(kinds(&run(input)), b"RSSSSSZTDCZ");
    }
```

`Terminate`のあとの`Query`には答えないので，種類の並びは1つ目の問い合わせの`T`，`D`，`C`，`Z`で終わる．

### 問い合わせへの返事

`run_query`は，問い合わせを文に分けて順に実行し，返すメッセージを並べる．

```rust
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
```

最初は`position: error.position()`と書き，2つ目の文のエラーの位置を確かめるテストが次のように失敗した．位置は文の先頭から数えたものなので，問い合わせの先頭からの位置にするには，前の文の文字の数を足す．

```text
thread 'server::connection::tests::error_stops_the_query_and_its_position_counts_from_the_query' (321130) panicked at src/server/connection.rs:347:9:
assertion `left == right` failed
  left: [RowDescription([FieldDescription { name: "COLUMN1", type_oid: 23, type_size: 4 }]), DataRow([Some("1")]), CommandComplete("SELECT 1"), ErrorResponse { code: "42601", message: "syntax error at or near \")\"", position: Some(12) }, ReadyForQuery { status: 73 }]
 right: [RowDescription([FieldDescription { name: "COLUMN1", type_oid: 23, type_size: 4 }]), DataRow([Some("1")]), CommandComplete("SELECT 1"), ErrorResponse { code: "42601", message: "syntax error at or near \")\"", position: Some(24) }, ReadyForQuery { status: 73 }]
```

`status: 73`は`b'I'`である．`u8`の`Debug`は数で表示する．

- `statements_with_offsets`は，`split_statements`で分けた文を元の問い合わせの中で探し，その前の文字の数と組にする．位置は文字で数えるので，バイトの位置でなく`chars().count()`を使う．
- `result_messages`は，行を返す文を`rows_messages`に任せ，ほかの文は`StatementResult`の`Display`(コマンドタグ)を`CommandComplete`にする．
- 型OIDは，列の最初の`NULL`でない値の型で決める(`column_type`)．

### 返事をまとめて書く

最初は，`send`でメッセージごとに`write_message(stream, message)`を呼んでいた．単体テストは通るが，`psql`で問い合わせると応答が遅い．
1つの`VALUES (1);`を100行並べたファイルを`psql -f`で流し，時間を比べた．

| 書き方 | 100個の問い合わせの時間 |
| --- | --- |
| メッセージごとに書く | 4.522 s |
| `Vec<u8>`に並べて1回で書く | 0.036 s |

メッセージごとに書くと，OSは最初の小さな書き込みを送ったあと，相手の確認応答を待って残りを送る(Nagleのアルゴリズム)．相手は確認応答を少し遅らせて送るので，問い合わせのたびに約45ミリ秒待つ．

```rust
fn send(stream: &mut impl Write, messages: &[BackendMessage]) -> Result<(), ProtocolError> {
    let mut bytes = Vec::new();
    for message in messages {
        write_message(&mut bytes, message)?;
    }
    stream.write_all(&bytes)?;
    stream.flush()?;
    Ok(())
}
```

### サブコマンド`serve`

`main`は，`repl`と`serve`でデータベースを開く処理を`open`にまとめ，エラーを`String`にして1か所で表示する．

```rust
fn serve(data_dir: Option<PathBuf>, port: u16) -> Result<(), String> {
    let mut database = open(data_dir)?;
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|error| error.to_string())?;
    println!("ferrodb listening on 127.0.0.1:{port}");
    for stream in listener.incoming() {
        let stream = match stream {
            Ok(stream) => stream,
            Err(error) => {
                eprintln!("ferrodb: {error}");
                continue;
            }
        };
        if let Err(error) = ferrodb::server::connection::handle(stream, &mut database) {
            eprintln!("ferrodb: {error}");
        }
    }
    Ok(())
}
```

1つの接続でエラーが起きても，サーバーは止めずに次の接続を待つ．

### 結合テスト

`connect`は，ポート0で待ち受けて1つの接続を扱うスレッドを作り，`postgres`の`Client`で接続する．

```rust
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
```

- `Database`はスレッドの中で作る．スレッドの間で値を渡す条件は，Iteration 21で扱う．
- エラーの位置を確かめるテストは，最初は存在しない表への問い合わせで書いた．`ferrodb`の表が見つからないエラーは位置を持たないので，位置を持つ構文エラーの`VALUES (1 +)`に変えた．

`psql`から接続した結果は，演習の手順の使用例のとおりである．

## 20-6 振り返り

1. メッセージのバイト配置，SSLの要求，複数の文，空の問い合わせ，エラーの位置，トランザクションの状態，実際のクライアントとのやりとりを確かめる項目があるかを比べる．
2. 単体テストでは，バイト配置と返すメッセージの並びを細かく確かめられる．ただし，期待値は，プロトコルを自分でどう読んだかで決まる．読み違えていれば，実装とテストが同じ誤りを持つ．`postgres`クレートの結合テストは，別の人の書いたクライアントが受け入れることを確かめる．メッセージをまとめて書かないと遅くなることは，`psql`で使って初めて分かった．
3. `ferrodb`では，`CREATE TABLE`と`INSERT`がそれぞれコミットされ，表`T`に1行が残る．PostgreSQLは，1つの`Query`の文を1つのトランザクションとして実行するので，エラーで`CREATE TABLE`も取り消され，表`T`はできない．同じにするには，トランザクションの外で複数の文を受け取ったとき，`run_query`で最初の文の前にトランザクションを始め，最後に成功すればコミットし，エラーなら中止する．文の中に`COMMIT`などがある場合の扱いも決める必要がある．
4. 行のない結果の列は，どれも`text`になる．正しくするには，実行計画の出力の列の型を`QueryResult`に持たせ，`RowDescription`に使う．
5. 2つ目の`psql`のTCPの接続はOSが受け付けるが，`ferrodb`は1つ目の接続を扱っている間`accept`を呼ばないので，起動のメッセージに返事がなく，2つ目は待ち続ける．1つ目が終わると，2つ目が使えるようになる．Iteration 21で，接続ごとにスレッドを作る．
6. 模範解答の設計ドキュメントは，照合スクリプトで問題がないことを確かめてある．

## 20-7 発展課題

解答例である．`FrontendMessage`に，拡張クエリプロトコルの`Sync`と，そのほかのメッセージを加える．

```rust
    /// 拡張クエリプロトコルの`Sync`．
    Sync,
    /// 拡張クエリプロトコルの，`Sync`のほかのメッセージ(`Parse`，`Bind`など)．
    Extended(u8),
```

`read_message`は，`S`を`Sync`に，`P`(`Parse`)，`B`(`Bind`)，`D`(`Describe`)，`E`(`Execute`)，`C`(`Close`)，`H`(`Flush`)を`Extended`にする．

```rust
        b'S' => Ok(Some(FrontendMessage::Sync)),
        kind @ (b'P' | b'B' | b'D' | b'E' | b'C' | b'H') => {
            Ok(Some(FrontendMessage::Extended(kind)))
        }
```

`handle`は，`Extended`を受け取ったら`Sync`まで読み捨て，エラーと`ReadyForQuery`を返す．

```rust
            FrontendMessage::Extended(_) => {
                while let Some(message) = read_message(&mut stream)? {
                    if message == FrontendMessage::Sync {
                        break;
                    }
                }
                let error = BackendMessage::ErrorResponse {
                    code: "0A000".to_string(),
                    message: "extended query protocol is not supported".to_string(),
                    position: None,
                };
                send(&mut stream, &[error, ready_for_query(db)])?;
            }
            FrontendMessage::Sync => send(&mut stream, &[ready_for_query(db)])?,
```

- 拡張クエリプロトコルでは，エラーのあとサーバーは`Sync`までのメッセージを読み捨て，`Sync`に`ReadyForQuery`で答える．クライアントは`ReadyForQuery`を受け取ってから次の問い合わせを送る．
- 知らない種類を`P`で確かめていた`server::message`の単体テストは，`Extended`にならない種類(`F`など)に変える．
- 結合テストで，`query`がエラーになったあと，同じ接続の`simple_query`が行を返すことを確かめた．
