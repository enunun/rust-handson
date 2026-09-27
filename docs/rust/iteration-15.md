# Iteration 15：ライフタイム，`Drop`，内部可変性

Iteration 15では，ディスクのページをメモリーの枠に置いて使い回すバッファプールを作る．
ページを使っている間はピン留めし，使い終わったら自動でピンを外す．
このノートでは，参照を持つ構造体とライフタイム注釈，値を捨てるときに走る`Drop`，`&self`のメソッドで状態を書き換える`Cell`と`RefCell`，トレイト境界を持つジェネリックな構造体を説明する．

## ライフタイム

参照は，指す値が生きている間だけ使える．参照が使える期間をライフタイムと呼ぶ．
コンパイラーは，参照が指す値より長く使われていないかを調べる(借用検査)．

```rust
pub fn pick() -> usize {
    let outer = String::from("outer");
    let result;
    {
        let inner = String::from("inner text");
        result = longer(&outer, &inner);
    }
    result.len()
}
```

```text
error[E0597]: `inner` does not live long enough
  --> src/lib.rs:10:33
   |
 9 |         let inner = String::from("inner text");
   |             ----- binding `inner` declared here
10 |         result = longer(&outer, &inner);
   |                                 ^^^^^^ borrowed value does not live long enough
11 |     }
   |     - `inner` dropped here while still borrowed
12 |     result.len()
   |     ------ borrow later used here
```

`inner`は内側のブロックの終わりで捨てられる．`longer`の戻り値は`inner`を指しうるので，ブロックの外で使えない．

### ライフタイム注釈

関数が参照を返すとき，戻り値がどの引数を借りているかを，ライフタイム注釈`'a`で書く．

```rust
pub fn longer<'a>(a: &'a str, b: &'a str) -> &'a str {
    if a.len() >= b.len() { a } else { b }
}
```

- `<'a>`はライフタイムの引数である．型の引数`<T>`と同じく，関数の名前のあとに書く．
- `&'a str`は，「ライフタイム`'a`の間は有効な`&str`」である．
- 戻り値の`&'a str`は，`a`と`b`のどちらかを借りていて，両方が有効な間だけ使えることを表す．

注釈は参照の有効な期間を変えない．呼び出し側で借用を正しく検査するための，関数の約束である．
注釈を書かないと，戻り値がどちらを借りているのか決められず，エラーになる．

```text
error[E0106]: missing lifetime specifier
 --> src/lib.rs:5:36
  |
5 | pub fn longer(a: &str, b: &str) -> &str {
  |                  ----     ----     ^ expected named lifetime parameter
  |
  = help: this function's return type contains a borrowed value, but the signature does not say whether it is borrowed from `a` or `b`
help: consider introducing a named lifetime parameter
  |
5 | pub fn longer<'a>(a: &'a str, b: &'a str) -> &'a str {
  |              ++++     ++          ++          ++
```

### 注釈を省ける場合

次の場合は，コンパイラーがライフタイムを決めるので，注釈を省ける(ライフタイムの省略)．

1. 参照の引数が1つなら，戻り値の参照はその引数を借りる．
2. `&self`か`&mut self`を受け取るメソッドなら，戻り値の参照は`self`を借りる．

```rust
pub fn first(items: &[i32]) -> &i32 {
    &items[0]
}

impl Holder {
    pub fn value(&self) -> &str {
        &self.value
    }
}
```

これまでの関数とメソッドは，どれもこの規則で決まる形だった．
Iteration 0のwinnowのパーサーが受け取る`&mut &str`は，1つの引数に2つのライフタイム(`&mut`と`&str`)を持つ．戻り値に借用を含めると，どちらのライフタイムか決まらない．

### 参照を持つ構造体

構造体のフィールドに参照を置くときは，構造体にライフタイムの引数を付ける．

```rust
pub struct View<'a> {
    text: &'a str,
}

impl<'a> View<'a> {
    pub fn new(text: &'a str) -> View<'a> {
        View { text }
    }

    pub fn first_word(&self) -> &'a str {
        self.text.split(' ').next().unwrap_or("")
    }
}
```

- `View<'a>`の値は，`text`が指す文字列より長く生きられない．
- `impl<'a> View<'a>`は，どのライフタイムの`View`にもメソッドを定義する．
- `first_word`は`&self`を受け取るが，戻り値を`&'a str`と書いた．戻り値は`View`でなく元の文字列を借りるので，`View`を捨てたあとも使える．

注釈を書かないと，次のエラーになる．

```text
error[E0106]: missing lifetime specifier
 --> src/lib.rs:2:11
  |
2 |     text: &str,
  |           ^ expected named lifetime parameter
  |
help: consider introducing a named lifetime parameter
  |
1 ~ pub struct View<'a> {
2 ~     text: &'a str,
  |
```

### `'_`と`'static`

- `'_`は，省略の規則で決まるライフタイムをそのまま使うと示す書き方である．`impl Drop for Noisy<'_>`や，戻り値の`PageGuard<'_>`のように書く．
- `'static`は，プログラムが終わるまで有効なライフタイムである．文字列リテラルは`&'static str`である．

戻り値の型が参照を含む構造体なら，`'_`を書いて借用していることを示す．省くと，警告になる．

```text
warning: hiding a lifetime that's elided elsewhere is confusing
  --> src/storage/buffer.rs:79:23
   |
79 |     pub fn fetch_page(&self, page_id: PageId) -> Result<PageGuard, BufferError> {
   |                       ^^^^^ the lifetime is elided here ^^^^^^^^^ the same lifetime is hidden here
   |
   = help: the same lifetime is referred to in inconsistent ways, making the signature confusing
   = note: `#[warn(mismatched_lifetime_syntaxes)]` on by default
help: use `'_` for type paths
   |
79 |     pub fn fetch_page(&self, page_id: PageId) -> Result<PageGuard<'_>, BufferError> {
   |                                                                  ++++
```

## `Drop`

`Drop`トレイトの`drop`メソッドは，値が捨てられるときに自動で呼ばれる．
変数は，宣言したブロックの終わりで，宣言と逆の順に捨てられる．`drop(値)`(Iteration 14)で先に捨てることもできる．

```rust
pub struct Noisy<'a> {
    pub name: &'static str,
    pub log: &'a RefCell<Vec<String>>,
}

impl Drop for Noisy<'_> {
    fn drop(&mut self) {
        self.log.borrow_mut().push(format!("drop {}", self.name));
    }
}

let log = RefCell::new(Vec::new());
{
    let _a = Noisy { name: "a", log: &log };
    let b = Noisy { name: "b", log: &log };
    let _c = Noisy { name: "c", log: &log };
    drop(b);
    log.borrow_mut().push("end of block".to_string());
}
assert_eq!(*log.borrow(), vec!["drop b", "end of block", "drop c", "drop a"]);
```

- `drop`は`&mut self`を受け取り，何も返さない．失敗しうる処理の結果は，呼び出し側に返せない．
- 変数名を`_a`のように`_`で始めると，使わない変数の警告が出ない．値はブロックの終わりまで残る．`let _ = 値;`と書くと，値はその場で捨てられる．

### RAII

資源を得る値を作り，その値が捨てられるときに資源を返す書き方を，RAII(Resource Acquisition Is Initialization)と呼ぶ．
`File`(ファイルを閉じる)，`RefCell`の`Ref`(借用を返す)も，この形である．
返し忘れがなく，`?`によって途中で関数を抜けても，値が捨てられるときに必ず返す．

借用検査は，資源を返したあとに値を使うことも防ぐ．次は，プールから借りたページを，プールを捨てたあとで使おうとした例である．

```text
error[E0505]: cannot move out of `pool` because it is borrowed
   --> src/storage/buffer.rs:385:14
    |
383 |         let pool = BufferPool::new(MemoryDiskManager::default(), 2);
    |             ---- binding `pool` declared here
384 |         let guard = pool.new_page().unwrap();
    |                     ---- borrow of `pool` occurs here
385 |         drop(pool);
    |              ^^^^ move out of `pool` occurs here
386 |         guard.write().insert(b"x").unwrap();
    |         ----- borrow later used here
```

## 内部可変性：`Cell`と`RefCell`

`&self`のメソッドは，ふつうはフィールドを書き換えられない．
`std::cell`の`Cell`と`RefCell`に入れた値は，共有の参照(`&`)からでも書き換えられる．これを内部可変性と呼ぶ．

### `Cell`

`Cell<T>`は，値を丸ごと取り出したり置き換えたりする．中の値への参照は作れない．

```rust
pub struct Counter {
    count: Cell<u32>,
}

impl Counter {
    pub fn hit(&self) -> u32 {
        self.count.set(self.count.get() + 1);
        self.count.get()
    }
}

let cell = Cell::new(Some(5));
assert_eq!(cell.take(), Some(5));
assert_eq!(cell.get(), None);
```

| メソッド | すること |
| --- | --- |
| `get()` | 値の複製を返す．`T`が`Copy`のときに使える |
| `set(値)` | 値を置き換える |
| `take()` | 値を取り出し，`Default`の値(`Option`なら`None`)を残す |

### `RefCell`

`RefCell<T>`は，中の値への参照を貸し出す．借用の規則(共有の参照はいくつでも，書き換えられる参照は1つだけ)を，コンパイル時でなく実行時に調べる．

```rust
let names = RefCell::new(vec!["a".to_string()]);
names.borrow_mut().push("b".to_string());
{
    let first = names.borrow();
    let second = names.borrow();
    assert_eq!(first.len() + second.len(), 4);
    assert!(names.try_borrow_mut().is_err());
}
assert!(names.try_borrow_mut().is_ok());
```

- `borrow()`は`Ref<T>`を，`borrow_mut()`は`RefMut<T>`を返す．どちらも`*`や`.`で中の値として使える．
- `Ref`と`RefMut`は，捨てられるときに借用を返す．
- 規則に反する借用をすると，パニックになる．`try_borrow_mut()`は，パニックの代わりに`Err`を返す．

```text
thread 'double_borrow' (40778) panicked at src/lib.rs:5:11:
RefCell already borrowed
```

コンパイル時の検査と違い，誤りは実行するまで見つからない．`Ref`や`RefMut`は短い間だけ持ち，関数をまたいで持ち続けない．

`Cell`と`RefCell`は，複数のスレッドから使えない．Iteration 21では，スレッドの間で共有できる`Mutex`と`RwLock`に置き換える．

## トレイト境界を持つジェネリックな構造体

構造体の型引数に，トレイト境界を付けられる．

```rust
pub struct Scaled<S: Shape> {
    shape: S,
    factor: f64,
}

impl<S: Shape> Scaled<S> {
    pub fn new(shape: S, factor: f64) -> Scaled<S> {
        Scaled { shape, factor }
    }

    pub fn area(&self) -> f64 {
        self.shape.area() * self.factor * self.factor
    }
}
```

- `impl<S: Shape> Scaled<S>`は，`Shape`を実装するどの`S`にもメソッドを定義する．
- `Scaled<Square>`と`Scaled<Circle>`は別の型になる．コンパイラーは使われる型ごとにコードを作る(静的ディスパッチ，Iteration 10)．

関数の型引数にも同じく境界を書ける．

```rust
pub fn total_area<S: Shape>(shapes: &[S]) -> f64 {
    shapes.iter().map(|s| s.area()).sum()
}
```

### `Box<dyn Trait>`にトレイトを実装する

`Box<dyn Shape>`は`Shape`そのものではないので，そのままでは`Scaled<S: Shape>`の`S`に使えない．
`Box<dyn Shape>`に`Shape`を実装し，中身に処理を任せれば使える．

```rust
impl Shape for Box<dyn Shape> {
    fn area(&self) -> f64 {
        (**self).area()
    }
}

let boxed: Box<dyn Shape> = Box::new(Square(1.0));
let scaled = Scaled::new(boxed, 2.0);
assert_eq!(scaled.area(), 4.0);
```

`self`は`&Box<dyn Shape>`なので，`*self`が`Box<dyn Shape>`，`**self`が`dyn Shape`である．`(**self).area()`は中身の`area`を呼ぶ．
`self.area()`と書くと，`Box<dyn Shape>`の`area`(この関数そのもの)を呼び続けてしまう．

## 関数を受け取る引数

`impl Fn(引数の型) -> 戻り値の型`は，その形で呼べる関数やクロージャを受け取る引数の型である．

```rust
pub fn victim(&mut self, is_pinned: impl Fn(usize) -> bool) -> Option<usize>
```

呼び出し側は，クロージャ(Iteration 7)を渡す．

```rust
clock.victim(|index| frames[index].pin_count.get() > 0)
```

`Fn`はクロージャを何度でも呼べることを表す．判定する側(`victim`)は，枠がピン留めされているかを，どこでどう数えているかを知らなくてよい．
