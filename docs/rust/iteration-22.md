# Iteration 22：`Condvar`，タイムアウト付きの待ち合わせ，`Barrier`

Iteration 22では，ほかのトランザクションが書き換えている行を書き換えるとき，そのトランザクションが終わるまで待つ．
このノートでは，条件が成り立つまでスレッドを眠らせる`Condvar`，待つ時間に上限を付ける方法，スレッドの足並みをそろえる`Barrier`を使ったテスト，構造体の更新構文と`get_or_insert_with`を説明する．

## `Duration`と`thread::sleep`

`std::time::Duration`は時間の長さである．`Duration::from_secs(60)`や`Duration::from_millis(50)`で作る．
`std::thread::sleep(duration)`は，今のスレッドをその時間だけ止める．`std::time::Instant::now()`は今の時刻で，`elapsed()`でそこからの経過時間を返す．

## `Condvar`：条件が成り立つまで待つ

ほかのスレッドが値を変えるまで待ちたいとき，`Mutex`を取り直しては値を調べる繰り返し(ビジーウェイト)は，CPUを使い続ける．
`std::sync::Condvar`(条件変数)は，`Mutex`と組にして使い，条件が成り立つまでスレッドを眠らせる．

```rust
let ready = Mutex::new(false);
let changed = Condvar::new();
thread::scope(|scope| {
    scope.spawn(|| {
        thread::sleep(Duration::from_millis(20));
        *ready.lock().unwrap() = true;
        changed.notify_all();
    });
    let guard = ready.lock().unwrap();
    let guard = changed.wait_while(guard, |ready| !*ready).unwrap();
    assert!(*guard);
});
```

| メソッド | すること |
| --- | --- |
| `wait(guard)` | ガードの`Mutex`を外して眠り，起こされたら`Mutex`を取り直してガードを返す |
| `wait_while(guard, condition)` | `condition`が真の間，`wait`を繰り返す |
| `notify_one()` | 眠っているスレッドを1つ起こす |
| `notify_all()` | 眠っているスレッドをすべて起こす |

- `wait`は，`Mutex`を外すことと眠ることを，1つの操作として行う．外してから眠るまでの間に知らせが来て，それを逃すことはない．
- 起こされても条件が成り立つとは限らない．`notify_all`はすべてのスレッドを起こし，OSの都合で理由なく起きること(spurious wakeup)もある．`wait_while`は起きるたびに条件を調べ直す．
- 知らせる側は，条件の値を変えてから`notify_all`を呼ぶ．値を変えるときは同じ`Mutex`を取る．待つ側は`Mutex`を持ったまま条件を調べて眠るので，調べてから眠るまでの間に値が変わることはない．

`Mutex`の中に条件の値がなく，別の場所(`ferrodb`ではトランザクションの状態の`RwLock`)にあるときも，知らせる側は`notify_all`の前に`Mutex`を取る．
待つ側が条件を調べてから眠るまでの間，待つ側は`Mutex`を持っている．知らせる側は`Mutex`を取れるまで待つので，知らせは待つ側が眠ったあとに届く．

## タイムアウト付きの待ち合わせ

`wait_timeout_while(guard, timeout, condition)`は，`condition`が真の間待つが，`timeout`を過ぎたら待つのをやめる．
ガードと`WaitTimeoutResult`を返し，`timed_out()`は，時間切れで待つのをやめたときに真である．

```rust
let ready = Mutex::new(false);
let changed = Condvar::new();
let start = Instant::now();
let guard = ready.lock().unwrap();
let (guard, result) = changed
    .wait_timeout_while(guard, Duration::from_millis(50), |ready| !*ready)
    .unwrap();
assert!(result.timed_out());
assert!(!*guard);
assert!(start.elapsed() >= Duration::from_millis(50));
```

2つのスレッドが互いの終わりを待つと，どちらも永遠に待つ(デッドロック)．待つ時間に上限を付ければ，片方がエラーで待つのをやめて，もう片方が進める．

## `Barrier`：スレッドの足並みをそろえる

`std::sync::Barrier::new(n)`は，`n`個のスレッドが`wait()`を呼ぶまで，呼んだスレッドを待たせる．`n`個目が呼ぶと，すべてのスレッドが同時に進む．

```rust
let barrier = Barrier::new(3);
let log = Mutex::new(Vec::new());
thread::scope(|scope| {
    for n in 0..3 {
        let barrier = &barrier;
        let log = &log;
        scope.spawn(move || {
            log.lock().unwrap().push(format!("before {n}"));
            barrier.wait();
            log.lock().unwrap().push(format!("after {n}"));
        });
    }
});
let log = log.into_inner().unwrap();
assert!(log[..3].iter().all(|entry| entry.starts_with("before")));
assert!(log[3..].iter().all(|entry| entry.starts_with("after")));
```

- `move`で`n`をスレッドに移すため，`barrier`と`log`は参照にしてから渡す．
- 並行処理のテストでは，「このスレッドがスナップショットを取った」などの時点をほかのスレッドと合わせるのに使う．

## 待っていることを確かめる：`is_finished`

`thread::scope`の`spawn`が返す`ScopedJoinHandle`の`is_finished()`は，スレッドが終わっていれば真を返す．待たずにすぐ返る．
あるスレッドが待たされていることは，少し時間をおいても終わっていないことで確かめる．

```rust
let gate = Mutex::new(());
let held = gate.lock().unwrap();
thread::scope(|scope| {
    let waiting = scope.spawn(|| drop(gate.lock().unwrap()));
    thread::sleep(Duration::from_millis(50));
    assert!(!waiting.is_finished());
    drop(held);
    waiting.join().unwrap();
});
```

時間をおくテストは，待たないはずのスレッドがたまたま遅れても通る．反対に，待つはずのスレッドが待たなかったことは確実に見つかる．

## 構造体の更新構文

`Config { timeout, ..self }`は，`timeout`だけを指定し，残りのフィールドを`self`から移した新しい構造体である．`..Config::default()`のように，既定値から作るときにも使う(Iteration 19の`Database::open`)．

```rust
impl Config {
    fn with_timeout(self, timeout: Duration) -> Config {
        Config { timeout, ..self }
    }
}

let config = Config::default().with_timeout(Duration::from_millis(50));
let verbose = Config { verbose: true, ..Config::default() };
```

`self`を受け取って新しい値を返すメソッドは，`Database::new().with_lock_timeout(...)`のように設定をつなげて書ける．

## `Option::get_or_insert_with`

`option.get_or_insert_with(f)`は，`None`なら`f()`の値を入れ，中の値への`&mut`を返す．すでに`Some`なら`f`を呼ばない．
REPEATABLE READのトランザクションは，最初の文でスナップショットを作り，2つ目の文からは同じものを使う．

```rust
let mut cached: Option<Vec<i32>> = None;
let mut calls = 0;
for _ in 0..3 {
    cached.get_or_insert_with(|| { calls += 1; vec![1, 2] });
}
assert_eq!(calls, 1);
```
