# 処理の流れ

`Database::execute`が`INSERT`を実行する流れを示す．ほかの文も，構文解析までは同じである．

```mermaid
sequenceDiagram
  participant developer as 開発者
  participant database
  participant lexer
  participant parser
  participant catalog
  participant eval
  developer->>database: execute(sql)
  database->>lexer: tokenize(sql)
  lexer-->>database: Vec<Spanned<Token>>
  database->>parser: parse(tokens)
  parser-->>database: Statement::Insert
  database->>catalog: table(name)
  catalog-->>database: &TableSchema
  loop VALUES の各行
    loop 行の各式
      database->>eval: eval(expr)
      eval-->>database: Value
      database->>catalog: Column::assign(value)
      catalog-->>database: 列の型に合わせた Value
    end
  end
  database->>database: すべての行を表の行に加える
  database-->>developer: StatementResult::Insert { count }
```

- どの段階でエラーが起きても，`execute`はそこで止まり，`?`が`From`の実装で段階のエラーを`Error`に変換して返す．
- `INSERT`は，すべての行を評価し，列に合わせ終えてから表の行に加える．途中でエラーになれば，1行も加えない．
- `CREATE TABLE`は`Catalog::create_table`で定義を加え，空の行の並びを用意する．`SELECT * FROM`は定義から列名を，行の並びから行を複製して返す．`VALUES`は各行の式を評価して表を作る．
- `lexer`は，引用符で囲まない識別子を大文字にし，`"`で囲んだ識別子はそのまま残す．
- `parser`は，`VALUES`の行によって値の数が違えば`ValuesLengthMismatch`を返す．式の優先順位は次のとおりである．
  - `OR`：1，`AND`：2，`NOT`：3，`IS [NOT] NULL`：4，比較：5(結合性なし)，連結`||`：7，加減算：10，乗除算：20，単項の`-`：30
- `eval`は，整数を`i64`で計算し，両辺が`INTEGER`なら結果を`INTEGER`の範囲に収める．片方が`BIGINT`なら結果は`BIGINT`である．
