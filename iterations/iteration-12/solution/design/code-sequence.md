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

`Database::execute`が`SELECT`を実行する流れを示す．名前を解決し，実行計画を作り，演算子の木で実行する．
図の実行計画は`SELECT name FROM users WHERE id > 1 ORDER BY name`のものである．

```mermaid
sequenceDiagram
  participant developer as 開発者
  participant database
  participant binder
  participant planner
  participant build as exec::build
  participant sort as Sort
  participant project as Project
  participant filter as Filter
  participant scan as SeqScan
  developer->>database: execute(sql)
  database->>binder: bind_select(select, schema)
  binder-->>database: BoundSelect
  database->>planner: plan(bound)
  planner-->>database: PlanNode
  database->>build: build(plan, rows)
  build-->>database: Box<dyn Executor>
  database->>sort: next()
  loop 子の行がなくなるまで
    sort->>project: next()
    project->>filter: next()
    loop 条件が真の行が見つかるまで
      filter->>scan: next()
      scan-->>filter: Some(row)
    end
    filter-->>project: Some(row)
    project-->>sort: Some(選択項目の値)
  end
  sort->>sort: キーの値で並べ替える
  sort-->>database: Some(1行目)
  loop 行がなくなるまで
    database->>sort: next()
    sort-->>database: Some(row) か None
  end
  database-->>developer: StatementResult::Rows
```

- 演算子は，親に`next`を呼ばれたときに，子の`next`を呼んで1行ずつ受け取る(Volcanoモデル)．`SeqScan`は行がなくなると`None`を返し，`None`は親へ順に伝わる．
- `Sort`は，最初の`next`で子の行をすべて読んで並べ替える．それ以外の演算子は，1行を受け取るたびに1行を返す．
- 実行計画の演算子は，下から`SeqScan`(結合なら`NestedLoopJoin`)，`Filter`(`WHERE`)，`HashAggregate`，`Filter`(`HAVING`)，`Project`，`Distinct`，`Sort`，`Limit`の順に重ねる．`Project`と`SeqScan`以外は，その句がなければ置かない．`HashAggregate`は集約する問い合わせにだけ置く．
- 結果の列にない式で並べ替えるときは，下の`Project`がその式を隠れた列として計算し，最上段の`Project`が取り除く．
- `ORDER BY`の名前だけのキーは，まず結果の列名(別名を含む)から探し，なければ表の列として解決する．選択項目と同じ式のキーは，結果の列の値を使う．
- `EXPLAIN`は，同じ実行計画を`plan::explain`で行の並びにして返す．演算子は実行しない．
- `Filter`は，条件が偽か`NULL`の行を返さない．条件が真偽値でなければ`ArgumentNotBoolean`になる．
- `Distinct`は`NULL`どうしを同じ値とみなす．`DISTINCT`では，結果の列から値を取れないキーは`OrderByNotInSelectList`になる．
- 並べ替えは安定である．すべてのキーが等しい行は，子の演算子が返した順序を保つ．
- どの段階でエラーが起きても，`execute`はそこで止まり，`?`が`From`の実装で段階のエラーを`Error`に変換して返す．
- `*`は，表のすべての列を指す`BoundExpr::Column`に展開する．
- 結果の列名は，別名，列の名前，`?column?`の順に決める．
- `VALUES`と`INSERT`の式は，列のない空の行について評価する．列を参照すると名前解決で`UndefinedColumn`になる．
- 式の優先順位は次のとおりである．
  - `OR`：1，`AND`：2，`NOT`：3，`IS [NOT] NULL`：4，比較：5(結合性なし)，連結`||`：7，加減算：10，乗除算：20，単項の`-`：30

## 入れ子ループ結合

`NestedLoopJoin`が`next`を呼ばれて，結合した行を返す流れを示す．
図の実行計画は`SELECT * FROM emp e LEFT JOIN dept d ON e.dept = d.code`のものである．

```mermaid
sequenceDiagram
  participant parent as 親の演算子
  participant join as NestedLoopJoin
  participant left as SeqScan EMP E
  participant right as SeqScan DEPT D
  parent->>join: next()
  opt 最初の next
    loop 内側の行がなくなるまで
      join->>right: next()
      right-->>join: Some(row)
    end
    join->>join: 内側の行を right_rows に持つ
  end
  loop 返す行が決まるまで
    opt 外側の今の行がない
      join->>left: next()
      left-->>join: Some(row)，なければ None を返して終わる
    end
    loop right_rows の position から末尾まで
      join->>join: 外側の行と内側の行をつなぎ，ON の条件を評価する
      opt 条件が真
        join-->>parent: Some(つないだ行)
      end
    end
    join->>join: current.take() で外側の今の行を手放す
    opt LEFT JOIN で相手がなかった
      join-->>parent: Some(外側の行 + NULL)
    end
  end
```

- 内側の行は最初の`next`で1度だけ読み，外側の行ごとに`right_rows`の先頭から照らし合わせる．
- 条件を満たす組を返すと，`position`を覚えたまま戻る．次の`next`は，同じ外側の行の続きから照らし合わせる．
- `CROSS JOIN`と`,`は条件を持たず，すべての組を返す．
- `ON`の条件が真偽値でなければ，`ArgumentNotBoolean`(`JOIN/ON`)になる．

## 集約

`HashAggregate`が，子の行をグループにまとめて集約した行を返す流れを示す．
図の実行計画は`SELECT dept, COUNT(*) AS n, SUM(salary) AS total FROM emp GROUP BY dept HAVING COUNT(*) > 1`のものである．

```mermaid
sequenceDiagram
  participant filter as Filter (HAVING)
  participant aggregate as HashAggregate
  participant accumulator as Accumulator
  participant scan as SeqScan EMP
  filter->>aggregate: next()
  opt 最初の next
    loop 子の行がなくなるまで
      aggregate->>scan: next()
      scan-->>aggregate: Some(row)
      aggregate->>aggregate: キーの値を求め，HashMap でグループの番号を引く
      opt 初めてのキー
        aggregate->>aggregate: グループと，集約関数ごとの Accumulator を作る
      end
      loop 集約関数ごと
        aggregate->>accumulator: add(引数の値)(NULL なら渡さない)
      end
    end
    loop グループが最初に現れた順
      aggregate->>accumulator: finish()
      accumulator-->>aggregate: Value
      aggregate->>aggregate: キーの値と結果を並べた行を作る
    end
  end
  aggregate-->>filter: Some(集約した行)
  filter->>filter: COUNT(*) > 1 を集約した行について評価する
```

- `HashMap`はキーの値の並びからグループの番号を引き，グループの`Accumulator`は番号の順で`Vec`へ置く．結果は，グループが最初に現れた順でなる．
- キーの`NULL`どうしは同じグループになる．
- `GROUP BY`がなければ，行が1つもなくてもグループを1つ作る．`COUNT`は0，ほかの集約関数は`NULL`を返す．
- 集約した行は，キーの値のあとに集約関数の結果を並べたものである．`HAVING`，選択項目，`ORDER BY`の式は，この行の列を参照する．

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
