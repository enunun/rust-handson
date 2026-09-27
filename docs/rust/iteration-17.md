# Iteration 17：`enum`による静的ディスパッチと`Bound`

Iteration 17では，Iteration 16のB+木を表のインデックスとして使う．列の型ごとに違う`BTree<i32>`，`BTree<String>`などを，1つの型にまとめて扱う．
このノートでは，型の違う値を`enum`でまとめる方法とトレイトオブジェクトとの違い，組の`match`，`Bound`の操作，構造体のフィールドを別々に借りる方法を説明する．

## `enum`でまとめる

`BTree<i32>`と`BTree<String>`は別の型なので，1つの`Vec`や`HashMap`に入れられない．
型の数が決まっているなら，列挙子ごとに1つの型を持つ`enum`でまとめられる．

```rust
pub enum Column {
    Ints(Vec<i32>),
    Texts(Vec<String>),
}

impl Column {
    pub fn push(&mut self, value: &Value) -> Result<(), String> {
        match (self, value) {
            (Column::Ints(values), Value::Int(n)) => values.push(*n),
            (Column::Texts(values), Value::Text(s)) => values.push(s.clone()),
            (_, other) => return Err(format!("{other:?} does not fit")),
        }
        Ok(())
    }

    pub fn count(&self) -> usize {
        match self {
            Column::Ints(values) => values.len(),
            Column::Texts(values) => values.len(),
        }
    }
}
```

- `match (self, value)`は，2つの値の組を1つの`match`で調べる．`self`の列挙子と`value`の列挙子の組み合わせごとに腕を書ける．
- `self`は`&mut Column`なので，`Column::Ints(values)`の`values`は`&mut Vec<i32>`になる．参照に`match`すると，中の値も参照で取り出される(Iteration 5)．
- `(_, other)`は，それ以外のすべての組み合わせである．

### トレイトオブジェクトとの比較

同じことは，トレイトとトレイトオブジェクト(Iteration 10)でも書ける．

```rust
pub trait Storage {
    fn push(&mut self, value: &Value) -> Result<(), String>;
    fn count(&self) -> usize;
}

pub struct IntStorage(Vec<i32>);

impl Storage for IntStorage {
    fn push(&mut self, value: &Value) -> Result<(), String> {
        match value {
            Value::Int(n) => {
                self.0.push(*n);
                Ok(())
            }
            other => Err(format!("{other:?} does not fit")),
        }
    }

    fn count(&self) -> usize {
        self.0.len()
    }
}

let mut columns: Vec<Box<dyn Storage>> = vec![Box::new(IntStorage(Vec::new()))];
columns[0].push(&Value::Int(1)).unwrap();
```

| | `enum` | トレイトオブジェクト(`Box<dyn Trait>`) |
| --- | --- | --- |
| 型を加える | `enum`に列挙子を加え，すべての`match`を直す | 新しい型にトレイトを実装するだけ．元のコードは変えない |
| 操作を加える | `match`を書いたメソッドを1つ加える | トレイトにメソッドを加え，すべての実装を直す |
| 呼び出し | `match`で列挙子を選ぶ(静的ディスパッチ) | 実行時にメソッドの表から選ぶ(動的ディスパッチ) |
| 値の置き場所 | `enum`の値そのもの．`Box`は要らない | 大きさが決まらないので，`Box`などの参照の先に置く |
| 型の引数 | 列挙子ごとに`BTree<i32>`のように決めた型を持てる | ジェネリックなメソッドを持つトレイトは`dyn`にできない |

インデックスのキーの型は，SQLの型(`INTEGER`，`BIGINT`，`BOOLEAN`，`VARCHAR`)の4つに決まっている．型が増えることは少なく，SQLの値の型と組にして`match`で選びたいので，`enum AnyIndex`にする．
Iteration 10の演算子は，これからも種類が増え，どれも`next`だけを持つので，トレイトオブジェクトにした．

## `unreachable!`で起こらない組み合わせを表す

`AnyIndex`に渡す値は，前もって`Column::assign`で列の型に合わせてある．型の合わない組み合わせは起こらないので，`unreachable!`(Iteration 11)で表す．

```rust
match (self, value) {
    (_, Value::Null) => Ok(()),
    (AnyIndex::Integer(tree), Value::Integer(n)) => tree.insert(*n, id),
    // ...
    (_, value) => unreachable!("{value:?} does not match the type of the index"),
}
```

呼び出し側の誤りで起きた場合は，パニックして誤りを知らせる．エラーとして返すと，すべての呼び出し側が起こらないエラーを扱うことになる．

## `Bound`を操作する

Iteration 16の`Bound`には，中の値を変換するメソッドがある．

| メソッド | すること |
| --- | --- |
| `as_ref()` | `Bound<T>`を，値を借りた`Bound<&T>`にする |
| `map(関数)` | 中の値を関数で変換する．`Unbounded`はそのまま |
| `cloned()` | `Bound<&T>`の値を複製して`Bound<T>`にする |

```rust
let owned: Bound<i32> = Bound::Excluded(5);
let borrowed: Bound<&i32> = owned.as_ref();
assert_eq!(borrowed.cloned(), Bound::Excluded(5));
assert_eq!(Bound::Included(2).map(|n| n * 3), Bound::Included(6));
```

実行計画は`Bound<Value>`を持ち，インデックスには`as_ref()`で`Bound<&Value>`として渡す．インデックスは`map`で，`Value`を列の型のキー(`i32`など)にする．

### 同じ名前を束縛する`|`のパターン

`|`でつないだパターンは，どの選択肢でも同じ名前と型の変数を束縛するなら，腕の中でその変数を使える．

```rust
pub fn lower_value(bound: Bound<&i32>) -> Option<i32> {
    match bound {
        Bound::Included(n) | Bound::Excluded(n) => Some(*n),
        Bound::Unbounded => None,
    }
}
```

`Included`と`Excluded`のどちらでも，中の値を`n`として比べられる．

## フィールドを別々に借りる

構造体の別々のフィールドは，同時に別々に借りられる．1つを`&mut`で，もう1つを`&`で借りてもよい．

```rust
pub fn bump(&mut self, name: &str) {
    let counts = &mut self.counts;
    let names = &self.names;
    counts[names[name]] += 1;
}
```

`&self`を受け取るメソッドを呼ぶと，構造体全体を借りる．フィールドを`&mut`で借りている間は呼べない．

```text
error[E0502]: cannot borrow `*self` as immutable because it is also borrowed as mutable
   --> src/database.rs:232:27
    |
228 |           let heap = self
    |  ____________________-
229 | |             .tables
    | |___________________- mutable borrow occurs here
...
232 |           let mut indexes = self.open_indexes(schema)?;
    |                             ^^^^ immutable borrow occurs here
233 |           let count = dml::insert(schema, heap, &mut indexes, new_rows)?;
    |                                           ---- mutable borrow later used here
```

必要なフィールドだけを引数に取る関数を作れば，メソッドと違って別々に借りられる．

```rust
let heap = self.tables.get_mut(&insert.table).expect("...");
let mut indexes = open_indexes(&self.catalog, &self.indexes, schema)?;
```

`open_indexes`は`self.indexes`だけを借りるので，`self.tables`を`&mut`で借りたままでも呼べる．

## 複数の参照を受け取る関数のライフタイム

参照の引数が2つ以上あり，戻り値が参照を含むなら，戻り値がどの引数を借りるかをライフタイム注釈で書く(Iteration 15)．

```rust
fn open_indexes<'a>(
    catalog: &Catalog,
    pools: &'a HashMap<String, BufferPool<Box<dyn DiskManager>>>,
    schema: &TableSchema,
) -> Result<Vec<ColumnIndex<'a>>, Error> {
    // ...
}
```

戻り値の`ColumnIndex<'a>`はバッファプールを借りるので，`pools`と同じ`'a`を付ける．`catalog`と`schema`の借用は，関数が返ったら終わる．
