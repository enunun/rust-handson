# Iteration 4：SQLSTATEとエラー位置(解説)

演習の各手順の模範解答と，その考え方を説明する．

## 4-1 準備

引き継いだ86個のテストがすべて通れば準備は終わりである．

## 4-2 文法と概念

課題の解答例である．

```rust
use std::fmt;
use std::num::ParseIntError;

#[derive(Debug, PartialEq)]
pub enum ConfigError {
    Missing { key: String },
    Invalid { key: String, value: String },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Missing { key } => write!(f, "missing key: {key}"),
            ConfigError::Invalid { key, value } => write!(f, "invalid value for {key}: {value}"),
        }
    }
}

impl std::error::Error for ConfigError {}

#[derive(Debug, PartialEq)]
pub struct PortError {
    pub reason: String,
}

impl From<ParseIntError> for PortError {
    fn from(error: ParseIntError) -> PortError {
        PortError {
            reason: error.to_string(),
        }
    }
}

pub fn port(text: &str) -> Result<u16, PortError> {
    let n: u16 = text.parse()?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t() {
        assert_eq!(
            ConfigError::Missing { key: "port".to_string() }.to_string(),
            "missing key: port"
        );
        assert_eq!(
            ConfigError::Invalid { key: "port".to_string(), value: "x".to_string() }.to_string(),
            "invalid value for port: x"
        );
        assert_eq!(port("5433"), Ok(5433));
        assert_eq!(port("70000").unwrap_err().reason, "number too large to fit in target type");
    }
}
```

`ConfigError`は，列挙子ごとに持つ情報が違う直和型である．`Display`の`match`は，列挙子ごとにメッセージを組み立てる．

## 4-3 テストリスト

模範解答は[TESTLIST.md](../TESTLIST.md)である．

- 既存のテストの変更を，各モジュールの最初の項目にした．型を変えてすべてのテストが通る状態に戻してから，新しい振る舞いを加える．
- 字句解析の既存のテストは，位置を比べない補助関数`kinds`で書き直した．位置は，それを確かめる1つの項目だけで比べる．位置を比べるテストが多いと，空白を1つ変えただけで多くのテストを直すことになる．
- 構文解析では，バックトラックで位置がずれる2つの入力(括弧の中，`,`のあと)と，文が途中で終わる入力を加えた．
- `error`モジュールの単体テストは，各段階のエラーを`Error`に変える規則を1つずつ確かめる．結合テストは，`execute`が実際にその変換を通してエラーを返すことを確かめる．

## 4-4 設計ドキュメント

| ファイル | 変更 | 理由 |
| --- | --- | --- |
| `c4-component.md` | `error`と，`crate`から`error`，`error`から`lexer`，`parser`，`eval`への依存を加えた | `From`の実装が各段階のエラー型を読む．各段階のモジュールは`Error`を知らない |
| `code-types.md` | `Error`を列挙型から構造体に変え，`SqlState`，`Spanned`を加えた．`ParseError`を列挙型にし，`LexError`に`found`を加えた | 利用者はどの種類のエラーも同じように扱う．構文解析のエラーは，トークンの有無で持つ情報が違う |
| `code-sequence.md` | 失敗したときに`Error::from`で変換する流れを加えた．`cut_err`の位置を説明に書いた | `?`が`From`を呼ぶ |

- `Error`と`LexError`などの関係は，値として持つのではなく変換するだけなので，依存(`..>`)で描いた．
- `Error`の非公開のフィールドを`-`で，公開のメソッドを`+`で描いた．

## 4-5 テスト駆動の実装

### `SqlState`とエラーの変換

```rust
#[test]
fn each_sqlstate_has_its_code() {
    assert_eq!(SqlState::SyntaxError.code(), "42601");
    assert_eq!(SqlState::NumericValueOutOfRange.code(), "22003");
    assert_eq!(SqlState::DivisionByZero.code(), "22012");
    assert_eq!(SqlState::DatatypeMismatch.code(), "42804");
}
```

`src/error.rs`を作り，`SqlState`を定義した．データを持たない列挙子だけなので`Copy`を導出し，`code`は`self`で受け取る．

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlState {
    SyntaxError,
    NumericValueOutOfRange,
    DivisionByZero,
    DatatypeMismatch,
}

impl SqlState {
    pub fn code(self) -> &'static str {
        match self {
            SqlState::SyntaxError => "42601",
            SqlState::NumericValueOutOfRange => "22003",
            SqlState::DivisionByZero => "22012",
            SqlState::DatatypeMismatch => "42804",
        }
    }
}
```

評価のエラーの変換は，SQLSTATEとメッセージの組を`match`で選び，1か所で`Error`を作る．

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Error {
    sqlstate: SqlState,
    message: String,
    position: Option<usize>,
}

impl From<EvalError> for Error {
    fn from(error: EvalError) -> Error {
        let (sqlstate, message) = match error {
            EvalError::NumericOutOfRange => (SqlState::NumericValueOutOfRange, "integer out of range"),
            EvalError::DivisionByZero => (SqlState::DivisionByZero, "division by zero"),
            EvalError::DatatypeMismatch => (SqlState::DatatypeMismatch, "datatype mismatch"),
        };
        Error {
            sqlstate,
            message: message.to_string(),
            position: None,
        }
    }
}
```

`Display`は`message`をそのまま書き出し，`std::error::Error`は空の実装にした．

### トークンに位置を持たせる

```rust
#[test]
fn tokens_carry_their_character_positions() {
    let mut positions = Vec::new();
    for token in tokenize("VALUES ('あ', 1)").unwrap() {
        positions.push(token.position);
    }
    assert_eq!(positions, vec![1, 8, 9, 12, 14, 15]);
}
```

`src/token.rs`に`Spanned<T>`を定義し，`tokenize`が`Vec<Spanned<Token>>`を返すようにした．
字句解析の入力を`LocatingSlice`にし，`with_span`でトークンのバイトの範囲を得る．パーサー関数の引数の型は，別名`Input`で書き換えた．

```rust
type Input<'i> = LocatingSlice<&'i str>;

pub fn tokenize(sql: &str) -> Result<Vec<Spanned<Token>>, LexError> {
    match tokens.parse(LocatingSlice::new(sql)) {
        Ok(tokens) => {
            let mut spanned = Vec::new();
            for (token, span) in tokens {
                spanned.push(Spanned {
                    value: token,
                    position: char_position(sql, span.start),
                });
            }
            Ok(spanned)
        }
        Err(error) => { ... }
    }
}

fn char_position(sql: &str, byte: usize) -> usize {
    sql[..byte].chars().count() + 1
}

fn tokens(input: &mut Input<'_>) -> winnow::Result<Vec<(Token, Range<usize>)>> {
    terminated(
        repeat(0.., preceded(multispace0, token.with_span())),
        multispace0,
    )
    .parse_next(input)
}
```

既存の字句解析のテストは，位置を除いたトークンの列で比べる補助関数に書き換えた．

```rust
fn kinds(sql: &str) -> Result<Vec<Token>, LexError> {
    let mut kinds = Vec::new();
    for token in tokenize(sql)? {
        kinds.push(token.value);
    }
    Ok(kinds)
}
```

### 字句解析のエラーを構文エラーにする

```rust
#[test]
fn lexical_error_becomes_a_syntax_error_near_the_character() {
    let error = Error::from(LexError {
        position: 3,
        found: '?',
    });
    assert_eq!(error.sqlstate(), SqlState::SyntaxError);
    assert_eq!(error.message(), "syntax error at or near \"?\"");
    assert_eq!(error.position(), Some(3));
}
```

メッセージには読めなかった文字が要るので，`LexError`に`found`を加えた．字句解析が止まるのは，読めない文字の手前だけなので，その位置には必ず文字がある．

```rust
        Err(error) => {
            let rest = &sql[error.offset()..];
            Err(LexError {
                position: char_position(sql, error.offset()),
                found: rest
                    .chars()
                    .next()
                    .expect("the lexer stops only before an unreadable character"),
            })
        }
```

`expect`は，`None`のときに与えたメッセージでパニックする．起こりえない場合を，理由とともに書いておく．

### 構文解析のエラーをトークンと位置で返す

```rust
#[test]
fn missing_operand_is_an_error() {
    assert_eq!(
        parse_sql("VALUES (1 +)"),
        Err(ParseError::UnexpectedToken {
            token: Token::RParen,
            position: 12
        })
    );
}
```

`ParseError`を列挙型にし，`parse`がトークンの列を`Spanned<Token>`で受け取るようにした．
`literal(Token::RParen)`で`Spanned<Token>`を読めるように，`PartialEq<Token>`を実装した．winnowは`Eq`も求めるので，`Spanned<T>`に`Eq`を導出した．

```rust
impl PartialEq<Token> for Spanned<Token> {
    fn eq(&self, other: &Token) -> bool {
        self.value == *other
    }
}
```

```rust
pub fn parse(tokens: &[Spanned<Token>]) -> Result<Values, ParseError> {
    match values.parse(TokenSlice::new(tokens)) {
        Ok(values) => Ok(values),
        Err(error) => match tokens.get(error.offset()) {
            Some(token) => Err(ParseError::UnexpectedToken {
                token: token.value.clone(),
                position: token.position,
            }),
            None => Err(ParseError::UnexpectedEnd),
        },
    }
}
```

`tokens.get(番号)`は，番号が範囲の外なら`None`を返す．文の最後まで読んで失敗したときは，番号がトークンの数と等しくなる．
既存の4つの構文エラーの項目と，文が途中で終わる項目は，これで通る．

### 括弧の中と`,`のあとの誤り

```rust
#[test]
fn error_inside_parentheses_reports_the_inner_token() {
    assert_eq!(
        parse_sql("VALUES ((1 +))"),
        Err(ParseError::UnexpectedToken {
            token: Token::RParen,
            position: 13
        })
    );
}
```

`operand`の`alt`がバックトラックするので，外側の`(`の位置になって失敗する．`,`のあとの誤りも同じく`,`の位置になる．

```text
assertion `left == right` failed
  left: Err(UnexpectedToken { token: LParen, position: 9 })
 right: Err(UnexpectedToken { token: RParen, position: 13 })
```

```text
assertion `left == right` failed
  left: Err(UnexpectedToken { token: Comma, position: 10 })
 right: Err(UnexpectedToken { token: RParen, position: 12 })
```

`(`を読んだあとと，式の並びの各要素に`cut_err`を付けた．
`cut_err`を使うには，エラーの型がバックトラックを区別できる`ErrMode`である必要があるので，構文解析器の関数の戻り値を`winnow::Result`から`ModalResult`に変えた．

```rust
            separated(1.., cut_err(expr), literal(Token::Comma)),
```

```rust
        preceded(
            literal(Token::LParen),
            cut_err(terminated(expr, literal(Token::RParen))),
        ),
```

### トークンの表示

```rust
#[test]
fn unexpected_token_becomes_a_syntax_error_near_the_token() {
    let error = Error::from(ParseError::UnexpectedToken {
        token: Token::String("it's".to_string()),
        position: 8,
    });
    assert_eq!(error.message(), "syntax error at or near \"'it''s'\"");
    assert_eq!(error.position(), Some(8));
}
```

メッセージにトークンを書くため，`Token`と`Keyword`に`Display`を実装した．文字列のトークンは，SQLの書き方に戻して表示する．

```rust
impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Keyword(keyword) => write!(f, "{keyword}"),
            Token::Integer(n) => write!(f, "{n}"),
            Token::String(s) => write!(f, "'{}'", s.replace('\'', "''")),
            Token::LParen => f.write_str("("),
            // 残りの記号も同じ形
        }
    }
}
```

```rust
impl From<ParseError> for Error {
    fn from(error: ParseError) -> Error {
        match error {
            ParseError::UnexpectedToken { token, position } => Error {
                sqlstate: SqlState::SyntaxError,
                message: format!("syntax error at or near \"{token}\""),
                position: Some(position),
            },
            ParseError::UnexpectedEnd => Error {
                sqlstate: SqlState::SyntaxError,
                message: "syntax error at end of input".to_string(),
                position: None,
            },
        }
    }
}
```

### 結合テストと`execute`

結合テストのエラーの項目を，`Error`のメソッドで比べる形に書き換えた．

```rust
#[test]
fn reports_a_syntax_error_with_its_position() {
    let err = execute("VALUES (1 +)").unwrap_err();
    assert_eq!(err.sqlstate(), SqlState::SyntaxError);
    assert_eq!(err.sqlstate().code(), "42601");
    assert_eq!(err.position(), Some(12));
    assert_eq!(err.to_string(), "syntax error at or near \")\"");
}
```

`lib.rs`の`Error`の列挙型を消し，`error::Error`を公開した．`From`を実装したので，`execute`の`map_err`は要らない(Refactor)．

```rust
pub fn execute(sql: &str) -> Result<QueryResult, Error> {
    let tokens = tokenize(sql)?;
    let values = parser::parse(&tokens)?;
    let mut columns = Vec::new();
    let mut row = Vec::new();
    for expr in &values.exprs {
        columns.push(format!("COLUMN{}", columns.len() + 1));
        row.push(eval::eval(expr)?);
    }
    Ok(QueryResult {
        columns,
        rows: vec![row],
    })
}
```

`tests/tokenize.rs`は，トークンに位置を付けた期待値に書き換えた．

## 4-6 振り返り

1. 既存のテストの変更は，字句解析(位置と`found`)，構文解析(エラーの期待値)，結合テスト(トークンとエラーの期待値)の3か所にある．
2. `Option`を2つ持つ`struct`では，「トークンがあるのに位置がない」「トークンがないのに位置がある」値も作れる．`Error`に変換するときも，4通りの組み合わせを扱うことになる．列挙型なら，意味のある2通りしか作れない．
3. 列挙型の`Error`のままでは，利用者はSQLSTATEを知るために`match`するか，メソッドを呼ぶことになる．PostgreSQLのプロトコル(Iteration 20)でエラーを返すときに要るのは，SQLSTATE，メッセージ，位置の組である．どの段階かを区別する必要はない．
4. フィールドが`pub`だと，利用者が`Error`を自由に作ったり書き換えたりできる．SQLSTATEとメッセージの組み合わせを`From`の実装だけで決められるように，非公開にした．
5. 各段階のモジュールが`Error`を直接返すと，`lexer`や`parser`がSQLSTATEやメッセージの文面を知ることになる．各段階は自分の言葉(`UnexpectedToken`など)でエラーを返し，`error`モジュールが1か所で翻訳する形にした．
6. 模範解答の設計ドキュメントは，照合スクリプトで問題がないことを確かめてある．

## 4-7 発展課題

解答例である．`take(n)`は，最初の`n`個の要素だけを取り出すイテレーターのメソッドである(イテレーターはIteration 7で扱う)．

```rust
    pub fn line_and_column(&self, sql: &str) -> Option<(usize, usize)> {
        let position = self.position?;
        let mut line = 1;
        let mut column = 1;
        for c in sql.chars().take(position - 1) {
            if c == '\n' {
                line += 1;
                column = 1;
            } else {
                column += 1;
            }
        }
        Some((line, column))
    }
```

```rust
#[test]
fn line_and_column_of_the_position() {
    let sql = "VALUES (1,\n  1 +)";
    let error = crate::execute(sql).unwrap_err();
    assert_eq!(error.position(), Some(17));
    assert_eq!(error.line_and_column(sql), Some((2, 6)));
    let error = Error::from(EvalError::DivisionByZero);
    assert_eq!(error.line_and_column(sql), None);
}
```

`self.position?`のように，`?`は`Option`にも使える．`None`なら，その場で`None`を返す．
