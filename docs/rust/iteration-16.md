# Iteration 16：トレイトの関連関数，イテレーターの実装，範囲

Iteration 16では，キーから行の位置を引くB+木を，バッファプールのページの上に作る．
このノートでは，キーの型をまとめるトレイトと関連関数，ライフタイムと型の引数を持つ構造体，`Iterator`の実装，範囲を受け取る`RangeBounds`，整列したスライスを調べるメソッドを説明する．

## トレイトの関連関数

トレイトには，`self`を受け取るメソッドのほかに，`self`を受け取らない関数(関連関数)も書ける．
関連関数で，バイト列や文字列から値を作る処理を型ごとに書ける．

```rust
pub trait Describe: Sized {
    fn describe(&self) -> String;
    fn parse(text: &str) -> Option<Self>;
}

impl Describe for i32 {
    fn describe(&self) -> String {
        format!("int {self}")
    }

    fn parse(text: &str) -> Option<i32> {
        text.strip_prefix("int ")?.parse().ok()
    }
}
```

- `Self`は，トレイトを実装する型を表す．`i32`の実装では`Self`が`i32`になる．
- トレイトは，自分で定義した型だけでなく，`i32`や`String`のような標準の型にも実装できる．
- 関連関数は`型::関数(...)`で呼ぶ．`i32::parse("int 7")`である．

型引数`T: Describe`の関数の中では，`T::parse`で，その型の関連関数を呼べる．

```rust
pub fn round_trip<T: Describe>(value: &T) -> Option<T> {
    T::parse(&value.describe())
}

assert_eq!(round_trip(&true), Some(true));
```

### `Sized`

`Option<Self>`を返すには，`Self`の大きさがコンパイル時に決まっている必要がある(`Sized`)．
トレイトの`Self`は，`dyn Describe`のように大きさの決まらない型にもなりうるので，何も書かないと次のエラーになる．

```text
error[E0277]: the size for values of type `Self` cannot be known at compilation time
 --> src/index/key.rs:9:32
  |
9 |     fn decode(bytes: &[u8]) -> Option<Self>;
  |                                ^^^^^^^^^^^^ doesn't have a size known at compile-time
  |
note: required by an implicit `Sized` bound in `Option`
 --> /rustc/48a229ceaefd4985c50990b14116b6d856af0985/library/core/src/option.rs:598:0
help: consider further restricting `Self`
  |
9 |     fn decode(bytes: &[u8]) -> Option<Self> where Self: Sized;
  |                                             +++++++++++++++++
```

`trait Describe: Sized`と書くと，大きさの決まった型だけがこのトレイトを実装できる．`: Sized`のように，トレイトの名前のあとに書いたトレイトを，スーパートレイトと呼ぶ．

## ビットの操作と符号の変換

| 書き方 | すること |
| --- | --- |
| `a ^ b` | ビットごとの排他的論理和．同じビットは0，違うビットは1になる |
| `n.cast_unsigned()` | 同じビットの並びのまま，符号なしの型にする(`i32`なら`u32`) |
| `n.cast_signed()` | 同じビットの並びのまま，符号ありの型にする |
| `n.to_be_bytes()` | ビッグエンディアン(上位のバイトが先)のバイトの配列にする |

```rust
assert_eq!((-1i32).cast_unsigned(), 0xffff_ffff);
assert_eq!(0x8000_0000u32.cast_signed(), i32::MIN);
assert_eq!(0b1100u8 ^ 0b1010, 0b0110);
assert_eq!(258u16.to_be_bytes(), [1, 2]);
```

`cast_unsigned`は値の範囲を調べない．`-1`は，ビットがすべて1の`u32`(4294967295)になる．範囲を調べる変換は`TryFrom`(Iteration 13)である．

## ライフタイムと型の引数を持つ構造体

構造体は，ライフタイムの引数と型の引数を両方持てる．ライフタイムの引数を先に書く．

```rust
pub struct BTree<'a, K: IndexKey> {
    pool: &'a BufferPool<Box<dyn DiskManager>>,
    key: PhantomData<K>,
}

impl<'a, K: IndexKey> BTree<'a, K> {
    pub fn create(pool: &'a BufferPool<Box<dyn DiskManager>>) -> Result<BTree<'a, K>, BTreeError> {
        // ...
    }
}
```

`BTree::<i32>::create(&pool)`のように，型の引数を`::<>`で指定して呼べる．`tree.insert(42, row_id)`のように，引数から推論できれば省ける．

### `PhantomData`

型の引数は，フィールドのどこかで使わなければならない．使わないと，次のエラーになる．

```text
error[E0392]: type parameter `K` is never used
  --> src/index/btree.rs:66:22
   |
66 | pub struct BTree<'a, K: IndexKey> {
   |                      ^ unused type parameter
   |
   = help: consider removing `K`, referring to it in a field, or using a marker such as `PhantomData`
```

`std::marker::PhantomData<T>`は，大きさが0で，型`T`を使っていることだけを表すフィールドの型である．値は`PhantomData`と書く．

```rust
pub struct Id<T> {
    value: u32,
    kind: PhantomData<T>,
}

impl<T> Id<T> {
    pub fn new(value: u32) -> Id<T> {
        Id { value, kind: PhantomData }
    }
}

pub fn find_user(id: Id<User>) -> u32 {
    id.value()
}
```

`Id<User>`と`Id<Order>`は，中身が同じ`u32`でも別の型なので，取り違えるとコンパイルエラーになる．

```text
error[E0308]: mismatched types
   --> src/lib.rs:155:15
    |
155 |     find_user(order)
    |     --------- ^^^^^ expected `Id<User>`, found `Id<Order>`
    |     |
    |     arguments to this function are incorrect
```

`BTree<'a, i32>`も，キーの型を値に持たないが，`insert`が`i32`のキーだけを受け取るようにできる．

## `Iterator`を実装する

`Iterator`トレイトを実装すると，`for`，`map`，`filter`，`collect`など，Iteration 7のイテレーターのメソッドがすべて使える．
実装するのは，返す値の型`Item`と，次の値を返す`next`だけである．

```rust
pub struct Countdown {
    remaining: u32,
}

impl Iterator for Countdown {
    type Item = u32;

    fn next(&mut self) -> Option<u32> {
        if self.remaining == 0 {
            return None;
        }
        self.remaining -= 1;
        Some(self.remaining + 1)
    }
}

let values: Vec<u32> = Countdown { remaining: 3 }.collect();
assert_eq!(values, vec![3, 2, 1]);
let total: u32 = Countdown { remaining: 4 }.filter(|n| n % 2 == 0).sum();
assert_eq!(total, 6);
```

- `type Item = u32;`は，トレイトの関連型である．トレイトを実装する側が，型を1つ決める．
- `next`は，値がなくなったら`None`を返す．
- `Self::Item`と書けば，`Item`に決めた型を指す．

途中で失敗しうるイテレーターは，`Item`を`Result`にする．`Result`のイテレーターを`collect`で`Result<Vec<_>, _>`に集めると，最初の`Err`で止まる(Iteration 7)．

```rust
let parsed: Result<Vec<i32>, _> = ["1", "2"].iter().map(|s| s.parse::<i32>()).collect();
assert_eq!(parsed, Ok(vec![1, 2]));
```

`Vec`の`into_iter()`が返す`std::vec::IntoIter<T>`もイテレーターの型である．構造体のフィールドに持てば，読み出した`Vec`の値を1つずつ返せる．

## 範囲と`RangeBounds`

`a..b`，`a..=b`，`a..`，`..b`，`..`は，それぞれ別の型の範囲である．
`std::ops::RangeBounds<T>`トレイトは，どの範囲も同じように扱う．

```rust
pub fn count_in(values: &[i32], bounds: impl RangeBounds<i32>) -> usize {
    values.iter().filter(|v| bounds.contains(v)).count()
}

let values = [1, 5, 10, 15];
assert_eq!(count_in(&values, 5..15), 2);
assert_eq!(count_in(&values, 5..=15), 3);
assert_eq!(count_in(&values, ..), 4);
assert_eq!(count_in(&values, (Bound::Excluded(5), Bound::Unbounded)), 2);
```

`start_bound()`と`end_bound()`は，範囲の始まりと終わりを`std::ops::Bound`で返す．

| `Bound` | 意味 |
| --- | --- |
| `Included(x)` | `x`を含む |
| `Excluded(x)` | `x`を含まない |
| `Unbounded` | 限りがない |

```rust
pub fn describe_start(bounds: impl RangeBounds<i32>) -> String {
    match bounds.start_bound() {
        Bound::Included(n) => format!(">= {n}"),
        Bound::Excluded(n) => format!("> {n}"),
        Bound::Unbounded => "any".to_string(),
    }
}

assert_eq!(describe_start(3..), ">= 3");
assert_eq!(Bound::Included(2).map(|n| n * 10), Bound::Included(20));
```

- `start_bound()`が返すのは`Bound<&T>`(値への参照)である．`Bound::map`で，中の値を変換した`Bound`を作れる．
- 始まりを含まない範囲の書き方はないので，`(Bound::Excluded(5), Bound::Unbounded)`のように`Bound`の組で書く．組も`RangeBounds`を実装する．

## 整列したスライスを調べる

| メソッド | すること |
| --- | --- |
| `partition_point(条件)` | 条件を満たす要素が前に，満たさない要素が後ろに並んでいるとき，境目の位置を返す |
| `binary_search(&x)` | `x`があれば`Ok(位置)`，なければ`Err(入れるべき位置)` |
| `split_off(n)` | `Vec`の`n`番目から後ろを切り取って返す．元の`Vec`には前の`n`個が残る |
| `windows(n)` | 隣り合う`n`個の要素のスライスを，1つずつずらして返す |

```rust
let sorted = [10, 20, 20, 30];
assert_eq!(sorted.partition_point(|&x| x < 20), 1);
assert_eq!(sorted.partition_point(|&x| x <= 20), 3);
assert_eq!(sorted.binary_search(&30), Ok(3));
assert_eq!(sorted.binary_search(&25), Err(3));
let mut items = vec![1, 2, 3, 4, 5];
let right = items.split_off(2);
assert_eq!(items, vec![1, 2]);
assert_eq!(right, vec![3, 4, 5]);
let pairs: Vec<i32> = [1, 4, 9].windows(2).map(|w| w[1] - w[0]).collect();
assert_eq!(pairs, vec![3, 5]);
```

`partition_point`と`binary_search`は，二分探索で位置を求める．要素が`n`個なら，およそ`log2(n)`回の比較で済む．
整列した`Vec`に値を入れるときは，`partition_point`で位置を求めて`insert`する．

## 借用が終わる時点

借用は，参照を最後に使った時点で終わる．
`BTree`はバッファプールを借りているが，`Drop`を実装していないので，最後に`tree`を使ったところで借用が終わる．そのあとは`drop(pool)`できる．

```rust
let pool = BufferPool::new(disk, 4);
let mut tree = BTree::<i32>::create(&pool)?;
tree.insert(1, row_id)?;
drop(pool); // treeはもう使わないので，poolを捨てられる
```

Iteration 15の`PageGuard`は`Drop`を実装するので，変数のスコープの終わりまで借用が続く．`Drop`が，捨てるときに借りたものを使うからである．
