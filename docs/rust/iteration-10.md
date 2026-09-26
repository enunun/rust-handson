# Iteration 10：トレイトとトレイトオブジェクト

Iteration 10では，`SELECT`を演算子の木で実行する．演算子はどれも「次の1行を返す」という同じ操作を持ち，子の演算子の種類を知らずにつながる．
このノートでは，共通の操作をトレイトとして定義し，違う型の値を`Box<dyn トレイト>`で同じように扱う方法を説明する．

## トレイトを定義する

Iteration 4では，標準ライブラリのトレイト(`Display`，`From`)を自分の型に実装した．
トレイトは自分でも定義できる．`trait 名前 { ... }`の中に，実装する型が持つべきメソッドのシグネチャを並べる．

```rust
pub trait Source {
    fn next(&mut self) -> Option<i32>;
}

pub struct Numbers {
    current: i32,
    end: i32,
}

impl Numbers {
    pub fn new(end: i32) -> Numbers {
        Numbers { current: 1, end }
    }
}

impl Source for Numbers {
    fn next(&mut self) -> Option<i32> {
        if self.current > self.end {
            return None;
        }
        self.current += 1;
        Some(self.current - 1)
    }
}
```

`impl Source for Numbers`の中には，トレイトのすべてのメソッドを書く．書き忘れるとコンパイルエラーになる．

## トレイトオブジェクト

`dyn Source`は，「`Source`を実装する，何かの型の値」を表す型(トレイトオブジェクト)である．
実際の型によって大きさが違うので，`dyn Source`の値をそのまま変数や戻り値にはできない．

```rust
pub fn numbers(end: i32) -> dyn Source {
    Numbers { current: 1, end }
}
```

```text
error[E0746]: return type cannot be a trait object without pointer indirection
  --> src/lib.rs:20:29
   |
20 | pub fn numbers(end: i32) -> dyn Source {
   |                             ^^^^^^^^^^ doesn't have a size known at compile-time
```

`Box<dyn Source>`や`&mut dyn Source`のように，ポインターの向こうに置いて使う．`Box`の大きさは，中身の型によらず一定である．

次の`Doubled`と`OnlyOdd`は，子の`Source`を`Box<dyn Source>`で持つ．子は`Numbers`と`OnlyOdd`のどちらでもよく，同じフィールドに入る．

```rust
pub struct Doubled {
    input: Box<dyn Source>,
}

impl Source for Doubled {
    fn next(&mut self) -> Option<i32> {
        self.input.next().map(|n| n * 2)
    }
}

pub struct OnlyOdd {
    input: Box<dyn Source>,
}

impl Source for OnlyOdd {
    fn next(&mut self) -> Option<i32> {
        while let Some(n) = self.input.next() {
            if n % 2 != 0 {
                return Some(n);
            }
        }
        None
    }
}

pub fn collect_all(source: &mut dyn Source) -> Vec<i32> {
    let mut values = Vec::new();
    while let Some(n) = source.next() {
        values.push(n);
    }
    values
}

let mut doubled = Doubled {
    input: Box::new(OnlyOdd {
        input: Box::new(Numbers::new(5)),
    }),
};
assert_eq!(collect_all(&mut doubled), vec![2, 6, 10]);
```

- `Box::new(値)`は，値をヒープに置いた`Box`を作る．`Box<Numbers>`は`Box<dyn Source>`が必要な場所にそのまま渡せる．
- `Box<dyn Source>`から`&mut dyn Source`を得るには，`as_mut()`を使う(`collect_all(boxed.as_mut())`)．
- `Option`の`map`は，`Some`の中身にクロージャを適用し，`None`はそのまま返す．

## 静的ディスパッチと動的ディスパッチ

トレイトを実装する型を受け取る関数は，ジェネリクスでも書ける．

```rust
pub fn sum_all<S: Source>(mut source: S) -> i32 {
    let mut total = 0;
    while let Some(n) = source.next() {
        total += n;
    }
    total
}

assert_eq!(sum_all(Numbers::new(4)), 10);
```

| 書き方 | 呼ぶメソッドが決まる時点 | 性質 |
| --- | --- | --- |
| ジェネリクス`<S: Source>` | コンパイル時(静的ディスパッチ) | 型ごとに関数が作られる．呼び出しは速いが，1つの`Vec`や変数に違う型の値を混ぜられない |
| トレイトオブジェクト`dyn Source` | 実行時(動的ディスパッチ) | 値と一緒にメソッドの表(vtable)を持ち，実行時に引く．違う型の値を同じ型として扱える |

演算子の木は，SQLの文によって形が変わる．`Filter`の子が`SeqScan`か`Sort`かは実行時まで決まらないので，`Box<dyn Executor>`を使う．

## `while let`

`while let パターン = 式 { ... }`は，式がパターンに一致するあいだ本体をくり返す．
`while let Some(row) = input.next()? { ... }`は，`next`が`None`を返すまで1行ずつ処理する，という形でよく使う．

## `let ... else`

`let パターン = 式 else { ... };`は，式がパターンに一致すれば変数を束縛し，一致しなければ`else`の本体を実行する．
`else`の本体は，`return`，`panic!`などで必ず抜けなければならない．

```rust
enum Shape {
    Circle { radius: u32 },
    Square { side: u32 },
}

fn side(shape: Shape) -> u32 {
    let Shape::Square { side } = shape else {
        panic!("not a square");
    };
    side
}
```

テストで，入れ子になった列挙型の値を1段ずつ取り出して確かめるときに使える．

## イテレーターを構造体に持つ

`vec.into_iter()`が返すイテレーターの型は`std::vec::IntoIter<T>`である．構造体のフィールドに持てば，`next`を呼ぶたびに1つずつ取り出せる．

```rust
pub struct SeqScan {
    rows: std::vec::IntoIter<Row>,
}
```
