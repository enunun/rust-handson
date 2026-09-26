# Iteration 3：所有権と借用，`String`と`&str`

Iteration 3では，文字列の値を扱う．
このノートでは，Rustの値の持ち方である所有権と借用，所有する文字列`String`と借りた文字列`&str`の違い，UTF-8の文字列の数え方を説明する．

## 所有権

Rustでは，すべての値にただ1つの所有者(その値を持つ変数)がいる．

- 所有者の変数がスコープを抜けると，値は片付けられる(メモリが解放される)．
- 値を別の変数に代入したり，関数に渡したりすると，所有権が移る(ムーブ)．移したあとの元の変数は使えない．

```rust
pub fn take(s: String) -> usize {
    s.len()
}

pub fn twice() -> usize {
    let a = String::from("hello");
    let n = take(a);
    n + a.len()
}
```

`take(a)`で`a`の所有権が`take`の引数`s`に移るので，そのあとの`a.len()`はコンパイルエラーになる．

```text
error[E0382]: borrow of moved value: `a`
 --> src/lib.rs:8:9
  |
6 |     let a = String::from("hello");
  |         - move occurs because `a` has type `String`, which does not implement the `Copy` trait
7 |     let n = take(a);
  |                  - value moved here
8 |     n + a.len()
  |         ^ value borrowed here after move
```

ガベージコレクションのある言語では，同じオブジェクトを複数の変数が指せる．Rustは所有者を1つに決めることで，いつ値を片付けるかをコンパイル時に決める．

### `Clone`と`Copy`

値を複製したいときは，`clone`メソッドを明示して呼ぶ．複製した値には別の所有者がつく．

```rust
let a = String::from("hello");
let b = a.clone();
assert_eq!(take(a), 5);
assert_eq!(b, "hello");
```

整数や真偽値のように複製が安い型は`Copy`を実装していて，代入しても元の変数を使い続けられる．`Copy`はIteration 18で詳しく扱う．

```rust
let x = 5;
let y = x;
assert_eq!(x + y, 10);
```

## 借用

所有権を移さずに値を使うには，参照(`&値`)を渡して値を借りる．

- `&T`(共有参照)は読むだけの参照で，同時にいくつあってもよい．
- `&mut T`(可変参照)は書き換えられる参照で，ある時点に1つしか持てない．可変参照がある間は，共有参照も持てない．

この規則は，「読んでいる途中で誰かが書き換える」ことをコンパイル時に防ぐ．
次の例では，`first`が`names`の要素を借りている間に`push`で`names`を書き換えようとしている．
`push`で`Vec`の中身が別の場所に移ると，`first`は古い場所を指すことになるので，コンパイラーが止める．

```rust
pub fn grow() -> usize {
    let mut names = vec![String::from("a")];
    let first = &names[0];
    names.push(String::from("b"));
    first.len()
}
```

```text
error[E0502]: cannot borrow `names` as mutable because it is also borrowed as immutable
 --> src/lib.rs:4:5
  |
3 |     let first = &names[0];
  |                  ----- immutable borrow occurs here
4 |     names.push(String::from("b"));
  |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ mutable borrow occurs here
5 |     first.len()
  |     ----- immutable borrow later used here
```

## `String`と`&str`

| 型 | 意味 | 作り方 |
| --- | --- | --- |
| `String` | 文字列を所有する．伸ばしたり書き換えたりできる | `String::from("abc")`，`"abc".to_string()`，`format!(...)`，`String::new()` |
| `&str` | どこかにある文字列の一部を借りて読む | 文字列リテラル`"abc"`，`&s`(`s`は`String`)，`s.as_str()` |

読むだけの引数を`&str`にすると，`String`と文字列リテラルのどちらを渡しても呼べる．`&String`は`&str`として渡せる．
関数が文字列を作って返すときや，構造体に文字列を持たせるときは`String`にする．

```rust
pub fn shout(text: &str) -> String {
    let mut loud = text.to_ascii_uppercase();
    loud.push_str("!");
    loud
}

assert_eq!(shout("hi"), "HI!");
```

`String`を受け取る関数に`&str`を渡すと，型の不一致になる．

```text
error[E0308]: mismatched types
 --> src/lib.rs:6:10
  |
6 |     take("hello")
  |     ---- ^^^^^^^ expected `String`, found `&str`
```

`push_str`は`String`の末尾に文字列を足す．`format!`は，書式に従って新しい`String`を作る．

```rust
assert_eq!(format!("{}-{}", "a", 1), "a-1");
```

## `match`で所有する値を取り出す

所有する値に対して`match`すると，列挙子の中の値の所有権がパターンの変数に移る．
取り出した`String`を書き換えるには，パターンの変数に`mut`を付ける．

```rust
match (left, right) {
    (Value::Varchar(mut a), Value::Varchar(b)) => {
        a.push_str(&b);
        Ok(Value::Varchar(a))
    }
    ...
}
```

`a`の文字列をそのまま再利用するので，新しい`String`を作らずに済む．
一方，参照に対して`match`すると取り出せるのは参照なので，値を持ち続けるには`clone`する．
`ferrodb`の構文解析器は`&Token`から構文木を作るので，トークンの文字列を`clone`して構文木に持たせる．

## UTF-8と文字の数

Rustの文字列はUTF-8で符号化される．英数字は1バイト，日本語の多くの文字は3バイトである．

- `len()`はバイトの数を返す．
- `chars()`は文字(`char`)を1つずつ取り出す．`chars().count()`で文字の数を数えられる．

```rust
let s = "日本語";
assert_eq!(s.len(), 9);
assert_eq!(s.chars().count(), 3);
assert_eq!(&s[0..3], "日");
```

`&s[開始..終了]`は，バイトの位置で文字列の一部を切り出す．文字の途中の位置で切るとパニックする．

```text
end byte index 1 is not a char boundary; it is inside '日' (bytes 0..3 of string)
```

winnowの`offset()`はバイトの位置を返すので，文字の位置にするには，そこまでの部分を切り出して`chars().count()`で数える．
`offset()`は文字の境界を指すので，この切り出しはパニックしない．

## winnowで文字列リテラルを読む

- `none_of('\'')`は，`'`以外の1文字を読む．
- `repeat(0.., p)`で`char`を集めると，結果を`String`として受け取れる．

```rust
fn string(input: &mut &str) -> winnow::Result<Token> {
    delimited('\'', repeat(0.., string_char), '\'')
        .map(Token::String)
        .parse_next(input)
}

fn string_char(input: &mut &str) -> winnow::Result<char> {
    alt(("''".value('\''), none_of('\''))).parse_next(input)
}
```
