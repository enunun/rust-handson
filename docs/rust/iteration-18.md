# Iteration 18：ニュータイプと，値を消費するメソッド

Iteration 18では，トランザクションを作り，タプルに作ったトランザクションと削除したトランザクションの番号を書く．
このノートでは，数を包んで別の型にするニュータイプ，`Copy`と`Clone`の違い，`self`を受け取って値を消費するメソッドによるAPIの設計，`enum`の状態を取り出して入れ替える`std::mem::take`を説明する．

## ニュータイプ

1つのフィールドだけを持つタプル構造体(Iteration 12)で，既存の型を包んだ新しい型をニュータイプと呼ぶ．

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Meters(pub u32);

impl Meters {
    pub const ZERO: Meters = Meters(0);

    pub fn plus(self, other: Meters) -> Meters {
        Meters(self.0 + other.0)
    }
}

assert_eq!(Meters(3).plus(Meters(3)), Meters(6));
assert_eq!(Meters::ZERO.0, 0);
assert!(Meters(1) < Meters(2));
```

- 中の値は`.0`で読む．
- `u32`とは別の型なので，トランザクションの番号(`TxnId`)をページの番号やスロットの番号と取り違えると，コンパイルエラーになる．
- 包むだけなので，実行時の大きさと速さは`u32`と同じである．
- `impl`の中の`const`は，型に結びついた定数(関連定数)である．`Meters::ZERO`のように型の名前で使う．`TxnId::INVALID`も関連定数である．

`derive`で，比較(`PartialOrd`，`Ord`)やハッシュ(`Hash`)を中の値と同じ順序と等しさで導出できる．

## `Copy`と`Clone`

| | `Clone` | `Copy` |
| --- | --- | --- |
| 複製の書き方 | `value.clone()`と明示する | 代入や引数で渡すと自動で複製される |
| 実装できる型 | どの型でも | すべてのフィールドが`Copy`の型だけ．`String`や`Vec`を持つ型にはできない |
| 使う場面 | 複製に手間がかかりうる値 | 小さく，複製しても意味の変わらない値(数，番号) |

`Copy`を導出するには，`Clone`も導出する．`Copy`の型の値は，代入してもムーブされず，元の変数を使い続けられる．

```rust
let a = Meters(3);
let b = a;          // aは複製され，使い続けられる
assert_eq!(a.plus(b), Meters(6));
```

`TxnId`は`Copy`にして，スナップショットやタプルのヘッダーに気軽に入れる．
トランザクションそのもの(`Transaction`)は，`Copy`と`Clone`のどちらも導出しない．複製できると，同じトランザクションを2回終えられてしまうからである．

## 値を消費するメソッド

`self`(参照でない)を受け取るメソッドは，呼ぶと値をムーブする．呼んだあとは，元の変数を使えない．

```rust
#[derive(Debug)]
pub struct Order {
    id: u32,
}

impl Order {
    pub fn close(self, ledger: &mut Ledger) {
        ledger.closed.push(self.id);
    }
}
```

同じ注文を2回閉じようとすると，コンパイルエラーになる．

```text
error[E0382]: use of moved value: `order`
  --> src/lib.rs:93:5
   |
91 |     let order = Order::new(7);
   |         ----- move occurs because `order` has type `Order`, which does not implement the `Copy` trait
92 |     order.close(&mut ledger);
   |           ------------------ `order` moved due to this method call
93 |     order.close(&mut ledger);
   |     ^^^^^ value used here after move
   |
note: `Order::close` takes ownership of the receiver `self`, which moves `order`
  --> src/lib.rs:27:18
   |
27 |     pub fn close(self, ledger: &mut Ledger) {
   |                  ^^^^
```

`Transaction::commit(self, manager)`と`Transaction::rollback(self, manager)`も同じ形である．コミットしたトランザクションで文を実行したり，もう一度中止したりする誤りを，実行する前にコンパイラーが見つける．
状態を表す値そのものを消費することで，「一度しかできない操作」を型で表せる．

## `std::mem::take`で状態を入れ替える

`&mut`で借りた値から，中身をムーブして取り出したいことがある．借りた値からは，代わりの値を置かずにムーブできない．
`std::mem::take(&mut 値)`は，中身を取り出し，代わりに`Default`の値を置く(`Option::take`(Iteration 11)の一般形である)．

```rust
#[derive(Debug, Default, PartialEq)]
pub enum Door {
    #[default]
    Closed,
    Open(String),
}

pub fn visit(door: &mut Door) -> String {
    match std::mem::take(door) {
        Door::Closed => {
            *door = Door::Open("guest".to_string());
            "opened".to_string()
        }
        Door::Open(name) => format!("{name} was inside"),
    }
}
```

- `enum`で`#[derive(Default)]`を使うときは，既定の列挙子の前に`#[default]`を書く．
- `take`で取り出した列挙子の中身(`name`)は，ムーブして使える．腕の中で，次の状態を`*door`に入れる．入れなければ，`Default`の値のままになる．

`Default`を実装しない型には使えない．

```text
error[E0277]: the trait bound `Door: Default` is not satisfied
  --> src/lib.rs:39:26
   |
39 |     match std::mem::take(door) {
   |           -------------- ^^^^ the trait `Default` is not implemented for `Door`
   |           |
   |           required by a bound introduced by this call
```

`Database::execute`は，セッションの状態(`Session::InTransaction(Transaction)`など)を`take`で取り出し，中の`Transaction`を`commit`で消費するか，次の状態に入れ直す．

## `Result`をつなぐ`map_err`と`and_then`

| メソッド | すること |
| --- | --- |
| `map_err(関数)` | `Err`の値を関数で変換する．`Ok`はそのまま |
| `and_then(関数)` | `Ok`なら，値を関数に渡して，関数が返す`Result`にする．`Err`はそのまま |

```rust
let statement = tokenize(sql)
    .map_err(Error::from)
    .and_then(|tokens| Ok(parse(&tokens)?));
```

`?`で関数を抜ける代わりに，結果を`Result`のまま変数に入れる．失敗したトランザクションの中では，構文のエラーもトランザクションを失敗させるので，エラーのときにも状態を入れ替えてから返す必要がある．

## 値を捕まえるクロージャを渡す

Iteration 15の`impl Fn`の引数には，外の変数を使うクロージャも渡せる．

```rust
pub fn count_matching(values: &[u32], keep: impl Fn(&u32) -> bool) -> usize {
    values.iter().filter(|v| keep(v)).count()
}

let limit = 5;
assert_eq!(count_matching(&[1, 6, 9], |v| *v > limit), 2);
```

`HeapFile::rows(schema, visible)`は，版のヘッダーを受け取って`bool`を返す`visible`で，返す版を選ぶ．
呼び出し側は，`|header| is_visible(header, snapshot, manager)`のように，スナップショットを捕まえたクロージャを渡す．`HeapFile`は，トランザクションやスナップショットを知らなくてよい．
