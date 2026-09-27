# Iteration 20：`std::net`，`Read`と`Write`の型引数，ビッグエンディアン

Iteration 20では，PostgreSQLのクライアントがTCPで接続して問い合わせられるサーバーを作る．
このノートでは，TCPの接続を受け付ける`TcpListener`，`Read`と`Write`の両方を境界にした型引数，自分の型への`Read`と`Write`の実装と`Cursor`によるテスト，ビッグエンディアンのバイト列の読み書き，テストでサーバーを動かすスレッド，`postgres`クレートを説明する．

## `TcpListener`と`TcpStream`

`std::net::TcpListener`は，TCPの接続を待ち受ける．`TcpStream`は1つの接続で，`Read`と`Write`を実装する．

| 関数とメソッド | すること |
| --- | --- |
| `TcpListener::bind(("127.0.0.1", 5433))` | アドレスとポートで待ち受けを始める．ポートを0にすると，OSが空いているポートを選ぶ |
| `listener.local_addr()` | 待ち受けているアドレス．ポートを0にしたときに，選ばれたポートを知る |
| `listener.accept()` | 次の接続を待ち，`(TcpStream, 相手のアドレス)`を返す |
| `listener.incoming()` | 接続を待っては返すイテレーター．要素は`io::Result<TcpStream>`である |
| `TcpStream::connect(addr)` | 相手に接続する(クライアントの側) |

`ferrodb`の`serve`は，`incoming`で接続を1つずつ受け取り，終わるまで扱ってから次の接続を待つ．

```rust
let listener = TcpListener::bind(("127.0.0.1", port))?;
for stream in listener.incoming() {
    // stream は io::Result<TcpStream>
}
```

`127.0.0.1`は自分の計算機だけから接続できるアドレスである．

## `Read + Write`を型引数の境界にする

型引数の境界は`+`でつなげる．`S: Read + Write`は，読めて，書ける型である．
次の関数は，4バイトの長さのあとに本文を置く「枠」を読み，大文字にして書き返す．

```rust
/// 4バイトの長さ(ビッグエンディアン，自分を含まない)のあとに本文を置く．
fn write_frame(output: &mut impl Write, text: &str) -> io::Result<()> {
    let len = u32::try_from(text.len()).expect("fits in u32");
    output.write_all(&len.to_be_bytes())?;
    output.write_all(text.as_bytes())
}

/// 長さと本文を読む．相手が閉じていれば`None`を返す．
fn read_frame(input: &mut impl Read) -> io::Result<Option<String>> {
    let mut len = [0; 4];
    if let Err(error) = input.read_exact(&mut len) {
        return match error.kind() {
            io::ErrorKind::UnexpectedEof => Ok(None),
            _ => Err(error),
        };
    }
    let mut body = vec![0; usize::try_from(u32::from_be_bytes(len)).expect("fits")];
    input.read_exact(&mut body)?;
    Ok(Some(String::from_utf8(body).expect("UTF-8")))
}

/// 受け取った本文を大文字にして返す．相手が閉じたら終わる．
fn echo_upper<S: Read + Write>(mut stream: S) -> io::Result<()> {
    while let Some(text) = read_frame(&mut stream)? {
        write_frame(&mut stream, &text.to_uppercase())?;
    }
    Ok(())
}
```

- `echo_upper`は`stream`を値で受け取るが，`read_frame`と`write_frame`には`&mut stream`を渡す．`R: Read`なら`&mut R`も`Read`を実装し，`Write`も同じである．そのため，`echo_upper(&mut stream)`と呼んで，終わったあとに`stream`を使い続けることもできる．
- `read_exact`は，バッファを埋めるだけのバイトが来る前に接続が閉じると，種類が`io::ErrorKind::UnexpectedEof`のエラーを返す．メッセージの先頭で閉じたなら，相手が接続を終えたとみなせる．

どちらかの境界を満たさない型を渡すと，コンパイルエラーになる．`&[u8]`は`Read`を実装するが，`Write`を実装しない．

```text
error[E0277]: the trait bound `&[u8]: std::io::Write` is not satisfied
  --> tests/bound.rs:12:16
   |
12 |     echo_upper(input).unwrap();
   |     ---------- ^^^^^ the trait `std::io::Write` is not implemented for `&[u8]`
   |     |
   |     required by a bound introduced by this call
   |
help: the trait `std::io::Write` is implemented for `&mut [u8]`
  --> /rustc/48a229ceaefd4985c50990b14116b6d856af0985/library/std/src/io/impls.rs:429:0
   = note: `std::io::Write` is implemented for `&mut [u8]`, but not for `&[u8]`
note: required by a bound in `echo_upper`
  --> tests/bound.rs:3:25
   |
 3 | fn echo_upper<S: Read + Write>(mut stream: S) -> io::Result<()> {
   |                         ^^^^^ required by this bound in `echo_upper`
```

## `Cursor`と，自分の型への`Read`と`Write`の実装

`std::io::Cursor<Vec<u8>>`は，バイト列を先頭から読む`Read`である．読んだ位置を覚えているので，続けて読めば続きが返る．
`Vec<u8>`は，書いたバイト列を末尾に加える`Write`である．この2つで，接続を使わずに読み書きの関数を確かめられる．

```rust
let mut bytes = Vec::new();
write_frame(&mut bytes, "hi").unwrap();
assert_eq!(bytes, [0, 0, 0, 2, b'h', b'i']);
let mut input = Cursor::new(bytes);
assert_eq!(read_frame(&mut input).unwrap(), Some("hi".to_string()));
assert_eq!(read_frame(&mut input).unwrap(), None);
```

`echo_upper`のように1つの値から読み書きする関数には，読む元と書く先を別に持つ型を作って渡す．
`Read`は`read`を，`Write`は`write`と`flush`を実装すれば，`read_exact`や`write_all`などのほかのメソッドは既定の実装で使える．

```rust
struct FakeStream {
    input: Cursor<Vec<u8>>,
    output: Vec<u8>,
}

impl Read for FakeStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.input.read(buf)
    }
}

impl Write for FakeStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.output.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

let mut stream = FakeStream {
    input: Cursor::new(b"\0\0\0\x02hi".to_vec()),
    output: Vec::new(),
};
echo_upper(&mut stream).unwrap();
assert_eq!(stream.output, b"\0\0\0\x02HI");
```

- `read`は，読んだバイト数を返す．0は，もう読むものがないことを表す．
- `Cursor<Vec<u8>>`も`Write`を実装するが，読む位置と同じ位置に書くので，読む元と書く先を1つの`Cursor`にすると，まだ読んでいないバイト列を書き換えてしまう．
- バイト文字列の`\x02`は，16進数で書いた1バイトである．

## ビッグエンディアンのバイト列

ネットワークのプロトコルは，数を上位のバイトから並べる(ビッグエンディアン)ことが多い．
`to_be_bytes`(Iteration 13)で書き，`from_be_bytes`で読む．

```rust
let bytes = [0, 0, 1, 2, 9];
let (head, rest) = bytes.split_at_checked(4).unwrap();
assert_eq!(u32::from_be_bytes(head.try_into().unwrap()), 258);
assert_eq!(rest, [9]);
assert_eq!(bytes.split_at_checked(6), None);
```

- `split_at_checked(n)`は，`split_at`(Iteration 13)と同じく2つに分けるが，長さが`n`より短ければパニックせずに`None`を返す．相手から届いたバイト列のように，長さを信用できないときに使う．
- `from_be_bytes`は固定長の配列を受け取る．スライスは`try_into`で配列にする．

## メッセージを組み立てる小さな道具

```rust
assert_eq!([&[b'Q'][..], &5u32.to_be_bytes(), b"x\0"].concat(), b"Q\0\0\0\x05x\0");
assert_eq!(char::from(b'Q'), 'Q');
assert_eq!(0x0003_0000u32, 196_608);
```

- スライスの並びの`concat()`は，要素のスライスをつないだ`Vec`を返す．並びの要素の型をそろえるため，最初の要素を`[..]`でスライスにしている．
- `char::from(u8)`は，バイトを同じ番号の文字にする．メッセージの種類のバイトを，エラーメッセージへ書くときに使う．
- 数のリテラルは`_`で区切って読みやすくできる．プロトコル3.0の番号196608は，上位16ビットが3，下位16ビットが0の数である．

## 小さな書き込みをまとめる

TCPでは，小さな書き込みを続けると，OSが前の書き込みの確認応答を待ってから次を送ることがあり，相手に届くのが遅れる．
1つの問い合わせに返すメッセージは，`Vec<u8>`に並べてから`write_all`の1回で書く．

```rust
let mut bytes = Vec::new();
for message in messages {
    write_message(&mut bytes, message)?;
}
stream.write_all(&bytes)?;
```

Iteration 19の`BufWriter`も書き込みをまとめるが，`BufWriter`は書く先を所有する．読みと書きを1つの値で行う`handle`では，`Vec<u8>`に並べるほうが簡単である．

## `clap`の既定値

`#[arg(long, default_value_t = 5433)]`は，引数を省略したときの値である．`default_value_t`には，フィールドの型の値を書く．

```rust
#[derive(Parser)]
struct Args {
    #[arg(long, default_value_t = 5433)]
    port: u16,
}

assert_eq!(Args::parse_from(["serve"]).port, 5433);
assert_eq!(Args::parse_from(["serve", "--port", "6000"]).port, 6000);
```

`parse_from`は，コマンドの引数の代わりに文字列の並びを読む．最初の要素はプログラムの名前として扱われる．

## テストでサーバーを動かす：`thread::spawn`

クライアントの`connect`は，サーバーが応答するまで待つ．サーバーとクライアントを1つのテストで動かすには，サーバーを別のスレッドで動かす．

```rust
let listener = TcpListener::bind("127.0.0.1:0").unwrap();
let addr = listener.local_addr().unwrap();
let server = thread::spawn(move || {
    let (stream, _) = listener.accept().unwrap();
    echo_upper(stream)
});
let mut client = TcpStream::connect(addr).unwrap();
write_frame(&mut client, "ferro").unwrap();
assert_eq!(read_frame(&mut client).unwrap(), Some("FERRO".to_string()));
drop(client);
assert!(server.join().unwrap().is_ok());
```

- `std::thread::spawn`は，クロージャを新しいスレッドで実行し，`JoinHandle`を返す．`join()`はスレッドが終わるのを待ち，クロージャが返した値を`Ok`に包んで返す．スレッドがパニックしたら`Err`である．
- `move`を付けたクロージャは，使う変数(ここでは`listener`)の所有権をクロージャに移す．新しいスレッドは呼んだ関数より長く動くことがあるので，借用でなく所有権を渡す必要がある．
- `drop(client)`で接続を閉じると，サーバーの`read_frame`が`None`を返し，`echo_upper`が終わる．
- ポートを0にして選ばせるので，テストを並列に動かしてもポートがぶつからない．

スレッドの間で値を共有する方法は，Iteration 21で扱う．

## `postgres`クレート

`postgres`は，PostgreSQLのクライアントのクレートである．`ferrodb`のサーバーを確かめる結合テストで使うので，テストだけの依存に加える．

```rust
use postgres::{Client, NoTls, SimpleQueryMessage};

let mut client = Client::connect("host=127.0.0.1 port=5433 user=alice dbname=ferro", NoTls)?;
client.batch_execute("CREATE TABLE t (a INTEGER); INSERT INTO t VALUES (1)")?;
for message in client.simple_query("SELECT * FROM t")? {
    if let SimpleQueryMessage::Row(row) = message {
        println!("{:?}", row.get(0)); // Some("1")
    }
}
```

| メソッド | すること |
| --- | --- |
| `Client::connect(設定, NoTls)` | 接続する．`NoTls`は暗号化しない接続である |
| `batch_execute(sql)` | Simple Queryで文を実行し，結果を捨てる |
| `simple_query(sql)` | Simple Queryで文を実行し，`SimpleQueryMessage`の`Vec`を返す |
| `close()` | `Terminate`を送って接続を閉じる．`Client`を捨てても同じである |

- `SimpleQueryMessage`は，列の情報(`RowDescription`)，行(`Row`)，文の終わり(`CommandComplete(行の数)`)である．`row.get(i)`は`i`番目の列の値をテキスト形式の`Option<&str>`で返す．`NULL`は`None`である．
- エラーの`code()`はSQLSTATEを`SqlState`で返す．`as_db_error()`で，サーバーが送ったメッセージと位置(`position()`)を読める．
- `query`や`execute`は，Simple Queryでなく拡張クエリプロトコルを使う．`ferrodb`はSimple Queryだけを実装するので，テストでは`batch_execute`と`simple_query`を使う．

## 最適化したビルド：`cargo run --release`

`cargo build`と`cargo run`は，速くコンパイルできるように最適化をしない．`--release`を付けると，最適化してビルドし，`target/release/`に置く．

```console
$ cargo run --release -q -- serve --data-dir ./data --port 5433
ferrodb listening on 127.0.0.1:5433
```

サーバーを長く動かして使うときは，最適化したビルドを使う．
