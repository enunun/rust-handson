# Iteration 9：並べ替えと件数の制限

このIterationでは，`ORDER BY`で問い合わせの結果を並べ替え，`OFFSET`と`FETCH FIRST`で返す行の数を制限する．
`SELECT DISTINCT`で重複する行も除く．
Rustでは，自分の型に順序を与える`Ord`の実装と，比べ方を指定する`sort_by`を学ぶ．

## 9-1 準備

`cargo test`を実行し，引き継いだテストがすべて通ることを確かめる．次は結果の要約である．

```text
test result: ok. 154 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

## 9-2 文法と概念

次の2つのノートを読む．

- [Rustのノート](../../../../docs/rust/iteration-09.md)：`Ordering`，`Ord`と`PartialOrd`の実装，`sort_by`，`skip`と`take`，`contains`による重複の検査
- [データベースのノート](../../../../docs/db/iteration-09.md)：`SELECT`の処理の順序，`ORDER BY`と名前の解決，`NULL`の並び順，`OFFSET`と`FETCH FIRST`，`DISTINCT`

読み終えたら，一時的なモジュールにテストを書いて，次の課題を確かめる．

1. `struct Point { x: i32, y: i32 }`に，`y`で比べ，等しければ`x`で比べる`Ord`を実装する．`Vec<Point>`を`sort`で並べ替えて確かめる．
2. `&mut [(&str, u32)]`(名前と点数)を，点数の降順，点数が等しければ名前の昇順に並べ替える関数を`sort_by`と`then_with`で書く．
3. `&mut [Option<i32>]`を，`None`を最後にして昇順に並べ替える関数を書く．
4. `&[i32]`から，先頭の`offset`個を飛ばし，`Option<usize>`の`fetch`個までを返す関数を`skip`と`take`で書く．`fetch`が`None`なら残りをすべて返す．

## 9-3 テストリスト

### 要件

- `ORDER BY a DESC, b`で並べ替える．`ASC`/`DESC`と`NULLS FIRST`/`NULLS LAST`に対応する．
- 既定では`NULL`を最大の値として扱う(`ASC`で最後，`DESC`で最初)．
- `ORDER BY`の名前だけのキーは，結果の列名(別名を含む)を表の列名より先に探す．選択していない列でも並べ替えられる．
- `OFFSET n ROWS`と`FETCH FIRST n ROWS ONLY`に対応する．`n`は0以上の整数のリテラルとする．
- `SELECT DISTINCT`で重複する行を除く．`NULL`どうしは同じ値とみなす．
- `SELECT DISTINCT`で，選択項目にない式で並べ替えたら`42P10`とし，メッセージを`for SELECT DISTINCT, ORDER BY expressions must appear in select list`とする．
- 処理の順序は，`WHERE`，選択項目の計算，`DISTINCT`，`ORDER BY`，`OFFSET`，`FETCH FIRST`とする．

### 使用例

```console
ferrodb> CREATE TABLE users (id INTEGER, name VARCHAR(20));
CREATE TABLE
ferrodb> INSERT INTO users VALUES (1, 'alice'), (2, 'bob'), (3, 'carol'), (4, 'dave');
INSERT 0 4
ferrodb> SELECT name FROM users ORDER BY name DESC OFFSET 1 ROWS FETCH FIRST 2 ROWS ONLY;
 NAME
-------
 carol
 bob
(2 rows)
```

### 作るもの

| モジュール | 作るもの |
| --- | --- |
| `sql::token`，`sql::lexer` | キーワード`ORDER`，`BY`，`ASC`，`DESC`，`NULLS`，`FIRST`，`LAST`，`OFFSET`，`ROWS`，`FETCH`，`ONLY`，`DISTINCT` |
| `sql::ast` | `struct OrderBy { expr, descending: bool, nulls_first: Option<bool> }`，`struct Limit { offset: usize, fetch: Option<usize> }`．`Select`に`distinct: bool`，`order_by: Vec<OrderBy>`，`limit: Limit`を加える |
| `value` | `impl Ord for Value`と`impl PartialOrd for Value`(`NULL`はどの値よりも大きい) |
| `exec::sort` | `struct SortOrder { descending: bool, nulls_first: bool }`，`struct KeyedRow { values: Row, keys: Row }`，`pub fn sort(rows: &mut [KeyedRow], orders: &[SortOrder])` |
| `plan::binder` | `BindError::OrderByNotInSelectList` |
| `error` | `SqlState::InvalidColumnReference`(`42P10`) |

### 書くときに考えること

- 並べ替えの単体テストは，`exec::sort`の`sort`にキーの値の並びを直接渡して確かめられる．`NULL`の位置，向き，2つ目のキーを分けて考える．
- キーが等しい行の順序が結果に影響するテストでは，最後のキーに`id`を加えて順序を決めるとよい．
- 別名，選択していない列，選択項目と同じ式のそれぞれで並べ替える例を考える．
- `OFFSET`と`FETCH FIRST`の境界(行の数より大きい`OFFSET`，`0`行)を考える．
- 既存のテストのうち，`Select`の構文木を比べているものはどう変わるか．

## 9-4 設計ドキュメント

- `c4-component.md`：`exec::sort`を加える．`exec::sort`はどのモジュールの何を使うか．
- `code-types.md`：`OrderBy`，`Limit`，`SortOrder`，`KeyedRow`を加え，`Select`を更新する．`Value`の順序も説明に書く．
- `code-sequence.md`：`SELECT`の処理の順序(`WHERE` → 選択項目 → `DISTINCT` → `ORDER BY` → `OFFSET`/`FETCH FIRST`)を示す．

更新したら，リポジトリのルートでMermaidの構文を検査する．

## 9-5 テスト駆動の実装

### 実装のヒント

- `ORDER BY`，`OFFSET`，`FETCH FIRST`はどれも省略できる．省略したときの値(空の並び，`0`，`None`)を返すパーサーにすると，`select`の組み立てが簡単になる．
- キーワードのあとに続く部分は`cut_err`で囲む．`OFFSET a ROWS`のような誤りは，`a`の位置の構文エラーになる．
- 行の数は，`Token::Integer`を`usize::try_from`で変換する．負の数は`-`と整数の2つのトークンなので，行の数としては読めない．
- `Value`の`Ord`は，`NULL`を最大にし，`INTEGER`と`BIGINT`を数として比べる．`Value`は`Eq`を導出しているので，`Integer(1)`と`BigInt(1)`を`Equal`にしてはいけない．
- `exec::sort`の比較では，`NULL`の位置を先に決め，`NULL`でない値だけを向きに従って比べる．
- `Database::select`では，行ごとに結果の値と並べ替えのキーの値の組(`KeyedRow`)を作る．キーの値は，結果の列から取るか，表の行について式を評価して求める．
- `DISTINCT`は，並べ替えの前に結果の値で重複を除く．`retain`と`contains`で書ける．
- `OFFSET`と`FETCH FIRST`は，並べ替えたあとに`skip`と`take`で行を取り出す．

### ツールの操作

- `value`と`exec::sort`の単体テストだけを，それぞれ実行する．

## 9-6 振り返り

1. 自分の`TESTLIST.md`を[模範解答のテストリスト](../../solution/TESTLIST.md)と比べる．
2. `Value`の`Ord`は，型の違う値(真偽値と文字列など)にも順序を決めた．同じ列の値は同じ型なのに，なぜすべての組の順序を決める必要があるか．`PartialOrd`だけを実装する設計と比べる．
3. `KeyedRow`は，結果の値と並べ替えのキーの値を別々に持つ．結果の値だけを並べ替える設計では，`ORDER BY id`(選択していない列)をどう扱えるか．
4. `DISTINCT`の重複の検査を`contains`で書いた．Iteration 8の`HashSet`で書くと，何が変わるか．
5. 設計ドキュメントと実装を見比べ，違うところがあれば設計ドキュメントを直す．

## 9-7 発展課題

標準SQLの`FETCH FIRST n ROWS WITH TIES`に対応する．
`ONLY`は`n`行で打ち切るが，`WITH TIES`は，`n`行目と並べ替えのキーが等しい行も続けて返す．

```sql
SELECT id FROM users ORDER BY age DESC NULLS LAST FETCH FIRST 1 ROWS WITH TIES;
```

`age`が`30`の行が2つあれば，1行目と同じキーを持つ2行目も返す．`WITH TIES`は`ORDER BY`と組み合わせて使う．
これまでと同じく，テストリスト，設計ドキュメント，実装の順に進める．
