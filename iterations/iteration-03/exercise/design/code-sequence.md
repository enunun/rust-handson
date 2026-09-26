# 処理の流れ

`execute`が`VALUES`の文を実行する流れを示す．

```mermaid
sequenceDiagram
  participant developer as 開発者
  participant crate
  participant lexer
  participant parser
  participant eval
  developer->>crate: execute(sql)
  crate->>lexer: tokenize(sql)
  lexer-->>crate: Vec<Token>
  crate->>parser: parse(tokens)
  parser-->>crate: Values
  loop Values の各式
    crate->>eval: eval(expr)
    eval->>eval: 部分式を再帰的に評価する
    eval-->>crate: Value
  end
  crate-->>developer: QueryResult
```

- どの段階でエラーが起きても，`execute`はそこで止まり，段階に応じた`Error`の列挙子で包んで返す．
- `parser`は，`VALUES`，括弧，コンマで区切った式の並びを読む．式は，winnowの`expression`に次の優先順位を渡して読む．
  - `OR`：1，`AND`：2，`NOT`：3，`IS [NOT] NULL`：4，比較：5(結合性なし)，加減算：10，乗除算：20，単項の`-`：30
- `eval`は，`AND`，`OR`，`NOT`の両辺を`Option<bool>`(`None`が不明)に変換し，3値論理の真理値表で計算する．
- `eval`は，算術演算と比較の片方が`NULL`なら`NULL`を返す．型が合わなければ`DatatypeMismatch`を返す．
- `eval`は，整数を`i32`の検査付き演算(`checked_add`など)で計算し，範囲を超えたら`NumericOutOfRange`，0での割り算は`DivisionByZero`を返す．
