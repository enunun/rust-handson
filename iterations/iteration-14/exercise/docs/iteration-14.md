# Iteration 14：ヒープファイルとデータディレクトリ

Iteration 13で作ったページを，ファイルに保存する．
表ごとのヒープファイルと，表の定義を書いたカタログのファイルを，データディレクトリに置く．`ferrodb repl --data-dir DIR`で起動し直しても，表と行が残る．
Rustでは，ファイルとディレクトリの操作，パスの型，コマンドライン引数の解析(`clap`)，テスト用の一時ディレクトリ(`tempfile`)を学ぶ．

## 14-1 準備

`cargo test`を実行し，引き継いだテストがすべて通ることを確かめる．次は結果の要約である．

```text
test result: ok. 235 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

今の`ferrodb`は，表をプロセスのメモリーに置く．REPLを起動し直すと，前に作った表はない．

```console
$ echo "CREATE TABLE t (a INTEGER); INSERT INTO t VALUES (1);" | cargo run -q
CREATE TABLE
INSERT 0 1
$ echo "SELECT * FROM t;" | cargo run -q
ERROR:  relation "T" does not exist
```

## 14-2 文法と概念

次の2つのノートを読む．

- [Rustのノート](../../../../docs/rust/iteration-14.md)：`std::fs`，`File`と`OpenOptions`，`Seek`と`read_exact`，`Path`と`PathBuf`，`clap`のderive，`ExitCode`，`tempfile`
- [データベースのノート](../../../../docs/db/iteration-14.md)：データディレクトリ，ヒープファイル，ページを読み書きする層，カタログ，起動の流れ

読み終えたら，次の課題を確かめる．テストだけで使うクレート`tempfile`を，開発用の依存に加えておく．

```console
$ cargo add --dev tempfile
    Updating crates.io index
      Adding tempfile v3.27.0 to dev-dependencies
             Features:
             + getrandom
             - nightly
    Updating crates.io index
     Locking 11 packages to latest Rust 1.98.1 compatible versions
      Adding bitflags v2.13.2
      Adding cfg-if v1.0.5
      Adding errno v0.3.14
      Adding fastrand v2.5.0
      Adding getrandom v0.4.3
      Adding libc v0.2.189
      Adding linux-raw-sys v0.12.1
      Adding once_cell v1.21.4
      Adding r-efi v6.0.0
      Adding rustix v1.1.5
      Adding tempfile v3.27.0
```

`Adding tempfile v3.27.0 to dev-dependencies`のとおり，`Cargo.toml`の`[dev-dependencies]`に加わる．

1. 一時ディレクトリに，`[1, 2, 3, 4]`を4回並べた16バイトのファイルを書く．`OpenOptions`で開き，`seek`で8バイト目に移ってから`read_exact`で4バイト読む．
2. 1のファイルの大きさを`fs::metadata`で調べる．20バイト目から4バイト読もうとすると，`kind()`は何になるか．
3. `Path::new("data").join("users").with_extension("heap")`を`display()`で書くと，どうなるか．
4. 文字列`"a/b"`を，UTF-8のバイトの16進数(`612f62`)にする関数を書く．

## 14-3 テストリスト

### 要件

- サブコマンド`repl`のオプション`--data-dir DIR`で，表をデータディレクトリのファイルに保存する．起動し直しても表と行が残る．
- カタログと，表ごとのヒープファイルを保存する．ヒープファイルの名前は，表の名前のUTF-8のバイトの16進数に`.heap`を付けたものとする(`T`なら`54.heap`)．
- `--data-dir`を省略したら，これまでどおりメモリーで動かす．
- データディレクトリがなければ作る．カタログのファイルがなければ，表のないデータベースとして始める．
- ファイルを読み書きできなければ，SQLSTATE `58030`とメッセージ`could not access file: 理由`のエラーにする．

ヒープファイルは，ページを番号の順に並べたファイルとする．`n`番目のページは，ファイルの`n * 8192`バイト目から始まる．
カタログのファイルの形式は自分で決めてよい．Iteration 13のタプルと同じく，数をリトルエンディアンで，文字列を長さとUTF-8のバイト列で書くと，符号化と復号を同じ考え方で書ける．

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

REPLは`Ctrl+D`で終える．

```rust
let mut db = Database::open(dir.path())?;
db.execute("CREATE TABLE t (a INTEGER)")?;
db.execute("INSERT INTO t VALUES (1)")?;
drop(db);
let mut db = Database::open(dir.path())?;
db.execute("SELECT * FROM t")?; // 1行が返る
```

### 作るもの

| モジュール | 作るもの |
| --- | --- |
| `storage::disk` | `pub type PageId = usize`，`trait DiskManager`(`read_page`，`write_page`，`allocate_page`，`page_count`)，`struct FileDiskManager`と`open(path: &Path)`，`create(path: &Path)`，`struct MemoryDiskManager` |
| `storage::page` | `pub type PageBytes = [u8; PAGE_SIZE]`，`Page::from_bytes(data: Box<PageBytes>)`，`Page::bytes(&self) -> &PageBytes` |
| `storage::heap` | `HeapFile::new(disk: Box<dyn DiskManager>)`．`insert`，`update`，`delete`，`tuples`，`rows`は`Result<_, HeapError>`を返す．`enum HeapError { Page(PageError), Tuple(TupleError), Io(io::Error) }` |
| `catalog` | `Catalog::table_names(&self) -> Vec<String>`，`save(&self, path: &Path) -> io::Result<()>`，`load(path: &Path) -> io::Result<Catalog>` |
| `database` | `Database::open(dir: &Path) -> Result<Database, Error>`．`CREATE TABLE`と`DROP TABLE`でカタログを保存し，ヒープファイルを作る，または消す |
| `error` | `SqlState::IoError`(`58030`)と，`io::Error`，`HeapError`からの変換 |
| `repl` | `pub fn run_with(database: Database, input: impl BufRead, output: impl Write, interactive: bool) -> io::Result<()>` |
| `src/main.rs` | `clap`によるサブコマンド`repl`と，オプション`--data-dir` |

`DiskManager`のメソッドは次のとおりとする．

```rust
pub trait DiskManager {
    fn read_page(&self, page_id: PageId, buffer: &mut PageBytes) -> io::Result<()>;
    fn write_page(&mut self, page_id: PageId, buffer: &PageBytes) -> io::Result<()>;
    fn allocate_page(&mut self) -> io::Result<PageId>;
    fn page_count(&self) -> usize;
}
```

`allocate_page`は，0で埋めたページを末尾に加え，その番号を返す．

### 書くときに考えること

- `DiskManager`の2つの実装は，同じ操作で同じ結果になるはずである．同じテストの手順を両方に使う方法を考える．
- ファイルに保存したことは，開き直してから読んで確かめる．ファイルを閉じるには，値を`drop`する．
- ファイルを使うテストが同じパスを使うと，並行して動くテストがぶつかる．
- カタログの単体テストでは，保存して読み込むと元に戻る項目のほかに，ファイルがないとき，ファイルが壊れているときを考える．
- `HeapFile`の操作が`Result`を返すようになると，`storage::heap`と`exec::dml`の単体テストのどこが変わるか．
- 結合テストは`tests/`に新しいファイルを作り，`Database::open`で開き直す流れを書く．引き継いだ結合テストは`Database::new`のまま通るはずである．

## 14-4 設計ドキュメント

- `c4-container.md`：データディレクトリを加える．中にどのファイルがあり，どのコンテナーが読み書きするか．
- `c4-component.md`：`storage::disk`を加える．`storage::heap`と`database`は，`storage::disk`の何を使うか．
- `code-types.md`：`DiskManager`と2つの実装，`HeapError`，`PageId`，`PageBytes`を加え，`HeapFile`，`Page`，`Catalog`，`Database`を更新する．トレイトと実装の関係はどの矢印で描くか．
- `layout.md`：ヒープファイルの中のページの並びと，カタログのファイルのバイト配置を加える．
- `code-sequence.md`：`main`が`--data-dir`でデータベースを開き，カタログを読んでヒープファイルを開く流れを加える．

更新したら，リポジトリのルートでMermaidの構文を検査する．

## 14-5 テスト駆動の実装

### 受講者が行うツール操作

`clap`を，機能`derive`を有効にして依存に加える．

```console
$ cargo add clap --features derive
    Updating crates.io index
      Adding clap v4.6.7 to dependencies
             Features:
             + color
             + derive
             + error-context
             + help
             + std
             + suggestions
             + usage
             - cargo
             - debug
             - deprecated
             - env
             - string
             - unicode
             - unstable-derive-ui-tests
             - unstable-doc
             - unstable-ext
             - unstable-markdown
             - unstable-styles
             - unstable-v5
             - wrap_help
    Updating crates.io index
     Locking 21 packages to latest Rust 1.98.1 compatible versions
```

`+`は有効な機能，`-`は無効な機能である．`derive`のほかに，既定で有効な機能が並ぶ．
`src/main.rs`をサブコマンドの形にしたら，`--`のあとにプログラムへの引数を渡して，ヘルプを確かめる．

```console
$ cargo run -q -- repl --help
標準入力からSQLを読み，結果を標準出力に書く

Usage: ferrodb repl [OPTIONS]

Options:
      --data-dir <DATA_DIR>  表を保存するデータディレクトリ．省略するとメモリーに置く
  -h, --help                 Print help
```

ヘルプの説明は，`///`のドキュメントコメントに書いたものが出る．

### 実装のヒント

- 下の層から作る．`storage::disk`，`storage::page`，`storage::heap`，`catalog`，`database`，`repl`と`main`の順にすれば，上の層のテストで下の層を信用できる．
- `MemoryDiskManager`は`Vec<Box<PageBytes>>`を持てばよい．ないページを読み書きしたら，`io::ErrorKind::UnexpectedEof`などのエラーにする．
- `FileDiskManager::open`は，ページの数をファイルの大きさから求める．`create`は，同じ名前のファイルがあれば中身を捨てる．`CREATE TABLE`では，前に消し損ねたファイルが残っていても空の表として始められる．
- `read_page`は`&self`を受け取る．フィールドの`File`で`seek`と`read_exact`を呼ぶには，ノートの「`&File`で読み書きする」を見る．
- `HeapFile`は`Box<dyn DiskManager>`を持ち，操作のたびにページを読み，変えたら書き戻す．ページの読み書きを`read`と`write`の小さな関数にまとめると，Iteration 13の`insert`や`update`の形をほぼ保てる．
- `tuples`はページを読んだ一時的な値からタプルを取り出すので，`&[u8]`でなく`Vec<u8>`で返す．
- `Database`は`#[derive(Debug)]`を持つので，`HeapFile`にも`Debug`が要る．`Box<dyn DiskManager>`は`Debug`を実装しないので，Iteration 13の`Page`と同じく自分で実装する．
- カタログの復号には，読んだ分だけ入力を進める`&mut &[u8]`を受け取る小さな関数(`read_u16`，`read_string`など)を作る．途中で足りなくなったら`io::ErrorKind::InvalidData`のエラーにする．
- カタログを保存するときは，`catalog.tmp`に書いてから`fs::rename`で名前を変える．
- `HashMap`の順序は決まっていないので，カタログに書く表の順序は名前で並べ替える．
- `Database::new`は`MemoryDiskManager`を，`Database::open`は`FileDiskManager`を使う．どちらのデータベースかは，`Option<PathBuf>`のフィールドで覚える．
- `run`は，`run_with(Database::new(), ...)`を呼ぶ形にすれば，引き継いだREPLのテストはそのまま通る．
- `main`は`ExitCode`を返し，データディレクトリを開けなければ`ferrodb: メッセージ`を標準エラー出力に書いて終える．

## 14-6 振り返り

1. 自分の`TESTLIST.md`を[模範解答のテストリスト](../../solution/TESTLIST.md)と比べる．
2. `HeapFile`は`DiskManager`をジェネリクス(`HeapFile<D: DiskManager>`)でなく，`Box<dyn DiskManager>`で持った．ジェネリクスにすると，`Database`の`tables`の型はどうなるか．
3. `HeapFile`は，タプルを1つ読み書きするたびに，ファイルからページを読む．300行を`INSERT`すると，何回ファイルを読むか．Iteration 15のバッファプールは，これをどう減らすか．
4. `CREATE TABLE`は，ヒープファイルを作ってからカタログを保存する．逆の順序にすると，その間でプロセスが止まったとき，次の起動で何が起きるか．
5. ヒープファイルの名前を，表の名前そのもの(`T.heap`)にしなかった理由を考える．表の名前が`../x`や`日本`のときはどうなるか．
6. 設計ドキュメントと実装を見比べ，違うところがあれば設計ドキュメントを直す．

## 14-7 発展課題

ヒープファイルの大きさは，ページの数の8192倍のはずである．
ファイルの末尾が切れたり，余計なバイトが付いたりしたヒープファイルを，`FileDiskManager::open`で見つけ，`io::ErrorKind::InvalidData`のエラーにする．
メッセージには，ファイルの大きさとパスを入れる．`ferrodb`は起動をやめ，エラーを表示する．

```console
$ head -c 100 /dev/zero >> data/54.heap
$ echo "SELECT * FROM t;" | cargo run -q -- repl --data-dir ./data
ferrodb: could not access file: size 8292 of ./data/54.heap is not a multiple of 8192
```

これまでと同じく，テストリスト，設計ドキュメント，実装の順に進める．
