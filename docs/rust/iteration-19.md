# Iteration 19：`Write`とジェネリクス，`BufWriter`，共有する値

Iteration 19では，変更を先にログ(WAL)へ書き，プロセスが止まったあとでログから変更をやり直す．
このノートでは，書き込み先を`Write`トレイトの型引数にする方法，書き込みをまとめる`BufWriter`とディスクに届ける`sync_data`，型引数を決めた`impl`，1つの値を複数の持ち主で共有する`Rc`，`if let`をつなぐ条件，子プロセスを強制終了させるテストを説明する．

## `Write`を型引数にする

`std::io::Write`は，バイト列を書ける型のトレイトである．`File`だけでなく，`Vec<u8>`や標準出力も`Write`を実装する(Iteration 6)．
書き込み先を型引数`W: Write`にすれば，本番ではファイルに，テストではメモリーの`Vec<u8>`に書ける．

```rust
pub struct Journal<W: Write> {
    out: BufWriter<W>,
    written: u64,
}

impl<W: Write> Journal<W> {
    pub fn new(out: W) -> Journal<W> {
        Journal {
            out: BufWriter::new(out),
            written: 0,
        }
    }

    pub fn log(&mut self, line: &str) -> io::Result<u64> {
        writeln!(self.out, "{line}")?;
        self.written += u64::try_from(line.len() + 1).expect("fits");
        Ok(self.written)
    }
}

let mut journal = Journal::new(Vec::new());
assert_eq!(journal.log("begin").unwrap(), 6);
assert_eq!(journal.log("commit").unwrap(), 13);
assert_eq!(journal.contents(), b"begin\ncommit\n");
```

`WalWriter<W: Write>`も同じ形で，単体テストでは`Vec<u8>`に書いて，レコードのバイト列を確かめる．

## `BufWriter`

`BufWriter<W>`は，書いたバイト列をメモリーにためておき，ある程度たまってから`W`にまとめて書く．
小さな書き込みのたびにOSを呼ばないので速くなる．

| メソッド | すること |
| --- | --- |
| `flush()` | ためたバイト列を`W`に書く(`Write`トレイトのメソッド) |
| `get_ref()` | 中の`W`への参照 |
| `into_inner()` | ためたバイト列を書いてから，中の`W`を取り出す |

- ためているバイト列より大きな書き込みは，ためずにそのまま`W`へ書く．既定のためる大きさは8KiBである．
- `BufWriter`を捨てるとき，ためたバイト列を書くが，失敗しても知らせない．失敗を知りたいときは，捨てる前に`flush`を呼ぶ．

## ディスクに届ける：`sync_data`

`write`や`flush`でファイルに書いても，内容はまずOSのメモリーに置かれ，あとでディスクに書かれる．プロセスが止まってもOSのメモリーの内容は残るが，OSや電源が止まると失われる．
`File::sync_data()`は，ファイルの内容がディスクに届くまで待つ．

| 操作 | 内容が届く先 |
| --- | --- |
| `BufWriter`に書く | プロセスのメモリー |
| `flush` | OSのメモリー |
| `sync_data` | ディスク |

`sync_data`は，ファイルの大きさなど内容を読むのに要る情報も書く．更新日時なども書くときは`sync_all`を使う．
ディスクに届くのを待つので遅い．コミットのたびに1回だけ呼ぶ．

## 型引数を決めた`impl`

`impl<W: Write> Journal<W>`は，どの`W`にもメソッドを定義する．
`impl Journal<File>`のように型引数を決めて書くと，その型のときだけ使えるメソッドを定義できる．

```rust
impl Journal<File> {
    pub fn sync(&mut self) -> io::Result<()> {
        self.out.flush()?;
        self.out.get_ref().sync_data()
    }
}

impl Journal<Vec<u8>> {
    pub fn contents(self) -> Vec<u8> {
        self.out.into_inner().expect("flush to a Vec never fails")
    }
}
```

`Vec<u8>`には`sync_data`がないので，`Journal<Vec<u8>>`で`sync`を呼ぶとコンパイルエラーになる．
`WalWriter<File>`の`open`と`flush_to`も，ファイルのときだけのメソッドである．

## `Rc<RefCell<T>>`で共有する

1つの値を，複数の構造体で持ちたいことがある．データベースと，表とインデックスのすべてのバッファプールは，同じログに書き込む．
`std::rc::Rc<T>`は，値を数える参照(参照カウント)で共有する．

- `Rc::clone(&rc)`は，値を複製せず，同じ値を指す`Rc`を増やす．
- 最後の`Rc`が捨てられたときに，値も捨てられる．
- `Rc`の中の値は，共有の参照でしか使えない．書き換えるには`RefCell`(Iteration 15)と組にする．

```rust
pub struct Writer {
    journal: Rc<RefCell<Journal<Vec<u8>>>>,
    name: &'static str,
}

impl Writer {
    pub fn say(&self, word: &str) -> u64 {
        self.journal
            .borrow_mut()
            .log(&format!("{}: {word}", self.name))
            .unwrap()
    }
}

let journal = Rc::new(RefCell::new(Journal::new(Vec::new())));
let alice = Writer { journal: Rc::clone(&journal), name: "alice" };
let bob = Writer { journal: Rc::clone(&journal), name: "bob" };
alice.say("hi");
bob.say("hello");
assert_eq!(Rc::strong_count(&journal), 3);
```

参照(`&`)で共有すると，持ち主のライフタイムを型に書く必要があり，持ち主より長く生きられない．`Rc`なら，最後の持ち主がいなくなるまで値が生きる．
`Rc`と`RefCell`は，1つのスレッドの中だけで使える．Iteration 21では，スレッドの間で共有できる`Arc`と`Mutex`に置き換える．

## `if let`をつなぐ条件

`if let`のパターンと，ほかの条件を`&&`でつなげる(Rust 2024)．すべてを満たしたときだけ，腕を実行する．

```rust
if let Some(half) = value.checked_div(2)
    && half * 2 == *value
    && *value > limit
{
    return Some(*value);
}
```

パターンで取り出した`half`は，あとの条件でも腕の中でも使える．
`if let`の中にもう1つ`if`を入れ子にすると，`cargo clippy`は1つにまとめるよう求める．

```text
error: this `if` statement can be collapsed
  --> src/lib.rs:55:9
   |
55 | /         if let Some(half) = value.checked_div(2) {
56 | |             if half * 2 == *value && *value > limit {
57 | |                 return Some(*value);
58 | |             }
59 | |         }
   | |_________^
```

## CRC-32：`crc32fast`

CRC-32は，バイト列から32ビットの値(チェックサム)を計算する方法である．バイト列の一部が変わると，ほとんどの場合に値が変わる．
`crc32fast`クレートの`crc32fast::hash(&bytes)`は，バイト列のCRC-32を`u32`で返す．

```rust
let crc = crc32fast::hash(b"commit");
assert_ne!(crc, crc32fast::hash(b"commiT"));
```

レコードと一緒にCRC-32を書いておき，読むときに計算し直して比べれば，書きかけで壊れたレコードを見つけられる．

## 子プロセスを強制終了させるテスト

プロセスが止まったあとの振る舞いを確かめるには，テストの中で別のプロセスを動かして，後片付けをさせずに止める．
テストのバイナリ自身を，環境変数を付けて子プロセスとして起動すれば，バイナリの名前によらず同じテストの中に書ける．

```rust
#[test]
fn child_process_is_killed() {
    if std::env::var("NOTES_CHILD").is_ok() {
        println!("child is running");
        std::process::abort();
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["tests::child_process_is_killed", "--exact", "--nocapture"])
        .env("NOTES_CHILD", "1")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("child is running"));
}
```

- `std::env::current_exe()`は，実行中のバイナリのパスである．`cargo test`では，テストのバイナリである．
- テストのバイナリに，テストの名前と`--exact`を渡すと，そのテストだけを実行する．
- `std::process::Command`の`env`で，子プロセスにだけ環境変数を渡す．子プロセスでは`std::env::var`が`Ok`を返す．
- `std::process::abort()`は，値を捨てずに(`Drop`を呼ばずに)，その場でプロセスを止める．バッファプールの`Drop`もページを書き戻さない．
- `status()`は子プロセスが終わるのを待って終了状態を返し，`output()`は標準出力と標準エラー出力も集めて返す．`abort`で止まった子プロセスの`status.success()`は偽である．
