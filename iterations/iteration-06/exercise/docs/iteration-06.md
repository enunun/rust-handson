# Iteration 6：REPL

このIterationでは，`ferrodb`コマンドを起動してSQLを対話的に実行できるREPLを作る．
入出力を引数に取る関数としてREPLを書き，テストで確かめる．大きくなったモジュールを階層に整理する．

## 6-1 準備

`cargo test`を実行し，引き継いだテストがすべて通ることを確かめる．次は結果の要約である．

```text
test result: ok. 114 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

## 6-2 文法と概念

次の2つのノートを読む．

- [Rustのノート](../../../../docs/rust/iteration-06.md)：バイナリクレート，`BufRead`と`Write`，引数の`impl Trait`，書式指定，モジュールの階層
- [データベースのノート](../../../../docs/db/iteration-06.md)：対話的なクライアント，結果集合とコマンドタグ，結果の表示

読み終えたら，一時的なモジュールにテストを書いて，次の課題を確かめる．

1. `impl BufRead`から行を読み，行番号を付けて`impl Write`に書く関数`number_lines`を書く．テストでは`"a\nb\n".as_bytes()`を入力に，`Vec<u8>`を出力に渡す．
2. `format!`で，`"ab"`を幅5の右寄せ，左寄せ，中央寄せにした結果を確かめる．
3. `vec!["a", "b"].join("|")`，`"-".repeat(3)`，`"x  ".trim_end()`の結果を確かめる．

## 6-3 テストリスト

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

### 作るもの

| モジュール | 作るもの |
| --- | --- |
| `repl` | `pub fn run(input: impl BufRead, mut output: impl Write, interactive: bool) -> io::Result<()>`，`pub fn split_statements(buffer: &str) -> (Vec<String>, String)` |
| `format` | `StatementResult`と`QueryResult`の`Display`の実装 |
| `src/main.rs` | 標準入出力と，標準入力が端末かどうかを`repl::run`に渡す |
| 階層の整理 | `sql`(`token`，`lexer`，`ast`，`parser`)と`exec`(`eval`)にまとめる |

### 書くときに考えること

- 表の整形，文の区切り，REPL全体を，それぞれ別の単体テストと結合テストで確かめる．
- 表の整形の境界を考える．値が見出しより長い列，見出しより短い列，行がない結果，1行だけの結果．
- 文の区切りの境界を考える．`;`のあとに残る文字，引用符の中の`;`，引用符の中の`''`．
- REPLのテストは，`&[u8]`を入力に，`Vec<u8>`を出力に渡して，書かれた文字列全体を比べる．
- モジュールの階層の整理は，振る舞いを変えないリファクタリングである．整理の前後で既存のテストがすべて通ることを確かめる．

## 6-4 設計ドキュメント

- `c4-context.md`：REPLを使う利用者を加える．
- `c4-container.md`：REPLのバイナリを加える．表と行がどこにあるかを説明に書く．
- `c4-component.md`：`main`，`repl`，`format`を加え，`sql`と`exec`の階層を反映する．子モジュールを宣言するだけの`sql`と`exec`は，`Container_Boundary`で囲んで描く．
- `code-sequence.md`：REPLが行を読み，文を区切って実行し，結果を書く流れを加える．

## 6-5 テスト駆動の実装

### 実装のヒント

- 最初にモジュールの階層を整理する．ファイルを`src/sql/`と`src/exec/`に移し，`src/sql.rs`と`src/exec.rs`で`pub mod`を宣言する．`use crate::lexer::...`のようなパスを直す．
- 表の整形では，まず列ごとの幅を求め，それから見出し，区切り線，各行を書く．`{:^width$}`などで寄せる．
- 各行の末尾の空白は`trim_end`で取り除く．
- 文の区切りでは，今いるのが引用符の中かどうかを`Option<char>`で覚えながら，1文字ずつ調べる．
- `repl::run`は，読んだ行をまだ終わっていない文の後ろに足してから，`split_statements`で区切る．
- プロンプトは改行を書かないので，書いたあとに`flush`する．
- `Database::execute`に渡す文には`;`を含めない．

### ツールの操作

- `src/main.rs`を作り，`cargo run`でREPLを起動する．入力を終えるには`Ctrl+D`を押す．
- `echo 'VALUES (1, TRUE);' | cargo run -q`のように，標準入力からSQLを渡せる．このときはプロンプトを表示しない．

```console
$ echo 'VALUES (1, TRUE);' | cargo run -q
 COLUMN1 | COLUMN2
---------+---------
       1 | t
(1 row)

```

## 6-6 振り返り

1. 自分の`TESTLIST.md`を[模範解答のテストリスト](../../solution/TESTLIST.md)と比べる．
2. `repl::run`が入出力を引数に取らず，関数の中で`std::io::stdin()`と`std::io::stdout()`を使う設計と比べる．テストはどう書くことになるか．
3. プロンプトを表示するかどうかを，引数`interactive`で決めた．`repl::run`の中で標準入力が端末かどうかを調べる設計と比べる．
4. 表の整形を`QueryResult`の`Display`の実装にした．`repl`の関数にした場合と比べる．
5. 文の区切りをSQLの字句解析器で行う設計と比べる．入力の途中(閉じていない文字列など)では，字句解析器はどう振る舞うか．
6. 設計ドキュメントと実装を見比べ，違うところがあれば設計ドキュメントを直す．

## 6-7 発展課題

`psql`と同じく，`\`で始まる行をREPLのコマンドとして扱う．

- `\q`：REPLを終了する．
- `\dt`：表の名前を，名前の順に1行ずつ表示する．
- それ以外の`\`で始まる行には`invalid command \x`のように表示する．

コマンドは，文の途中ではない行の先頭に書いたときだけ扱う．
これまでと同じく，テストリスト，設計ドキュメント，実装の順に進める．
