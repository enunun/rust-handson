# 型

トークン，構文木，値，エラー，結果の型を示す．
構文木`Expr`は，自分自身を`Box`で持つ再帰的な直和型である．

```mermaid
classDiagram
  class Token {
    <<enumeration>>
    Keyword(Keyword)
    Integer(i64)
    LParen
    RParen
    Comma
    Plus
    Minus
    Star
    Slash
  }
  class Keyword {
    <<enumeration>>
    Values
  }
  class Values {
    <<struct>>
    +exprs: Vec~Expr~
  }
  class Expr {
    <<enumeration>>
    Integer(i64)
    Unary(op, operand)
    Binary(op, left, right)
  }
  class UnaryOp {
    <<enumeration>>
    Neg
  }
  class BinaryOp {
    <<enumeration>>
    Add
    Sub
    Mul
    Div
  }
  class Value {
    <<enumeration>>
    Integer(i32)
  }
  class QueryResult {
    <<struct>>
    +columns: Vec~String~
    +rows: Vec~Vec~Value~~
  }
  class Error {
    <<enumeration>>
    Lex(LexError)
    Parse(ParseError)
    Eval(EvalError)
  }
  class LexError {
    <<struct>>
    +position: usize
  }
  class ParseError {
    <<struct>>
  }
  class EvalError {
    <<enumeration>>
    NumericOutOfRange
    DivisionByZero
  }
  Token *-- Keyword
  Values *-- Expr
  Expr *-- Expr
  Expr *-- UnaryOp
  Expr *-- BinaryOp
  QueryResult *-- Value
  Error *-- LexError
  Error *-- ParseError
  Error *-- EvalError
```

- `Expr::Unary`のフィールドは`op: UnaryOp`，`operand: Box<Expr>`である．
- `Expr::Binary`のフィールドは`op: BinaryOp`，`left: Box<Expr>`，`right: Box<Expr>`である．
- `ParseError`はフィールドを持たない構造体である．
- `Error`の列挙子は，どの段階で失敗したかを表し，その段階のエラーを持つ．
- 各段階の関数のシグネチャは次のとおりである．
  - `lexer::tokenize(sql: &str) -> Result<Vec<Token>, LexError>`
  - `parser::parse(tokens: &[Token]) -> Result<Values, ParseError>`
  - `eval::eval(expr: &Expr) -> Result<Value, EvalError>`
  - `execute(sql: &str) -> Result<QueryResult, Error>`
