# Iteration 5：`HashMap`，構造体の所有，`for`のいろいろな形

Iteration 5では，表を作って行を挿入し，問い合わせる．
このノートでは，名前で値を引く`HashMap`，状態を持つ構造体の作り方，省略できるものを型で表す方法，`for`で添字や組を扱う方法を説明する．

## `HashMap`

`std::collections::HashMap<K, V>`は，キー`K`から値`V`を引く表である．

| 書き方 | すること |
| --- | --- |
| `HashMap::new()` | 空の表を作る |
| `map.insert(key, value)` | キーと値を加える．同じキーがあれば値を置き換える |
| `map.get(&key)` | 値への参照を`Option<&V>`で返す |
| `map.get_mut(&key)` | 書き換えられる参照を`Option<&mut V>`で返す |
| `map.contains_key(&key)` | キーがあるかを返す |
| `map[&key]` | 値への参照を返す．キーがなければパニックする |

`HashMap<String, V>`は，`&str`のキーでも引ける．

```rust
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct Inventory {
    counts: HashMap<String, i64>,
}

impl Inventory {
    pub fn new() -> Inventory {
        Inventory::default()
    }

    pub fn add(&mut self, item: &str, count: i64) {
        match self.counts.get_mut(item) {
            Some(current) => *current += count,
            None => {
                self.counts.insert(item.to_string(), count);
            }
        }
    }

    pub fn count(&self, item: &str) -> i64 {
        match self.counts.get(item) {
            Some(count) => *count,
            None => 0,
        }
    }
}
```

`*current += count`の`*`は，参照の先の値を書き換える．
Iteration 12では，この「なければ加え，あれば書き換える」をもっと短く書く`entry`を使う．

## `#[derive(Default)]`

`Default`は，型の「既定の値」を作るトレイトである．フィールドがすべて`Default`を実装していれば導出できる．`HashMap`の既定の値は空の表である．
引数のない`new`を持つ型に`Default`がないと，`cargo clippy`が`Default`の実装を求める．上の例のように`Default`を導出し，`new`から`Self::default()`を呼ぶ．

## 構造体が値を所有する

構造体のフィールドは，その値を所有する．`Database`は`Catalog`と行の表を所有し，`Catalog`は`TableSchema`を所有する．
所有する値を引数で受け取ると，その中身をそのまま別の構造体に移せる．複製は要らない．

```rust
fn create_table(&mut self, create: CreateTable) -> Result<StatementResult, Error> {
    let mut columns = Vec::new();
    for column in create.columns {
        columns.push(Column {
            name: column.name,
            data_type: column.data_type,
        });
    }
    ...
}
```

`for column in create.columns`は，`Vec`の所有権を受け取り，要素を1つずつ取り出す．借りるだけなら`for column in &create.columns`と書く．

## 省略できるものを型で表す

`INSERT INTO t (a, b) VALUES ...`の列の並びは省略できる．省略を`Option`で表す．

```rust
pub struct Insert {
    pub table: String,
    pub columns: Option<Vec<String>>,
    pub values: Values,
}
```

空の`Vec`で「省略」を表すこともできるが，それでは「省略した」と「空の並びを書いた」を区別できない．
`Option`にすると，`match`で`Some`と`None`の両方を扱うことをコンパイラーが求める．

## `match`のガード

パターンのあとに`if 条件`を書くと，条件を満たすときだけその腕に一致する．

```rust
pub fn describe(n: i64) -> &'static str {
    match n {
        0 => "zero",
        n if n < 0 => "negative",
        _ => "positive",
    }
}
```

`matches!(値, パターン)`は，値がパターンに一致するかを`bool`で返す．ガードも書ける．

```rust
let value: Option<i64> = Some(3);
assert!(matches!(value, Some(n) if n > 2));
```

## `for`で添字や組を扱う

`iter()`は`Vec`の要素を借りて順に返すイテレーターを作る．イテレーターにはメソッドで機能を足せる．

- `iter().enumerate()`は，添字と要素の組`(index, element)`を返す．
- `a.iter().zip(&b)`は，2つの並びの要素を先頭から組にして返す．短い方で終わる．

```rust
let names = vec!["a", "b", "c"];
let mut seen = Vec::new();
for (index, name) in names.iter().enumerate() {
    seen.push(format!("{index}:{name}"));
}
assert_eq!(seen, vec!["0:a", "1:b", "2:c"]);

let scores = vec![10, 20];
let mut pairs = Vec::new();
for (name, score) in names.iter().zip(&scores) {
    pairs.push(format!("{name}={score}"));
}
assert_eq!(pairs, vec!["a=10", "b=20"]);
```

イテレーターはIteration 7で詳しく扱う．

## スライスと`vec!`

- `&v[..i]`は，`v`の先頭から`i`個の要素を借りるスライスである．`&v[i..]`は`i`番目から最後まで．
- `vec![値; n]`は，同じ値を`n`個並べた`Vec`を作る．値は`Clone`で複製される．

```rust
assert_eq!(&names[..2], &["a", "b"]);
assert_eq!(vec![0; 3], vec![0, 0, 0]);
```

## `expect`

`Option`や`Result`の`expect(メッセージ)`は，`None`や`Err`のときにメッセージを付けてパニックする．
プログラムの作りから起こりえない場合にだけ使い，メッセージには「なぜ起こりえないか」を書く．

```rust
let table_rows = self
    .rows
    .get_mut(&insert.table)
    .expect("every table in the catalog has its rows");
```
