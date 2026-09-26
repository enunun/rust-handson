# 処理の流れ

## REPL

REPLが行を読み，`;`で終わった文を実行して結果を書く流れを示す．

```mermaid
sequenceDiagram
  participant user as 利用者
  participant main
  participant repl
  participant database
  user->>main: 起動する
  main->>repl: run(stdin, stdout, interactive)
  loop 入力の各行
    user->>repl: 1行を入力する
    repl->>repl: split_statements(pending)
    loop 終わった各文
      repl->>database: execute(statement)
      database-->>repl: StatementResult か Error
      repl-->>user: 表，コマンドタグ，または ERROR: メッセージ
    end
  end
  repl->>database: 残りの文を execute
```

- `split_statements`は，引用符の外の`;`で文を区切る．区切った文と，まだ終わっていない残りを返す．
- `interactive`が真なら，文の途中かどうかでプロンプト`ferrodb>`か`->`を書く．どちらも9文字の幅で，最後に空白を1つ置く．
- 問い合わせの結果の表のあとには空行を書く．

## SELECT

`Database::execute`が`SELECT`を実行する流れを示す．

```mermaid
sequenceDiagram
  participant developer as 開発者
  participant database
  participant parser
  participant catalog
  participant binder
  participant eval
  developer->>database: execute(sql)
  database->>parser: tokenize したトークンを parse
  parser-->>database: Statement::Select
  database->>catalog: table(from)
  catalog-->>database: &TableSchema
  database->>binder: 選択項目と WHERE の条件を bind
  binder-->>database: Vec<BoundExpr>，Option<BoundExpr>
  loop 表の各行
    database->>eval: eval_condition(filter, row, "WHERE")
    eval-->>database: bool
    opt 条件が真
      database->>eval: 選択項目ごとに eval(expr, row)
      eval-->>database: Value
    end
  end
  database-->>developer: StatementResult::Rows
```

- どの段階でエラーが起きても，`execute`はそこで止まり，`?`が`From`の実装で段階のエラーを`Error`に変換して返す．
- `*`は，表のすべての列を指す`BoundExpr::Column`に展開する．
- 結果の列名は，別名，列の名前，`?column?`の順に決める．
- `WHERE`の条件が偽か`NULL`の行は結果に含めない．真偽値でなければ`ArgumentNotBoolean`になる．
- `VALUES`と`INSERT`の式は，列のない空の行について評価する．列を参照すると名前解決で`UndefinedColumn`になる．
- 式の優先順位は次のとおりである．
  - `OR`：1，`AND`：2，`NOT`：3，`IS [NOT] NULL`：4，比較：5(結合性なし)，連結`||`：7，加減算：10，乗除算：20，単項の`-`：30

## UPDATE

`Database::execute`が`UPDATE`を実行する流れを示す．新しい行をすべて計算し，制約を検査してから表の行を置き換える．

```mermaid
sequenceDiagram
  participant developer as 開発者
  participant database
  participant catalog
  participant binder
  participant dml as exec::dml
  participant eval
  developer->>database: execute(sql)
  database->>catalog: table(table)
  catalog-->>database: &TableSchema
  database->>binder: 代入の式と WHERE の条件を bind
  binder-->>database: Vec<(usize, BoundExpr)>，Option<BoundExpr>
  database->>dml: update(schema, rows, assignments, filter)
  loop 表の各行
    dml->>eval: matches_filter(filter, row)
    eval-->>dml: bool
    opt 条件が真
      dml->>eval: 代入ごとに eval(expr, row)
      eval-->>dml: Value
      dml->>catalog: Column::assign(value)
      catalog-->>dml: 列の型に合わせた Value
    end
  end
  dml->>dml: 表の複製に新しい行を書き込む
  dml->>dml: check_constraints(schema, 複製)
  dml->>dml: 表の行を複製で置き換える
  dml-->>database: 書き換えた行の数
  database-->>developer: StatementResult::Update
```

- 代入の式は，すべて書き換える前の行で評価する．`SET a = b, b = a`は2つの列の値を入れ替える．
- 評価，型の変換，制約の検査のどこでエラーになっても，表の行を置き換える前に止まるので，表は変わらない．
- 制約は，すべての行を書き換えたあとの表で検査する．`SET id = id + 1`は，書き換えの途中で値が重なっても，終わったときに一意なら成功する．
- `INSERT`も，加える行をすべて評価し，`Column::assign`で列の型に合わせてから`exec::dml::insert`に渡す．`insert`は表の複製に行を加えて制約を検査し，表の行を置き換える．
- `DELETE`は，先にすべての行の条件を評価し，途中でエラーにならなければ`retain`で行を消す．
