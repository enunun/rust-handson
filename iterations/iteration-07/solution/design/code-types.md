# 型

データベース，カタログ，構文木，値，エラー，結果の型を示す．トークンの型(`Token`，`Keyword`，`Spanned`)は，この図では省く．
構文木`Expr`は，自分自身を`Box`で持つ再帰的な直和型である．

```mermaid
classDiagram
  class Database {
    <<struct>>
    -catalog: Catalog
    -rows: HashMap~String, Vec~Vec~Value~~~
    +new() Database
    +execute(sql: &str) Result~StatementResult, Error~
  }
  class StatementResult {
    <<enumeration>>
    Rows(QueryResult)
    CreateTable
    Insert(count)
  }
  class Catalog {
    <<struct>>
    -tables: HashMap~String, TableSchema~
    +create_table(schema: TableSchema) Result
    +table(name: &str) Result~&TableSchema, SchemaError~
  }
  class TableSchema {
    <<struct>>
    +name: String
    +columns: Vec~Column~
    +column_index(name: &str) Result~usize, SchemaError~
  }
  class Column {
    <<struct>>
    +name: String
    +data_type: DataType
    +assign(value: Value) Result~Value, SchemaError~
  }
  class DataType {
    <<enumeration>>
    Integer
    BigInt
    Boolean
    Varchar(usize)
  }
  class SchemaError {
    <<enumeration>>
    UndefinedTable(table)
    DuplicateTable(table)
    UndefinedColumn(table, column)
    DuplicateColumn(column)
    TypeMismatch(column, expected, found)
    ValueTooLong(data_type)
    OutOfRange
    MoreValuesThanColumns
    MoreColumnsThanValues
  }
  class Statement {
    <<enumeration>>
    Values(Values)
    CreateTable(CreateTable)
    Insert(Insert)
    Select(Select)
  }
  class CreateTable {
    <<struct>>
    +name: String
    +columns: Vec~ColumnDef~
  }
  class ColumnDef {
    <<struct>>
    +name: String
    +data_type: DataType
  }
  class Insert {
    <<struct>>
    +table: String
    +columns: Option~Vec~String~~
    +values: Values
  }
  class Select {
    <<struct>>
    +items: Vec~SelectItem~
    +from: String
    +filter: Option~Expr~
  }
  class SelectItem {
    <<enumeration>>
    Wildcard
    Expr(expr, alias)
  }
  class BoundExpr {
    <<enumeration>>
    Constant(Value)
    Column(usize)
    Unary(op, operand)
    Binary(op, left, right)
    IsNull(operand, negated)
  }
  class BindError {
    <<enumeration>>
    UndefinedColumn(column)
  }
  class Values {
    <<struct>>
    +rows: Vec~Vec~Expr~~
  }
  class Expr {
    <<enumeration>>
    Integer(i64)
    Boolean(bool)
    String(String)
    Null
    Column(String)
    Unary(op, operand)
    Binary(op, left, right)
    IsNull(operand, negated)
  }
  class UnaryOp {
    <<enumeration>>
    Neg
    Not
  }
  class BinaryOp {
    <<enumeration>>
    Arithmetic(ArithmeticOp)
    Comparison(ComparisonOp)
    Concat
    And
    Or
  }
  class ArithmeticOp {
    <<enumeration>>
    Add
    Sub
    Mul
    Div
  }
  class ComparisonOp {
    <<enumeration>>
    Eq
    NotEq
    Lt
    LtEq
    Gt
    GtEq
  }
  class Value {
    <<enumeration>>
    Integer(i32)
    BigInt(i64)
    Boolean(bool)
    Varchar(String)
    Null
    +from_truth(truth: Option~bool~) Value
    +is_null() bool
    +type_name() &str
  }
  class QueryResult {
    <<struct>>
    +columns: Vec~String~
    +rows: Vec~Vec~Value~~
  }
  class Error {
    <<struct>>
    -sqlstate: SqlState
    -message: String
    -position: Option~usize~
    +sqlstate() SqlState
    +message() &str
    +position() Option~usize~
  }
  class SqlState {
    <<enumeration>>
    SyntaxError
    NumericValueOutOfRange
    DivisionByZero
    DatatypeMismatch
    StringDataRightTruncation
    UndefinedTable
    DuplicateTable
    UndefinedColumn
    DuplicateColumn
    +code() &str
  }
  class LexError {
    <<struct>>
    +position: usize
    +found: char
  }
  class ParseError {
    <<enumeration>>
    UnexpectedToken(token, position)
    UnexpectedEnd
    ValuesLengthMismatch
  }
  class EvalError {
    <<enumeration>>
    NumericOutOfRange
    DivisionByZero
    DatatypeMismatch
    ArgumentNotBoolean(clause, found)
  }
  Database *-- Catalog
  Catalog *-- TableSchema
  TableSchema *-- Column
  Column *-- DataType
  StatementResult *-- QueryResult
  Statement *-- Values
  Statement *-- CreateTable
  Statement *-- Insert
  Statement *-- Select
  CreateTable *-- ColumnDef
  ColumnDef *-- DataType
  Insert *-- Values
  Select *-- SelectItem
  SelectItem *-- Expr
  BoundExpr *-- BoundExpr
  BoundExpr *-- Value
  Error ..> BindError
  Error ..> SchemaError
  Values *-- Expr
  Expr *-- Expr
  Expr *-- UnaryOp
  Expr *-- BinaryOp
  BinaryOp *-- ArithmeticOp
  BinaryOp *-- ComparisonOp
  QueryResult *-- Value
  Error *-- SqlState
  Error ..> LexError
  Error ..> ParseError
  Error ..> EvalError
```

- `Expr::Unary`のフィールドは`op: UnaryOp`，`operand: Box<Expr>`である．
- `Expr::Binary`のフィールドは`op: BinaryOp`，`left: Box<Expr>`，`right: Box<Expr>`である．
- `Expr::IsNull`のフィールドは`operand: Box<Expr>`，`negated: bool`である．`negated`が`true`なら`IS NOT NULL`を表す．
- `BinaryOp`は演算子を種類ごとの型に分ける．算術演算を評価する関数は`ArithmeticOp`だけを受け取る．
- `Value::Null`は，型によらず値がないことを表す．真偽値の`UNKNOWN`も`Value::Null`になる．
- `ParseError::UnexpectedToken`は，読めなかったトークンと，その文字の位置を持つ．`UnexpectedEnd`は文が途中で終わったことを表し，位置を持たない．
- `Error`は，どの段階のエラーもSQLSTATE，メッセージ，位置の組で表す．フィールドは非公開で，同じ名前のメソッドで読む．
- `Error`は`From<LexError>`，`From<ParseError>`，`From<EvalError>`を実装する．`?`はこの変換を使う．
- `StatementResult`と`QueryResult`は`Display`を実装する(`format`モジュール)．問い合わせの結果は表に，`CREATE TABLE`と`INSERT`はコマンドタグ(`CREATE TABLE`，`INSERT 0 2`)になる．
- `plan::binder::bind`は，`Expr`の列の名前を`&[Column]`の中の番号に解決し，リテラルを値にして`BoundExpr`を作る．`exec::eval::eval`は`BoundExpr`と行`&[Value]`から値を計算する．列の名前が残った式は評価できない．
- `Select::filter`は`WHERE`の条件で，書かなければ`None`である．
- `Database`は，表の定義を`Catalog`に，表の行を表の名前ごとの`Vec<Vec<Value>>`に持つ．
- `Insert::columns`が`None`なら，すべての列に定義の順で値を入れる．
- `Column::assign`は，値を列の型に合わせる．`INTEGER`と`BIGINT`は範囲に収まれば互いに変換し，`VARCHAR(n)`は文字の数を調べる．型が合わなければ`SchemaError`を返す．
- `SchemaError`の各列挙子は，メッセージに必要な情報を持つ．`Error`への変換で，SQLSTATEとメッセージになる．
