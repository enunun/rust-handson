# Iteration 13：バイト列を扱う

Iteration 13では，行をバイト列(タプル)に符号化し，8192バイトのページに置く．
このノートでは，決まった大きさのバイト列を表す配列，数とバイト列の変換，失敗しうる型の変換を説明する．

## 固定長配列とスライス

`[T; N]`は，`T`の値を`N`個並べた配列の型である．`N`はコンパイル時に決まる．
`[値; N]`は，同じ値を`N`個並べた配列を作る．

```rust
const SIZE: usize = 8;

let mut buffer = [0u8; SIZE];
buffer[2..4].copy_from_slice(&[7, 9]);
assert_eq!(buffer, [0, 0, 7, 9, 0, 0, 0, 0]);
let slice: &[u8] = &buffer[2..4];
assert_eq!(slice.len(), 2);
```

- `buffer[2..4]`は，配列の一部を指すスライスである．`&[u8]`は長さを実行時に持ち，配列の`[u8; 8]`は長さを型に持つ．
- `copy_from_slice`は，同じ長さのスライスの値を書き写す．長さが違うと`panic!`になる．
- `[値; N]`の値は`N`回複製されるので，`Copy`を実装する型でなければならない．`Copy`でない値を並べるには`vec![値; N]`を使う．

8192バイトの配列を構造体に何度も持たせると，値をムーブするたびに8192バイトを書き写す．`Box<[u8; 8192]>`にすれば，配列はヒープに置かれ，ムーブするのはポインターだけになる．

```rust
let boxed: Box<[u8; SIZE]> = Box::new([1; SIZE]);
assert_eq!(boxed[7], 1);
```

## `const`

`const 名前: 型 = 値;`は，コンパイル時に決まる定数である．配列の長さにも使える．
定数の名前は大文字で書く(`PAGE_SIZE`)．式の中で使うと，その値に置き換わる．

```rust
pub const PAGE_SIZE: usize = 8192;
pub const MAX_TUPLE_SIZE: usize = PAGE_SIZE - HEADER_SIZE - SLOT_SIZE;
```

定数の値には，ほかの定数を使った式を書ける．

## 数とバイト列

整数型は，バイト列との変換のメソッドを持つ．

| メソッド | すること |
| --- | --- |
| `n.to_le_bytes()` | リトルエンディアン(下位のバイトが先)のバイトの配列にする |
| `n.to_be_bytes()` | ビッグエンディアン(上位のバイトが先)のバイトの配列にする |
| `u16::from_le_bytes(配列)` | リトルエンディアンのバイトの配列から数を作る |

```rust
assert_eq!(258u16.to_le_bytes(), [2, 1]);
assert_eq!(258u16.to_be_bytes(), [1, 2]);
assert_eq!(u16::from_le_bytes([2, 1]), 258);
assert_eq!((-2i32).to_le_bytes(), [0xfe, 0xff, 0xff, 0xff]);
```

負の数は2の補数で表す．`from_le_bytes`が受け取るのはスライスでなく，長さの決まった配列である．

## `TryFrom`と`TryInto`

失敗しうる型の変換は，`TryFrom`トレイトで表す．`T::try_from(値)`は`Result`を返す．
`try_into()`は同じ変換を，変換先の型を推論させて書く形である．

```rust
let bytes = [1u8, 0, 0, 0, 9];
let array: [u8; 4] = bytes[..4].try_into().unwrap();
assert_eq!(i32::from_le_bytes(array), 1);
assert!(<[u8; 4]>::try_from(&bytes[..3]).is_err());
assert!(u16::try_from(70_000usize).is_err());
assert_eq!(u8::try_from(200i32), Ok(200u8));
```

- スライスから配列への変換は，長さが合わなければ失敗する．
- 大きな整数型から小さな整数型への変換は，範囲に収まらなければ失敗する．Iteration 1の`i32::try_from(n)`も同じトレイトである．
- 失敗しない変換(`u16`から`usize`など)は`From`で，`usize::from(n)`と書く．

## `split_at`

スライスの`split_at(n)`は，先頭の`n`個と残りの2つのスライスに分ける．
バイト列の先頭から，決まった長さの値を順に読み取るときに使う．

```rust
let bytes = [1, 2, 3, 4, 5];
let (head, rest) = bytes.split_at(2);
assert_eq!(head, [1, 2]);
assert_eq!(rest, [3, 4, 5]);
```

`n`がスライスの長さより大きいと`panic!`になる．壊れたデータを読むときは，先に長さを調べる．

## ビットの操作

| 演算子 | すること |
| --- | --- |
| `a & b` | ビットごとの論理積 |
| `a \| b` | ビットごとの論理和．`a \|= b`は`a = a \| b` |
| `1 << n` | `n`ビット目だけが1の数 |

```rust
let mut bitmap = [0u8; 2];
let index = 9;
bitmap[index / 8] |= 1 << (index % 8);
assert_eq!(bitmap, [0, 0b0000_0010]);
assert!(bitmap[1] & (1 << 1) != 0);
assert_eq!(9usize.div_ceil(8), 2);
```

- `0b0000_0010`は2進数のリテラルである．`_`は読みやすくするための区切りで，値に影響しない．
- `div_ceil`は，割り算の結果を切り上げる．9列のビットマップは2バイトになる．

## バイト文字列

`b"abc"`は，ASCIIの文字のバイトの配列`&[u8; 3]`を表すリテラルである．
`str`の`as_bytes()`は，文字列のUTF-8のバイト列を返す．

```rust
let text: &[u8; 3] = b"abc";
assert_eq!(text, &[97, 98, 99]);
assert_eq!("日本".as_bytes().len(), 6);
```

## `Debug`を自分で実装する

`#[derive(Debug)]`はすべてのフィールドを書く．8192バイトの配列を持つ構造体では，読める長さにならない．
`Debug`を自分で実装すれば，書くものを選べる．`debug_struct`は，`Page { slot_count: 2, ... }`の形の出力を作る．

```rust
impl fmt::Debug for Page {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Page")
            .field("slot_count", &self.slot_count())
            .field("free_space", &self.free_space())
            .finish()
    }
}
```
