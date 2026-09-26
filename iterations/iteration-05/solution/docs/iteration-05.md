# Iteration 5：表の作成と挿入(解説)

演習の各手順の模範解答と，その考え方を説明する．

## 5-1 準備

引き継いだ97個のテストがすべて通れば準備は終わりである．

## 5-2 文法と概念

課題の解答例である．

```rust
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct PhoneBook {
    numbers: HashMap<String, String>,
}

impl PhoneBook {
    pub fn new() -> PhoneBook {
        PhoneBook::default()
    }

    pub fn add(&mut self, name: &str, number: &str) -> Result<(), String> {
        if self.numbers.contains_key(name) {
            return Err(format!("{name} already exists"));
        }
        self.numbers.insert(name.to_string(), number.to_string());
        Ok(())
    }

    pub fn find(&self, name: &str) -> Option<&String> {
        self.numbers.get(name)
    }
}

pub fn label(age: Option<u32>) -> String {
    match age {
        Some(age) if age >= 20 => "adult".to_string(),
        Some(_) => "minor".to_string(),
        None => "unknown".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t() {
        let mut book = PhoneBook::new();
        book.add("alice", "123").unwrap();
        assert_eq!(book.add("alice", "456"), Err("alice already exists".to_string()));
        assert_eq!(book.find("alice"), Some(&"123".to_string()));
        assert_eq!(book.find("bob"), None);
        assert_eq!(label(Some(30)), "adult");
        assert_eq!(label(Some(3)), "minor");
        assert_eq!(label(None), "unknown");
        let words = vec!["x", "y", "z"];
        let mut numbered = Vec::new();
        for (i, w) in words.iter().enumerate() {
            numbered.push(format!("{}.{w}", i + 1));
        }
        assert_eq!(numbered, vec!["1.x", "2.y", "3.z"]);
    }
}
```

`label`の`match`は上から順に調べるので，ガードのない`Some(_)`は，ガードに合わなかった`Some`だけに一致する．

## 5-3 テストリスト

模範解答は[TESTLIST.md](../TESTLIST.md)である．

- 既存のテストの変更は4か所ある．字句解析の未知の単語，構文解析の補助関数，評価の範囲外のリテラル，結合テストの`execute`である．各モジュールの最初の項目にした．
- 字句解析は，識別子の形(数字と`_`，キーワードを含む単語，引用符で囲んだ名前)を先に，キーワードの追加をあとにした．
- 構文解析は，`VALUES`の複数の行，`CREATE TABLE`，`INSERT`，`SELECT`の順にした．どの文も，最小の形のあとに誤りの形を置いた．
- 代入の型変換は，組み合わせが多いので`catalog`の単体テストで確かめ，結合テストではSQLSTATEとメッセージを確かめる．
- 文の原子性は，2行目でエラーになる`INSERT`のあとに`SELECT`で行がないことを確かめる．

## 5-4 設計ドキュメント

| ファイル | 変更 | 理由 |
| --- | --- | --- |
| `c4-context.md` | 開発者がすることを「表を作り，行を挿入し，問い合わせる」にした | 扱う文が増えた |
| `c4-container.md` | ライブラリが表と行をメモリーに持つことを書いた | 状態を持つようになった |
| `c4-component.md` | `database`と`catalog`を加え，入口を`database`にした．`DataType`を使う`ast`，`parser`，`catalog`から`value`への依存を加えた | ルートは公開する名前をまとめるだけになった |
| `code-types.md` | `Database`，`Catalog`，`TableSchema`，`Column`，`DataType`，`SchemaError`，`Statement`と各文の型，`StatementResult`を加えた．トークンの型を省いた | 図が大きくなったので，このIterationで注目する型に絞った |
| `code-sequence.md` | `INSERT`の流れに描き直した | カタログとの照合と，行を加える時点を示す |

- `DataType`は，構文木(`ColumnDef`)とカタログ(`Column`)の両方が使うので，どちらにも依存しない`value`に置いた．
- `SchemaError`は，表の定義と照らし合わせる処理と同じ`catalog`に置いた．`error`が`SchemaError`を翻訳する．

## 5-5 テスト駆動の実装

### 識別子

```rust
#[test]
fn word_that_is_not_a_keyword_is_an_identifier() {
    assert_eq!(
        kinds("1 users"),
        Ok(vec![Token::Integer(1), Token::Identifier("USERS".to_string())])
    );
}
```

`Token::Identifier(String)`を加えた．英字の並びを読んでいた`keyword`は，単語を読んでキーワードか識別子を返す`word`に変えた．
英字か`_`で始まり，英字，数字，`_`が続く単語を読む．`take()`は，組にしたパーサーが読んだ部分の文字列を返す．

```rust
fn word(input: &mut Input<'_>) -> winnow::Result<Token> {
    (
        one_of(('a'..='z', 'A'..='Z', '_')),
        take_while(0.., ('a'..='z', 'A'..='Z', '0'..='9', '_')),
    )
        .take()
        .map(to_word_token)
        .parse_next(input)
}

fn to_word_token(word: &str) -> Token {
    let upper = word.to_ascii_uppercase();
    match to_keyword(&upper) {
        Some(keyword) => Token::Keyword(keyword),
        None => Token::Identifier(upper),
    }
}
```

数字と`_`，`VALUESX`の項目は，書いた時点で通る．

### 引用符で囲んだ識別子

```rust
#[test]
fn quoted_identifier_may_be_a_keyword_and_contain_quotes() {
    assert_eq!(
        kinds("\"select\" \"a\"\"b\""),
        Ok(vec![
            Token::Identifier("select".to_string()),
            Token::Identifier("a\"b".to_string())
        ])
    );
}
```

文字列リテラルと同じ形で，`"`で囲んだ部分を読む．正規化もキーワードの判定もしない．

```rust
fn quoted_identifier(input: &mut Input<'_>) -> winnow::Result<Token> {
    delimited('"', repeat(0.., quoted_identifier_char), '"')
        .map(Token::Identifier)
        .parse_next(input)
}

fn quoted_identifier_char(input: &mut Input<'_>) -> winnow::Result<char> {
    alt(("\"\"".value('"'), none_of('"'))).parse_next(input)
}
```

キーワードの2つの項目では，`Keyword`の列挙子と，`to_keyword`と`Display`の表に1行ずつ加えた．

### 複数の行の`VALUES`

```rust
#[test]
fn values_with_several_rows() {
    assert_eq!(
        statement("VALUES (1, 2), (3, 4)"),
        Ok(Statement::Values(Values {
            rows: vec![vec![int(1), int(2)], vec![int(3), int(4)]]
        }))
    );
}
```

`parse`が`Statement`を返すように変え，`Values`を行の並びにした．既存の構文解析のテストは，1行目の式を返す補助関数で書き直した．

```rust
fn parse_sql(sql: &str) -> Result<Vec<Expr>, ParseError> {
    match parse(&tokenize(sql).unwrap())? {
        Statement::Values(mut values) => Ok(values.rows.remove(0)),
        other => panic!("not a VALUES statement: {other:?}"),
    }
}
```

```rust
fn statement(input: &mut Tokens<'_>) -> ModalResult<Statement> {
    alt((
        values.map(Statement::Values),
        create_table.map(Statement::CreateTable),
        insert.map(Statement::Insert),
        select.map(Statement::Select),
    ))
    .parse_next(input)
}

fn values(input: &mut Tokens<'_>) -> ModalResult<Values> {
    preceded(
        literal(keyword(Keyword::Values)),
        cut_err(separated(1.., row, literal(Token::Comma))),
    )
    .map(to_values)
    .parse_next(input)
}

fn row(input: &mut Tokens<'_>) -> ModalResult<Vec<Expr>> {
    delimited(
        literal(Token::LParen),
        separated(1.., cut_err(expr), literal(Token::Comma)),
        literal(Token::RParen),
    )
    .parse_next(input)
}
```

`keyword(Keyword::Values)`は`Token::Keyword(Keyword::Values)`を作るだけの補助関数である．

### 行の長さの検査

```rust
#[test]
fn values_rows_of_different_lengths_are_an_error() {
    assert_eq!(
        statement("VALUES (1, 2), (3)"),
        Err(ParseError::ValuesLengthMismatch)
    );
}
```

`ParseError`に位置を持たない列挙子を加え，構文解析が成功したあとに行の長さを調べる．

```rust
        Ok(statement) => {
            match &statement {
                Statement::Values(values) => check_row_lengths(values)?,
                Statement::Insert(insert) => check_row_lengths(&insert.values)?,
                Statement::CreateTable(_) | Statement::Select(_) => {}
            }
            Ok(statement)
        }

fn check_row_lengths(values: &Values) -> Result<(), ParseError> {
    let width = values.rows[0].len();
    for row in &values.rows {
        if row.len() != width {
            return Err(ParseError::ValuesLengthMismatch);
        }
    }
    Ok(())
}
```

`separated(1.., ...)`が1行以上を求めるので，`values.rows[0]`は必ずある．

### `CREATE TABLE`

```rust
#[test]
fn create_table_with_each_data_type() {
    assert_eq!(
        statement("CREATE TABLE t (a INTEGER, b INT, c BIGINT, d BOOLEAN, e VARCHAR(10))"),
        Ok(Statement::CreateTable(CreateTable {
            name: "T".to_string(),
            columns: vec![
                ColumnDef {
                    name: "A".to_string(),
                    data_type: DataType::Integer
                },
                // B から E も同じ形
            ]
        }))
    );
}
```

`value`に`DataType`を，`ast`に`CreateTable`と`ColumnDef`を定義した．
構文解析の結果の組を構造体にする関数は，引数にタプルのパターンを書いた．

```rust
fn create_table(input: &mut Tokens<'_>) -> ModalResult<CreateTable> {
    preceded(
        (
            literal(keyword(Keyword::Create)),
            literal(keyword(Keyword::Table)),
        ),
        cut_err((
            identifier,
            delimited(
                literal(Token::LParen),
                separated(1.., column_def, literal(Token::Comma)),
                literal(Token::RParen),
            ),
        )),
    )
    .map(to_create_table)
    .parse_next(input)
}

fn to_create_table((name, columns): (String, Vec<ColumnDef>)) -> CreateTable {
    CreateTable { name, columns }
}
```

`VARCHAR`の長さは，ガードで1以上の整数だけを受け付ける．`ok()`は，`Result`を`Option`に変える．

```rust
fn varchar_length(token: &Spanned<Token>) -> Option<usize> {
    match token.value {
        Token::Integer(n) if n >= 1 => usize::try_from(n).ok(),
        _ => None,
    }
}
```

`VARCHAR(0)`と`CREATE TABLE t ()`の項目は，`cut_err`のおかげで誤りのあるトークンの位置が返り，書いた時点で通る．

### `INSERT`と`SELECT`

```rust
#[test]
fn insert_into_named_columns() {
    assert_eq!(
        statement("INSERT INTO t (b, a) VALUES (1, 2)"),
        Ok(Statement::Insert(Insert {
            table: "T".to_string(),
            columns: Some(vec!["B".to_string(), "A".to_string()]),
            values: Values {
                rows: vec![vec![int(1), int(2)]]
            }
        }))
    );
}
```

列の並びは省略できるので，`opt`で読み，`Option<Vec<String>>`にした．

```rust
fn insert(input: &mut Tokens<'_>) -> ModalResult<Insert> {
    preceded(
        (
            literal(keyword(Keyword::Insert)),
            literal(keyword(Keyword::Into)),
        ),
        cut_err((
            identifier,
            opt(delimited(
                literal(Token::LParen),
                separated(1.., identifier, literal(Token::Comma)),
                literal(Token::RParen),
            )),
            values,
        )),
    )
    .map(to_insert)
    .parse_next(input)
}
```

`SELECT`は`SELECT * FROM 表`だけを読む．`SELECT *`で終わる項目は，`cut_err`の中で文が終わるので`UnexpectedEnd`になる．

### `BIGINT`

```rust
#[test]
fn literal_beyond_integer_is_a_bigint() {
    assert_eq!(eval_sql("2147483647"), Ok(Value::Integer(i32::MAX)));
    assert_eq!(eval_sql("2147483648"), Ok(Value::BigInt(2147483648)));
}

#[test]
fn arithmetic_with_a_bigint_is_bigint() {
    assert_eq!(eval_sql("2147483647 + 2147483648"), Ok(Value::BigInt(4294967295)));
    assert_eq!(eval_sql("2147483648 - 1"), Ok(Value::BigInt(2147483647)));
}
```

`Value::BigInt(i64)`を加え，`INTEGER`に収まらないリテラルを`BIGINT`にした．
算術は，両辺を`i64`として計算し，両辺が`INTEGER`のときだけ結果を`INTEGER`の範囲に戻す．`i32`の演算は`i64`で正確に計算できるので，範囲外の判定もこれで済む．

```rust
fn arithmetic(op: &ArithmeticOp, left: Value, right: Value) -> Result<Value, EvalError> {
    let (a, b) = match (integer_of(&left), integer_of(&right)) {
        (Some(a), Some(b)) => (a, b),
        _ => return null_or_mismatch(&left, &right),
    };
    let result = match op {
        ArithmeticOp::Add => a.checked_add(b),
        // 引き算，掛け算，割り算も同じ
    };
    let n = match result {
        Some(n) => n,
        None => return Err(EvalError::NumericOutOfRange),
    };
    if matches!(left, Value::BigInt(_)) || matches!(right, Value::BigInt(_)) {
        Ok(Value::BigInt(n))
    } else {
        match i32::try_from(n) {
            Ok(n) => Ok(Value::Integer(n)),
            Err(_) => Err(EvalError::NumericOutOfRange),
        }
    }
}

fn integer_of(value: &Value) -> Option<i64> {
    match value {
        Value::Integer(n) => Some(i64::from(*n)),
        Value::BigInt(n) => Some(*n),
        Value::Boolean(_) | Value::Varchar(_) | Value::Null => None,
    }
}
```

比較も，整数どうしなら`integer_of`で`i64`にして比べる．単項の`-`は，`INTEGER`と`BIGINT`をそれぞれの型のまま`checked_neg`する．

### カタログ

```rust
#[test]
fn creating_the_same_table_twice_is_an_error() {
    let mut catalog = Catalog::default();
    catalog.create_table(users()).unwrap();
    assert_eq!(
        catalog.create_table(users()),
        Err(SchemaError::DuplicateTable {
            table: "USERS".to_string()
        })
    );
}
```

`Catalog`は表の名前から`TableSchema`を引く`HashMap`を持つ．列の名前の重複は，各列をそれより前の列と比べて調べる．

```rust
impl Catalog {
    pub fn create_table(&mut self, schema: TableSchema) -> Result<(), SchemaError> {
        if self.tables.contains_key(&schema.name) {
            return Err(SchemaError::DuplicateTable { table: schema.name });
        }
        for (index, column) in schema.columns.iter().enumerate() {
            for earlier in &schema.columns[..index] {
                if earlier.name == column.name {
                    return Err(SchemaError::DuplicateColumn {
                        column: column.name.clone(),
                    });
                }
            }
        }
        self.tables.insert(schema.name.clone(), schema);
        Ok(())
    }
}
```

### 代入の型変換

```rust
#[test]
fn integers_are_converted_to_the_column_type() {
    assert_eq!(
        column("A", DataType::BigInt).assign(Value::Integer(1)),
        Ok(Value::BigInt(1))
    );
    assert_eq!(
        column("A", DataType::Integer).assign(Value::BigInt(1)),
        Ok(Value::Integer(1))
    );
    assert_eq!(
        column("A", DataType::Integer).assign(Value::BigInt(2147483648)),
        Err(SchemaError::OutOfRange)
    );
}
```

列の型と値の組に対する`match`で，代入の規則を1か所に書いた．最後の腕は，どの組にも当てはまらない型の不一致である．

```rust
impl Column {
    pub fn assign(&self, value: Value) -> Result<Value, SchemaError> {
        match (&self.data_type, value) {
            (_, Value::Null) => Ok(Value::Null),
            (DataType::Integer, Value::Integer(n)) => Ok(Value::Integer(n)),
            (DataType::Integer, Value::BigInt(n)) => match i32::try_from(n) {
                Ok(n) => Ok(Value::Integer(n)),
                Err(_) => Err(SchemaError::OutOfRange),
            },
            (DataType::BigInt, Value::Integer(n)) => Ok(Value::BigInt(i64::from(n))),
            (DataType::BigInt, Value::BigInt(n)) => Ok(Value::BigInt(n)),
            (DataType::Boolean, Value::Boolean(b)) => Ok(Value::Boolean(b)),
            (DataType::Varchar(length), Value::Varchar(s)) => {
                if s.chars().count() > *length {
                    Err(SchemaError::ValueTooLong {
                        data_type: self.data_type.clone(),
                    })
                } else {
                    Ok(Value::Varchar(s))
                }
            }
            (expected, value) => Err(SchemaError::TypeMismatch {
                column: self.name.clone(),
                expected: expected.clone(),
                found: value.type_name(),
            }),
        }
    }
}
```

### `Database`と結合テスト

```rust
#[test]
fn creates_a_table_inserts_rows_and_selects_them() {
    let mut db = Database::new();
    assert_eq!(
        db.execute("CREATE TABLE users (id INTEGER, name VARCHAR(20))"),
        Ok(StatementResult::CreateTable)
    );
    assert_eq!(
        db.execute("INSERT INTO users VALUES (1, 'alice'), (2, 'bob')"),
        Ok(StatementResult::Insert { count: 2 })
    );
    assert_eq!(
        rows(&mut db, "SELECT * FROM users"),
        QueryResult {
            columns: vec!["ID".to_string(), "NAME".to_string()],
            rows: vec![
                vec![Value::Integer(1), varchar("alice")],
                vec![Value::Integer(2), varchar("bob")],
            ]
        }
    );
}
```

`database`モジュールに`Database`，`StatementResult`，`QueryResult`を置き，ルートの`execute`を`Database::execute`に移した(Refactor)．
`execute`は文の種類ごとにメソッドを呼ぶ．

```rust
    pub fn execute(&mut self, sql: &str) -> Result<StatementResult, Error> {
        let tokens = tokenize(sql)?;
        match parse(&tokens)? {
            Statement::Values(values) => Ok(StatementResult::Rows(evaluate_values(&values)?)),
            Statement::CreateTable(create) => self.create_table(create),
            Statement::Insert(insert) => self.insert(insert),
            Statement::Select(select) => self.select(&select),
        }
    }
```

`INSERT`は，列の番号の並びを決めてから，各行を`NULL`で埋めた行に値を入れていく．
すべての行を`new_rows`に作り終えてから表の行に加えるので，途中でエラーになれば1行も加わらない．

```rust
    fn insert(&mut self, insert: Insert) -> Result<StatementResult, Error> {
        let schema = self.catalog.table(&insert.table)?;
        let targets = target_columns(schema, insert.columns)?;
        let mut new_rows = Vec::new();
        for exprs in &insert.values.rows {
            if exprs.len() > targets.len() {
                return Err(SchemaError::MoreValuesThanColumns.into());
            }
            if exprs.len() < targets.len() {
                return Err(SchemaError::MoreColumnsThanValues.into());
            }
            let mut row = vec![Value::Null; schema.columns.len()];
            for (expr, &index) in exprs.iter().zip(&targets) {
                row[index] = schema.columns[index].assign(eval(expr)?)?;
            }
            new_rows.push(row);
        }
        let count = new_rows.len();
        let table_rows = self
            .rows
            .get_mut(&insert.table)
            .expect("every table in the catalog has its rows");
        table_rows.append(&mut new_rows);
        Ok(StatementResult::Insert { count })
    }
```

- `SchemaError::MoreValuesThanColumns.into()`は，`From`の実装で`Error`に変換する．`?`を使わずに`Err`を作るときは`into()`を呼ぶ．
- `for (expr, &index) in ...`の`&index`は，参照から値を取り出すパターンである．
- `append`は，別の`Vec`の要素をすべて移す．

`error`には，`SchemaError`の各列挙子を，PostgreSQLと同じSQLSTATEとメッセージに翻訳する`From`を加えた．
`tables.rs`のほかの項目(大文字と小文字，名前付きの列，エラーのSQLSTATE，原子性)は，ここまでの実装で通る．

## 5-6 振り返り

1. 既存のテストの変更は4か所である．型や関数の形が変わると，テストの補助関数を変えるだけで多くの項目を直せることがある．
2. `Table { schema, rows }`にまとめると，定義を引くときも行を引くときも同じ表を探せばよく，単純になる．一方，Iteration 13で行をページに，Iteration 14でファイルに移すとき，定義と行が同じ構造体にあると，定義を扱うコードまで変わりやすい．定義と行を分けておくと，行の置き場所だけを変えられる．
3. 空の`Vec`で「指定しない」を表すと，`INSERT INTO t () VALUES ()`のように空の並びを書いた場合と区別できない．`Option`なら区別でき，`match`で両方を扱うことをコンパイラーが求める．
4. `SchemaError`を`database`に置くと，`error`が`database`に依存し，`database`も`error`に依存する(循環)．照合をする`catalog`に置けば，依存は一方向になる．
5. 代入の規則は列の型で決まるので，`Column`のメソッドにした．`Value`のメソッドにすると，値が列の定義(名前と型)を知ることになる．`database`の関数にすると，カタログの単体テストで規則を確かめられない．
6. 模範解答の設計ドキュメントは，照合スクリプトで問題がないことを確かめてある．

## 5-7 発展課題

解答例である．挿入する行を，`VALUES`の並びと既定値の1行の直和型`InsertSource`で表す．

```rust
#[derive(Debug, PartialEq)]
pub struct Insert {
    pub table: String,
    pub columns: Option<Vec<String>>,
    pub source: InsertSource,
}

#[derive(Debug, PartialEq)]
pub enum InsertSource {
    Values(Values),
    DefaultValues,
}
```

```rust
            alt((
                values.map(InsertSource::Values),
                (
                    literal(keyword(Keyword::Default)),
                    literal(keyword(Keyword::Values)),
                )
                    .void()
                    .map(default_values),
            )),

fn default_values(_: ()) -> InsertSource {
    InsertSource::DefaultValues
}
```

`InsertSource`は構文木を含み`Clone`を導出していないので，`value`は使えない．`void()`で結果を捨て，`()`から`InsertSource`を作る関数を渡す．

```rust
        let values = match insert.source {
            InsertSource::Values(values) => values,
            InsertSource::DefaultValues => {
                self.rows
                    .get_mut(&insert.table)
                    .expect("every table in the catalog has its rows")
                    .push(vec![Value::Null; schema.columns.len()]);
                return Ok(StatementResult::Insert { count: 1 });
            }
        };
```

```rust
#[test]
fn inserts_a_row_of_default_values() {
    let mut db = Database::new();
    db.execute("CREATE TABLE t (a INTEGER, b BOOLEAN)").unwrap();
    assert_eq!(
        db.execute("INSERT INTO t DEFAULT VALUES"),
        Ok(StatementResult::Insert { count: 1 })
    );
    assert_eq!(
        rows(&mut db, "SELECT * FROM t").rows,
        vec![vec![Value::Null, Value::Null]]
    );
}
```

キーワード`DEFAULT`と，`check_row_lengths`で`InsertSource::Values`だけを調べる変更も要る．
