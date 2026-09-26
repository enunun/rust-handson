# Iteration 8：要素の書き換えと借用の衝突

Iteration 8では，`UPDATE`と`DELETE`で表の行を書き換え，消す．制約の検査で値の重なりを調べる．
このノートでは，`Vec`の要素をその場で書き換えるメソッドと，書き換えの最中に同じ`Vec`を読もうとして起きる借用の衝突と，その避け方を説明する．
重なりを調べるための集合`HashSet`と，1つのパターンだけを調べる`if let`も扱う．

## `iter_mut`で要素を書き換える

`iter_mut()`は，要素への書き換えられる参照`&mut T`を返すイテレーターを作る．`*`で参照の先の値を書き換える．

```rust
let mut scores = vec![50, 80, 30];
for score in scores.iter_mut() {
    *score += 10;
}
assert_eq!(scores, vec![60, 90, 40]);
```

`zip`でほかの並びと組にすれば，要素ごとに別の値を書き込める．

## `retain`で要素を消す

`retain(p)`は，`p`が真を返す要素だけを残し，ほかを消す．残る要素の順序は変わらない．

```rust
scores.retain(|&score| score >= 50);
assert_eq!(scores, vec![60, 90]);
```

`retain`は，先頭の要素から順に1回ずつクロージャを呼ぶ．
そのため，要素ごとの真偽値を先に計算して並べておけば，イテレーターで1つずつ読みながら使える．

```rust
let mut names = vec!["alice", "bob", "carol"];
let mut keep = vec![true, false, true].into_iter();
names.retain(|_| keep.next().unwrap());
assert_eq!(names, vec!["alice", "carol"]);
```

このクロージャは，捕捉した変数`keep`を書き換える(`next`を呼ぶ)．`keep`は`mut`で宣言する．
`retain`のクロージャは`bool`を返すので，中で`Result`のエラーを返せない．エラーになりうる計算は，`retain`の前に済ませておく．

## 借用の衝突

Iteration 3で説明したとおり，ある値への`&mut`の借用がある間は，同じ値をほかの方法で借用できない．
`iter_mut()`のイテレーターは，`for`の間ずっと`Vec`を`&mut`で借用している．
そのため，ループの中で同じ`Vec`を読むと，コンパイルエラーになる．

次は，各点を「最高点との差」に書き換えようとした例である．

```rust
pub fn gaps() -> Vec<i32> {
    let mut scores = vec![50, 80, 30];
    for score in scores.iter_mut() {
        let best = *scores.iter().max().unwrap();
        *score = best - *score;
    }
    scores
}
```

```text
error[E0502]: cannot borrow `scores` as immutable because it is also borrowed as mutable
 --> src/lib.rs:4:21
  |
3 |     for score in scores.iter_mut() {
  |                  -----------------
  |                  |
  |                  mutable borrow occurs here
  |                  mutable borrow later used here
4 |         let best = *scores.iter().max().unwrap();
  |                     ^^^^^^ immutable borrow occurs here
```

`max`は，イテレーターの最大の要素を`Option`で返す．
このコードが通ると，ループの途中で書き換えた値が最高点の計算に混ざる．借用の規則は，この誤りをコンパイル時に防いでいる．

## 計算と変更を分ける

借用の衝突は，たいてい「読んで計算する段階」と「書き換える段階」を分けると解ける．
先に`&`の借用で必要な値を計算し，その借用が終わってから`&mut`で書き換える．

```rust
let mut scores = vec![50, 80, 30];
let best = *scores.iter().max().unwrap();
for score in scores.iter_mut() {
    *score = best - *score;
}
assert_eq!(scores, vec![30, 0, 50]);
```

計算の結果が要素ごとに違うなら，計算の段階で結果を別の`Vec`に集め，書き換えの段階で`zip`して書き込む．
計算の途中でエラーになっても，元の`Vec`はまだ変わっていない．
`ferrodb`の`UPDATE`と`DELETE`は，この形で書く．

構造体の異なるフィールドは，別々に借用できる．`self.catalog`を`&`で，`self.rows`を`&mut`で同時に借用してもよい．
ただし，`&mut self`を受け取るメソッドを呼ぶと，構造体全体を借用するので，ほかのフィールドの借用と衝突する．

## `if let`

`if let パターン = 式 { ... }`は，式がパターンに一致したときだけ本体を実行する．
一致しない場合に何もしないなら，`match`より短く書ける．

```rust
let changes = vec![Some(3), None, Some(4)];
let mut total = 0;
for change in changes {
    if let Some(n) = change {
        total += n;
    }
}
assert_eq!(total, 7);
```

`else`を付ければ，一致しなかったときの処理も書ける．

## `HashSet`

`std::collections::HashSet`は，重なりのない値の集合である．
`insert`は，値が集合になかったら加えて`true`を，すでにあったら`false`を返す．

```rust
use std::collections::HashSet;

let mut seen = HashSet::new();
assert!(seen.insert("alice"));
assert!(seen.insert("bob"));
assert!(!seen.insert("alice"));
assert!(seen.contains("bob"));
assert_eq!(seen.len(), 2);
```

`insert`の戻り値を見れば，並びの中に同じ値が2度現れたかを1回の走査で調べられる．

`HashSet`の要素と`HashMap`のキーの型は，`Eq`と`Hash`を実装していなければならない．
自分で定義した型は，`#[derive(PartialEq, Eq, Hash)]`で導出できる．フィールドの型もすべて`Eq`と`Hash`を実装している必要がある．

```rust
#[derive(Debug, PartialEq, Eq, Hash)]
enum Color {
    Red,
    Rgb(u8, u8, u8),
}

let mut colors = HashSet::new();
colors.insert(Color::Red);
colors.insert(Color::Rgb(255, 0, 0));
assert!(!colors.insert(Color::Red));
```

`ferrodb`の`Value`は，`i32`，`i64`，`bool`，`String`を持つ．どれも`Eq`と`Hash`を実装しているので，`Value`にも導出できる．

## `Vec`のメソッド

| メソッド | すること |
| --- | --- |
| `contains(&x)` | `x`と等しい要素があるかを返す(要素の型は`PartialEq`を実装する) |
| `extend(iter)` | イテレーターの要素をすべて末尾に加える |
| `to_vec()` | スライス`&[T]`を複製した`Vec<T>`を作る(`T`は`Clone`を実装する) |

```rust
let flags = vec!["NOT NULL", "UNIQUE"];
assert!(flags.contains(&"UNIQUE"));
```
