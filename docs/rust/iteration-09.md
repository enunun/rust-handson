# Iteration 9：順序の実装と並べ替え

Iteration 9では，`ORDER BY`で行を並べ替え，`OFFSET`と`FETCH FIRST`で行の数を制限し，`DISTINCT`で重複を除く．
このノートでは，自分の型に順序を与える`Ord`と`PartialOrd`の実装と，比べ方を指定して並べ替える`sort_by`を説明する．

## `Ordering`

`std::cmp::Ordering`は，2つの値を比べた結果を表す列挙型である．列挙子は`Less`，`Equal`，`Greater`の3つである．
`a.cmp(&b)`は，`a`が`b`より小さければ`Less`を返す．

| メソッド | 返す値 |
| --- | --- |
| `reverse()` | `Less`と`Greater`を入れ替える |
| `then(other)` | 自分が`Equal`なら`other`，そうでなければ自分 |
| `then_with(f)` | 自分が`Equal`ならクロージャ`f`の結果，そうでなければ自分．`Equal`でなければ`f`を呼ばない |

```rust
use std::cmp::Ordering;

assert_eq!(1.cmp(&2), Ordering::Less);
assert_eq!(1.cmp(&2).reverse(), Ordering::Greater);
assert_eq!(Ordering::Equal.then(Ordering::Less), Ordering::Less);
assert_eq!(Ordering::Greater.then(Ordering::Less), Ordering::Greater);
```

`then`と`then_with`は，「1つ目の基準で決まらなければ2つ目の基準で比べる」という辞書式の比較を作る．

## `Ord`と`PartialOrd`を実装する

`<`や`>`で比べられる型は，`PartialOrd`を実装している．すべての値の組の大小が決まる型は，さらに`Ord`を実装する．
`Ord`は`cmp`メソッド，`PartialOrd`は`partial_cmp`メソッドを持つ．
`Ord`を実装する型は，`Eq`と`PartialOrd`も実装していなければならない．

```rust
#[derive(Debug, PartialEq, Eq)]
struct Version {
    major: u32,
    minor: u32,
}

impl Ord for Version {
    fn cmp(&self, other: &Version) -> Ordering {
        self.major
            .cmp(&other.major)
            .then_with(|| self.minor.cmp(&other.minor))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Version) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

let v1_2 = Version { major: 1, minor: 2 };
let v1_10 = Version { major: 1, minor: 10 };
assert!(v1_2 < v1_10);
assert_eq!(v1_2.max(v1_10), Version { major: 1, minor: 10 });
```

- `partial_cmp`は，`Ord`の`cmp`の結果を`Some`で包んで返すのが定石である．
- `Ord`を実装すると，`max`，`min`，`sort`などが使える．
- `#[derive(Debug, PartialEq)]`だけで`Ord`を実装すると，次のエラーになる．

```text
error[E0277]: the trait bound `Version: Eq` is not satisfied
 --> src/lib.rs:9:14
  |
9 | impl Ord for Version {
  |              ^^^^^^^ the trait `Eq` is not implemented for `Version`
  |
note: required by a bound in `Ord`
```

`cmp`は`==`と矛盾してはいけない．`a.cmp(&b)`が`Equal`を返すのは，`a == b`のときだけである．

`#[derive(PartialOrd, Ord)]`でも導出できる．導出した順序は，構造体ならフィールドを宣言の順に比べ，列挙型なら列挙子を宣言の順に並べる．
`ferrodb`の`Value`は，`INTEGER`と`BIGINT`を数として比べたいので，導出せずに`Ord`を実装する．

## `sort_by`

`Vec`(とスライス)の`sort_by`は，2つの要素を比べて`Ordering`を返すクロージャで並べ替える．
`sort`は要素の`Ord`で並べ替える．

```rust
let mut words = vec!["pear", "fig", "apple", "kiwi"];
words.sort_by(|a, b| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
assert_eq!(words, vec!["fig", "kiwi", "pear", "apple"]);

let mut numbers = vec![3, 1, 2];
numbers.sort_by(|a, b| b.cmp(a));
assert_eq!(numbers, vec![3, 2, 1]);
```

`sort_by`は安定な並べ替えである．比べて`Equal`になる要素は，元の順序を保つ．

```rust
let mut pairs = vec![("b", 1), ("a", 2), ("c", 1)];
pairs.sort_by(|x, y| x.1.cmp(&y.1));
assert_eq!(pairs, vec![("b", 1), ("c", 1), ("a", 2)]);
```

元の順序を保たなくてよければ，より速い`sort_unstable_by`も使える．

## `skip`と`take`

イテレーターの`skip(n)`は先頭の`n`個の要素を飛ばし，`take(n)`は先頭の`n`個までを返す．

```rust
let page: Vec<i32> = (1..=10).skip(3).take(2).collect();
assert_eq!(page, vec![4, 5]);
```

`usize::MAX`は`usize`の最大値である．`take(usize::MAX)`は，実際には制限しない．

## `contains`による重複の検査

`Vec`の`contains`は，等しい要素があるかを先頭から順に調べる．
すでに集めた要素に含まれていなければ加える，という形で重複を除ける．

```rust
let rows = vec![vec![1, 2], vec![1, 2], vec![3]];
let mut unique: Vec<Vec<i32>> = Vec::new();
for row in rows {
    if !unique.contains(&row) {
        unique.push(row);
    }
}
assert_eq!(unique, vec![vec![1, 2], vec![3]]);
```

要素の数を`n`とすると，比べる回数は最大で`n`の2乗に比例する．要素が多いなら，Iteration 8の`HashSet`を使うほうが速い．

## `Option`の既定値

`unwrap_or(値)`は，`None`なら引数の値を返す．`unwrap_or_default()`は，`None`なら型の既定値(`Default`)を返す．`Vec`の既定値は空の`Vec`である．

```rust
let keys: Option<Vec<i32>> = None;
assert_eq!(keys.unwrap_or_default(), Vec::<i32>::new());
assert_eq!(Some(3).unwrap_or(0), 3);
```
