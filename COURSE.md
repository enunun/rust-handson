# コース計画

このファイルは，教材を作る人とエージェントのための計画書である．
受講者向けの内容は[README.md](README.md)と[docs/ROADMAP.md](docs/ROADMAP.md)にある．
各Iterationで作る機能と学ぶことはロードマップだけに書き，このファイルには決定事項と規約を書く．

## 受講者と目標

- 受講者は，Rust以外の言語(Python，JavaScript，Go，Javaなど)で実務の経験があり，テスト駆動開発と設計図にも慣れている．Rustは初めてである．
- 受講者の多くはオブジェクト指向言語に慣れていて，代数的データ型による型のモデリングには慣れていない．Iteration 1で直積型と直和型を正面から扱い，以降のIterationでも型の設計を比べる問いを振り返りに入れる．
- 修了すると，受講者は次のことができる．
  - 所有権，借用，ライフタイムを理解し，コンパイラーのエラーを読んで直せる．
  - 代数的データ型(`struct`と`enum`の組み合わせ)で，不正な状態を表現できない型を設計できる．トレイト，ジェネリクス，`Result`と`Option`を使い分けられる．
  - 外部のクレートを使い，ファイル入出力，ネットワーク，スレッドを含む実用的なプログラムを作れる．
  - `cargo test`で単体テストと結合テストを書き，Red → Green → Refactorで開発できる．
- 教材は日本語で書く．文体は常体(である調)で，句読点は「，」と「．」を使う．`pnpm lint`(textlintとmarkdownlint)を通す．
- 規模は23回のIteration(0〜22)で，1回あたり90〜120分とする．

## 題材

PostgreSQLのクライアントから接続できるリレーショナルデータベース`ferrodb`を育てる．
完成形の使用例，対応するSQLの範囲，各Iterationの内容は[docs/ROADMAP.md](docs/ROADMAP.md)にある．

SQLの方針は次のとおりである．

- 標準SQL(SQL:2023)のCore SQLに従う．引用符で囲まない識別子は大文字に正規化する．
- 式だけの問い合わせには，標準の表値構成子`VALUES`を使う．
- SQLSTATEは，標準が定めていないサブクラスをPostgreSQLのコードに合わせる．エラーメッセージの文面もPostgreSQLに合わせる．
- 拡張は`CREATE INDEX`，`DROP INDEX`，`EXPLAIN`，`CHECKPOINT`だけとする．

解析の方針は次のとおりである．

- 汎用の解析機構はwinnowに任せる(コンビネーター，Prattによる優先順位解析の`expression`)．
- SQL固有のもの(トークンの定義，文法，演算子の優先順位表，識別子の正規化)は自作する．

## 設計ドキュメント

各パッケージの`design/`に置く．記法はMarkdownの中のMermaidとする．

| ファイル | Mermaid | 示すもの | 始まり |
| --- | --- | --- | --- |
| `c4-context.md` | `C4Context` | `ferrodb`を使う人やプログラムと`ferrodb`の関係 | Iteration 0 |
| `c4-container.md` | `C4Container` | ライブラリ，REPL，サーバーのプロセス，データディレクトリの中のファイル | Iteration 0 |
| `c4-component.md` | `C4Component` | モジュールと，モジュールの間の依存 | Iteration 0 |
| `code-types.md` | `classDiagram` | 型(構造体，列挙型，トレイト)とその関係 | Iteration 0 |
| `code-sequence.md` | `sequenceDiagram` | 主な処理での呼び出しの順序 | Iteration 0 |
| `layout.md` | `packet` | ページ，タプル，WALレコード，プロトコルのメッセージのバイト配置 | Iteration 13 |

`c4-component.md`は次の規約で書き，`scripts/check-design.mjs`で実装と照合する．

- 1つのモジュールを`Component(別名, "モジュールのパス", "Rust module", "説明")`で表す．パスは`sql::lexer`のように`crate::`を省いて書く．
- `lib.rs`は`"crate"`，`main.rs`は`"main"`というパスで表す．
- モジュールAのコードがモジュールBの項目を参照していれば，`Rel(A, B, "説明")`を描く．`use`，パス式，`pub use`のどれで参照していても依存とみなす．
- 外部のクレートはComponent図に描かず，図の下の説明に書く．
- `mod`の宣言だけを持つモジュール(`sql`，`exec`など)は名前空間として`Container_Boundary`で描く．照合スクリプトは，名前空間をComponentとして求めない．

`code-types.md`の`class`の名前は，ソースコードの`struct`，`enum`，`trait`，`type`の名前と一致させる．照合スクリプトはこれも確かめる．

設計ドキュメントの書き方は[docs/design.md](docs/design.md)にある．

## 開発環境

### ツール

- Rust 1.98.1をmiseで入れる(`mise.toml`)．エディションは2024とする．
- テストは`cargo test`，整形は`cargo fmt`，リントは`cargo clippy -- -D warnings`を使う．
- 解析はwinnow 1.0を使う．以降のIterationで加えるクレートは，ロードマップの「受講者が行うツール操作」に書く．
- リンカーの`gcc`と，Iteration 20から使う`psql`(`postgresql-client`)をDockerfileで入れる．
- Mermaidの構文検査と設計の照合には，Node(`mermaid`と`jsdom`)を使う．

### リポジトリの構成

```text
COURSE.md                          コース計画(教材を作る人向け)
README.md                          コースの概要とIterationの一覧(受講者向け)
Cargo.toml                         模範解答のためのCargoワークスペース
docs/ROADMAP.md                    各Iterationの要件，学ぶこと，設計ドキュメントの更新
docs/tdd.md                        テスト駆動開発とテストリストの書き方
docs/design.md                     設計ドキュメントの書き方
docs/rust/README.md                Rustのノートの目次
docs/rust/iteration-NN.md          Iteration NNで初めて使うRustの文法と概念
docs/db/README.md                  データベースのノートの目次
docs/db/iteration-NN.md            Iteration NNで初めて扱うデータベースの理論
scripts/check-mermaid.mjs          Mermaidの構文検査
scripts/check-design.mjs           設計ドキュメントと実装の照合
scripts/test-exercises.sh          演習パッケージのビルドとテスト
iterations/iteration-NN/
  exercise/                        受講者が作業する場所
  solution/                        演習を終えた状態と模範解答
```

Iteration番号は，ディレクトリ名とファイル名に2桁(`iteration-00`)で書き，本文に`Iteration 0`と書く．

### パッケージ

- 模範解答のパッケージ名は`ferrodb-NN-solution`とし，`[lib] name = "ferrodb"`を書く．ルートの`Cargo.toml`が`members = ["iterations/*/solution"]`でワークスペースに含める．
- 演習のパッケージ名は`ferrodb`とする．演習はワークスペースに含めない単独のパッケージで，受講者は`exercise/`の中で`cargo`を実行する．
  - Cargoの`exclude`はglobを使えないため，Iterationを作るたびに，ルートの`Cargo.toml`の`exclude`に`iterations/iteration-NN/exercise`を加える．
- 演習の`Cargo.toml`は，1つ前の模範解答の`Cargo.toml`からパッケージ名だけを変えたものとする．そのため模範解答の`Cargo.toml`はワークスペースから設定を継承せず，依存の版を直接書く．
- バイナリ名はパッケージ名になるため，演習と模範解答で異なる．テストはバイナリを起動せず，ライブラリの関数を呼ぶ．REPLは入出力を引数に取る`repl::run`としてテストする．

### コマンド

| 目的 | 受講者(`exercise/`の中) | 教材を作る人(リポジトリのルート) |
| --- | --- | --- |
| ビルド | `cargo build` | `cargo build -p ferrodb-NN-solution` |
| テスト | `cargo test` | `cargo test -p ferrodb-NN-solution` |
| 単体テストだけ | `cargo test --lib` | `cargo test -p ferrodb-NN-solution --lib` |
| 実行 | `cargo run -- 引数` | `cargo run -p ferrodb-NN-solution -- 引数` |
| 整形とリント | `cargo fmt`，`cargo clippy` | `mise run fmt`，`mise run lint` |
| リポジトリ全体の検査 | なし | `mise run check` |

RustにはREPLがないため，ノートの例は`cargo test`で動く小さなテストか，`src/main.rs`や`examples/`で試せるプログラムとして書く．
Iteration 6からは`ferrodb`のREPLで，Iteration 20からは`psql`でSQLを試せる．

`mise run check`は次を実行する．

- `cargo fmt --all --check`と`cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`(すべての模範解答)
- `scripts/test-exercises.sh`(`Cargo.toml`のあるすべての演習をビルドしてテストする)
- `pnpm lint`(textlint，markdownlint，Mermaidの構文検査，設計の照合)

### 受講者が行うツール操作

完全なコマンドは，初めて使うIterationの演習の手順で示す．それ以降は，することだけを書く．

| Iteration | 操作 |
| --- | --- |
| 0 | `cargo init --lib --name ferrodb`，`cargo add`，`cargo build`，`cargo test`，`cargo test --lib`，`cargo test --test 名前`，`cargo test フィルター`，`cargo fmt`，`cargo clippy` |
| 6 | `src/main.rs`の追加，`cargo run`，標準入力からの入力 |
| 14 | `cargo add`の`--features`と`--dev`，`cargo run -- 引数` |
| 19 | 依存の追加(コマンドは示さない) |
| 20 | テスト用の依存の追加(コマンドは示さない)，`cargo run --release` |

### ノート

- Rustの文法と概念は`docs/rust/iteration-NN.md`，データベースの理論は`docs/db/iteration-NN.md`に書く．
- それぞれの`README.md`に目次を置き，Iterationを作るたびに更新する．
- 例は実際に動かした結果を載せる．

## テスト

- 単体テストは，各モジュールのファイルの末尾の`#[cfg(test)] mod tests`に書く．`scripts/check-design.mjs`は，`#[cfg(test)]`から後ろを依存の照合から除く．
- 結合テストは`tests/`に機能ごとのファイルで書く(`tests/tokenize.rs`など)．ファイル名にIteration番号を入れない．
- テストリストは各パッケージの`TESTLIST.md`に書く．見出しは「単体テスト」と「結合テスト」とし，項目はチェックボックスにする．

## Iteration 0の演習の形

Iteration 0の`exercise/`には，Cargoのパッケージがない．受講者が`cargo init`で作る．

```text
iterations/iteration-00/exercise/
  README.md
  TESTLIST.md            見出しだけのひな形
  docs/iteration-00.md   演習の手順
  design/
    c4-context.md        見出しと，描くものを説明するコメント
    c4-container.md
    c4-component.md
    code-types.md
    code-sequence.md
```

受講者は`cargo init --lib --name ferrodb`で`Cargo.toml`と`src/lib.rs`を作り，`src/token.rs`，`src/lexer.rs`，`tests/tokenize.rs`を自分で加える．

## 落とし穴

- コンテナにリンカー(`cc`)がないと，`cargo test`が`linker 'cc' not found`で失敗する．Dockerfileで`gcc`と`libc6-dev`を入れる．
- Cargoのワークスペースの`exclude`はglobを受け付けない．演習ディレクトリを1つずつ書く．
- 演習と模範解答ではバイナリ名が違うため，`env!("CARGO_BIN_EXE_...")`を使うテストは両者で同じコードにできない．
- Mermaidの`mermaid.parse`はDOMを必要とする．Nodeでは`jsdom`で`window`と`document`を用意してから読み込む．
- `postgres`クレートの`query`は拡張クエリプロトコルを使う．`ferrodb`はSimple Queryだけを実装するので，テストでは`simple_query`と`batch_execute`を使う．
- winnowの`literal`で`TokenSlice<Token>`のトークンを比べるには，`Token`が`Eq`を実装している必要がある．
- `TokenSlice`から`any`で読んだ値は`&Token`である．`verify_map`に渡す関数は`&Token`を受け取る．
- winnowの`Prefix`と`Infix`は関数ポインターを受け取るので，クロージャを教える前でも名前付きの関数で書ける．
- winnowのパーサーの戻り値が`&str`を含むと，コンパイラーは`&mut &str`の2つのライフタイムのどちらを使うか決められず，エラーになる．ライフタイムを教える前の例では，戻り値に借用を含めない．
- `cargo clippy -- -D warnings`は，`match`による`Err`の早期リターンを`?`に直すよう求める(`question_mark`)．`?`はIteration 1で教える．
- winnow 1.0の`alt`のタプルに並べられる選択肢は9個までである．10個にすると`Alt<_, _, _>`を満たさないというエラーになる．種類ごとの関数に分けるか，`alt`を入れ子にする．
- 演習はどれもパッケージ名が`ferrodb`で版も同じなので，1つのターゲットディレクトリを共有すると，結合テストが別の演習のライブラリにリンクされる．演習ごとにターゲットディレクトリを分ける．
- winnow 1.0の`cut_err`は，エラーの型が`ErrMode`でなければ使えない．`cut_err`を使うパーサーは，戻り値を`winnow::ModalResult<T>`にする．
- 開発用のコンテナにはPostgreSQLのサーバーがない．`psql`でPostgreSQLに接続した結果を教材に載せることはできない．`psql`の出力は，Iteration 20以降に`ferrodb`のサーバーへ接続して得たものだけを載せる．
- REPLの対話的なセッションの出力は，`script -q -c バイナリ /dev/null`に入力を少しずつ流して得る．標準入力が端末になるので，プロンプトも表示される．出力の`\r`は`tr -d '\r'`で取り除く．
- 前後に空白を含むコードスパン(末尾に空白のあるプロンプトなど)はmarkdownlintのMD038に違反する．空白は文章で説明する．
- Mermaidの`sequenceDiagram`では，メッセージの文字列の中の`;`が行の区切りになり，構文エラーになる．`#59;`と書く．
- サーバーがTCPに小さなメッセージを1つずつ書くと，Nagleのアルゴリズムと遅延確認応答のため，`psql`の問い合わせのたびに約45ミリ秒遅れる．1つの問い合わせに返すメッセージは`Vec<u8>`に並べてから1回で書く．
- 教材の出力を取るためにサーバーを背景で起動したときは，`pkill -f`でなくプロセスの番号で止める．`pkill -f`のパターンは，それを実行したシェルのコマンド行にも一致して，シェルごと止める．
