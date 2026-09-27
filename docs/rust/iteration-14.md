# Iteration 14：ファイルとコマンドライン引数

Iteration 14では，表をデータディレクトリのファイルに保存し，`ferrodb repl --data-dir DIR`で開く．
このノートでは，ファイルとディレクトリの操作，ファイルの好きな位置の読み書き，パスの型，コマンドライン引数の解析，テスト用の一時ディレクトリを説明する．

## `std::fs`の関数

`std::fs`は，ファイルとディレクトリを操作する関数をまとめたモジュールである．どの関数も`io::Result`を返す．

| 関数 | すること |
| --- | --- |
| `fs::write(path, bytes)` | ファイルを作り(あれば中身を捨て)，バイト列を書く |
| `fs::read(path)` | ファイルの中身をすべて読み，`Vec<u8>`で返す |
| `fs::create_dir_all(path)` | ディレクトリを，途中のディレクトリも含めて作る．すでにあれば何もしない |
| `fs::rename(from, to)` | 名前を変える．`to`があれば置き換える |
| `fs::remove_file(path)` | ファイルを消す |
| `fs::read_dir(path)` | ディレクトリの中の項目を順に返すイテレーター |
| `fs::metadata(path)` | 大きさ(`len()`)などの情報 |

```rust
let dir = tempfile::tempdir().unwrap();
let data = dir.path().join("data");
fs::create_dir_all(&data).unwrap();
let path = data.join("hello.txt");
fs::write(&path, b"hello").unwrap();
assert_eq!(fs::read(&path).unwrap(), b"hello");
assert_eq!(fs::metadata(&path).unwrap().len(), 5);
fs::rename(&path, data.join("world.txt")).unwrap();
let names: Vec<String> = fs::read_dir(&data)
    .unwrap()
    .map(|entry| entry.unwrap().file_name().into_string().unwrap())
    .collect();
assert_eq!(names, vec!["world.txt"]);
```

- `read_dir`の項目は`io::Result<DirEntry>`である．`file_name()`は`OsString`(OSのファイル名の文字列)を返し，`into_string()`で`String`にする．UTF-8でない名前なら`Err`になる．
- `read_dir`の順序は決まっていない．比べるときは並べ替える．
- `tempfile::tempdir()`は，後で説明する一時ディレクトリである．

`fs::write`で書いている途中にプロセスが止まると，ファイルは書きかけのまま残る．
別の名前のファイルに書いてから`fs::rename`で名前を変えれば，ファイルは前の中身か新しい中身のどちらかになる．同じファイルシステムの中の名前の変更は，1回の操作で置き換わるからである．

### エラーの種類で分ける

`io::Error`の`kind()`は，エラーの種類を`io::ErrorKind`で返す．
`match`のガード(Iteration 5)と組み合わせると，特定の種類のエラーだけを別に扱える．

```rust
pub fn read_or_empty(path: &Path) -> io::Result<Vec<u8>> {
    match fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error),
    }
}
```

| `io::ErrorKind` | 起きるとき |
| --- | --- |
| `NotFound` | ファイルがない |
| `PermissionDenied` | 権限がない |
| `UnexpectedEof` | 読みたい長さの前にファイルが終わった |
| `InvalidData` | 読んだデータが正しくない．自分で作るエラーに使う |

自分でエラーを作るには，`io::Error::new(種類, メッセージ)`と書く．

```rust
io::Error::new(io::ErrorKind::InvalidData, "invalid catalog file")
```

## `File`と`OpenOptions`

ファイルを開いたまま何度も読み書きするには，`std::fs::File`を使う．
開き方は`OpenOptions`で指定する．

```rust
let mut file = OpenOptions::new()
    .read(true)
    .write(true)
    .create(true)
    .truncate(false)
    .open(&path)?;
```

| メソッド | 意味 |
| --- | --- |
| `read(true)` | 読める |
| `write(true)` | 書ける |
| `create(true)` | なければ作る |
| `truncate(true)` | あれば中身を捨てる．`false`なら中身を残す |

`create(true)`を指定して`truncate`を書かないと，`cargo clippy`が，中身を残すのか捨てるのかを明示するよう求める．
`File`は値が捨てられるときにファイルを閉じる．閉じる関数を呼ぶ必要はない．

## `Read`，`Write`，`Seek`

ファイルの読み書きのメソッドは，トレイト`std::io::Read`，`Write`，`Seek`にある．使うには，トレイトを`use`する．

| メソッド | トレイト | すること |
| --- | --- | --- |
| `seek(SeekFrom::Start(n))` | `Seek` | 次に読み書きする位置を，先頭から`n`バイト目にする |
| `read_exact(&mut buffer)` | `Read` | `buffer`の長さだけ読む．足りなければ`UnexpectedEof`のエラー |
| `write_all(&bytes)` | `Write` | バイト列をすべて書く |

```rust
file.write_all(&[1, 2, 3, 4, 5, 6, 7, 8])?;
file.seek(SeekFrom::Start(4))?;
file.write_all(&[9, 9])?;
let mut buffer = [0; 4];
file.seek(SeekFrom::Start(2))?;
file.read_exact(&mut buffer)?;
assert_eq!(buffer, [3, 4, 9, 9]);
file.seek(SeekFrom::Start(6))?;
let error = file.read_exact(&mut buffer).unwrap_err();
assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
```

- ファイルは「今の位置」を持ち，読み書きするとその分だけ進む．`seek`で位置を移せば，ファイルの好きな場所を読み書きできる．
- `read`は，求めた長さより少なく読んで返すことがある．決まった長さを読むときは`read_exact`を使う．
- `write`も同じく，一部だけ書いて返すことがある．すべて書くときは`write_all`を使う．
- `SeekFrom::Start`は`u64`を受け取る．`usize`の位置は`u64::try_from`で変換する．

### `&File`で読み書きする

読み書きは今の位置を変えるので，`read_exact`や`seek`は`&mut self`を受け取る．
`&self`のメソッドで，フィールドの`File`を読もうとすると，次のエラーになる．

```rust
impl Blocks {
    pub fn read_block(&self, index: u64, buffer: &mut [u8; 4]) -> io::Result<()> {
        self.file.seek(SeekFrom::Start(index * 4))?;
        self.file.read_exact(buffer)
    }
}
```

```text
error[E0596]: cannot borrow `self.file` as mutable, as it is behind a `&` reference
  --> src/lib.rs:12:9
   |
12 |         self.file.seek(SeekFrom::Start(index * 4))?;
   |         ^^^^^^^^^ `self` is a `&` reference, so it cannot be borrowed as mutable
   |
help: consider changing this to be a mutable reference
   |
11 |     pub fn read_block(&mut self, index: u64, buffer: &mut [u8; 4]) -> io::Result<()> {
   |                        +++
```

標準ライブラリは，`File`だけでなく`&File`にも`Read`，`Write`，`Seek`を実装している．ファイルの位置はOSが持っているので，共有の参照からでも読み書きできる．
`&File`を`mut`の変数に入れれば，その変数の`&mut`を通してメソッドを呼べる．

```rust
impl Blocks {
    pub fn read_block(&self, index: u64, buffer: &mut [u8; 4]) -> io::Result<()> {
        let mut file = &self.file;
        file.seek(SeekFrom::Start(index * 4))?;
        file.read_exact(buffer)
    }
}
```

`mut`なのは変数`file`(参照)で，`Blocks`は`&self`のままである．読むだけの操作を`&self`のメソッドにしておけば，呼び出し側は`&mut`の借用を用意しなくてよい．

## `Path`と`PathBuf`

ファイルのパスは，`std::path::Path`と`PathBuf`で表す．
`String`と`&str`の関係と同じく，`PathBuf`は値を所有し，`&Path`は借用である．`&PathBuf`は`&Path`として渡せる．

```rust
let dir: PathBuf = PathBuf::from("data");
let path: PathBuf = dir.join("catalog");
assert_eq!(path, Path::new("data/catalog"));
assert_eq!(path.with_extension("tmp"), Path::new("data/catalog.tmp"));
assert_eq!(Path::new("data/54.heap").extension().unwrap(), "heap");
assert_eq!(format!("{}", path.display()), "data/catalog");
let borrowed: &Path = &path;
let owned: PathBuf = borrowed.to_path_buf();
assert_eq!(owned, path);
```

| メソッド | すること |
| --- | --- |
| `join(名前)` | パスの後ろに名前をつないだ`PathBuf`を作る．区切りの`/`はOSに合わせて入る |
| `with_extension(拡張子)` | 拡張子を置き換えた(なければ加えた)`PathBuf`を作る |
| `extension()` | 拡張子．なければ`None` |
| `display()` | `{}`で書ける形にする |
| `to_path_buf()` | `&Path`を複製して`PathBuf`にする |

- パスはOSのファイル名なので，UTF-8とは限らない．そのため`Path`は`Display`を実装しない．書くときは`display()`を使う．
- 関数の引数は`&Path`にし，構造体のフィールドは`PathBuf`にする．`&str`と`String`の使い分けと同じである．
- `fs`の関数は，`&Path`，`&PathBuf`，`&str`のどれでも受け取る．

## 16進数の書式指定

`{:x}`は整数を16進数で書く．`{:02x}`は，幅を2にして，足りなければ前を`0`で埋める．

```rust
assert_eq!(format!("{:02x}", 10), "0a");
assert_eq!(format!("{:x}", 255), "ff");
let hex: String = "T1".bytes().map(|byte| format!("{byte:02x}")).collect();
assert_eq!(hex, "5431");
let hex: String = "日".bytes().map(|byte| format!("{byte:02x}")).collect();
assert_eq!(hex, "e697a5");
```

`str`の`bytes()`は，UTF-8のバイトを順に返すイテレーターである．`String`のイテレーターを`collect`すると，文字列をつないだ`String`になる．

## `HashMap`のキーを集める

`HashMap`の`keys()`は，キーの参照を返すイテレーターである．`cloned()`で参照の指す値を複製すると，所有する値のイテレーターになる．

```rust
let mut names: Vec<String> = ages.keys().cloned().collect();
names.sort();
assert_eq!(names, vec!["alice", "bob"]);
```

`HashMap`の順序は決まっていない．ファイルに書くなど，毎回同じ順序が要るときは並べ替える．

## スライスを読み進める

`&mut &[u8]`は，「スライスの参照」を書き換えられる参照である．
関数の中で先頭を読み，残りのスライスを入れ直すと，呼び出し側の変数も読んだ分だけ進む．Iteration 0のwinnowのパーサーが受け取る`&mut &str`と同じ形である．

```rust
pub fn read_u8(input: &mut &[u8]) -> Option<u8> {
    let (&first, rest) = input.split_first()?;
    *input = rest;
    Some(first)
}

let bytes = [7, 8, 9];
let mut input: &[u8] = &bytes;
assert_eq!(read_u8(&mut input), Some(7));
assert_eq!(read_u8(&mut input), Some(8));
assert_eq!(input, [9]);
```

`*input = rest`は，参照の指すスライス(変数`input`)を，残りのスライスに置き換える．バイト列そのものは複製しない．

## `drop`

`drop(値)`は，値をその場で捨てる．`File`を持つ値を捨てると，ファイルが閉じる．
テストで，ファイルを閉じてから開き直すときに使う．

```rust
let mut db = Database::open(dir.path())?;
db.execute("CREATE TABLE t (a INTEGER)")?;
drop(db);
let db = Database::open(dir.path())?;
```

`drop`のあとで`db`を使うと，ムーブ済みの値を使うエラーになる．値を捨てるときに走る処理は，Iteration 15の`Drop`トレイトで扱う．

## コマンドライン引数：`clap`

`clap`は，コマンドライン引数を解析するクレートである．
機能(feature)`derive`を有効にすると，構造体と列挙型に`#[derive(Parser)]`を付けるだけで，引数の定義と解析とヘルプの表示ができる．

```sh
cargo add clap --features derive
```

`Cargo.toml`には，有効にした機能が書かれる．

```toml
[dependencies]
clap = { version = "4.6.7", features = ["derive"] }
```

クレートの機能は，必要なときだけ有効にする追加の部分である．有効にしなければ，その部分はコンパイルされない．

次のプログラムは，サブコマンド`count`でディレクトリの中のファイルを数える．

```rust
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// ファイルを数えるプログラム
#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// ディレクトリの中のファイルを数える
    Count {
        /// 数えるディレクトリ
        dir: PathBuf,
        /// この拡張子のファイルだけを数える
        #[arg(long)]
        extension: Option<String>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Count { dir, extension } => match std::fs::read_dir(&dir) {
            Ok(entries) => {
                let mut count = 0;
                for entry in entries {
                    let path = entry.unwrap().path();
                    let matched = match &extension {
                        Some(ext) => path.extension() == Some(ext.as_ref()),
                        None => true,
                    };
                    if matched {
                        count += 1;
                    }
                }
                println!("{count}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("count: {}: {error}", dir.display());
                ExitCode::FAILURE
            }
        },
    }
}
```

- `#[derive(Parser)]`の構造体が，プログラムの引数全体である．`Cli::parse()`は，引数を解析して`Cli`を返す．引数が正しくなければ，エラーを表示してプログラムを終える．
- `#[command(subcommand)]`のフィールドの型に`#[derive(Subcommand)]`の列挙型を書くと，列挙子がサブコマンドになる．`Count`は`count`になる．
- 列挙子のフィールドが，サブコマンドの引数になる．属性のないフィールドは位置で渡す引数，`#[arg(long)]`は`--extension`のような名前付きの引数である．フィールドの`extension`はそのまま，`data_dir`は`--data-dir`になる．
- `Option<T>`のフィールドは，省略できる引数になる．`bool`のフィールドは，値を取らないフラグになる．
- `///`のドキュメントコメントが，ヘルプの説明になる．

`--`のあとに書いた引数は，`cargo`でなく，実行するプログラムに渡る．

```console
$ cargo run -q -- count --help
ディレクトリの中のファイルを数える

Usage: notes count [OPTIONS] <DIR>

Arguments:
  <DIR>  数えるディレクトリ

Options:
      --extension <EXTENSION>  この拡張子のファイルだけを数える
  -h, --help                   Print help
$ cargo run -q -- count d
3
$ cargo run -q -- count d --extension rs
2
$ cargo run -q -- count
error: the following required arguments were not provided:
  <DIR>

Usage: notes count <DIR>

For more information, try '--help'.
```

ディレクトリ`d`には，`a.rs`，`b.rs`，`c.txt`の3つのファイルを置いた．ヘルプの`notes`は，パッケージの名前から決まるバイナリの名前である．

## 終了コード：`ExitCode`

`main`は`std::process::ExitCode`を返せる．`ExitCode::SUCCESS`は0，`ExitCode::FAILURE`は1でプロセスを終える．
シェルやほかのプログラムは，終了コードで成功と失敗を見分ける．

```console
$ cargo run -q -- count nowhere
count: nowhere: No such file or directory (os error 2)
$ echo $?
1
```

`main`が`io::Result<()>`を返すと，`Err`のときに`Error: ...`の形でエラーの`Debug`表示が出る．`ExitCode`を返せば，エラーの書き方を自分で決められる．
`eprintln!`は，`println!`と同じ書式で標準エラー出力に書く．

## テスト用の一時ディレクトリ：`tempfile`

ファイルを使うテストは，テストごとに別のディレクトリを使えば，並行して動くほかのテストとぶつからない．
`tempfile`クレートの`tempdir()`は，一時ディレクトリを作る．戻り値の`TempDir`を捨てると，ディレクトリは中身ごと消える．

テストだけで使うクレートは，`--dev`を付けて開発用の依存(`[dev-dependencies]`)に加える．

```sh
cargo add --dev tempfile
```

```toml
[dev-dependencies]
tempfile = "3.27.0"
```

開発用の依存は，テスト(`#[cfg(test)]`の単体テストと`tests/`の結合テスト)からだけ使える．ライブラリやバイナリには入らない．

```rust
let dir = tempfile::tempdir().unwrap();
let path = dir.path().to_path_buf();
fs::write(path.join("a"), b"x").unwrap();
assert!(path.exists());
drop(dir);
assert!(!path.exists());
```

`dir.path()`は，一時ディレクトリの`&Path`である．`dir`を捨てるまで，ディレクトリは残る．
`let path = tempfile::tempdir().unwrap().path().to_path_buf();`のように`TempDir`を変数に入れないと，その文の終わりで`TempDir`が捨てられ，ディレクトリも消える．
