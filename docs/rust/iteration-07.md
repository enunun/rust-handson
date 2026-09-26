# Iteration 7：イテレーターとクロージャ

Iteration 7では，`SELECT`で列と式を選び，`WHERE`で行を絞り込む．
このノートでは，並びの要素を順に処理するイテレーターと，その場で書く小さな関数であるクロージャを説明する．

## イテレーター

イテレーターは，要素を1つずつ返す値である．`next()`を呼ぶたびに`Some(要素)`を返し，終わると`None`を返す．
`for`文は，内部でイテレーターの`next()`を呼んでいる．

```rust
let numbers = vec![1, 2, 3, 4];
let mut iter = numbers.iter();
assert_eq!(iter.next(), Some(&1));
assert_eq!(iter.next(), Some(&2));
```

`Vec`からイテレーターを作るメソッドは3つある．

| メソッド | 返す要素 | 元の`Vec` |
| --- | --- | --- |
| `iter()` | 要素への参照`&T` | そのまま使える |
| `iter_mut()` | 書き換えられる参照`&mut T` | そのまま使える |
| `into_iter()` | 要素そのもの`T` | 所有権が移り，使えなくなる |

範囲`0..n`(`n`を含まない)と`1..=n`(`n`を含む)もイテレーターである．

## クロージャ

クロージャは，`|引数| 式`と書く名前のない関数である．周りの変数を使える(捕捉する)．

```rust
let limit = 2;
let above: Vec<i64> = numbers.iter().copied().filter(|&n| n > limit).collect();
assert_eq!(above, vec![3, 4]);
```

- 引数の型と戻り値の型は，たいてい推論される．
- 本体が複数の文なら`|x| { ... }`と波括弧で囲む．
- 名前の付いた関数や，データを持つ列挙子もクロージャの代わりに渡せる(`.map(Some)`，`.map(BoundExpr::Column)`)．

## イテレーターのメソッド

イテレーターにメソッドをつないで，処理を組み立てる．

| メソッド | すること |
| --- | --- |
| `map(f)` | 各要素に`f`を適用する |
| `filter(p)` | `p`が真の要素だけを残す |
| `copied()` | `&T`の要素を`T`の値にする(`T`が`Copy`のとき) |
| `enumerate()`，`zip(other)` | 添字との組，別の並びとの組にする |
| `position(p)` | `p`が真になる最初の要素の添字を`Option`で返す |
| `any(p)`，`all(p)` | `p`が真の要素があるか，すべてが真かを返す |
| `collect()` | 要素を集めて`Vec`や`String`などを作る |

```rust
let doubled: Vec<i64> = numbers.iter().map(|n| n * 2).collect();
assert_eq!(doubled, vec![2, 4, 6, 8]);
assert_eq!(numbers.iter().position(|&n| n == 3), Some(2));
assert!(numbers.iter().any(|&n| n > 3));

let squares: Vec<i64> = (1..=3).map(|n| n * n).collect();
assert_eq!(squares, vec![1, 4, 9]);
```

`map`や`filter`は，新しいイテレーターを返すだけで，まだ何もしない．`collect`や`for`で要素を取り出したときに，はじめて計算する．

`|&n|`のように引数にパターンを書くと，参照を外して値を受け取れる．

`Vec`の`extend`は，イテレーターの要素をすべて末尾に加える．

## `collect`と`Result`

`Result`を返す`map`を`collect`すると，`Result<Vec<T>, E>`にまとめられる．
すべてが`Ok`なら`Ok(Vec)`になり，1つでも`Err`があれば最初の`Err`になる．そこで処理は止まる．

```rust
pub fn parse_all(texts: &[&str]) -> Result<Vec<i64>, std::num::ParseIntError> {
    texts.iter().map(|text| text.parse::<i64>()).collect()
}

assert_eq!(parse_all(&["1", "2"]), Ok(vec![1, 2]));
assert!(parse_all(&["1", "x", "3"]).is_err());
```

集める先の型は，変数の型や戻り値の型から推論される．推論できないときは`collect::<Result<Vec<_>, _>>()`のように書く．`_`は推論に任せる型である．

## `Option`のメソッド

`ok_or_else(f)`は，`Option`を`Result`に変える．`None`なら`f`が作った値を`Err`にする．

```rust
let found: Option<usize> = None;
assert_eq!(found.ok_or_else(|| "missing".to_string()), Err("missing".to_string()));
```

引数のないクロージャは`||`と書く．`ok_or_else`は，`None`のときだけエラーの値を作る．

## `for`とイテレーターの使い分け

- 要素ごとに値を作って集めるなら，`map`と`collect`が短く書ける．
- 途中で`continue`や`return`をしたい，複数の値を同時に書き換えたい，といった場合は`for`文のほうが読みやすい．

`ferrodb`の`SELECT`では，行ごとに条件を調べて`continue`するところを`for`で，選択項目ごとに値を計算するところを`map`と`collect`で書く．
