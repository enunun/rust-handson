# Iteration 12：値をキーにする`HashMap`とトレイトオブジェクトの集まり

Iteration 12では，`GROUP BY`で行をグループにまとめ，グループごとに集約関数を計算する．
グループは，キーの値の並び(`Vec<Value>`)を`HashMap`のキーにして引く．集約関数ごとの途中の結果は，同じトレイトを実装する違う型の値として並べる．
このノートでは，`HashMap`のキーになる条件と`entry` API，トレイトオブジェクトを作って返す関数を説明する．

## `HashMap`のキーになる型

`HashMap<K, V>`と`HashSet<K>`のキーの型`K`は，`Eq`と`Hash`を実装していなければならない．
`HashMap`は，キーの`hash`で値の置き場所を決め，同じ置き場所のキーを`==`で比べて探す．

そのため，2つの実装は次の約束を守らなければならない．

- `a == b`なら，`a`と`b`のハッシュ値は等しい．
- `==`は反射的である(`a == a`)．`Eq`は，この性質を持つことを表す印である．

`#[derive(PartialEq, Eq, Hash)]`で導出した実装は，フィールドをすべて比べ，すべてをハッシュ値に混ぜるので，約束を守る．
`Vec<T>`は，`T`が`Eq`と`Hash`を実装していれば，自分も実装する．
`ferrodb`の`Value`は導出しているので，`Vec<Value>`(`Row`)をそのままキーにできる．導出した`==`では`Value::Null == Value::Null`が真なので，`NULL`のキーは1つのグループになる．

等しさの規則を自分で決めたいときは，3つのトレイトを実装する．次は，大文字と小文字を区別しない名前である．

```rust
use std::hash::{Hash, Hasher};

#[derive(Debug)]
pub struct Name(pub String);

impl PartialEq for Name {
    fn eq(&self, other: &Name) -> bool {
        self.0.to_uppercase() == other.0.to_uppercase()
    }
}

impl Eq for Name {}

impl Hash for Name {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.to_uppercase().hash(state);
    }
}

let mut ages = HashMap::new();
ages.insert(Name("alice".to_string()), 30);
assert_eq!(ages.get(&Name("ALICE".to_string())), Some(&30));
```

- `struct Name(pub String)`は，名前のないフィールドを1つ持つ構造体(タプル構造体)である．フィールドは`self.0`で読む．
- `Eq`はメソッドを持たないので，`impl Eq for Name {}`と書くだけでよい．
- `hash`では，`==`で比べるときと同じ形(大文字にした文字列)をハッシュ値に混ぜる．元の文字列を混ぜると，`==`で等しい2つの名前のハッシュ値が違ってしまい，`HashMap`で見つからなくなる．

## `entry` API

`map.entry(key)`は，キーの場所を表す値を返す．キーの有無によらず，1回の探索で値を読み書きできる．

| メソッド | すること |
| --- | --- |
| `or_insert(値)` | キーがなければ値を入れる．どちらの場合も，値への`&mut`を返す |
| `or_insert_with(f)` | キーがなければ，クロージャ`f`が作った値を入れる．キーがあれば`f`を呼ばない |

```rust
let words = ["b", "a", "b", "c", "b"];
let mut counts: HashMap<&str, i32> = HashMap::new();
for word in words {
    *counts.entry(word).or_insert(0) += 1;
}
assert_eq!(counts["b"], 3);
```

`HashMap`は要素を並べる順序を決めない．最初に現れた順を保ちたいなら，値を`Vec`に置き，`HashMap`にはその添字を入れる．

```rust
let mut order: Vec<&str> = Vec::new();
let mut indexes: HashMap<&str, usize> = HashMap::new();
for word in words {
    let index = *indexes.entry(word).or_insert_with(|| {
        order.push(word);
        order.len() - 1
    });
    assert_eq!(order[index], word);
}
assert_eq!(order, vec!["b", "a", "c"]);
```

`or_insert_with`のクロージャは，初めてのキーのときだけ`order`に加えて，その添字を返す．

## トレイトオブジェクトを作って返す関数

トレイトを実装する違う型の値を，条件に応じて作り分ける関数は，`Box<dyn トレイト>`を返す．

```rust
pub trait Counter {
    fn add(&mut self, n: i64);
    fn finish(&self) -> i64;
}

#[derive(Default)]
struct Total {
    total: i64,
}

#[derive(Default)]
struct Largest {
    largest: i64,
}

// Total と Largest に Counter を実装する(省略)

pub fn counter(name: &str) -> Box<dyn Counter> {
    match name {
        "total" => Box::new(Total::default()),
        _ => Box::new(Largest::default()),
    }
}

let mut counters = [counter("total"), counter("largest")];
for n in [3, 7, 5] {
    for counter in counters.iter_mut() {
        counter.add(n);
    }
}
let results: Vec<i64> = counters.iter().map(|counter| counter.finish()).collect();
assert_eq!(results, vec![15, 7]);
```

- `match`の腕はどれも`Box<dyn Counter>`になるので，違う型の値を返せる．
- `[Box<dyn Counter>; 2]`の配列のように，違う型の値を1つの並びに置ける．`Vec<Box<dyn Counter>>`も同じである．`ferrodb`では，1つのグループの集約関数ごとの途中の結果をこの形で持つ．
- `#[derive(Default)]`で，フィールドをすべて既定値(0)にした値を`Total::default()`で作れる．

## `ok_or`

`Option`の`ok_or(エラー)`は，`None`を`Err(エラー)`に，`Some(値)`を`Ok(値)`にする．
`ok_or_else`と違い，エラーの値を先に作っておく．エラーの値が安く作れるなら，こちらが短く書ける．

```rust
assert_eq!(i64::MAX.checked_add(1).ok_or("overflow"), Err("overflow"));
assert_eq!(1i64.checked_add(1).ok_or("overflow"), Ok(2));
```

## 使わない引数

使わない引数の名前を`_`で始めると(`_value`)，使っていないという警告が出ない．
トレイトのメソッドの引数を，一部の実装だけが使わない場合(`COUNT`は値を見ないで数える)に便利である．
