//! PostgreSQLのフロントエンド/バックエンドプロトコル(3.0)のメッセージと，その読み書き．
//! 数はすべてビッグエンディアンで，文字列は0のバイトで終わる．

use std::fmt;
use std::io::{self, Read, Write};

/// プロトコル3.0の番号．上位16ビットが3，下位16ビットが0である．
const PROTOCOL_3_0: u32 = 196_608;
/// SSLで接続したいという要求の番号．
const SSL_REQUEST: u32 = 80_877_103;
/// GSSAPIで暗号化したいという要求の番号．
const GSSENC_REQUEST: u32 = 80_877_104;

/// クライアント(フロントエンド)が送るメッセージ．
#[derive(Debug, PartialEq)]
pub enum FrontendMessage {
    /// SSLかGSSAPIで暗号化したいという要求．
    EncryptionRequest,
    /// 接続の始まり．`parameters`は`user`や`database`などの名前と値の組である．
    Startup { parameters: Vec<(String, String)> },
    /// Simple Queryの問い合わせ．
    Query(String),
    /// 接続の終わり．
    Terminate,
}

/// サーバー(バックエンド)が送るメッセージ．
#[derive(Debug, PartialEq)]
pub enum BackendMessage {
    AuthenticationOk,
    ParameterStatus {
        name: String,
        value: String,
    },
    /// 次の問い合わせを受け付ける．`status`はトランザクションの状態(`I`，`T`，`E`)である．
    ReadyForQuery {
        status: u8,
    },
    RowDescription(Vec<FieldDescription>),
    /// 1行の値．`None`は`NULL`である．
    DataRow(Vec<Option<String>>),
    CommandComplete(String),
    EmptyQueryResponse,
    ErrorResponse {
        code: String,
        message: String,
        position: Option<usize>,
    },
}

/// 結果の列の名前と型．
#[derive(Debug, Clone, PartialEq)]
pub struct FieldDescription {
    pub name: String,
    pub type_oid: u32,
    pub type_size: i16,
}

/// プロトコルに従わないメッセージや，読み書きのエラー．
#[derive(Debug)]
pub enum ProtocolError {
    Io(io::Error),
    /// 知らないプロトコルの番号．
    UnsupportedProtocol(u32),
    /// 扱わない種類のメッセージ．
    UnsupportedMessage(u8),
    /// メッセージの中身が正しくない．
    Malformed,
}

impl From<io::Error> for ProtocolError {
    fn from(error: io::Error) -> ProtocolError {
        ProtocolError::Io(error)
    }
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProtocolError::Io(error) => write!(f, "connection error: {error}"),
            ProtocolError::UnsupportedProtocol(code) => {
                write!(f, "unsupported protocol {code}")
            }
            ProtocolError::UnsupportedMessage(kind) => {
                write!(f, "unsupported message type {}", char::from(*kind))
            }
            ProtocolError::Malformed => f.write_str("malformed message"),
        }
    }
}

/// 起動のメッセージを読む．起動のメッセージは，種類のバイトを持たない．
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

/// 起動のあとのメッセージを読む．接続が閉じていれば`None`を返す．
pub fn read_message(input: &mut impl Read) -> Result<Option<FrontendMessage>, ProtocolError> {
    let mut kind = [0; 1];
    if let Err(error) = input.read_exact(&mut kind) {
        return match error.kind() {
            io::ErrorKind::UnexpectedEof => Ok(None),
            _ => Err(error.into()),
        };
    }
    let body = read_body(input)?;
    match kind[0] {
        b'Q' => {
            let mut rest = &body[..];
            Ok(Some(FrontendMessage::Query(read_cstring(&mut rest)?)))
        }
        b'X' => Ok(Some(FrontendMessage::Terminate)),
        other => Err(ProtocolError::UnsupportedMessage(other)),
    }
}

/// メッセージの長さ(4バイト，自分を含む)を読み，残りのバイト列を返す．
fn read_body(input: &mut impl Read) -> Result<Vec<u8>, ProtocolError> {
    let mut len = [0; 4];
    input.read_exact(&mut len)?;
    let len = usize::try_from(u32::from_be_bytes(len)).expect("u32 fits in usize");
    let mut body = vec![0; len.checked_sub(4).ok_or(ProtocolError::Malformed)?];
    input.read_exact(&mut body)?;
    Ok(body)
}

/// 0のバイトで終わる文字列を読み，`input`を残りのバイト列にする．
fn read_cstring(input: &mut &[u8]) -> Result<String, ProtocolError> {
    let end = input
        .iter()
        .position(|&byte| byte == 0)
        .ok_or(ProtocolError::Malformed)?;
    let text = String::from_utf8(input[..end].to_vec()).map_err(|_| ProtocolError::Malformed)?;
    *input = &input[end + 1..];
    Ok(text)
}

/// メッセージを書く．種類の1バイトのあとに，長さ(自分を含む4バイト)と中身を置く．
pub fn write_message(output: &mut impl Write, message: &BackendMessage) -> io::Result<()> {
    let (kind, body) = message.encode();
    let len = u32::try_from(body.len() + 4).expect("message fits in u32");
    let mut bytes = vec![kind];
    bytes.extend_from_slice(&len.to_be_bytes());
    bytes.extend_from_slice(&body);
    output.write_all(&bytes)
}

impl BackendMessage {
    /// 種類のバイトと，中身のバイト列．
    fn encode(&self) -> (u8, Vec<u8>) {
        let mut body = Vec::new();
        let kind = match self {
            BackendMessage::AuthenticationOk => {
                body.extend_from_slice(&0u32.to_be_bytes());
                b'R'
            }
            BackendMessage::ParameterStatus { name, value } => {
                write_cstring(&mut body, name);
                write_cstring(&mut body, value);
                b'S'
            }
            BackendMessage::ReadyForQuery { status } => {
                body.push(*status);
                b'Z'
            }
            BackendMessage::RowDescription(fields) => {
                write_count(&mut body, fields.len());
                for field in fields {
                    write_cstring(&mut body, &field.name);
                    body.extend_from_slice(&0u32.to_be_bytes());
                    body.extend_from_slice(&0u16.to_be_bytes());
                    body.extend_from_slice(&field.type_oid.to_be_bytes());
                    body.extend_from_slice(&field.type_size.to_be_bytes());
                    body.extend_from_slice(&(-1i32).to_be_bytes());
                    body.extend_from_slice(&0u16.to_be_bytes());
                }
                b'T'
            }
            BackendMessage::DataRow(values) => {
                write_count(&mut body, values.len());
                for value in values {
                    match value {
                        Some(text) => {
                            let len = i32::try_from(text.len()).expect("value fits in i32");
                            body.extend_from_slice(&len.to_be_bytes());
                            body.extend_from_slice(text.as_bytes());
                        }
                        None => body.extend_from_slice(&(-1i32).to_be_bytes()),
                    }
                }
                b'D'
            }
            BackendMessage::CommandComplete(tag) => {
                write_cstring(&mut body, tag);
                b'C'
            }
            BackendMessage::EmptyQueryResponse => b'I',
            BackendMessage::ErrorResponse {
                code,
                message,
                position,
            } => {
                for (field, value) in [
                    (b'S', "ERROR"),
                    (b'V', "ERROR"),
                    (b'C', code.as_str()),
                    (b'M', message.as_str()),
                ] {
                    body.push(field);
                    write_cstring(&mut body, value);
                }
                if let Some(position) = position {
                    body.push(b'P');
                    write_cstring(&mut body, &position.to_string());
                }
                body.push(0);
                b'E'
            }
        };
        (kind, body)
    }
}

fn write_cstring(body: &mut Vec<u8>, text: &str) {
    body.extend_from_slice(text.as_bytes());
    body.push(0);
}

fn write_count(body: &mut Vec<u8>, count: usize) {
    let count = i16::try_from(count).expect("count fits in i16");
    body.extend_from_slice(&count.to_be_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// 長さを前に付けた起動のメッセージ．
    fn startup_bytes(code: u32, rest: &[u8]) -> Vec<u8> {
        let len = u32::try_from(8 + rest.len()).unwrap();
        let mut bytes = len.to_be_bytes().to_vec();
        bytes.extend_from_slice(&code.to_be_bytes());
        bytes.extend_from_slice(rest);
        bytes
    }

    fn encoded(message: &BackendMessage) -> Vec<u8> {
        let mut bytes = Vec::new();
        write_message(&mut bytes, message).unwrap();
        bytes
    }

    #[test]
    fn startup_message_has_parameters() {
        let bytes = startup_bytes(PROTOCOL_3_0, b"user\0alice\0database\0ferro\0\0");
        assert_eq!(
            read_startup(&mut Cursor::new(bytes)).unwrap(),
            FrontendMessage::Startup {
                parameters: vec![
                    ("user".to_string(), "alice".to_string()),
                    ("database".to_string(), "ferro".to_string()),
                ]
            }
        );
    }

    #[test]
    fn ssl_request_is_an_encryption_request() {
        let bytes = startup_bytes(SSL_REQUEST, b"");
        assert_eq!(
            read_startup(&mut Cursor::new(bytes)).unwrap(),
            FrontendMessage::EncryptionRequest
        );
        let bytes = startup_bytes(196_609, b"");
        assert!(matches!(
            read_startup(&mut Cursor::new(bytes)),
            Err(ProtocolError::UnsupportedProtocol(196_609))
        ));
    }

    #[test]
    fn query_and_terminate_are_read_until_the_end() {
        let mut bytes = vec![b'Q', 0, 0, 0, 13];
        bytes.extend_from_slice(b"VALUES 1\0");
        bytes.extend_from_slice(&[b'X', 0, 0, 0, 4]);
        let mut input = Cursor::new(bytes);
        assert_eq!(
            read_message(&mut input).unwrap(),
            Some(FrontendMessage::Query("VALUES 1".to_string()))
        );
        assert_eq!(
            read_message(&mut input).unwrap(),
            Some(FrontendMessage::Terminate)
        );
        assert_eq!(read_message(&mut input).unwrap(), None);
    }

    #[test]
    fn unknown_message_type_is_an_error() {
        let bytes = vec![b'P', 0, 0, 0, 4];
        assert!(matches!(
            read_message(&mut Cursor::new(bytes)),
            Err(ProtocolError::UnsupportedMessage(b'P'))
        ));
    }

    #[test]
    fn backend_messages_have_a_type_and_a_big_endian_length() {
        assert_eq!(
            encoded(&BackendMessage::AuthenticationOk),
            vec![b'R', 0, 0, 0, 8, 0, 0, 0, 0]
        );
        assert_eq!(
            encoded(&BackendMessage::ReadyForQuery { status: b'I' }),
            vec![b'Z', 0, 0, 0, 5, b'I']
        );
        assert_eq!(
            encoded(&BackendMessage::CommandComplete("SELECT 1".to_string())),
            [&[b'C', 0, 0, 0, 13][..], b"SELECT 1\0"].concat()
        );
    }

    #[test]
    fn data_row_writes_null_as_minus_one() {
        assert_eq!(
            encoded(&BackendMessage::DataRow(vec![Some("42".to_string()), None])),
            vec![
                b'D', 0, 0, 0, 16, 0, 2, 0, 0, 0, 2, b'4', b'2', 0xff, 0xff, 0xff, 0xff
            ]
        );
    }

    #[test]
    fn row_description_has_the_type_of_each_field() {
        let field = FieldDescription {
            name: "A".to_string(),
            type_oid: 23,
            type_size: 4,
        };
        let bytes = encoded(&BackendMessage::RowDescription(vec![field]));
        assert_eq!(&bytes[..7], &[b'T', 0, 0, 0, 26, 0, 1]);
        assert_eq!(&bytes[7..9], b"A\0");
        assert_eq!(&bytes[15..19], &23u32.to_be_bytes());
        assert_eq!(&bytes[19..21], &4i16.to_be_bytes());
    }

    #[test]
    fn error_response_has_fields_and_a_terminator() {
        let bytes = encoded(&BackendMessage::ErrorResponse {
            code: "42601".to_string(),
            message: "syntax error".to_string(),
            position: Some(8),
        });
        let body = &bytes[5..];
        assert_eq!(
            body,
            b"SERROR\0VERROR\0C42601\0Msyntax error\0P8\0\0".as_slice()
        );
    }
}
