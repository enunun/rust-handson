# 型

データベース，カタログ，構文木，値，エラー，結果の型と，`SELECT`の名前解決，実行計画，演算子の型を示す．トークンの型(`Token`，`Keyword`，`Spanned`)は，この図では省く．
構文木`Expr`は，自分自身を`Box`で持つ再帰的な直和型である．

## データベースと構文木

```mermaid
classDiagram
  class Database {
    <<struct>>
    -catalog: Catalog
    -tables: HashMap~String, HeapFile~
    -indexes: HashMap~String, BufferPool~
    -data_dir: Option~PathBuf~
    -transactions: TransactionManager
    -session: Session
    -wal: Option~Rc~RefCell~WalWriter~File~~~~
    +new() Database
    +open(dir: &Path) Result~Database, Error~
    +execute(sql: &str) Result~StatementResult, Error~
    +transaction_status() TransactionStatus
  }
  class Session {
    <<enumeration>>
    Idle
    InTransaction(Transaction)
    Failed(Transaction)
  }
  class TransactionStatus {
    <<enumeration>>
    Idle
    InTransaction
    Failed
  }
  class StatementResult {
    <<enumeration>>
    Rows(QueryResult)
    CreateTable
    Insert(count)
    Update(count)
    Delete(count)
    DropTable
    CreateIndex
    DropIndex
    StartTransaction
    Commit
    Rollback
    Checkpoint
  }
  class Catalog {
    <<struct>>
    -tables: HashMap~String, TableSchema~
    -indexes: HashMap~String, IndexDef~
    +create_table(schema: TableSchema) Result
    +drop_table(name: &str) Result
    +create_index(index: IndexDef) Result
    +drop_index(name: &str) Result~IndexDef, SchemaError~
    +index(name: &str) Result~&IndexDef, SchemaError~
    +indexes_of(table: &str) Vec~&IndexDef~
    +table_names() Vec~String~
    +save(path: &Path) io::Result
    +load(path: &Path) io::Result~Catalog~
    +table(name: &str) Result~&TableSchema, SchemaError~
  }
  class TableSchema {
    <<struct>>
    +name: String
    +columns: Vec~Column~
    +unique_constraints: Vec~UniqueConstraint~
    +column_index(name: &str) Result~usize, SchemaError~
  }
  class Column {
    <<struct>>
    +name: String
    +data_type: DataType
    +nullable: bool
    +assign(value: Value) Result~Value, SchemaError~
  }
  class UniqueConstraint {
    <<struct>>
    +name: String
    +column: usize
  }
  class IndexDef {
    <<struct>>
    +name: String
    +table: String
    +column: usize
  }
  class ConstraintError {
    <<enumeration>>
    NotNull(table, column)
    Unique(constraint)
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
    MultiplePrimaryKeys(table)
    UndefinedTableToDrop(table)
    UndefinedIndex(index)
    IndexRequiredByConstraint(index, table)
    DuplicateAssignment(column)
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
    Update(Update)
    Delete(Delete)
    DropTable(DropTable)
    CreateIndex(CreateIndex)
    DropIndex(DropIndex)
    Explain(Select)
    StartTransaction
    Commit
    Rollback
    Checkpoint
  }
  class CreateIndex {
    <<struct>>
    +name: String
    +table: String
    +column: String
  }
  class DropIndex {
    <<struct>>
    +name: String
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
    +constraints: Vec~ColumnConstraint~
  }
  class ColumnConstraint {
    <<enumeration>>
    NotNull
    PrimaryKey
    Unique
  }
  class Insert {
    <<struct>>
    +table: String
    +columns: Option~Vec~String~~
    +values: Values
  }
  class Select {
    <<struct>>
    +distinct: bool
    +items: Vec~SelectItem~
    +from: TableRef
    +filter: Option~Expr~
    +group_by: Vec~Expr~
    +having: Option~Expr~
    +order_by: Vec~OrderBy~
    +limit: Limit
  }
  class AggregateFunc {
    <<enumeration>>
    Count
    Sum
    Avg
    Min
    Max
    +name() &str
  }
  class TableRef {
    <<enumeration>>
    Table(name, alias)
    Join(Box~Join~)
  }
  class Join {
    <<struct>>
    +left: TableRef
    +right: TableRef
    +kind: JoinKind
    +condition: Option~Expr~
  }
  class JoinKind {
    <<enumeration>>
    Cross
    Inner
    Left
  }
  class OrderBy {
    <<struct>>
    +expr: Expr
    +descending: bool
    +nulls_first: Option~bool~
  }
  class Limit {
    <<struct>>
    +offset: usize
    +fetch: Option~usize~
  }
  class Update {
    <<struct>>
    +table: String
    +assignments: Vec~Assignment~
    +filter: Option~Expr~
  }
  class Assignment {
    <<struct>>
    +column: String
    +value: Expr
  }
  class Delete {
    <<struct>>
    +table: String
    +filter: Option~Expr~
  }
  class DropTable {
    <<struct>>
    +name: String
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
    +display(columns: &[String]) String
  }
  class BindError {
    <<enumeration>>
    UndefinedColumn(column)
    UndefinedQualifiedColumn(table, column)
    AmbiguousColumn(column)
    UndefinedTable(table)
    MissingFromEntry(table)
    DuplicateTableName(table)
    OrderByNotInSelectList
    NotGrouped(column)
    NestedAggregate
    AggregateNotAllowed(clause)
    MisplacedAggregate
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
    QualifiedColumn(table, column)
    Aggregate(func, arg, distinct)
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
    InvalidTableDefinition
    NotNullViolation
    UniqueViolation
    InvalidColumnReference
    AmbiguousColumn
    DuplicateAlias
    GroupingError
    UndefinedFunction
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
    UndefinedFunction(name, argument)
  }
  Database *-- Catalog
  Database *-- Session
  Database ..> TransactionStatus
  Catalog *-- TableSchema
  Catalog *-- IndexDef
  TableSchema *-- Column
  Column *-- DataType
  TableSchema *-- UniqueConstraint
  StatementResult *-- QueryResult
  Statement *-- Values
  Statement *-- CreateTable
  Statement *-- CreateIndex
  Statement *-- DropIndex
  Statement *-- Insert
  Statement *-- Select
  Statement *-- Update
  Statement *-- Delete
  Statement *-- DropTable
  CreateTable *-- ColumnDef
  ColumnDef *-- DataType
  ColumnDef *-- ColumnConstraint
  Update *-- Assignment
  Assignment *-- Expr
  Insert *-- Values
  Select *-- SelectItem
  Select *-- OrderBy
  Select *-- TableRef
  TableRef *-- Join
  Join *-- TableRef
  Join *-- JoinKind
  Select *-- Limit
  OrderBy *-- Expr
  SelectItem *-- Expr
  BoundExpr *-- BoundExpr
  BoundExpr *-- Value
  Error ..> BindError
  Error ..> SchemaError
  Values *-- Expr
  Expr *-- Expr
  Expr *-- AggregateFunc
  Expr *-- UnaryOp
  Expr *-- BinaryOp
  BinaryOp *-- ArithmeticOp
  BinaryOp *-- ComparisonOp
  QueryResult *-- Value
  Error *-- SqlState
  Error ..> LexError
  Error ..> ParseError
  Error ..> EvalError
  Error ..> ConstraintError
```

- `Expr::Unary`のフィールドは`op: UnaryOp`，`operand: Box<Expr>`である．
- `Expr::Binary`のフィールドは`op: BinaryOp`，`left: Box<Expr>`，`right: Box<Expr>`である．
- `Expr::IsNull`のフィールドは`operand: Box<Expr>`，`negated: bool`である．`negated`が`true`なら`IS NOT NULL`を表す．
- `BinaryOp`は演算子を種類ごとの型に分ける．算術演算を評価する関数は`ArithmeticOp`だけを受け取る．
- `Value::Null`は，型によらず値がないことを表す．真偽値の`UNKNOWN`も`Value::Null`になる．
- `ParseError::UnexpectedToken`は，読めなかったトークンと，その文字の位置を持つ．`UnexpectedEnd`は文が途中で終わったことを表し，位置を持たない．
- `Error`は，どの段階のエラーもSQLSTATE，メッセージ，位置の組で表す．フィールドは非公開で，同じ名前のメソッドで読む．
- `Error`は`From<LexError>`，`From<ParseError>`，`From<EvalError>`，`From<BindError>`，`From<SchemaError>`，`From<ConstraintError>`を実装する．`?`はこの変換を使う．
- `StatementResult`と`QueryResult`は`Display`を実装する(`format`モジュール)．問い合わせの結果は表に，それ以外の文はコマンドタグ(`CREATE TABLE`，`INSERT 0 2`，`UPDATE 1`，`DELETE 1`，`DROP TABLE`)になる．
- `plan::binder::bind`は，`Expr`の列の名前を`&[Column]`の中の番号に解決し，リテラルを値にして`BoundExpr`を作る．`exec::eval::eval`は`BoundExpr`と行`&[Value]`から値を計算する．列の名前が残った式は評価できない．
- `Select::from`は`FROM`の表である．`,`で区切った表は，左から順に`JoinKind::Cross`の`Join`でつなぐ．`Join::condition`は`ON`の条件で，`CROSS JOIN`では`None`である．
- `Expr::QualifiedColumn`は，表の名前か別名で修飾した列名(`e.name`)である．
- `Select::filter`は`WHERE`の条件で，書かなければ`None`である．
- `Select::order_by`は`ORDER BY`のキーの並びで，書かなければ空である．`OrderBy::nulls_first`は，`NULLS FIRST`なら`Some(true)`，`NULLS LAST`なら`Some(false)`，書かなければ`None`である．
- `Limit`は`OFFSET`と`FETCH FIRST`の行の数である．書かなければ`offset`は0，`fetch`は`None`(制限なし)である．
- `Statement::Explain`は，`EXPLAIN`のあとの`SELECT`を持つ．
- `BoundExpr::display`は，`EXPLAIN`に表示する式の文字列を返す．列は引数の列名で表し，演算は括弧で囲む．
- `Value`は`Ord`を実装する．`NULL`はどの値よりも大きく，`INTEGER`と`BIGINT`は数として比べる．この順序は並べ替えと重複の除去に使い，式の比較演算(3値論理)には使わない．
- `Database`は，表の定義を`Catalog`に，表の行を表の名前ごとの`HeapFile`に，インデックスのページをインデックスの名前ごとの`BufferPool`に持つ．
- `Catalog`はインデックスの定義`IndexDef`も持つ．表とインデックスは同じ名前を使えない．一意性制約ごとに，制約と同じ名前のインデックスを作り，そのインデックスは`DROP INDEX`で消せない．`Row`は`Vec<Value>`の別名である．
- `Column::nullable`が`false`の列は`NULL`を持てない．`CREATE TABLE`で`NOT NULL`か`PRIMARY KEY`を書いた列である．
- `UniqueConstraint`は，一意性制約の名前と列の番号を持つ．`PRIMARY KEY`の制約の名前は`表_PKEY`，`UNIQUE`の制約の名前は`表_列_KEY`である．同じ列に`PRIMARY KEY`と`UNIQUE`を書いたら，`PRIMARY KEY`の制約だけを作る．
- `ConstraintError`は`exec::dml`の型で，変更したあとの行が制約に違反したことを表す．
- `Update::assignments`は`SET`の`列 = 式`の並びである．実行の前に，列の番号と`BoundExpr`の組に名前を解決する．
- `Update::filter`と`Delete::filter`は`WHERE`の条件で，書かなければ`None`である．
- `Insert::columns`が`None`なら，すべての列に定義の順で値を入れる．
- `Column::assign`は，値を列の型に合わせる．`INTEGER`と`BIGINT`は範囲に収まれば互いに変換し，`VARCHAR(n)`は文字の数を調べる．型が合わなければ`SchemaError`を返す．
- `SchemaError`の各列挙子は，メッセージに必要な情報を持つ．`Error`への変換で，SQLSTATEとメッセージになる．

## 名前解決と実行計画

`plan::binder`が`SELECT`の名前を解決した`BoundSelect`，`plan::planner`が作る実行計画`PlanNode`，`exec`の演算子の型を示す．

```mermaid
classDiagram
  class BoundSelect {
    <<struct>>
    +from: BoundFrom
    +input_columns: Vec~String~
    +names: Vec~String~
    +items: Vec~BoundExpr~
    +filter: Option~BoundExpr~
    +aggregate: Option~BoundAggregate~
    +distinct: bool
    +order_by: Vec~BoundOrderBy~
    +limit: Limit
  }
  class BoundAggregate {
    <<struct>>
    +keys: Vec~BoundExpr~
    +calls: Vec~AggregateCall~
    +having: Option~BoundExpr~
  }
  class AggregateCall {
    <<struct>>
    +func: AggregateFunc
    +arg: Option~BoundExpr~
    +distinct: bool
    +display(columns: &[String]) String
  }
  class BoundFrom {
    <<enumeration>>
    Table(table, alias, columns)
    Join(left, right, kind, condition)
  }
  class Scope {
    <<struct>>
    -columns: Vec~ScopeColumn~
    +table(table: &str, columns: &[Column]) Scope
    +join(right: Scope) Scope
    +qualified_names() Vec~String~
  }
  class ScopeColumn {
    <<struct>>
    +table: String
    +name: String
  }
  class BoundOrderBy {
    <<struct>>
    +source: SortSource
    +order: SortOrder
  }
  class SortSource {
    <<enumeration>>
    Output(usize)
    Input(BoundExpr)
  }
  class SortOrder {
    <<struct>>
    +descending: bool
    +nulls_first: bool
    +new(descending: bool, nulls_first: Option~bool~) SortOrder
  }
  class PlanNode {
    <<enumeration>>
    SeqScan(table, alias, columns)
    IndexScan(table, alias, columns, index, lower, upper, conditions)
    NestedLoopJoin(left, right, kind, condition, columns)
    Filter(input, predicate, clause)
    HashAggregate(input, keys, calls, columns)
    Project(input, exprs, names)
    Distinct(input)
    Sort(input, keys)
    Limit(input, offset, fetch)
    +columns() &[String]
  }
  class SortKey {
    <<struct>>
    +expr: BoundExpr
    +order: SortOrder
  }
  class Executor {
    <<trait>>
    +next() Result~Option~Row~, Error~
  }
  class SeqScan {
    <<struct>>
    -rows: IntoIter~Row~
  }
  class IndexScan {
    <<struct>>
    -rows: IntoIter~Row~
  }
  class NestedLoopJoin {
    <<struct>>
    -left: Box~dyn Executor~
    -right: Box~dyn Executor~
    -kind: JoinKind
    -condition: Option~BoundExpr~
    -right_width: usize
    -right_rows: Option~Vec~Row~~
    -current: Option~Row~
    -position: usize
    -matched: bool
  }
  class Filter {
    <<struct>>
    -input: Box~dyn Executor~
    -predicate: BoundExpr
    -clause: &str
  }
  class HashAggregate {
    <<struct>>
    -input: Box~dyn Executor~
    -keys: Vec~BoundExpr~
    -calls: Vec~AggregateCall~
    -results: Option~IntoIter~Row~~
  }
  class Accumulator {
    <<trait>>
    +add(value: &Value) Result~(), EvalError~
    +finish() Value
  }
  class Count {
    <<struct>>
    -count: i64
  }
  class Sum {
    <<struct>>
    -total: Option~i64~
  }
  class Avg {
    <<struct>>
    -total: i64
    -count: i64
  }
  class Extreme {
    <<struct>>
    -best: Option~Value~
    -max: bool
  }
  class DistinctValues {
    <<struct>>
    -seen: HashSet~Value~
    -inner: Box~dyn Accumulator~
  }
  class Project {
    <<struct>>
    -input: Box~dyn Executor~
    -exprs: Vec~BoundExpr~
  }
  class Distinct {
    <<struct>>
    -input: Box~dyn Executor~
    -seen: Vec~Row~
  }
  class Sort {
    <<struct>>
    -input: Box~dyn Executor~
    -keys: Vec~SortKey~
    -sorted: Option~IntoIter~Row~~
  }
  class Limit {
    <<struct>>
    -input: Box~dyn Executor~
    -offset: usize
    -remaining: Option~usize~
  }
  class KeyedRow {
    <<struct>>
    -values: Row
    -keys: Row
  }
  BoundSelect *-- BoundFrom
  BoundSelect *-- BoundOrderBy
  BoundSelect *-- BoundAggregate
  BoundAggregate *-- AggregateCall
  BoundFrom *-- BoundFrom
  Scope *-- ScopeColumn
  BoundOrderBy *-- SortSource
  BoundOrderBy *-- SortOrder
  PlanNode *-- PlanNode
  PlanNode *-- SortKey
  SortKey *-- SortOrder
  Executor <|.. SeqScan
  Executor <|.. IndexScan
  Executor <|.. NestedLoopJoin
  Executor <|.. HashAggregate
  HashAggregate o-- Executor
  HashAggregate ..> Accumulator
  Accumulator <|.. Count
  Accumulator <|.. Sum
  Accumulator <|.. Avg
  Accumulator <|.. Extreme
  Accumulator <|.. DistinctValues
  DistinctValues o-- Accumulator
  NestedLoopJoin o-- Executor
  Executor <|.. Filter
  Executor <|.. Project
  Executor <|.. Distinct
  Executor <|.. Sort
  Executor <|.. Limit
  Filter o-- Executor
  Project o-- Executor
  Distinct o-- Executor
  Sort o-- Executor
  Limit o-- Executor
  Sort ..> KeyedRow
```

- この図の`Limit`は`exec::limit`の演算子である．`BoundSelect::limit`の型は，前の図の構文木の`Limit`である．
- `BoundSelect`は，`FROM`の行の修飾した列名`input_columns`，結果の列名`names`と，それぞれの式`items`を持つ．`SortSource::Output`は結果の列の番号，`Input`は表の行について評価する式である．
- `SortOrder::new`は，`NULLS`を書かなければ`NULL`を最大の値として扱う位置にする．
- `PlanNode`は，子の演算子を`input: Box<PlanNode>`で持つ再帰的な直和型である．`columns`は，その演算子が返す行の列名を返す．`EXPLAIN`は，この列名で式を表示する．
- `SortKey::expr`は，`Sort`の子の演算子が返す行について評価する．結果の列にない式で並べ替えるとき，`plan`はその式を`Project`の隠れた列として計算し，`SortKey`はその列を指す．
- 演算子は，子の演算子を`Box<dyn Executor>`で持つ．どの演算子の子にも，どの演算子でもつなげる．
- `SeqScan`は表の行の複製を持ち，順に返す．`IndexScan`は，インデックスのキーの範囲にある行を，キーの順に持って返す．`Sort`は最初の`next`で子の行をすべて読んで並べ替え，`KeyedRow`で行とキーの値を組にする．
- `Scope`は，式から見える列の並びである．`FROM`の表の列を，結合した行の列と同じ順に並べる．名前だけの列は並び全体から探し，2つ以上見つかれば`AmbiguousColumn`になる．修飾した列は，その表の列だけから探す．
- `ScopeColumn::table`は表の別名で，別名がなければ表の名前である．`EXPLAIN`は`表.列`の形で列を表示する．
- `NestedLoopJoin`は，最初の`next`で内側(右)の行をすべて`right_rows`に読み，外側(左)の行`current`ごとに`position`で内側の行を先頭からたどる．`matched`は，今の外側の行に条件を満たす相手があったかを表す．`LEFT JOIN`で相手がなければ，内側の`right_width`個の列を`NULL`にした行を返す．
- `Expr::Aggregate`は集約関数の呼び出しで，`arg`は`COUNT(*)`なら`None`である．`Expr::contains_aggregate`は，式の中に集約関数があるかを返す．
- `BoundSelect::aggregate`は，集約する問い合わせなら`Some`である．集約した行は，`keys`の値のあとに`calls`の結果を並べたものである．選択項目，`having`，`ORDER BY`の式は，集約した行について評価する．`input_columns`は，集約した行の列名(`EMP.DEPT`，`COUNT(*)`)になる．
- `AggregateCall::arg`は`FROM`の行について評価する．同じ呼び出しが何度現れても，`calls`には1つだけ入れる．
- `BindError::MisplacedAggregate`は，`bind`が集約関数を見つけたことを表す．呼び出した側が，句の名前を持つ`AggregateNotAllowed`や`NestedAggregate`に変える．
- `Accumulator`は，1つのグループの1つの集約関数の途中の結果である．`HashAggregate`は`NULL`でない値だけを`add`に渡す．`COUNT(*)`には，行ごとに`TRUE`を渡す．`DistinctValues`は，まだ受け取っていない値だけを中の`Accumulator`に渡す．
- `Extreme`は`MIN`と`MAX`を兼ね，`max`で区別する．値は`Value`の順序で比べる．

## ストレージ

`storage`の型を示す．バイト配置は[layout.md](layout.md)にある．

```mermaid
classDiagram
  class HeapFile {
    <<struct>>
    -pool: BufferPool~Box~dyn DiskManager~~
    +new(disk: Box~dyn DiskManager~) HeapFile
    +with_log(disk: Box~dyn DiskManager~, log: PageLog) HeapFile
    +flush() Result~(), HeapError~
    +insert(tuple: &[u8]) Result~RowId, HeapError~
    +update(id: RowId, tuple: &[u8]) Result~RowId, HeapError~
    +delete(id: RowId) Result~bool, HeapError~
    +tuples() Result~Vec~(RowId, Vec~u8~)~, HeapError~
    +get(id: RowId) Result~Option~Vec~u8~~, HeapError~
    +rows(schema: &TableSchema, visible: impl Fn(&TupleHeader) -> bool) Result~Vec~(RowId, Row)~, HeapError~
    +set_xmax(id: RowId, xmax: TxnId) Result~bool, HeapError~
  }
  class HeapError {
    <<enumeration>>
    Page(PageError)
    Tuple(TupleError)
    Buffer(BufferError)
  }
  class BufferPool~D: DiskManager~ {
    <<struct>>
    -disk: RefCell~D~
    -frames: Vec~Frame~
    -page_table: RefCell~HashMap~PageId, usize~~
    -replacer: RefCell~ClockReplacer~
    -log: Option~PageLog~
    +new(disk: D, frame_count: usize) BufferPool~D~
    +with_log(disk: D, frame_count: usize, log: PageLog) BufferPool~D~
    +page_count() usize
    +fetch_page(page_id: PageId) Result~PageGuard, BufferError~
    +new_page() Result~PageGuard, BufferError~
    +flush_all() io::Result
  }
  class Frame {
    <<struct>>
    -page_id: Cell~Option~PageId~~
    -pin_count: Cell~usize~
    -dirty: Cell~bool~
    -page: RefCell~Page~
    -page_lsn: Cell~Lsn~
  }
  class PageGuard~'a~ {
    <<struct>>
    -frame: &'a Frame
    -log: Option~&'a PageLog~
    -written: Cell~bool~
    +page_id() PageId
    +read() Ref~Page~
    +write() RefMut~Page~
  }
  class PageLog {
    <<struct>>
    +wal: Rc~RefCell~WalWriter~File~~~
    +file: String
  }
  class ClockReplacer {
    <<struct>>
    -referenced: Vec~bool~
    -hand: usize
    +new(frame_count: usize) ClockReplacer
    +access(index: usize)
    +victim(is_pinned: impl Fn(usize) -> bool) Option~usize~
  }
  class BufferError {
    <<enumeration>>
    AllPinned
    Io(io::Error)
  }
  class DiskManager {
    <<trait>>
    +read_page(page_id: PageId, buffer: &mut PageBytes) io::Result
    +write_page(page_id: PageId, buffer: &PageBytes) io::Result
    +allocate_page() io::Result~PageId~
    +page_count() usize
    +sync() io::Result
  }
  class FileDiskManager {
    <<struct>>
    -file: File
    -page_count: usize
    +open(path: &Path) io::Result~FileDiskManager~
    +create(path: &Path) io::Result~FileDiskManager~
  }
  class MemoryDiskManager {
    <<struct>>
    -pages: Vec~Box~PageBytes~~
  }
  class PageId {
    <<type>>
    usize
  }
  class PageBytes {
    <<type>>
    [u8; PAGE_SIZE]
  }
  class RowId {
    <<struct>>
    +page: usize
    +slot: SlotId
  }
  class Page {
    <<struct>>
    -data: Box~PageBytes~
    +new() Page
    +from_bytes(data: Box~PageBytes~) Page
    +bytes() &PageBytes
    +slot_count() SlotId
    +insert(tuple: &[u8]) Result~SlotId, PageError~
    +get(slot: SlotId) Option~&[u8]~
    +update(slot: SlotId, tuple: &[u8]) Result
    +delete(slot: SlotId) bool
  }
  class SlotId {
    <<type>>
    u16
  }
  class PageError {
    <<enumeration>>
    PageFull
    TupleTooLarge(size)
  }
  class TupleError {
    <<enumeration>>
    Truncated
    InvalidBoolean
    InvalidUtf8
  }
  HeapFile *-- BufferPool
  BufferPool *-- DiskManager
  BufferPool *-- Frame
  BufferPool *-- ClockReplacer
  BufferPool *-- PageLog
  BufferPool ..> PageGuard
  BufferPool ..> BufferError
  PageGuard --> Frame
  Frame *-- Page
  HeapFile ..> Page
  HeapFile ..> RowId
  HeapFile ..> HeapError
  DiskManager <|.. FileDiskManager
  DiskManager <|.. MemoryDiskManager
  RowId *-- SlotId
  Page ..> PageError
```

- `storage::tuple`の`encode_tuple`と`decode_tuple`は，行とタプルのバイト列を変換する関数である．復号には，列の型を知るために`TableSchema`が要る．
- `SlotId`は`u16`の別名で，ページの中のスロットの番号である．消したタプルのスロットの番号は，ほかのタプルに使わない．
- `RowId`は，表の中の行の位置である．`UPDATE`で行が同じページに入らなくなると，別のページに移り，`RowId`が変わる．
- `HeapFile`は，操作のたびにページを`BufferPool`からピン留めして読み書きする．新しいタプルは最後のページに置き，入らなければページを加える．消したタプルの領域は再利用しない．
- `DiskManager`は，ページ番号でページを読み書きする．`FileDiskManager`はファイルの`page_id * PAGE_SIZE`バイト目から，`MemoryDiskManager`はメモリーの`Vec`のページを読み書きする．
- `BufferPool`は`DiskManager`のページを`frame_count`個の`Frame`に置く．`page_table`はページ番号から枠の番号を引く．枠に置いたページは，追い出すまでディスクを読まない．
- `fetch_page`と`new_page`はピン留めした`PageGuard`を返し，`PageGuard`を捨てるとピンが外れる．ピン留めされたページは追い出さない．
- `PageGuard::write`は枠に変更の印(`dirty`)を付ける．印のあるページは，追い出すとき，`flush_all`のとき，`BufferPool`を捨てるときにディスクへ書き戻す．
- `BufferPool`のメソッドは`&self`を受け取る．枠の状態は`Cell`と`RefCell`で書き換える．
- `ClockReplacer`は，枠ごとの参照ビットと針で，追い出す枠を選ぶ．
- `DiskManager`は`Box<dyn DiskManager>`にも実装してあり，`HeapFile`は`BufferPool<Box<dyn DiskManager>>`を持つ．
- `Database::open`はデータディレクトリのカタログを読み，表ごとの`FileDiskManager`を開く．`Database::new`の表は`MemoryDiskManager`を使う．
- `PageError::PageFull`は`HeapFile`の中で扱い，外には返さない．`TupleTooLarge`は，どのページにも入らない大きさのタプルで，`Error`への変換で`54000`になる．

## インデックス

`index`の型を示す．ノードのバイト配置は[layout.md](layout.md)にある．

```mermaid
classDiagram
  class IndexKey {
    <<trait>>
    +encode() Vec~u8~
    +decode(bytes: &[u8])$ Option~Self~
  }
  class BTree~'a, K: IndexKey~ {
    <<struct>>
    -pool: &'a BufferPool~Box~dyn DiskManager~~
    -key: PhantomData~K~
    +create(pool: &'a BufferPool)$ Result~BTree, BTreeError~
    +open(pool: &'a BufferPool)$ Result~BTree, BTreeError~
    +insert(key: K, id: RowId) Result
    +get(key: &K) Result~Option~RowId~, BTreeError~
    +delete(key: &K, id: RowId) Result~bool, BTreeError~
    +range(bounds: impl RangeBounds~K~) Result~RangeIter, BTreeError~
  }
  class RangeIter~'a, K: IndexKey~ {
    <<struct>>
    -pool: &'a BufferPool~Box~dyn DiskManager~~
    -entries: vec::IntoIter~Entry~
    -next_leaf: Option~PageId~
    -start: Bound~Vec~u8~~
    -end: Bound~Vec~u8~~
    +next() Option~Result~(K, RowId), BTreeError~~
  }
  class AnyIndex~'a~ {
    <<enumeration>>
    Integer(BTree~i32~)
    BigInt(BTree~i64~)
    Boolean(BTree~bool~)
    Varchar(BTree~String~)
    +create(data_type: &DataType, pool: &'a BufferPool)$ Result~AnyIndex, BTreeError~
    +open(data_type: &DataType, pool: &'a BufferPool)$ Result~AnyIndex, BTreeError~
    +insert(value: &Value, id: RowId) Result
    +delete(value: &Value, id: RowId) Result
    +range(lower: Bound~&Value~, upper: Bound~&Value~) Result~Vec~RowId~, BTreeError~
    +lookup(value: &Value) Result~Vec~RowId~, BTreeError~
  }
  class ColumnIndex~'a~ {
    <<struct>>
    +column: usize
    +index: AnyIndex
  }
  class Entry {
    <<struct>>
    -key: Vec~u8~
    -id: RowId
  }
  class Node {
    <<enumeration>>
    Leaf(entries, next)
    Internal(first, children)
  }
  class BTreeError {
    <<enumeration>>
    KeyTooLarge(size)
    Corrupted
    Buffer(BufferError)
  }
  ColumnIndex *-- AnyIndex
  AnyIndex *-- BTree
  BTree ..> IndexKey
  BTree ..> RangeIter
  BTree ..> Node
  BTree ..> BTreeError
  RangeIter *-- Entry
  RangeIter ..> IndexKey
  Node *-- Entry
```

- `IndexKey`は，`i32`(`INTEGER`)，`i64`(`BIGINT`)，`bool`(`BOOLEAN`)，`String`(`VARCHAR`)に実装する．`encode`したバイト列を辞書式に比べた順序は，元の値の順序と同じである．`decode`は`self`を受け取らない関連関数である．
- `BTree`は，借りたバッファプールのページにノードを置く．ページ0は根のページ番号を書くメタページで，木の状態はすべてページにある．`BTree`の値そのものはプールへの参照だけを持つ．
- `BTree`のキーの型`K`はフィールドに現れないので，`PhantomData<K>`で型に結びつける．
- 項目`Entry`は，符号化したキーと`RowId`の組で並べる．同じキーの項目も，`RowId`で区別して順に並ぶ．内部ノードの区切りも`Entry`である．
- `Node`は，ページのバイト列と相互に変換する．操作のたびにページからノードを読み，変えたノードをページに書く．
- `RangeIter`は葉の項目を`Vec`に読み出してからピンを外し，読み終えたら右隣の葉を読む．範囲の終わりを超えたら止まる．
- `RowId`は，`PartialOrd`と`Ord`を導出し，ページとスロットの順に並ぶ．
- `AnyIndex`は，列の型ごとの`BTree`を`enum`でまとめる．SQLの`Value`を受け取り，列挙子に合う型のキーにして木を使う．`NULL`は木に入れない．
- `ColumnIndex`は，表の列の番号と`AnyIndex`の組で，`exec::dml`が行を加え，書き換え，消すときにインデックスを更新するために使う．

## トランザクション

`txn`の型と，タプルのヘッダーを示す．

```mermaid
classDiagram
  class TxnId {
    <<struct>>
    +0: u32
    +INVALID$ TxnId
  }
  class TxnStatus {
    <<enumeration>>
    InProgress
    Committed
    Aborted
  }
  class Transaction {
    <<struct>>
    -xid: TxnId
    +xid() TxnId
    +commit(self, manager: &mut TransactionManager)
    +rollback(self, manager: &mut TransactionManager)
  }
  class TransactionManager {
    <<struct>>
    -statuses: Vec~TxnStatus~
    +begin() Transaction
    +status(xid: TxnId) TxnStatus
    +snapshot(xid: TxnId) Snapshot
    +set_status(xid: TxnId, status: TxnStatus)
    +abort_unfinished()
    +save(path: &Path) io::Result
    +load(path: &Path)$ io::Result~TransactionManager~
  }
  class Snapshot {
    <<struct>>
    +xid: TxnId
    +xmax: TxnId
    +active: Vec~TxnId~
  }
  class TransactionError {
    <<enumeration>>
    InFailedTransaction
    AlreadyInProgress
    NotInTransactionBlock(statement)
  }
  class TupleHeader {
    <<struct>>
    +xmin: TxnId
    +xmax: TxnId
    +new(xmin: TxnId)$ TupleHeader
    +encode() [u8; 8]
    +split(bytes: &[u8])$ Result~(TupleHeader, &[u8]), TupleError~
  }
  class BuildContext~'a~ {
    <<struct>>
    +tables: &'a HashMap~String, HeapFile~
    +indexes: &'a HashMap~String, BufferPool~
    +catalog: &'a Catalog
    +snapshot: &'a Snapshot
    +manager: &'a TransactionManager
  }
  TransactionManager *-- TxnStatus
  TransactionManager ..> Transaction
  TransactionManager ..> Snapshot
  Transaction *-- TxnId
  Snapshot *-- TxnId
  TupleHeader *-- TxnId
  BuildContext --> Snapshot
  BuildContext --> TransactionManager
```

- `TxnId`は`u32`を包むニュータイプである．`Copy`を導出し，値として気軽に渡す．番号0(`TxnId::INVALID`)は，削除されていない版の`xmax`に使う．
- `Transaction`は`Clone`と`Copy`のどちらも導出しない．`commit`と`rollback`は`self`を受け取って値を消費するので，終えたトランザクションをもう一度終えるコードはコンパイルできない．
- `TransactionManager`は，番号ごとの状態を`Vec`に持つ．データディレクトリでは，チェックポイントで状態をファイル`xact`に書く．ファイルを読むとき，進行中のまま残った番号は中止したものとみなす．チェックポイントのあとの開始，コミット，中止は，ログからやり直す．
- `Snapshot`は，文を実行する時点で進行中のほかのトランザクション(`active`)と，まだ始まっていない番号の始まり(`xmax`)を持つ．`is_visible(header, snapshot, manager)`は，版を作ったトランザクションが見えて，削除したトランザクションが見えなければ真を返す．自分の変更は見える．
- `TupleHeader`は，ページに置くタプルの先頭の8バイトである．`HeapFile::rows`は，`visible`がヘッダーに真を返す版だけを復号する．`HeapFile::set_xmax`は，版のヘッダーの`xmax`だけを書き換える．
- `Database`の`Session`は，トランザクションの外(`Idle`)，中(`InTransaction`)，失敗したトランザクションの中(`Failed`)のどれかである．`execute`は`std::mem::take`で状態を取り出し，文の結果に合わせて次の状態を入れる．
- `exec::build`の`build`は，表，インデックス，カタログ，スナップショット，トランザクションの状態を`BuildContext`にまとめて受け取る．

## ログ

`wal`の型を示す．レコードのバイト配置は[layout.md](layout.md)にある．

```mermaid
classDiagram
  class Lsn {
    <<struct>>
    +0: u64
  }
  class WalRecord {
    <<enumeration>>
    Begin(xid)
    Commit(xid)
    Abort(xid)
    PageImage(file, page, bytes)
    Checkpoint
  }
  class WalWriter~W: Write~ {
    <<struct>>
    -out: BufWriter~W~
    -next: Lsn
    -flushed: Lsn
    -failed: bool
    +new(out: W, start: Lsn)$ WalWriter~W~
    +append(record: &WalRecord) io::Result~Lsn~
    +next_lsn() Lsn
    +open(path: &Path, end: Lsn)$ io::Result~WalWriter~File~~
    +flush_to(lsn: Lsn) io::Result
  }
  WalWriter ..> WalRecord
  WalWriter ..> Lsn
  WalRecord *-- TxnId
```

- `Lsn`は，ログのファイルの先頭からのバイト数で，レコードの終わりの位置を表す．
- `WalWriter`は，どの`Write`にもレコードを書ける．単体テストでは`Vec<u8>`に書く．`open`と`flush_to`は`WalWriter<File>`だけのメソッドで，`flush_to`は`BufWriter`を書き出してから`File::sync_data`でディスクに届くまで待つ．
- `wal::read_records`はバイト列からレコードを読み，途中で終わるレコードかCRC-32の合わないレコードで止まる．`wal::recover`は，最後の`Checkpoint`のあとのレコードをやり直す．
- `BufferPool`は，`PageLog`があれば，`write`で書き換えた`PageGuard`を捨てるときに`PageImage`を記録し，枠の`page_lsn`をレコードの位置にする．枠のページを書き戻す前に，`flush_to(page_lsn)`でログをディスクに届ける．
- `PageLog`の`WalWriter`は`Rc<RefCell<...>>`で，データベースと，表とインデックスのすべてのプールが共有する．
- `Database`は，開くときにリカバリしてからチェックポイントを取る．コミットでは`Commit`を記録し，`flush_to`でディスクに届けてから，トランザクションをコミット済みにする．

## サーバー

`server`の型を示す．メッセージのバイト配置は[layout.md](layout.md)にある．

```mermaid
classDiagram
  class FrontendMessage {
    <<enumeration>>
    EncryptionRequest
    Startup(parameters)
    Query(String)
    Terminate
  }
  class BackendMessage {
    <<enumeration>>
    AuthenticationOk
    ParameterStatus(name, value)
    ReadyForQuery(status)
    RowDescription(Vec~FieldDescription~)
    DataRow(Vec~Option~String~~)
    CommandComplete(String)
    EmptyQueryResponse
    ErrorResponse(code, message, position)
  }
  class FieldDescription {
    <<struct>>
    +name: String
    +type_oid: u32
    +type_size: i16
  }
  class ProtocolError {
    <<enumeration>>
    Io(io::Error)
    UnsupportedProtocol(u32)
    UnsupportedMessage(u8)
    Malformed
  }
  BackendMessage *-- FieldDescription
```

- `FrontendMessage`はクライアントが送るメッセージ，`BackendMessage`はサーバーが送るメッセージである．
- `server::message`の`read_startup(input: &mut impl Read)`は，種類のバイトのない起動のメッセージを読む．`read_message(input: &mut impl Read)`は，種類のバイトで始まるメッセージを読み，接続が閉じていれば`None`を返す．どちらも`FrontendMessage`か`ProtocolError`を返す．`write_message(output: &mut impl Write, message: &BackendMessage)`は，メッセージをバイト列にして書く．
- `server::connection`の`handle<S: Read + Write>(stream: S, db: &mut Database) -> Result<(), ProtocolError>`は，`Read`と`Write`を実装するどの型でも扱える．`main`は`TcpStream`を，単体テストは`Cursor`でバイト列を読ませる型を渡す．
- `ReadyForQuery`の`status`は，`Database::transaction_status`の`Idle`，`InTransaction`，`Failed`を，`I`，`T`，`E`のバイトにしたものである．
- `FieldDescription`の`type_oid`と`type_size`は，列の最初の`NULL`でない値の型で次のように決める．値がすべて`NULL`の列は`text`とする．

| 値の型 | `type_oid` | `type_size` |
| --- | --- | --- |
| `INTEGER` | 23 | 4 |
| `BIGINT` | 20 | 8 |
| `BOOLEAN` | 16 | 1 |
| `VARCHAR` | 1043 | -1 |
| すべて`NULL`(`text`) | 25 | -1 |
