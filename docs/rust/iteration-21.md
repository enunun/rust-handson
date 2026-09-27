# Iteration 21：スレッド，`Arc`，`Mutex`と`RwLock`，`Send`と`Sync`

Iteration 21では，接続ごとにスレッドを作り，複数のクライアントが1つのデータベースを同時に使えるようにする．
このノートでは，スレッドの間で値を共有する`Arc`，1つのスレッドだけが値を書き換えられるようにする`Mutex`と`RwLock`，スレッドに渡せる型を表す`Send`と`Sync`，借用を使えるスコープ付きのスレッドを説明する．

## スレッドと`move`クロージャ

`std::thread::spawn`は，クロージャを新しいスレッドで実行し，`JoinHandle`を返す(Iteration 20)．`join()`は，スレッドが終わるのを待つ．
新しいスレッドは，作った関数が返ったあとも動き続けることがある．そのため，`spawn`に渡すクロージャは，周りの変数を借用できない．`move`を付けて，使う値の所有権をクロージャに移す．

## `Arc`：スレッドの間で共有する

`Rc`(Iteration 19)は，1つの値を複数の持ち主で共有するが，参照の数を数える操作がスレッドの間で安全でない．`Rc`をほかのスレッドに渡すと，コンパイルエラーになる．

```text
error[E0277]: `Rc<i32>` cannot be sent between threads safely
 --> tests/rc_send.rs:8:19
  |
8 |     thread::spawn(move || println!("{cloned}")).join().unwrap();
  |     ------------- -------^^^^^^^^^^^^^^^^^^^^^
  |     |             |
  |     |             `Rc<i32>` cannot be sent between threads safely
  |     |             within this `{closure@tests/rc_send.rs:8:19: 8:26}`
  |     required by a bound introduced by this call
  |
  = help: within `{closure@tests/rc_send.rs:8:19: 8:26}`, the trait `Send` is not implemented for `Rc<i32>`
```

`std::sync::Arc`(Atomically Reference Counted)は，参照の数をスレッドの間で安全に数える`Rc`である．使い方は`Rc`と同じで，`Arc::clone`で持ち主を増やす．
`Arc`が渡すのは`&T`だけなので，中の値を書き換えるには，次の`Mutex`か`RwLock`と組み合わせる．

## `Mutex`：1つのスレッドだけが使う

`std::sync::Mutex<T>`は，値を1つのスレッドだけが使えるようにする．`lock()`は，ほかのスレッドが使い終わるまで待ってから，ガード(`MutexGuard<T>`)を返す．
ガードは`&mut T`のように使え，ガードを捨てると，ほかのスレッドが`lock`できるようになる．

```rust
let counter = Arc::new(Mutex::new(0));
let mut handles = Vec::new();
for _ in 0..4 {
    let counter = Arc::clone(&counter);
    handles.push(thread::spawn(move || {
        for _ in 0..1000 {
            *counter.lock().unwrap() += 1;
        }
    }));
}
for handle in handles {
    handle.join().unwrap();
}
assert_eq!(*counter.lock().unwrap(), 4000);
assert_eq!(Arc::strong_count(&counter), 1);
```

- 各スレッドは，`Arc::clone`で作った自分の`Arc`を`move`で受け取る．スレッドが終わると，その`Arc`は捨てられ，`strong_count`は1に戻る．
- `*counter.lock().unwrap() += 1`は，読んで，足して，書くまでを1つのスレッドだけが行う．`Mutex`なしでは，2つのスレッドが同じ値を読んで，1つの加算は失われうる．

`lock()`は`Result`を返す．ガードを持ったスレッドのパニックで，値は書きかけのまま残りうる．そのため，`Mutex`は毒された(poisoned)状態になり，それからの`lock()`は`Err`を返す．

```rust
let data = Arc::new(Mutex::new(1));
let cloned = Arc::clone(&data);
let result = thread::spawn(move || {
    let _guard = cloned.lock().unwrap();
    panic!("oops");
})
.join();
assert!(result.is_err());
assert!(data.lock().is_err());
```

`ferrodb`は，毒された`Mutex`を回復せず，`expect`でパニックする．

## `RwLock`：読むスレッドは同時に，書くスレッドは1つだけ

`std::sync::RwLock<T>`は，`read()`で読み取りのガードを，`write()`で書き込みのガードを返す．
読み取りのガードは複数のスレッドが同時に持てる．書き込みのガードは，ほかにガードがないときだけ取れる．

```rust
let prices = RwLock::new(HashMap::from([("apple", 100)]));
{
    let first = prices.read().unwrap();
    let second = prices.read().unwrap();
    assert_eq!(first["apple"] + second["apple"], 200);
}
prices.write().unwrap().insert("apple", 120);
assert_eq!(prices.read().unwrap()["apple"], 120);
assert!(prices.try_write().is_ok());
let reader = prices.read().unwrap();
assert!(prices.try_write().is_err());
drop(reader);
```

`try_write`は，待たずに，取れなければ`Err`を返す．読み取りのガード`reader`がある間は，書き込みのガードを取れない．

| 型 | 同時に持てるガード | `ferrodb`で守るもの |
| --- | --- | --- |
| `Mutex<T>` | 1つ | ログ，バッファプールの枠の割り当て，枠の状態 |
| `RwLock<T>` | 読み取りは複数，書き込みは1つ | 表とインデックス(`Storage`)，トランザクションの状態，ページ |

- `Cell`と`RefCell`(Iteration 15)は，1つのスレッドの中の内部可変性である．`Mutex`と`RwLock`は，複数のスレッドの間の内部可変性である．どちらも`&self`から中の値を書き換えられる．
- `RefCell`は借用の規則に反すると実行時にパニックする．`Mutex`と`RwLock`は，ほかのスレッドがガードを持っていれば待つ．同じスレッドが同じ`Mutex`を2回`lock`すると，デッドロックするかパニックする．

## ガードを持つ期間

ガードは，捨てるまでほかのスレッドを待たせる．ガードを持つ期間は短くする．

- 式の中で作ったガード(`*counter.lock().unwrap() += 1`)は，文の終わりで捨てられる．
- `let`で変数に入れたガードは，ブロックの終わりか`drop`で捨てられる．
- `let _ = mutex.lock()`はガードをすぐに捨て，ロックしたつもりで何も守らない．Rustはこれをエラーにする．

```text
error: non-binding let on a synchronization lock
 --> tests/let_lock.rs:6:9
  |
6 |     let _ = count.lock().unwrap();
  |         ^ this lock is not assigned to a binding and is immediately dropped
  |
  = note: `#[deny(let_underscore_lock)]` (part of `#[deny(let_underscore)]`) on by default
help: consider binding to an unused variable to avoid immediately dropping the value
  |
6 |     let _unused = count.lock().unwrap();
  |          ++++++
help: consider immediately dropping the value
  |
6 -     let _ = count.lock().unwrap();
6 +     drop(count.lock().unwrap());
  |
```

複数の`Mutex`や`RwLock`を持つときは，どのスレッドも同じ順に取る．スレッド1がAを持ってBを待ち，スレッド2がBを持ってAを待つと，どちらも進めない．
`ferrodb`は，表とインデックス，トランザクションの状態，ログの順に取る．
`RwLock`の読み取りのガードを，同じスレッドで2回取ることも避ける．間に別のスレッドが書き込みを待っていると，OSによっては2回目の`read`がその書き込みを待つ．書き込みは1回目の読み取りを待つので，どちらも進めない．

## `Send`と`Sync`

スレッドに渡せるかは，2つのマーカートレイトで決まる．どちらもメソッドを持たず，コンパイラーが型の中身から自動で実装する．

| トレイト | 意味 | 実装しない型の例 |
| --- | --- | --- |
| `Send` | 値の所有権をほかのスレッドに移せる | `Rc<T>` |
| `Sync` | `&T`を複数のスレッドで共有できる | `Cell<T>`，`RefCell<T>` |

- `thread::spawn`は，クロージャと返す値が`Send`であることを求める．
- `Arc<T>`は，`T`が`Send`と`Sync`のときに`Send`である．`Mutex<T>`は，`T`が`Send`なら`Sync`になる．`RefCell`を`Mutex`に入れれば共有できるのはこのためである．
- 構造体は，すべてのフィールドが`Sync`のときだけ`Sync`である．`Cell`のフィールドが1つでもあれば，構造体全体を共有できない．

`Box<dyn Trait>`は，トレイトが何を実装する型かを知らないので，`Send`を実装しない．
スーパートレイト(Iteration 16)に`Send`を書けば，そのトレイトを実装する型はどれも`Send`で，`Box<dyn Trait>`も`Send`になる．

```rust
trait Store: Send {
    fn put(&mut self, value: i32);
}

let store: Box<dyn Store> = Box::new(VecStore(Vec::new()));
let shared = Mutex::new(store);
thread::scope(|scope| {
    scope.spawn(|| shared.lock().unwrap().put(1));
});
```

`Box<dyn Trait + Send>`と書いて，その場所だけで`Send`を求めることもできる．

## `thread::scope`：借用を使うスレッド

`thread::scope`は，スコープの中で作ったスレッドがすべて終わるまで待ってから返る．スレッドが周りの変数より長く生きないので，`Arc`や`move`なしで借用を使える．

```rust
let words = vec!["a", "bb", "ccc"];
let total = Mutex::new(0);
thread::scope(|scope| {
    for word in &words {
        scope.spawn(|| *total.lock().unwrap() += word.len());
    }
});
assert_eq!(total.into_inner().unwrap(), 6);
assert_eq!(words.len(), 3);
```

- `scope.spawn`のクロージャは`total`と`word`を借用する．スコープのあとは，`words`と`total`をそのまま使える．
- `Mutex::into_inner`は，`Mutex`を消費して中の値を取り出す．ほかにガードがないことは所有権で分かるので，待たない．

テストで，1つの値を複数のスレッドから使うときに便利である．
