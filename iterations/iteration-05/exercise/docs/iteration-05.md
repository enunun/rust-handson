# Iteration 5：表の作成と挿入

このIterationでは，`CREATE TABLE`で表を作り，`INSERT`で行を挿入し，`SELECT * FROM`で問い合わせる．
表の定義を持つカタログと，SQLを実行する`Database`を作る．識別子，`BIGINT`型，代入の型変換も扱う．

## 5-1 準備

`cargo test`を実行し，引き継いだテストがすべて通ることを確かめる．次は結果の要約である．

```text
test result: ok. 86 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

## 5-2 文法と概念

次の2つのノートを読む．

- [Rustのノート](../../../../docs/rust/iteration-05.md)：`HashMap`，`Default`，構造体が値を所有する，`Option`で省略を表す，`match`のガード，`enumerate`と`zip`，スライス，`expect`
- [データベースのノート](../../../../docs/db/iteration-05.md)：表とスキーマ，カタログ，識別子，データ型，代入の規則，文の原子性

読み終えたら，一時的なモジュールにテストを書いて，次の課題を確かめる．

1. 名前から電話番号を引く`struct PhoneBook`を作る．`HashMap<String, String>`を持ち，`Default`を導出する．関連関数`new`，同じ名前があればエラーにするメソッド`add(&mut self, name: &str, number: &str) -> Result<(), String>`，`find(&self, name: &str) -> Option<&String>`を書く．
2. 年齢`Option<u32>`を受け取り，20以上なら`"adult"`，それ未満なら`"minor"`，`None`なら`"unknown"`を返す関数を，ガードを使った`match`で書く．
3. `vec!["x", "y", "z"]`を，`enumerate`を使って`"1.x"`，`"2.y"`，`"3.z"`の並びにする．

## 5-3 テストリスト

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

### 作るもの

| モジュール | 作るもの |
| --- | --- |
| `token`，`lexer` | `Token::Identifier(String)`と，キーワード`CREATE`，`TABLE`，`INSERT`，`INTO`，`SELECT`，`FROM`，`INTEGER`，`INT`，`BIGINT`，`BOOLEAN`，`VARCHAR` |
| `ast` | `enum Statement { Values, CreateTable, Insert, Select }`，`struct CreateTable`，`struct ColumnDef`，`struct Insert { table, columns: Option<Vec<String>>, values }`，`struct Select { table }`．`Values`を`rows: Vec<Vec<Expr>>`にする |
| `parser` | `parse`が`Result<Statement, ParseError>`を返す．`ParseError::ValuesLengthMismatch` |
| `value` | `Value::BigInt(i64)`，`enum DataType { Integer, BigInt, Boolean, Varchar(usize) }` |
| `catalog` | `struct Catalog`，`struct TableSchema { name, columns }`，`struct Column { name, data_type }`，`enum SchemaError` |
| `database` | `pub struct Database`(`new`，`execute(&mut self, sql: &str) -> Result<StatementResult, Error>`)，`pub enum StatementResult { Rows(QueryResult), CreateTable, Insert { count } }`，`pub struct QueryResult` |
| ルート(`lib.rs`) | `execute`関数をなくし，`Database`を公開する |

### 書くときに考えること

- 既存のテストで期待値が変わるものを探す．キーワードでない単語，範囲を超える整数リテラル，`Values`の形，`execute`の呼び方．
- 識別子の正規化の境界を考える．大文字と小文字，数字と`_`，キーワードを含む単語，引用符の中の引用符．
- 代入の型変換を，カタログの単体テストで型の組み合わせごとに確かめる．
- 文の原子性を確かめるには，どんな`INSERT`を実行して，何を確かめればよいか．
- 結合テストの新しいファイル`tests/tables.rs`を作る．

## 5-4 設計ドキュメント

- `c4-context.md`，`c4-container.md`：`ferrodb`でできること，ライブラリが持つものを書き直す．
- `c4-component.md`：`catalog`と`database`を加える．入口がルートから`database`に移る．どのモジュールが`DataType`を使うか．
- `code-types.md`：データベース，カタログ，文の構文木，`SchemaError`を加える．図が大きくなるので，変わらないトークンの型は省いてよい(省いたことを図の上に書く)．
- `code-sequence.md`：`INSERT`で，カタログの定義を引き，各行の式を評価し，列の型に合わせてから行を加える流れを描く．

## 5-5 テスト駆動の実装

### 実装のヒント

- 識別子は，英字か`_`で始まり，英字，数字，`_`が続く．winnowの`one_of`と`take_while`には文字の範囲の組(`('a'..='z', 'A'..='Z', '_')`)を渡せる．`take()`は，読んだ部分の文字列を返す．
- 読んだ単語を大文字にしてからキーワードの表を引き，キーワードでなければ識別子にする．
- 文の種類は，`alt`で`VALUES`，`CREATE TABLE`，`INSERT`，`SELECT`を試す．最初のキーワードを読んだら，`cut_err`でバックトラックを止める．
- 構文解析の結果を組み立てる関数は，タプルを受け取るパターン(`fn to_insert((table, columns, values): (...)) -> Insert`)にすると，クロージャなしで書ける．
- 行の長さの検査は，構文解析が成功したあとに`Statement`を調べて行う．
- 表の定義と行は別々に持つ．`Database`は`Catalog`と，表の名前から行の並びを引く`HashMap`を持つ．
- 代入の型変換は，列の型と値の組に対する`match`で書く．
- `INSERT`は，すべての行を作り終えてから表の行に加える．

### ツールの操作

- 新しい結合テストのファイルを作ったら，そのファイルだけを実行してRedを確かめる．
- `cargo clippy`が`Default`の実装を求めたら，`#[derive(Default)]`を加える．

## 5-6 振り返り

1. 自分の`TESTLIST.md`を[模範解答のテストリスト](../../solution/TESTLIST.md)と比べる．既存のテストの変更を，いくつ挙げられたか．
2. 表の定義(`Catalog`)と行(`Database`の`HashMap`)を1つの構造体`Table { schema, rows }`にまとめる設計と比べる．Iteration 13で行をページに移すとき，どちらが変更しやすいか．
3. `Insert::columns`を`Vec<String>`にし，空の並びで「列を指定しない」を表す設計と比べる．
4. `SchemaError`は`catalog`モジュールに置いた．`SchemaError`を`database`モジュールに置くと，モジュールの依存はどうなるか．
5. 代入の型変換を`Column`のメソッドにした．`Value`のメソッドや，`database`の関数にした場合と比べる．
6. 設計ドキュメントと実装を見比べ，違うところがあれば設計ドキュメントを直す．

## 5-7 発展課題

標準SQLの`INSERT INTO t DEFAULT VALUES`に対応する．すべての列が既定値(このIterationでは`NULL`)の1行を挿入する．
挿入する行が「`VALUES`の並び」か「既定値の1行」のどちらかであることを，型でどう表すかを考える．
これまでと同じく，テストリスト，設計ドキュメント，実装の順に進める．
