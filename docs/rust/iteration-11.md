# Iteration 11：状態を持つ演算子と`Option`の操作

Iteration 11では，2つの表を結合する演算子`NestedLoopJoin`を作る．
この演算子は，子の演算子を2つ持ち，外側の今の行，内側の行の並び，内側のどこまで読んだかを状態として持つ．
このノートでは，状態を持つ構造体を書くときに使う`Option`の操作と，スライスのパターンなどを説明する．

## トレイトオブジェクトを組み合わせる構造体

構造体は，トレイトオブジェクトをいくつでもフィールドに持てる．

```rust
pub struct NestedLoopJoin {
    left: Box<dyn Executor>,
    right: Box<dyn Executor>,
    // ...
}
```

`left`と`right`には，`SeqScan`でも別の`NestedLoopJoin`でも入る．
`NestedLoopJoin`自身も`Executor`を実装するので，結合を結合の子にして，3つ以上の表をつなげる．

## `Option::take`と`Option::replace`

`take`は，`Option`の中身を取り出して返し，元の場所を`None`にする．
`replace(値)`は，新しい値を入れ，前の中身を返す．

```rust
let mut current = Some(3);
assert_eq!(current.take(), Some(3));
assert_eq!(current, None);
assert_eq!(current.take(), None);

let mut current = Some(1);
assert_eq!(current.replace(2), Some(1));
assert_eq!(current, Some(2));
```

構造体のフィールドにある値を，所有権ごと取り出したいときに使う．
`&mut self`のメソッドの中では，`self.current`をそのままムーブできない(フィールドが空になってしまう)．
`self.current.take()`なら，フィールドに`None`を残して，中身の所有権を得られる．

## スライスのパターン

`match`で，スライスの要素の数と中身を調べられる．

```rust
pub fn describe(found: &[usize]) -> String {
    match found {
        [] => "none".to_string(),
        [index] => format!("one at {index}"),
        [first, ..] => format!("many, first at {first}"),
    }
}

assert_eq!(describe(&[]), "none");
assert_eq!(describe(&[4]), "one at 4");
assert_eq!(describe(&[2, 5]), "many, first at 2");
```

- `[]`は空，`[index]`は要素が1つのスライスに一致する．
- `..`は，残りの0個以上の要素を表す．
- `Vec`は`as_slice()`でスライスにしてから`match`する．

名前解決では，名前に一致した列の番号を集め，0個ならない列，1個なら解決，2個以上ならあいまい，と分ける．

## `fold`

イテレーターの`fold(初期値, f)`は，初期値から始めて，要素を1つずつクロージャ`f`で畳み込む．
`reduce(f)`は，最初の要素を初期値にして同じように畳み込み，結果を`Option`で返す．要素がなければ`None`である．

```rust
let total = [1, 2, 3].iter().fold(10, |sum, n| sum + n);
assert_eq!(total, 16);

let text = ["a", "b", "c"]
    .into_iter()
    .fold(String::from("x"), |left, right| format!("({left} {right})"));
assert_eq!(text, "(((x a) b) c)");
```

2つ目の例のように，左から順に入れ子にしていくので，左結合の木を作るときに使える．
`FROM a JOIN b ON ... JOIN c ON ...`は，`a`から始めて`b`，`c`を順に結合する．

## `self`を受け取るメソッド

メソッドの最初の引数を`self`(や`mut self`)にすると，値の所有権を受け取る．呼んだあと，元の変数は使えない．

```rust
#[derive(Debug, Default, PartialEq)]
pub struct Path {
    parts: Vec<String>,
}

impl Path {
    pub fn join(mut self, other: Path) -> Path {
        self.parts.extend(other.parts);
        self
    }
}

let a = Path { parts: vec!["usr".to_string()] };
let b = Path { parts: vec!["bin".to_string()] };
assert_eq!(a.join(b).parts, vec!["usr", "bin"]);
```

受け取った値を作り変えて返すので，複製せずに済む．名前解決の`Scope::join`は，この形で2つの列の並びをつなぐ．

## `Vec`の末尾に加える

| メソッド | すること |
| --- | --- |
| `extend_from_slice(&[T])` | スライスの要素を複製して末尾に加える(`T`は`Clone`を実装する) |
| `extend(iter)` | イテレーターの要素を末尾に加える |

`std::iter::repeat_n(値, n)`は，同じ値を`n`回返すイテレーターである．

```rust
let mut row = vec![1, 2];
row.extend_from_slice(&[3, 4]);
row.extend(std::iter::repeat_n(0, 2));
assert_eq!(row, vec![1, 2, 3, 4, 0, 0]);
```

## `Option<String>`から`&String`を得る

`Option::as_ref()`は，`&Option<T>`を`Option<&T>`にする．中身を借用したまま，`unwrap_or`などで既定の値と比べられる．

```rust
let name = "EMP".to_string();
let alias: Option<String> = Some("E".to_string());
assert_eq!(alias.as_ref().unwrap_or(&name), "E");
```

## `unreachable!`

`unreachable!("理由")`は，そこに来ないはずの場所に書く．来てしまったら`panic!`と同じくプログラムを止める．
直前の処理で必ず値を入れた`Option`を取り出すときなど，型では表せない前提を書き残すために使う．
