# ロードマップ

このハンズオンでは，PostgreSQLのクライアントから接続できるリレーショナルデータベース`ferrodb`を，23回のIterationで少しずつ育てる．
SQLの字句解析から始め，構文解析と実行，ページ単位のストレージ，B+木インデックス，トランザクション，WAL，PostgreSQL互換のネットワークプロトコルまでを自作する．

## 完成形

最後のIterationを終えると，`ferrodb`は次のように動く．

```console
$ cargo run --release -q -- serve --data-dir ./data --port 5433
ferrodb listening on 127.0.0.1:5433
```

別の端末から`psql`で接続し，標準SQLで表を作って問い合わせる．

```console
$ psql -h 127.0.0.1 -p 5433 -U alice ferro
ferro=> CREATE TABLE dept (code VARCHAR(10) PRIMARY KEY, title VARCHAR(20) NOT NULL);
CREATE TABLE
ferro=> CREATE TABLE emp (id INTEGER PRIMARY KEY, name VARCHAR(20) NOT NULL, dept VARCHAR(10), salary INTEGER);
CREATE TABLE
ferro=> INSERT INTO dept VALUES ('dev', 'Development'), ('ops', 'Operations');
INSERT 0 2
ferro=> INSERT INTO emp VALUES (1, 'Sato', 'dev', 500), (2, 'Suzuki', 'dev', 450), (3, 'Tanaka', 'ops', 400), (4, 'Ito', NULL, 300);
INSERT 0 4
ferro=> SELECT d.title, COUNT(*) AS n, SUM(e.salary) AS total
ferro->   FROM emp e LEFT JOIN dept d ON e.dept = d.code
ferro->  GROUP BY d.title
ferro->  ORDER BY n DESC, d.title
ferro->  FETCH FIRST 2 ROWS ONLY;
    TITLE    | N | TOTAL
-------------+---+-------
 Development | 2 |   950
 Operations  | 1 |   400
(2 rows)

ferro=> CREATE INDEX emp_salary ON emp (salary);
CREATE INDEX
ferro=> EXPLAIN SELECT name FROM emp WHERE salary >= 450;
                      QUERY PLAN
------------------------------------------------------
 Project [EMP.NAME]
   IndexScan EMP_SALARY on EMP (EMP.SALARY >= 450)
(2 rows)
```

2つのセッションが同じ行を更新すると，スナップショット分離によって後から更新した側がエラーになる．

```console
-- セッションA
ferro=> START TRANSACTION ISOLATION LEVEL REPEATABLE READ;
START TRANSACTION
ferro=*> UPDATE emp SET salary = salary + 10 WHERE id = 1;
UPDATE 1

-- セッションB
ferro=> START TRANSACTION ISOLATION LEVEL REPEATABLE READ;
START TRANSACTION
ferro=*> UPDATE emp SET salary = salary + 20 WHERE id = 1;
(セッションAの終了を待つ)

-- セッションA
ferro=*> COMMIT;
COMMIT

-- セッションB
ERROR:  could not serialize access due to concurrent update
ferro=!> ROLLBACK;
ROLLBACK
```

サーバーのプロセスを`kill -9`で強制終了して起動し直しても，コミット済みのデータはWALから復旧する．

## 対応するSQL

標準SQL(SQL:2023)のCore SQLのうち，次の範囲に対応する．

- データ型：`INTEGER`，`BIGINT`，`BOOLEAN`，`VARCHAR(n)`と`NULL`
- 式：算術演算，比較演算，`AND`/`OR`/`NOT`と3値論理，`IS [NOT] NULL`，文字列の連結`||`
- DDL：`CREATE TABLE`，`DROP TABLE`，制約`PRIMARY KEY`/`NOT NULL`/`UNIQUE`
- DML：`INSERT`，`UPDATE`，`DELETE`
- 問い合わせ：`VALUES`，`SELECT`の`WHERE`/`ORDER BY`/`OFFSET`/`FETCH FIRST`/`DISTINCT`，`CROSS`/`INNER`/`LEFT JOIN`，`GROUP BY`/`HAVING`と集約関数`COUNT`/`SUM`/`AVG`/`MIN`/`MAX`
- トランザクション：`START TRANSACTION`，`COMMIT`，`ROLLBACK`，分離レベル`READ COMMITTED`と`REPEATABLE READ`
- エラー：SQLSTATEを返す．標準が定めていないサブクラスはPostgreSQLのコードに合わせる

標準に従い，引用符で囲まない識別子は大文字に正規化する．`"name"`のように二重引用符で囲んだ識別子は，大文字と小文字を区別する．

標準SQLにない次の2つは，拡張として実装する．

- `CREATE INDEX`/`DROP INDEX`：B+木インデックスを作る
- `EXPLAIN`：実行計画を表示する

## Iterationの進め方

各Iterationは`iterations/iteration-NN/`にあり，`exercise/`と`solution/`の2つのディレクトリからなる．
Iteration 0では，受講者が`exercise/`に`cargo init`でCargoパッケージを作る．
Iteration 1からの`exercise/`は，1つ前のIterationの`solution/`と同じ内容から始まる．
`exercise/`は単独のCargoパッケージなので，`cargo`のコマンドは`exercise/`の中で実行する．
受講者は`exercise/`で次の順に作業する．

1. このロードマップの「要件」と「使用例」を読み，確かめるべき振る舞いをテストリストに書き出す．
2. `design/`の設計ドキュメントを，このIterationの終わりの状態に更新する．
3. テストリストの項目を1つずつ，Red → Green → Refactorで実装する．
4. 実装を終えたら設計ドキュメントと実装を見比べ，名前と依存関係を一致させる(設計レビュー)．

作業を終えたら`solution/`と見比べる．`solution/`にはテストリスト，設計ドキュメント，実装の模範解答と，その解説がある．

テストリストの書き方は[tdd.md](tdd.md)，設計ドキュメントの書き方は[design.md](design.md)にある．
新しく使うRustの文法と概念は`docs/rust/iteration-NN.md`，データベースの理論は`docs/db/iteration-NN.md`で説明する．

## テストの分け方

- 単体テストは，各モジュールの`#[cfg(test)] mod tests`に書く．字句解析器のトークン列，構文解析器の構文木，式の評価，ページのバイト配置，B+木の操作など，モジュールの関数を直接確かめる．
- 結合テストは，パッケージの`tests/`に書く．ライブラリの公開API(`Database::execute`など)にSQLを渡し，結果の表やエラーのSQLSTATEを確かめる．
  - Iteration 6からは，REPLの関数に入力の文字列を渡し，出力の文字列を確かめるテストも加える．
  - Iteration 20からは，テストの中でサーバーを起動し，`postgres`クレートのクライアントから接続して確かめる．

## Iteration一覧

| # | 作る機能 | Rustで学ぶこと | データベースで学ぶこと |
| --- | --- | --- | --- |
| 0 | プロジェクトの作成と，`VALUES (1, 2 + 3)`の字句解析 | Cargo，関数，`enum`，`Vec`，`Result`，`#[test]`，winnowの基本 | SQL処理の流れ，字句解析 |
| 1 | `VALUES`の算術式を構文解析して評価する | 代数的データ型，再帰的な`enum`と`Box`，`match`，整数の検査付き演算 | 構文木，演算子の優先順位 |
| 2 | 真偽値，比較，`NULL`と3値論理 | `Option`，タプルの`match`，メソッド | 3値論理 |
| 3 | 文字列リテラルと連結 | 所有権，ムーブ，借用，`String`と`&str`，`Clone` | 文字列型 |
| 4 | SQLSTATEとエラー位置の報告 | エラー型の設計，`?`，`From`，`Display`，`std::error::Error` | SQLSTATE |
| 5 | `CREATE TABLE`，`INSERT`，`SELECT * FROM` | `struct`，`impl`，`&mut self`，`HashMap` | カタログ，スキーマ，型検査 |
| 6 | REPLとモジュール階層の整理 | バイナリクレート，標準入出力，モジュールと可視性，`Display`の実装 | 結果の表示 |
| 7 | `WHERE`，列の選択，別名 | イテレータ，クロージャ，`collect`と`Result` | 名前解決(バインド) |
| 8 | `UPDATE`，`DELETE`，`DROP TABLE`，制約 | `iter_mut`，`retain`，借用の衝突の避け方，`HashSet` | 整合性制約 |
| 9 | `ORDER BY`，`OFFSET`，`FETCH FIRST`，`DISTINCT` | `Ord`/`PartialOrd`の実装，`sort_by` | `NULL`の順序 |
| 10 | 実行計画と`EXPLAIN` | トレイト，トレイトオブジェクト，`Box<dyn Trait>` | 論理計画，Volcanoモデル |
| 11 | `CROSS`/`INNER`/`LEFT JOIN` | トレイトオブジェクトの組み合わせ，`Option::take` | 結合，入れ子ループ結合 |
| 12 | `GROUP BY`，`HAVING`，集約関数 | `Hash`/`Eq`の実装，`entry` API | 集約，グループ化の規則 |
| 13 | ページとタプルのバイト表現 | 配列とスライス，`to_le_bytes`，`TryFrom`，定数 | スロット付きページ |
| 14 | ヒープファイルとデータディレクトリ | ファイル入出力，`Path`，`clap`によるサブコマンド | ヒープファイル，永続化 |
| 15 | バッファプール | ライフタイム注釈，`Drop`とRAII，内部可変性，ジェネリクス | バッファ管理，置換方式 |
| 16 | B+木 | トレイト境界，関連型，`Iterator`の実装 | B+木，順序を保つキーの符号化 |
| 17 | インデックスの利用 | `enum`による静的ディスパッチ，`std::ops::Bound` | インデックススキャン，一意性の検査 |
| 18 | トランザクションとMVCC | ニュータイプ，`Copy`，所有権を使うAPI設計 | MVCC，可視性の判定 |
| 19 | WALとクラッシュリカバリ | `Write`トレイト，`BufWriter`，`fsync`，プロセスを使うテスト | WAL，REDO，チェックポイント |
| 20 | PostgreSQL互換プロトコル | `std::net`，`Read`/`Write`のジェネリクス，ビッグエンディアン | フロントエンド/バックエンドプロトコル |
| 21 | 複数の同時接続 | スレッド，`Arc`，`Mutex`/`RwLock`，`Send`/`Sync` | ラッチとロック |
| 22 | 分離レベルと書き込みの競合 | `Condvar`，並行処理のテスト | スナップショット分離，更新の競合 |

## Iteration 0：プロジェクトの作成と字句解析

### 要件

- `cargo init`で，ライブラリクレート`ferrodb`のパッケージを作る．
- SQLの文字列を，トークンの列に分ける．
- 対象は`VALUES (1, 2 + 3)`の形の文である．キーワード`VALUES`，整数，`(`，`)`，`,`，`+`，`-`，`*`，`/`を扱う．
- キーワードは大文字と小文字を区別しない．
- 空白と改行は読み飛ばす．
- 整数は64ビット符号付き整数(`i64`)として読む．
- 解釈できない文字，`VALUES`以外の単語，`i64`に収まらない整数があれば，その位置(先頭の文字を1として数えた番号)を含むエラーを返す．

### 使用例

```rust
use ferrodb::{tokenize, Keyword, LexError, Token};

assert_eq!(
    tokenize("values (1, 2 + 3)"),
    Ok(vec![
        Token::Keyword(Keyword::Values),
        Token::LParen,
        Token::Integer(1),
        Token::Comma,
        Token::Integer(2),
        Token::Plus,
        Token::Integer(3),
        Token::RParen,
    ])
);
assert_eq!(tokenize("VALUES (1 ? 2)"), Err(LexError { position: 11 }));
```

### モジュール

- `token`：`enum Token`，`enum Keyword`
- `lexer`：`pub fn tokenize(sql: &str) -> Result<Vec<Token>, LexError>`，`struct LexError { position: usize }`

### 設計ドキュメントの更新

- `c4-context.md`：ライブラリを使う開発者と`ferrodb`
- `c4-container.md`：ライブラリクレート`ferrodb`
- `c4-component.md`：`crate`(`lib.rs`)，`lexer`，`token`と，その間の依存
- `code-types.md`：`Token`，`Keyword`，`LexError`
- `code-sequence.md`：利用者が`tokenize`を呼び，トークン列を受け取る流れ

### 学ぶこと

- Rust：Cargoのパッケージとクレート，`Cargo.toml`，`fn`，整数型，`&str`，`enum`と`#[derive]`，`struct`の基本，`Vec`，`Option`と`Result`の基本，`if`と`match`の基本，`#[test]`と`assert_eq!`，`mod`と`pub use`
- winnow：パーサー関数の形`fn(&mut &str) -> winnow::Result<T>`，`alt`，`repeat`，`preceded`と`terminated`，`value`と`map`，`Parser::parse`
- データベース：SQLが字句解析 → 構文解析 → 計画 → 実行の順に処理されること

### 受講者が行うツール操作

- `cargo init --lib --name ferrodb`で，`exercise/`をCargoパッケージにする．
- `cargo add winnow`で依存を追加する．
- `cargo build`でビルドし，`cargo test`でテストを実行する．
- `cargo test --lib`で単体テストだけを，`cargo test --test tokenize`で1つの結合テストだけを実行する．
- `cargo fmt`で整形し，`cargo clippy`でリントを実行する．

## Iteration 1：算術式の構文解析と評価

### 要件

- `VALUES (1 + 2 * 3, -4)`のように，1行の`VALUES`を構文解析して評価し，結果の行を返す．
- 演算子`+`，`-`，`*`，`/`と単項の`-`，括弧に対応する．優先順位は標準SQLに従う．
- 整数は`INTEGER`(32ビット符号付き)として計算する．範囲を超えたらエラーにする．
- `0`で割ったらエラーにする．整数の割り算は0の方向に切り捨てる．
- 結果の列名は`COLUMN1`，`COLUMN2`，…とする．

### 使用例

```rust
let result = ferrodb::execute("VALUES (1 + 2 * 3, -(4 - 6) / 2)").unwrap();
assert_eq!(result.columns, vec!["COLUMN1", "COLUMN2"]);
assert_eq!(result.rows, vec![vec![Value::Integer(7), Value::Integer(1)]]);
```

### モジュール

- `ast`：`enum Expr`，`enum BinaryOp`，`enum UnaryOp`，`struct Values`
- `token`：`Token`と`Keyword`に`Eq`を導出する(winnowの`literal`でトークンを比べるため)．
- `parser`：`pub fn parse(tokens: &[Token]) -> Result<Values, ParseError>`．SQLの優先順位表を書き，式の解析にwinnowの`expression`を使う．
- `value`：`enum Value { Integer(i32) }`
- `eval`：`pub fn eval(expr: &Expr) -> Result<Value, EvalError>`
- ルート：`pub fn execute(sql: &str) -> Result<QueryResult, Error>`

### 設計ドキュメントの更新

- `c4-component.md`：`parser`，`ast`，`value`，`eval`と，`execute`を持つルートモジュールを加える．
- `code-types.md`：`Expr`，`Values`，`Value`，`QueryResult`と各エラー型を加える．
- `code-sequence.md`：`execute`が字句解析 → 構文解析 → 評価を呼ぶ流れにする．

### 学ぶこと

- Rust：代数的データ型(直積型の`struct`と直和型の`enum`，その組み合わせ)，不正な状態を表現できない型の設計，再帰的な`enum`と`Box`，網羅的な`match`と列挙子の分解，`checked_add`などの検査付き演算，`?`によるエラーの伝播と`map_err`，`for`と`mut`
- winnow：トークン列(`&[Token]`)を入力にするパーサー，`expression`による優先順位解析
- データベース：構文木，演算子の優先順位と結合性

### 既存テストへの影響

なし．

## Iteration 2：真偽値，比較，NULLと3値論理

### 要件

- リテラル`TRUE`，`FALSE`，`UNKNOWN`，`NULL`を扱う．
- 比較演算子`=`，`<>`，`<`，`<=`，`>`，`>=`と，論理演算子`AND`，`OR`，`NOT`を扱う．
- `NULL`との比較は`UNKNOWN`になる．`AND`と`OR`は3値論理の真理値表に従う．
- `IS NULL`と`IS NOT NULL`を扱う．
- 算術演算の片方が`NULL`なら結果は`NULL`になる．
- 型の合わない演算(`1 + TRUE`など)はエラーにする．

### 使用例

```rust
let result = ferrodb::execute("VALUES (1 < 2 AND NULL, NULL IS NULL, 1 + NULL)").unwrap();
assert_eq!(result.rows, vec![vec![Value::Null, Value::Boolean(true), Value::Null]]);
```

### モジュール

- `token`：比較演算子と，キーワード`TRUE`，`FALSE`，`UNKNOWN`，`NULL`，`AND`，`OR`，`NOT`，`IS`を加える．
- `value`：`Value::Boolean(bool)`，`Value::Null`と，メソッド`is_null`，関連関数`from_truth`を加える．
- `ast`：`Expr::Boolean`，`Expr::Null`，`Expr::IsNull`，`UnaryOp::Not`を加える．`BinaryOp`を`Arithmetic(ArithmeticOp)`，`Comparison(ComparisonOp)`，`And`，`Or`に分ける．
- `eval`：`EvalError::DatatypeMismatch`を加える．

### リファクタリング

`BinaryOp`の算術演算子を`ArithmeticOp`に移す．演算子の種類ごとに型を分けると，算術演算だけを扱う関数が算術演算子だけを受け取れる．

### 設計ドキュメントの更新

- `code-types.md`：`Value`の新しい列挙子と，演算子の型を加える．
- `code-sequence.md`：演算子の優先順位と，3値論理による評価を加える．

### 学ぶこと

- Rust：`Option<bool>`による3値の表現，入れ子の直和型による演算子の分類，`std::cmp::Ordering`，`NULL`を`Value`の列挙子にするか`Option<Value>`で包むかの設計の比較，タプルに対する`match`と`|`によるパターンの組み合わせ，`impl`ブロック，メソッドと関連関数
- データベース：`NULL`の意味と3値論理

### 既存テストへの影響

構文解析のテストの期待値で，`BinaryOp::Add`などが`BinaryOp::Arithmetic(ArithmeticOp::Add)`などに変わる．

## Iteration 3：文字列

### 要件

- 文字列リテラル`'abc'`を扱う．`'it''s'`のように，引用符を2つ重ねると1つの引用符になる．
- 文字列の連結`||`と，文字列どうしの比較を扱う．
- 文字列の比較は，Unicodeのコードポイント順とする．
- 文字列に日本語などが含まれても，エラーの位置はバイトではなく文字の番号で数える．

### 使用例

```rust
let result = ferrodb::execute("VALUES ('it''s' || ' ok', 'a' < 'b')").unwrap();
assert_eq!(result.rows, vec![vec![Value::Varchar("it's ok".to_string()), Value::Boolean(true)]]);
```

### モジュール

- `token`：`Token::String(String)`，`Token::Concat`を加える．
- `lexer`：文字列リテラルを読む．エラーの位置を文字の数で数える．
- `ast`：`Expr::String(String)`，`BinaryOp::Concat`を加える．
- `value`：`Value::Varchar(String)`を加える．

### 設計ドキュメントの更新

- `code-types.md`：`Value::Varchar`と文字列のトークンを加える．

### 学ぶこと

- Rust：所有権とムーブ，借用(`&`)，`String`と`&str`の違い，UTF-8と`chars`，`Clone`と`clone`，`to_string`と`format!`
- データベース：文字列型と照合順序

### 既存テストへの影響

なし．

## Iteration 4：SQLSTATEとエラー位置

### 要件

- すべてのエラーを1つの型`Error`にまとめる．エラーはSQLSTATE，メッセージ，SQL文中の位置(分かる場合)を持つ．
- 構文エラーは`42601`，数値の範囲外は`22003`，0での割り算は`22012`，型の不一致は`42804`を返す．
- 構文エラーの位置は，問題のあるトークンの先頭の文字位置(1から数える)とする．文が途中で終わっていれば，位置のない`syntax error at end of input`とする．
- 括弧の中や`,`のあとの誤りでは，括弧や`,`ではなく，誤りのあるトークンの位置を返す．

### 使用例

```rust
let err = ferrodb::execute("VALUES (1 +)").unwrap_err();
assert_eq!(err.sqlstate(), SqlState::SyntaxError);
assert_eq!(err.sqlstate().code(), "42601");
assert_eq!(err.position(), Some(12));
assert_eq!(err.to_string(), "syntax error at or near \")\"");
```

### モジュール

- `error`：`struct Error`，`enum SqlState`，`impl Display`，`impl std::error::Error`，各モジュールのエラーからの`From`
- `token`：位置を付けた値`struct Spanned<T>`と，`impl PartialEq<Token> for Spanned<Token>`，`impl Display for Token`
- `lexer`：`tokenize`が`Vec<Spanned<Token>>`を返す．`LexError`に読めなかった文字`found`を加える．
- `parser`：`ParseError`を`UnexpectedToken { token, position }`と`UnexpectedEnd`の列挙型にする．

### リファクタリング

`execute`の中の`map_err`による変換を，`From`の実装による`?`の自動変換に置き換える．

### 設計ドキュメントの更新

- `c4-component.md`：`error`モジュールと，`crate`から`error`，`error`から`lexer`，`parser`，`eval`への依存を加える．
- `code-types.md`：`Error`，`SqlState`，`Spanned`，`ParseError`の列挙子を加える．
- `code-sequence.md`：各段階のエラーが`From`で`Error`に変わる流れを加える．

### 学ぶこと

- Rust：代数的データ型によるエラー型の設計(列挙子ごとに持つ情報を変えるか，共通の`struct`にまとめるか)，`From`の実装と`?`による変換，`Display`と`std::error::Error`の実装，非公開のフィールドとアクセサー，ジェネリックな構造体，`&'static str`
- winnow：`LocatingSlice`と`with_span`，`ModalResult`と`cut_err`
- データベース：SQLSTATEのクラスとサブクラス

### 既存テストへの影響

- 字句解析のテストは，トークンの位置を除いて比べる．`LexError`の期待値に`found`が加わる．
- 構文解析のエラーの期待値が，`ParseError`の列挙子と位置に変わる．
- 結合テストでは，トークンの期待値に位置が加わり，エラーの期待値が`Error`のSQLSTATE，位置，メッセージの比較に変わる．

## Iteration 5：表の作成と挿入

### 要件

- `CREATE TABLE t (a INTEGER, b VARCHAR(10), c BOOLEAN, d BIGINT)`で表を作る．`INT`は`INTEGER`と同じ型である．
- 引用符で囲まない識別子は大文字に正規化する．`"x"`は大文字と小文字を区別する．
- `INSERT INTO t VALUES (...), (...)`と，列を指定する`INSERT INTO t (a, b) VALUES (...)`で行を挿入する．指定しない列は`NULL`になる．
- `VALUES`は複数の行を持てる．行によって値の数が違えば構文エラーにする．
- `SELECT * FROM t`で全行を返す．
- 挿入する値の型が列の型と合わなければ`42804`，`VARCHAR(n)`より長い文字列は`22001`とする．長さは文字の数で数える．
- `BIGINT`の列と演算を扱う．`INTEGER`に収まらない整数リテラルは`BIGINT`になる．`INTEGER`と`BIGINT`の演算は`BIGINT`になる．
- 存在しない表は`42P01`，既にある表の作成は`42P07`，存在しない列は`42703`，同じ名前の列の定義は`42701`とする．
- `INSERT`の値の数と列の数が違えば構文エラーにする．
- 1つの`INSERT`の途中でエラーになったら，その文では1行も挿入しない．

### 使用例

```rust
let mut db = Database::new();
db.execute("CREATE TABLE users (id INTEGER, name VARCHAR(20))")?;
db.execute("INSERT INTO users VALUES (1, 'alice'), (2, 'bob')")?;
assert_eq!(
    db.execute("SELECT * FROM users")?,
    StatementResult::Rows(QueryResult {
        columns: vec!["ID".to_string(), "NAME".to_string()],
        rows: vec![
            vec![Value::Integer(1), Value::Varchar("alice".to_string())],
            vec![Value::Integer(2), Value::Varchar("bob".to_string())],
        ],
    })
);
```

### モジュール

- `token`，`lexer`：識別子`Token::Identifier(String)`と，文と型のキーワードを加える．
- `ast`：`enum Statement`，`CreateTable`，`ColumnDef`，`Insert`，`Select`を加える．`Values`を複数の行`rows: Vec<Vec<Expr>>`にする．
- `parser`：`parse`が`Statement`を返す．`ParseError::ValuesLengthMismatch`を加える．
- `value`：`Value::BigInt(i64)`と，列の型`enum DataType`を加える．
- `catalog`：`struct Catalog`，`struct TableSchema`，`struct Column`と，スキーマとの照合のエラー`enum SchemaError`
- `database`：`struct Database`，`fn execute(&mut self, sql: &str) -> Result<StatementResult, Error>`，`enum StatementResult`，`struct QueryResult`

### リファクタリング

ルートの`execute`関数を`Database::execute`に移す．

### 設計ドキュメントの更新

- `c4-component.md`：`catalog`と`database`を加え，`database`を入口にする．
- `code-types.md`：`Database`，`Catalog`，`TableSchema`，`Column`，`DataType`，`Statement`，`StatementResult`，`SchemaError`を加える．
- `code-sequence.md`：`INSERT`がカタログで型を検査して行を追加する流れを加える．

### 学ぶこと

- Rust：`HashMap`，`#[derive(Default)]`，`Option<Vec<_>>`で「省略できる」を表す型，`match`のガード，`for`での`enumerate`と`zip`，スライス`&v[..i]`，`vec![値; n]`，`matches!`，構造体が値を所有すること
- データベース：カタログとスキーマ，識別子の正規化，代入の型変換，文単位の原子性

### 既存テストへの影響

- 字句解析のテストで，キーワードでない単語が字句解析のエラーではなく識別子になる．
- 構文解析のテストで，`parse`の結果が`Statement`に，`Values`が行の並びに変わる．
- 評価のテストで，`2147483648`が範囲外のエラーではなく`BIGINT`の値になる．
- `execute`を呼ぶ結合テストが`Database::new().execute(...)`に変わる．結果の型が`StatementResult`に変わる．

## Iteration 6：REPL

### 要件

- `ferrodb`コマンドを起動すると，対話的にSQLを受け付ける．
- `;`で終わるまでを1文とする．複数行にまたがってよい．1行に複数の文を書いてよい．引用符の中の`;`は文の終わりではない．
- 問い合わせの結果は，`psql`の整列形式に似た表で表示する．見出しは中央，数値は右，それ以外は左に寄せる．`NULL`は空，真偽値は`t`と`f`で表示し，最後に行の数を書く．
- `CREATE TABLE`などの結果は，`CREATE TABLE`，`INSERT 0 2`のようなコマンドタグで表示する．
- エラーは`ERROR:  メッセージ`の形で表示し，続けて次の文を受け付ける．
- 端末から入力するときは，プロンプト`ferrodb>`を表示する．文の途中では，`ferrodb>`と同じ幅になるように右に寄せた`->`を表示する．
- 標準入力が終わったら終了する．`;`で終わっていない文が残っていれば，それも実行する．

### 使用例

```console
$ cargo run
ferrodb> CREATE TABLE users (id INTEGER, name VARCHAR(20));
CREATE TABLE
ferrodb> INSERT INTO users VALUES (1, 'alice'),
      ->   (2, 'bob');
INSERT 0 2
ferrodb> SELECT * FROM users;
 ID | NAME
----+-------
  1 | alice
  2 | bob
(2 rows)
```

### モジュール

- `repl`：次の2つの関数を持つ．
  - `pub fn run(input: impl BufRead, mut output: impl Write, interactive: bool) -> io::Result<()>`
  - `pub fn split_statements(buffer: &str) -> (Vec<String>, String)`
- `src/main.rs`：標準入出力と，標準入力が端末かどうかを`repl::run`に渡す．
- `format`：`StatementResult`と`QueryResult`の`Display`の実装による，表とコマンドタグの整形

### リファクタリング

モジュールを`sql`(`token`，`lexer`，`ast`，`parser`)と`exec`(`eval`)の階層に整理する．公開するものは`lib.rs`の`pub use`で選ぶ．

### 設計ドキュメントの更新

- `c4-context.md`：REPLを使う利用者を加える．
- `c4-container.md`：REPLのバイナリを加える．
- `c4-component.md`：モジュールの階層と`format`，`main`を反映する．
- `code-sequence.md`：REPLが1文を読み，実行して表示する流れを加える．

### 学ぶこと

- Rust：ライブラリクレートとバイナリクレート，`std::io`の`BufRead`，`Write`，`io::Result`，引数の`impl Trait`，書式指定(幅と寄せ)，`IsTerminal`，モジュール階層，`&[u8]`と`Vec<u8>`を使う入出力のテスト
- データベース：結果集合とコマンドタグ

### 既存テストへの影響

なし(`use`のパスは`pub use`で保つ)．

### 受講者が行うツール操作

- `src/main.rs`を作り，`cargo run`でREPLを起動する．
- `echo 'VALUES (1);' | cargo run -q`のように，標準入力からSQLを渡す．

## Iteration 7：WHERE，列の選択，別名

### 要件

- `SELECT a, b + 1 AS c FROM t WHERE a > 1`のように，列と式を選び，別名を付ける．`AS`は省略できる．`*`と式を混ぜてもよい．
- 結果の列名は，別名があれば別名，列そのものなら列名，それ以外の式は`?column?`とする．
- `WHERE`の条件が`TRUE`の行だけを返す．`FALSE`と`UNKNOWN`の行は返さない．
- 存在しない列は`42703`とする．`VALUES`の式は列を参照できない．
- `WHERE`の条件が真偽値でなければ`42804`とする．条件の型は，各行を評価するときに調べる．

### 使用例

```console
ferrodb> SELECT name, id * 10 AS score FROM users WHERE id >= 2;
 NAME | SCORE
------+-------
 bob  |    20
(1 row)
```

### モジュール

- `sql::ast`：`Expr::Column`，`enum SelectItem { Wildcard, Expr { expr, alias } }`を加え，`Select`を`items`，`from`，`filter`にする．
- `plan::binder`：名前を解決した式`enum BoundExpr`と，`fn bind(expr: &Expr, columns: &[Column]) -> Result<BoundExpr, BindError>`
- `exec::eval`：`fn eval(expr: &BoundExpr, row: &[Value])`と，条件を評価する`fn eval_condition`

### リファクタリング

- 評価の対象を`Expr`から`BoundExpr`に変える．リテラルの値への変換を，評価から名前解決に移す．
- `Database`の中の`for`による`Vec`の組み立てを，イテレーターの`map`と`collect`に置き換える．

### 設計ドキュメントの更新

- `c4-component.md`：`plan::binder`と依存を加える．
- `code-types.md`：`BoundExpr`，`SelectItem`，`BindError`を加える．
- `code-sequence.md`：構文解析と実行の間にバインドを加える．

### 学ぶこと

- Rust：イテレーター(`iter`，`into_iter`，`map`，`position`，`extend`)，クロージャ，`collect`で`Result<Vec<_>, _>`を作る方法，`ok_or_else`，範囲`0..n`のイテレーター
- データベース：名前解決(バインド)，`WHERE`と3値論理，選択リストと別名

### 既存テストへの影響

- `SELECT * FROM t`の構文解析のテストの期待値が，`Select`の新しいフィールドに変わる．
- 評価のテストの補助関数で，評価の前に`bind`を呼ぶ．

## Iteration 8：更新，削除，制約

### 要件

- `UPDATE t SET a = a + 1 WHERE ...`と`DELETE FROM t WHERE ...`を扱う．結果は`UPDATE n`，`DELETE n`とする．
- `DROP TABLE t`で表を消す．
- 列制約`NOT NULL`，`PRIMARY KEY`，`UNIQUE`を扱う．違反は`23502`，`23505`とする．
- `PRIMARY KEY`は`NOT NULL`と`UNIQUE`を合わせたものとする．表に2つ書いたら`42P16`とする．
- `UNIQUE`の列には`NULL`をいくつでも入れられる．
- 1つの文の途中で制約に違反したら，その文による変更をすべて取り消す．制約は文の終わりに検査する．
- `SET`の式は書き換える前の行で評価する．同じ列に2度代入したら`42601`とする．
- ない表の`DROP TABLE`は`42P01`とする．

### 使用例

```console
ferrodb> CREATE TABLE users (id INTEGER PRIMARY KEY, name VARCHAR(20) NOT NULL);
CREATE TABLE
ferrodb> INSERT INTO users VALUES (1, 'alice'), (1, 'bob');
ERROR:  duplicate key value violates unique constraint "USERS_PKEY"
ferrodb> UPDATE users SET name = 'carol' WHERE id = 1;
UPDATE 0
```

### モジュール

- `catalog`：`Column`に制約を加える．
- `exec::dml`：`INSERT`，`UPDATE`，`DELETE`の実行と制約の検査

### リファクタリング

- `WHERE`の条件を満たすかを返す`matches_filter`を`database`から`exec::eval`に移し，`SELECT`，`UPDATE`，`DELETE`で使う．

### 設計ドキュメントの更新

- `c4-component.md`：`exec::dml`を加える．
- `code-types.md`：制約と`Statement`の新しい列挙子を加える．
- `code-sequence.md`：`UPDATE`が新しい行を計算し，制約を検査してから反映する流れを加える．

### 学ぶこと

- Rust：`iter_mut`，`retain`，借用の衝突とその避け方(計算と変更を分ける)，`HashSet`
- データベース：整合性制約，文単位の原子性

### 既存テストへの影響

- `CREATE TABLE`の構文解析のテストの期待値で，`ColumnDef`に`constraints`が加わる．
- テストの補助関数で作る`Column`に`nullable`が，`TableSchema`に`unique_constraints`が加わる．

## Iteration 9：並べ替えと件数の制限

### 要件

- `ORDER BY a DESC, b`で並べ替える．`ASC`/`DESC`と`NULLS FIRST`/`NULLS LAST`に対応する．
- 既定では`NULL`を最大の値として扱う(`ASC`で最後，`DESC`で最初)．
- `OFFSET n ROWS`と`FETCH FIRST n ROWS ONLY`に対応する．
- `ORDER BY`の名前だけのキーは，結果の列名(別名を含む)を表の列名より先に探す．選択していない列でも並べ替えられる．
- `OFFSET n ROWS`と`FETCH FIRST n ROWS ONLY`の`n`は，0以上の整数のリテラルとする．
- `SELECT DISTINCT`で重複する行を除く．`NULL`どうしは同じ値とみなす．
- `SELECT DISTINCT`で，選択項目にない式で並べ替えたら`42P10`とする．

### 使用例

```console
ferrodb> SELECT name FROM users ORDER BY name DESC OFFSET 1 ROWS FETCH FIRST 2 ROWS ONLY;
```

### モジュール

- `value`：`Value`どうしの順序(`NULL`の扱いを含む)
- `exec::sort`

### 設計ドキュメントの更新

- `code-types.md`：`OrderBy`，`Limit`を加える．
- `code-sequence.md`：`SELECT`の処理順(`WHERE` → 射影 → `DISTINCT` → `ORDER BY` → `OFFSET`/`FETCH`)を示す．

### 学ぶこと

- Rust：`Ord`と`PartialOrd`の実装，`Ordering`，`sort_by`と`then_with`，`contains`による重複の検査
- データベース：`NULL`の並び順，`ORDER BY`で出力列の別名を使う規則

### 既存テストへの影響

- 構文解析のテストの`Select`の期待値に，`distinct`，`order_by`，`limit`が加わる．

## Iteration 10：実行計画とEXPLAIN

### 要件

- `SELECT`を，実行計画(演算子の木)に変換してから実行する．結果は変えない．
- `EXPLAIN SELECT ...`で実行計画を表示する．演算子を1行に1つ，子を親より2文字深く字下げして書く．`EXPLAIN`は演算子を動かさない．
- 結果の列にない式で並べ替えるときは，その式を`Project`の隠れた列として計算し，最上段の`Project`で取り除く．

### 使用例

```console
ferrodb> EXPLAIN SELECT name FROM users WHERE id > 1 ORDER BY name;
     QUERY PLAN
---------------------
 Sort [NAME]
   Project [NAME]
     Filter (ID > 1)
       SeqScan USERS
(4 rows)
```

### モジュール

- `plan::binder`：`fn bind_select(select: &Select, schema: &TableSchema) -> Result<BoundSelect, BindError>`
- `plan::planner`：`fn plan(select: &BoundSelect) -> PlanNode`
- `plan::explain`：`fn explain(plan: &PlanNode) -> Vec<String>`
- `exec`：`trait Executor { fn next(&mut self) -> Result<Option<Row>, Error>; }`と，演算子ごとのモジュールの`SeqScan`，`Filter`，`Project`，`Sort`，`Distinct`，`Limit`
- `exec::build`：実行計画から`Box<dyn Executor>`の木を作る．

### リファクタリング

Iteration 7〜9で1つの関数に書いた`SELECT`の処理を，演算子ごとの`Executor`に分ける．

### 設計ドキュメントの更新

- `c4-component.md`：`plan::planner`，`plan::explain`と`exec`の演算子を加える．
- `code-types.md`：`PlanNode`，`Executor`とその実装を加える．
- `code-sequence.md`：各演算子が`next`で1行ずつ親の演算子へ渡す流れにする．

### 学ぶこと

- Rust：トレイトの定義と実装，トレイトオブジェクトと`Box<dyn Executor>`，静的ディスパッチと動的ディスパッチ
- データベース：論理計画と物理計画，Volcano(イテレータ)モデル

### 既存テストへの影響

なし．

## Iteration 11：結合

### 要件

- `FROM a, b`，`CROSS JOIN`，`INNER JOIN ... ON`，`LEFT [OUTER] JOIN ... ON`を扱う．
- 表の別名(`FROM emp e`)と，修飾した列名(`e.name`)を扱う．`EXPLAIN`では列名を修飾して表示する．
- どちらの表の列か決まらない列名は`42702`とする．`FROM`にない表で修飾した列名は`42P01`，同じ名前(別名)の表が2つあれば`42712`とする．
- `LEFT JOIN`で相手のない行は，右側の列を`NULL`にする．

### 使用例

```console
ferrodb> SELECT e.name, d.title FROM emp e LEFT JOIN dept d ON e.dept = d.code;
  NAME  |    TITLE
--------+-------------
 Sato   | Development
 Suzuki | Development
 Tanaka | Operations
 Ito    |
(4 rows)
```

### モジュール

- `sql::ast`：`enum TableRef`，`struct Join`，`enum JoinKind`，`Expr::QualifiedColumn`
- `plan::binder`：複数の表にまたがる名前解決(`Scope`)．`bind_select`はカタログから`FROM`の表を引く．
- `exec::join`：`NestedLoopJoin`

### 設計ドキュメントの更新

- `code-types.md`：`TableRef`，`Scope`，`NestedLoopJoin`を加える．
- `code-sequence.md`：入れ子ループ結合が内側を繰り返し読む流れを加える．

### 学ぶこと

- Rust：トレイトオブジェクトを組み合わせる構造体，`Option::take`と`replace`
- データベース：直積と結合，外部結合，入れ子ループ結合

### 既存テストへの影響

- `EXPLAIN`の期待値のうち，列名の表示が修飾名(`USERS.NAME`)に変わる．
- 構文解析のテストの`Select::from`の期待値が，表の名前から`TableRef`に変わる．
- 名前解決のテストで，`bind`に渡す列の並びが`Scope`に，`bind_select`に渡す表の定義がカタログに変わる．

## Iteration 12：集約

### 要件

- 集約関数`COUNT(*)`，`COUNT(x)`，`SUM`，`AVG`，`MIN`，`MAX`と，`DISTINCT`付きの集約を扱う．
- `GROUP BY`と`HAVING`を扱う．`GROUP BY`がなく集約関数だけがある場合は，全体で1つのグループとする．
- 集約関数は`NULL`を無視する．空の集合の`COUNT`は`0`，それ以外は`NULL`とする．
- `AVG`は整数の平均を0の方向に切り捨てた`BIGINT`とする．
- `COUNT`は`BIGINT`，`SUM`は整数の合計を`BIGINT`，`MIN`と`MAX`は引数と同じ型で返す．`SUM`と`AVG`の引数が整数でなければ`42883`とする．
- `GROUP BY`にない列を集約せずに選んだら`42803`とする．`WHERE`などの集約関数と，入れ子の集約関数も`42803`とする．
- グループは，最初に現れた順で返す．`GROUP BY`のキーの`NULL`どうしは同じグループにする．

### 使用例

```console
ferrodb> SELECT dept, COUNT(*) AS n, SUM(salary) AS total FROM emp GROUP BY dept HAVING COUNT(*) > 1;
 DEPT | N | TOTAL
------+---+-------
 dev  | 2 |   950
(1 row)
```

### モジュール

- `exec::aggregate`：`HashAggregate`と，集約関数ごとの`trait Accumulator`

### 設計ドキュメントの更新

- `code-types.md`：`Accumulator`とその実装，`HashAggregate`を加える．
- `code-sequence.md`：集約の流れを加える．

### 学ぶこと

- Rust：`Hash`と`Eq`の実装(`Value`をキーにする)，`HashMap`の`entry` API，`Box<dyn Accumulator>`
- データベース：グループ化の規則，集約と`NULL`

### 既存テストへの影響

- 構文解析のテストの`Select`の期待値に，`group_by`と`having`が加わる．
- `Filter::new`が，条件を書いた句の名前(`WHERE`か`HAVING`)を受け取る．

## Iteration 13：ページとタプルのバイト表現

### 要件

- 行(タプル)をバイト列に符号化し，元に戻せる．`NULL`はビットマップで表す．
- 8192バイトのスロット付きページに，タプルを追加，取得，削除，更新できる．
- ページに入らないタプルは，エラーを返す．SQLでは`54000`とする．
- この時点では，表のデータはメモリ上のページの列に置く．SQLとしての振る舞いは変えない．

### 使用例

```rust
let mut page = Page::new();
let slot = page.insert(&encode_tuple(&row, &schema))?;
assert_eq!(decode_tuple(page.get(slot).unwrap(), &schema)?, row);
```

### モジュール

- `storage::tuple`：`fn encode_tuple(row: &[Value], schema: &TableSchema) -> Vec<u8>`，`fn decode_tuple(...)`
- `storage::page`：`struct Page`，`SlotId`
- `storage::heap`：表のタプルを置くページの列`struct HeapFile`と，行の位置`RowId`．Iteration 14でファイルに保存する．

### リファクタリング

表の行を`Vec<Row>`からページの列(`HeapFile`)に置き換える．`UPDATE`と`DELETE`は，行の位置(`RowId`)のタプルを書き換える．

### 設計ドキュメントの更新

- `layout.md`：新たに作り，ページとタプルのバイト配置をpacket図で描く．
- `c4-component.md`：`storage::page`，`storage::tuple`，`storage::heap`を加える．
- `code-types.md`：`Page`，`SlotId`，`RowId`，`HeapFile`を加える．
- `code-sequence.md`：`UPDATE`がページのタプルを書き換える流れにする．

### 学ぶこと

- Rust：固定長配列とスライス，`to_le_bytes`/`from_le_bytes`，`TryFrom`，`const`，`split_at`
- データベース：スロット付きページ，タプルの符号化

### 既存テストへの影響

- `exec::dml`の単体テストで，表の行を`Vec<Row>`でなく`HeapFile`で渡す．

## Iteration 14：ヒープファイルとデータディレクトリ

### 要件

- サブコマンド`repl`のオプション`--data-dir DIR`で，表をデータディレクトリのファイルに保存する．起動し直しても表と行が残る．
- カタログと，表ごとのヒープファイルを保存する．ヒープファイルの名前は，表の名前のUTF-8のバイトの16進数とする．
- `--data-dir`を省略したら，これまでどおりメモリ上で動かす．

### 使用例

```console
$ cargo run -q -- repl --data-dir ./data
ferrodb> CREATE TABLE t (a INTEGER);
CREATE TABLE
ferrodb> INSERT INTO t VALUES (1);
INSERT 0 1
$ cargo run -q -- repl --data-dir ./data
ferrodb> SELECT * FROM t;
 A
---
 1
(1 row)
```

### モジュール

- `storage::disk`：`trait DiskManager`と，ファイル用とメモリ用の実装
- `storage::heap`：`HeapFile`のページを`DiskManager`で読み書きする．
- `catalog`：カタログの保存と読み込み
- `database`：`Database::open(dir: &Path)`を加える．
- `repl`：データベースを受け取る`run_with`
- `src/main.rs`：`clap`によるサブコマンド`repl`

### 設計ドキュメントの更新

- `c4-container.md`：データディレクトリ(カタログ，ヒープファイル)を加える．
- `c4-component.md`：`storage::disk`と，`storage::heap`から`storage::disk`への依存を加える．
- `layout.md`：ヒープファイルとカタログファイルの配置を加える．
- `code-sequence.md`：起動時にカタログを読み込む流れを加える．

### 学ぶこと

- Rust：`std::fs`，`File`と`Seek`，`read_exact`，`Path`と`PathBuf`，`clap`のderive，テスト用の一時ディレクトリ(`tempfile`)
- データベース：ヒープファイル，データディレクトリ

### 既存テストへの影響

- `HeapFile::new`が`Box<dyn DiskManager>`を受け取り，`HeapFile`の操作が`Result`を返す．`storage::heap`と`exec::dml`の単体テストが変わる．

### 受講者が行うツール操作

- `cargo add clap --features derive`で，機能を指定して依存を追加する．
- `cargo add --dev tempfile`で，テストだけで使う依存を追加する．
- `cargo run -- repl --help`のように，`--`のあとにプログラムへの引数を渡す．

## Iteration 15：バッファプール

### 要件

- ページをバッファプール経由で読み書きする．枠の数はプールを作るときに決める．ヒープファイルは，ファイルごとに16枠のプールを持つ．
- 使われているページ(ピン留めされたページ)は追い出さない．
- 枠が足りなければ，クロック方式で追い出すページを選ぶ．変更されたページは書き戻してから追い出す．
- すべての枠がピン留めされていたら，エラーを返す．
- プールを捨てるとき(データベースを閉じるとき)に，変更されたページを書き戻す．

### 使用例

```rust
let pool = BufferPool::new(disk, 3);
let guard = pool.fetch_page(page_id)?;   // ピン留め
let slot = guard.write().insert(&bytes)?; // 変更を記録
drop(guard);                               // ピンを外す
```

### モジュール

- `storage::buffer`：`struct BufferPool<D: DiskManager>`，`struct PageGuard<'a>`，`struct ClockReplacer`，`enum BufferError`
- `storage::disk`：`Box<dyn DiskManager>`に`DiskManager`を実装する．
- `storage::heap`：`HeapFile`が`BufferPool`を通してページを読み書きする．

### 設計ドキュメントの更新

- `c4-component.md`：`storage::buffer`を加え，`heap`がページを`buffer`で読み書きするようにする．
- `code-types.md`：`BufferPool`，`PageGuard`，`ClockReplacer`を加える．
- `code-sequence.md`：ページの取得，ピン留め，追い出しの流れを加える．

### 学ぶこと

- Rust：ライフタイム注釈，`Drop`によるRAII，`Cell`と`RefCell`，ジェネリクスとトレイト境界
- データベース：バッファ管理，ピン留め，クロック置換

### 既存テストへの影響

- `storage::heap`の単体テストは，ページの数を`BufferPool`から読む．
- `HeapError::Io`は`HeapError::Buffer`になる．

## Iteration 16：B+木

### 要件

- キーから`RowId`を引くB+木を，バッファプールのページ上に作る．
- 挿入，完全一致の検索，範囲の走査，削除を行える．
- 葉と内部ノードは，いっぱいになったら分割する．削除では併合しない．
- キーは`INTEGER`，`BIGINT`，`BOOLEAN`，`VARCHAR`とし，バイト列の比較で順序が保たれるように符号化する．
- 同じキーを複数の行に使える．項目はキーと行の位置の組で並べる．
- 符号化したキーが2000バイトを超えたらエラーにする．
- ページ0に根のページ番号を書き，木の状態をすべてページに置く．

### 使用例

```rust
let mut tree = BTree::<i32>::create(&pool)?;
tree.insert(42, row_id)?;
assert_eq!(tree.get(&42)?, Some(row_id));
let entries: Vec<(i32, RowId)> = tree.range(10..50)?.collect::<Result<_, _>>()?;
```

### モジュール

- `index::key`：`trait IndexKey`(順序を保つ符号化と復号)
- `index::btree`：`struct BTree<'a, K: IndexKey>`，`struct RangeIter<'a, K>`，`enum BTreeError`
- `storage::heap`：`RowId`に順序を導出する．
- `lib.rs`：`index`と`storage`をクレートの外に公開する．

### 設計ドキュメントの更新

- `c4-component.md`：`index`を加える．
- `code-types.md`：`IndexKey`，`BTree`，`RangeIter`を加える．
- `layout.md`：葉ノードと内部ノードのバイト配置を加える．
- `code-sequence.md`：挿入による葉の分割の流れを加える．

### 学ぶこと

- Rust：トレイト境界と関連関数，ジェネリックな構造体，`Iterator`の実装，`RangeBounds`，`PhantomData`
- データベース：B+木，ノードの分割，順序を保つキーの符号化

### 既存テストへの影響

なし．

## Iteration 17：インデックスの利用

### 要件

- `CREATE INDEX name ON t (col)`と`DROP INDEX name`で，1列のインデックスを作る．既存の行もインデックスに入れる．
- `PRIMARY KEY`と`UNIQUE`の列には，自動でインデックスを作る．一意性の検査はインデックスで行う．
- `WHERE col = 定数`と，`<`，`<=`，`>`，`>=`を`AND`でつないだ条件にインデックスが使えるなら，インデックススキャンを選ぶ．
- `INSERT`，`UPDATE`，`DELETE`でインデックスも更新する．
- 一意性制約のインデックスは，制約と同じ名前(`EMP_PKEY`など)で作る．表とインデックスは同じ名前を使えない(`42P07`)．
- ないインデックスの`DROP INDEX`は`42704`，一意性制約のインデックスの`DROP INDEX`は`2BP01`とする．
- `NULL`はインデックスに入れない．インデックスを使うのは，1つの表を読む`SELECT`とする．
- インデックスは，名前の16進数に`.index`を付けたファイルに置く．

### 使用例

```console
ferrodb> CREATE INDEX emp_salary ON emp (salary);
CREATE INDEX
ferrodb> EXPLAIN SELECT name FROM emp WHERE salary >= 450;
```

### モジュール

- `index`：`enum AnyIndex`(キーの型ごとの`BTree`をまとめる)
- `index`：`struct ColumnIndex`(列の番号と`AnyIndex`の組)
- `exec::index_scan`：`IndexScan`
- `plan::planner`：インデックスを使える条件の検出
- `sql::parser`：`CREATE INDEX`と`DROP INDEX`
- `catalog`：`struct IndexDef`と，インデックスの定義の保存
- `exec::dml`：行を変えるときにインデックスを更新する．
- `database`：インデックスごとのバッファプール

### リファクタリング

Iteration 8で全行を調べていた一意性の検査を，インデックスによる検査に置き換える．

### 設計ドキュメントの更新

- `c4-component.md`：`plan`と`exec`から`index`への依存を加える．
- `code-types.md`：`AnyIndex`，`IndexScan`，カタログのインデックス情報を加える．
- `code-sequence.md`：プランナーがスキャン方法を選ぶ流れを加える．
- `c4-container.md`：インデックスのファイルを加える．
- `layout.md`：カタログのファイルにインデックスの定義を加える．

### 学ぶこと

- Rust：`enum`による静的ディスパッチとトレイトオブジェクトとの比較，`std::ops::Bound`
- データベース：インデックススキャンと全件走査の選択，一意性制約の実装

### 既存テストへの影響

- `exec::dml`の関数がインデックスを受け取る．全行を調べていた`check_constraints`の単体テストは，`check_not_null`と，インデックスで調べる一意性の検査のテストに変わる．
- `plan::planner`の単体テストで，インデックスのある列の条件は`IndexScan`になる．
- `PRIMARY KEY`の列を`WHERE`で比べる問い合わせの`EXPLAIN`は，`IndexScan`になる．

## Iteration 18：トランザクションとMVCC

### 要件

- `START TRANSACTION`，`COMMIT`，`ROLLBACK`を扱う．トランザクションの外の文は，それぞれを1つのトランザクションとして自動でコミットする．
- タプルに作成したトランザクション(`xmin`)と削除したトランザクション(`xmax`)を記録する．`UPDATE`は古い版を削除済みにして新しい版を挿入する．
- 行の可視性は，スナップショットとトランザクションの状態(進行中，コミット済み，中止)で決める．
- `ROLLBACK`したトランザクションの変更は見えなくなる．
- トランザクションの中でエラーが起きたら，`ROLLBACK`までの文を`25P02`で拒否する．
- トランザクションの中での`START TRANSACTION`は`25001`とする．
- REPLのプロンプトは，トランザクションの中では`ferrodb*>`，失敗したトランザクションの中では`ferrodb!>`とする．

### 使用例

```console
ferrodb> START TRANSACTION;
START TRANSACTION
ferrodb*> DELETE FROM emp;
DELETE 4
ferrodb*> ROLLBACK;
ROLLBACK
ferrodb> SELECT COUNT(*) FROM emp;
 COUNT
-------
     4
(1 row)
```

### モジュール

- `txn`：`struct TxnId`，`struct TransactionManager`，`struct Snapshot`，`fn is_visible(header: &TupleHeader, snapshot: &Snapshot, ...) -> bool`
- `database`：セッションの状態(トランザクションの外，中，失敗)

### 設計ドキュメントの更新

- `c4-component.md`：`txn`を加える．
- `code-types.md`：`TxnId`，`Snapshot`，`TupleHeader`，`TransactionManager`を加える．
- `layout.md`：タプルヘッダーに`xmin`と`xmax`を加える．
- `code-sequence.md`：`UPDATE`が新しい版を作る流れと，可視性の判定を加える．

### 学ぶこと

- Rust：ニュータイプパターン，`Copy`と`Clone`，値を消費するメソッド(`fn commit(self)`)によるAPI設計
- データベース：MVCC，スナップショット，可視性の規則

### 既存テストへの影響

なし．

## Iteration 19：WALとクラッシュリカバリ

### 要件

- ページの変更，コミット，中止は，先にWALファイルへ記録する．
- コミットはWALをディスクに書き込んで(fsync)から完了とする．
- ページを書き戻す前に，そのページの変更を記録したWALを書き込む．
- 起動時にWALを読み，最後のチェックポイントからREDOする．コミットの記録がないトランザクションは中止扱いにする．
- WALのレコードにチェックサムを付け，壊れた末尾のレコードは読み捨てる．
- `CHECKPOINT`文で，変更されたページを書き戻してチェックポイントを記録する．

### 使用例

```console
$ cargo run -q -- repl --data-dir ./data
ferrodb> INSERT INTO t VALUES (2);
INSERT 0 1
(プロセスを強制終了する)
$ cargo run -q -- repl --data-dir ./data
ferrodb> SELECT * FROM t;
 A
---
 1
 2
(2 rows)
```

### モジュール

- `wal`：`enum WalRecord`，`struct Lsn`，`struct WalWriter<W: Write>`，`fn recover(...)`
- `storage::page`：ページヘッダーに`page_lsn`を加える．
- `storage::buffer`：書き戻しの前にWALを書き込む．

### 設計ドキュメントの更新

- `c4-container.md`：WALファイルを加える．
- `c4-component.md`：`wal`を加える．
- `layout.md`：WALレコードの配置と，ページヘッダーの`page_lsn`を加える．
- `code-sequence.md`：コミットとリカバリの流れを加える．

### 学ぶこと

- Rust：`Write`トレイトとジェネリクス，`BufWriter`，`File::sync_data`，テストのバイナリを子プロセスとして起動して強制終了させるテスト
- データベース：WALの規則，REDO，チェックポイント，MVCCでUNDOが要らない理由

### 既存テストへの影響

なし．

### 受講者が行うツール操作

- `crc32fast`を依存に追加する．

## Iteration 20：PostgreSQL互換プロトコル

### 要件

- サブコマンド`serve`の`--data-dir DIR --port N`で，TCPの接続を1つずつ受け付ける．
- PostgreSQLのフロントエンド/バックエンドプロトコル3.0の，起動，認証(常に成功)，Simple Query，終了を実装する．SSLの要求には`N`を返す．
- 問い合わせの結果を`RowDescription`，`DataRow`，`CommandComplete`で返す．値はテキスト形式で返す．
- エラーは`ErrorResponse`でSQLSTATEと位置を返す．
- `ReadyForQuery`でトランザクションの状態(`I`，`T`，`E`)を返す．
- 1つの`Query`に`;`で区切った複数の文があれば，順に実行する．

### 使用例

```console
$ cargo run --release -q -- serve --data-dir ./data --port 5433
ferrodb listening on 127.0.0.1:5433
$ psql -h 127.0.0.1 -p 5433 -U alice ferro
ferro=> SELECT * FROM t;
```

### モジュール

- `server::message`：`enum FrontendMessage`，`enum BackendMessage`と，その読み書き
- `server::connection`：`fn handle<S: Read + Write>(stream: S, db: &mut Database) -> Result<(), ProtocolError>`
- `src/main.rs`：サブコマンド`serve`

### 設計ドキュメントの更新

- `c4-context.md`：`psql`などのPostgreSQLクライアントを加える．
- `c4-container.md`：サーバーのプロセスとTCPの接続を加える．
- `c4-component.md`：`server`を加える．
- `layout.md`：メッセージの配置を加える．
- `code-sequence.md`：起動からSimple Queryまでのメッセージのやりとりを加える．

### 学ぶこと

- Rust：`std::net::TcpListener`，`Read`と`Write`をトレイト境界にしたジェネリクス，`Cursor`によるテスト，`to_be_bytes`
- データベース：フロントエンド/バックエンドプロトコル，型OID，テキスト形式

### 既存テストへの影響

なし．

### 受講者が行うツール操作

- `postgres`をテストだけで使う依存に追加する．
- `cargo run --release`で，最適化したビルドを実行する．

## Iteration 21：複数の同時接続

### 要件

- 接続ごとにスレッドを作り，複数のクライアントが同時に問い合わせられる．
- 各接続はそれぞれのセッション(トランザクションの状態)を持つ．
- 他のセッションの，コミットしていない変更は見えない．
- カタログ，バッファプール，トランザクション管理，WALを，複数のスレッドから安全に使えるようにする．

### 使用例

```console
-- セッションA
ferro=> START TRANSACTION;
ferro=*> INSERT INTO t VALUES (3);
-- セッションB
ferro=> SELECT COUNT(*) FROM t;   -- 3はまだ見えない
```

### モジュール

- `database`：`Database`を`Arc`で共有し，`Session`を接続ごとに作る．
- `storage::buffer`：ページごとの読み書きラッチ

### リファクタリング

`&mut Database`で渡していた状態を，共有する部分(`Database`)とセッションごとの部分(`Session`)に分ける．

### 設計ドキュメントの更新

- `c4-container.md`：接続ごとのスレッドを加える．
- `code-types.md`：`Session`と，共有する構造体のロックを加える．
- `code-sequence.md`：2つのセッションが並行に動く流れを加える．

### 学ぶこと

- Rust：`std::thread`，`Arc`，`Mutex`と`RwLock`，`Send`と`Sync`，`move`クロージャ，`thread::scope`によるテスト
- データベース：ラッチとロックの違い，読み取りがブロックされない理由

### 既存テストへの影響

`Database::execute`を呼ぶテストが`Session::execute`に変わる．

## Iteration 22：分離レベルと書き込みの競合

### 要件

- `START TRANSACTION ISOLATION LEVEL READ COMMITTED | REPEATABLE READ`で分離レベルを選ぶ．既定は`READ COMMITTED`とする．
- `READ COMMITTED`は文ごとに，`REPEATABLE READ`はトランザクションの最初の文でスナップショットを取る．
- 別のトランザクションが更新中の行を更新しようとしたら，そのトランザクションの終了を待つ．
  - 相手がコミットしたら，`REPEATABLE READ`では`40001`とする．`READ COMMITTED`では最新の版に対して条件を評価し直して更新する．
  - 相手が中止したら，そのまま更新する．
- `SERIALIZABLE`は`0A000`とする．

### 使用例

完成形の2つのセッションの例のとおり．

### モジュール

- `txn`：行の更新待ち(`Condvar`)と分離レベル

### 設計ドキュメントの更新

- `code-types.md`：`IsolationLevel`と待ち合わせの構造体を加える．
- `code-sequence.md`：更新の競合で待ち，エラーにする流れを加える．

### 学ぶこと

- Rust：`Condvar`，タイムアウト付きの待ち合わせ，`Barrier`を使った並行処理のテスト
- データベース：分離レベル，スナップショット分離，更新の競合(first-updater-wins)

### 既存テストへの影響

なし．
