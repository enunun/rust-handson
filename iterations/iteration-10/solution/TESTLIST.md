# テストリスト

## 単体テスト

### sql::parser

- [x] `EXPLAIN`は`SELECT`を持つ．`EXPLAIN`のあとが`SELECT`でなければ，その位置の構文エラーになる

### plan::binder

- [x] `bind_select`は，選択項目を結果の列名と式にする．表の列名も持つ
- [x] 名前だけの並べ替えのキーは，表の列より結果の列名(別名)を先に探す
- [x] 選択項目と同じ式のキーは，結果の列の値を使う
- [x] それ以外のキーは，表の行について評価する．`DISTINCT`では`OrderByNotInSelectList`になる
- [x] 式は，列名と括弧で表示する(`((ID * 10) > 1)`，`(NOT (NAME IS NULL))`，`'it''s'`)

### plan::planner

- [x] `SELECT name FROM users`は，`SeqScan`の上の`Project`になる
- [x] 演算子は，下から`Filter`，`Project`，`Distinct`，`Sort`，`Limit`の順に重なる
- [x] 結果の列にないキーは`Project`の隠れた列になり，最上段の`Project`で取り除く

### plan::explain

- [x] 演算子を1行に1つ書き，子を2文字深く字下げする
- [x] 並べ替えのキーに，`DESC`と，既定と違う`NULLS`の位置を書き添える
- [x] `Limit`は，`OFFSET`と`FETCH FIRST`の書かれた方を表示する

### exec::scan

- [x] `SeqScan`は，すべての行を順に返し，最後に`None`を返す

### exec::filter

- [x] `Filter`は，条件が真の行だけを返す(偽と`NULL`は返さない)

### exec::project

- [x] `Project`は，各行について式の値を計算する

### exec::distinct

- [x] `Distinct`は，同じ行を1度だけ，最初に現れた順で返す．`NULL`の行も1つにまとめる

### exec::sort

- [x] `Sort`は，子の行をキーの順に返す

### exec::limit

- [x] `Limit`は，`offset`行を飛ばし，`fetch`行まで返す
- [x] `fetch`がなければ，残りをすべて返す
- [x] 行の数より大きい`offset`と，`fetch`が0なら，行を返さない

## 結合テスト

### explain

- [x] `EXPLAIN`は，実行計画を字下げした木として，`QUERY PLAN`の列に返す
- [x] `DISTINCT`と`OFFSET`は，`Distinct`と`Limit`になる
- [x] 結果の列にないキーで並べ替えると，最上段の`Project`が隠れた列を取り除く．`SELECT`の結果は変わらない
- [x] `EXPLAIN`は，`SELECT`と同じ名前解決のエラーを返す
- [x] `EXPLAIN`は演算子を動かさない(`1 / 0`でもエラーにならない)

### repl

- [x] `EXPLAIN`の結果を表として表示する

### select，order_by(既存)

- [x] 演算子に置き換えたあとも，引き継いだテストがすべて通る
