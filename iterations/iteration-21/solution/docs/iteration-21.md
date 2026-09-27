# Iteration 21：複数の同時接続(解説)

演習の各手順の模範解答と，その考え方を説明する．

## 21-1 準備

引き継いだ473個のテストがすべて通れば準備は終わりである．
Iteration 20の`serve`は，1つの接続を終わるまで扱ってから次の`accept`を呼ぶ．2つ目の`psql`のTCPの接続はOSが受け付けるが，起動のメッセージに返事が来ないので待ち続ける．

## 21-2 文法と概念

課題の解答例である．`tests/`に置いた結合テストで確かめた．

```rust
#[test]
fn tasks() {
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

    let lock = RwLock::new(0);
    let reader = lock.read().unwrap();
    assert!(lock.try_write().is_err());
    drop(reader);

    let words = vec!["a", "bb", "ccc"];
    let total = Mutex::new(0);
    thread::scope(|scope| {
        for word in &words {
            scope.spawn(|| *total.lock().unwrap() += word.len());
        }
    });
    assert_eq!(total.into_inner().unwrap(), 6);
}
```

- 課題1：スレッドが終わると，各スレッドの`Arc`は捨てられるので，`strong_count`は1である．
- 課題2：``Rc<i32>` cannot be sent between threads safely``というエラー(E0277)になる．全文はノートにある．
- 課題3：`try_write`は，読み取りのガードがある間は`Err`を返す．
- 課題5：`Cell<i32>`は`Sync`でないので，`&Cell<i32>`をほかのスレッドに渡せない．

```text
error[E0277]: `Cell<i32>` cannot be shared between threads safely
  --> tests/cell_sync.rs:12:21
   |
12 |         scope.spawn(|| counter.count.set(counter.count.get() + 1));
   |               ----- ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ `Cell<i32>` cannot be shared between threads safely
   |               |
   |               required by a bound introduced by this call
   |
   = help: the trait `Sync` is not implemented for `Cell<i32>`
   = note: if you want to do aliasing and mutation between multiple threads, use `std::sync::RwLock` or `std::sync::atomic::AtomicI32` instead
   = note: required for `&Cell<i32>` to implement `Send`
```

フィールドを`Mutex<i32>`にして`*counter.count.lock().unwrap() += 1`と書けば，コンパイルでき，2つのスレッドが1ずつ足して2になる．

## 21-3 テストリスト

模範解答は[TESTLIST.md](../TESTLIST.md)である．

- バッファプールを複数のスレッドから使うことは`storage::buffer`の単体テストで，セッションごとの状態とコミット前の行が見えないことは結合テスト`tests/session.rs`で確かめた．
- トランザクションの中でセッションを捨てたときの中止は，SQLからは違いが見えない．進行中のままでも中止でも，その版はほかのセッションから見えないからである．そこで，`database`の単体テストで，トランザクションの状態が`Aborted`になることを確かめた．
- 2つのクライアントが同時に接続できることは，結合テスト`tests/server.rs`で確かめた．Iteration 20のように接続を1つずつ扱う受け付けのループでは，2つ目の`Client::connect`が返らず，テストが終わらない．
- 引き継いだテストのうち，`Database::execute`を呼ぶテストは`Session::execute`を呼ぶ形に変わる．`handle`に`&mut Database`を渡していたテストは，`Arc<Database>`を渡す．

## 21-4 設計ドキュメント

| ファイル | 変更 | 理由 |
| --- | --- | --- |
| `c4-container.md` | 接続のスレッドを加え，`serve`がスレッドを作り，スレッドが`Session`で文を実行するようにした | 接続ごとにスレッドで扱う |
| `c4-component.md` | `database`，`server::connection`，`repl`，`main`の説明と依存の説明を直した | `Session`を作って文を実行するようになった |
| `code-types.md` | `Database`を共有する部分にし，`Storage`，`Session`，`SessionState`を加えた．`BufferPool`の`PoolState`，`Frame`，`FrameState`を描き直した | 共有する状態と，それを守るロックが変わった |
| `code-sequence.md` | サーバーの接続を`serve`のスレッドと接続のスレッドに分け，2つのセッションの流れを加えた | 接続ごとのスレッドと，ラッチを持つ期間を示す |

- `Storage`は，カタログ，表，インデックスをまとめた構造体である．3つを別々の`RwLock`にすると，途中の状態をほかのスレッドに見せてしまう．たとえば，カタログには表の定義があるのに，ヒープファイルはまだない状態である．1つの`RwLock`で守れば，`CREATE TABLE`のあとの状態だけが見える．
- `TransactionManager`は，`Storage`と別の`RwLock`で守る．コミットはページを読み書きしないので，`Storage`のガードを取らずに済む．

## 21-5 テスト駆動の実装

### バッファプール

最初に，複数のスレッドから`fetch_page`を呼ぶテストを書いた．

```rust
    #[test]
    fn threads_share_the_pool() {
        let pool = BufferPool::new(disk_with(8), 4);
        std::thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| {
                    for _ in 0..50 {
                        for i in 0..8 {
                            assert_eq!(first_tuple(&pool.fetch_page(i).unwrap()), vec![i as u8]);
                        }
                    }
                });
            }
        });
    }
```

8ページを4つの枠で読むので，スレッドは互いのページを追い出しながら読む．Iteration 20のプールでは，コンパイルエラーになる．エラーは`RefCell`と`Cell`のフィールドごとに出る．最初のエラーは次のとおりである．

```text
error[E0277]: `std::cell::RefCell<CountingDisk>` cannot be shared between threads safely
   --> iterations/iteration-21/solution/src/storage/buffer.rs:450:29
    |
450 |                   scope.spawn(|| {
    |  _______________________-----_^
    | |                       |
    | |                       required by a bound introduced by this call
451 | |                     for _ in 0..50 {
452 | |                         for i in 0..8 {
453 | |                             assert_eq!(first_tuple(&pool.fetch_page(i).unwrap()), vec![i as u8]);
...   |
456 | |                 });
    | |_________________^ `std::cell::RefCell<CountingDisk>` cannot be shared between threads safely
    |
    = help: within `buffer::BufferPool<CountingDisk>`, the trait `Sync` is not implemented for `std::cell::RefCell<CountingDisk>`
    = note: if you want to do aliasing and mutation between multiple threads, use `std::sync::RwLock` instead
note: required because it appears within the type `buffer::BufferPool<CountingDisk>`
```

プールの状態を，次の3つに分けて守る．

```rust
struct Frame {
    page: RwLock<Page>,
    state: Mutex<FrameState>,
}

struct PoolState<D> {
    disk: D,
    page_ids: Vec<Option<PageId>>,
    page_table: HashMap<PageId, usize>,
    replacer: ClockReplacer,
}

pub struct BufferPool<D: DiskManager> {
    frames: Vec<Frame>,
    state: Mutex<PoolState<D>>,
    log: Option<PageLog>,
}
```

- ページの中身は，枠ごとの`RwLock<Page>`(ページのラッチ)で守る．`PageGuard::read`と`write`は，`RwLock`のガードを返す．
- ピン留めの数，変更の印，`page_lsn`は，枠ごとの`Mutex<FrameState>`で守る．`PageGuard`を捨てるときは，この`Mutex`だけを取ってピンを外す．
- どの枠にどのページがあるか，ディスク，置換方式は，プールの`Mutex<PoolState>`で守る．

```rust
    pub fn fetch_page(&self, page_id: PageId) -> Result<PageGuard<'_>, BufferError> {
        let mut state = lock(&self.state);
        if let Some(&index) = state.page_table.get(&page_id) {
            return Ok(self.pin(&mut state, index, page_id));
        }
        let index = self.free_frame(&mut state)?;
        let mut data = Box::new([0; PAGE_SIZE]);
        state.disk.read_page(page_id, &mut data)?;
        self.place(&mut state, index, page_id, Page::from_bytes(data), false);
        Ok(self.pin(&mut state, index, page_id))
    }
```

ページ表を引いてピン留めするまでと，追い出す枠を選んで空けるまでを，同じ`Mutex<PoolState>`の中で行う．別々に取ると，あるスレッドの選んだ枠を，追い出す直前に別のスレッドがピン留めし，ピン留めしたページを追い出してしまう．

- ページの番号は`PageGuard`に持たせた．枠の中のページの番号は，ピン留めしている間は変わらないからである．
- `lock`は`Mutex::lock`の`expect`をまとめた関数である．毒された`Mutex`はパニックにする．
- ページを読むだけの引き継いだテスト`pool.fetch_page(0).unwrap().read();`は，`RwLockReadGuard`が`#[must_use]`なので警告になった．`let _ =`にすると`let_underscore_lock`のエラーになるので，`drop(...)`にした．
- `DiskManager`にスーパートレイト`Send`を加えた．`Box<dyn DiskManager>`が`Send`でなければ，`Mutex<PoolState<Box<dyn DiskManager>>>`は`Sync`にならない．

### `Database`と`Session`

`Database`は，すべてのセッションが`Arc`で共有する部分である．

```rust
pub struct Database {
    storage: RwLock<Storage>,
    transactions: RwLock<TransactionManager>,
    data_dir: Option<PathBuf>,
    wal: Option<Arc<Mutex<WalWriter<File>>>>,
}

pub struct Session {
    database: Arc<Database>,
    state: SessionState,
}
```

`Session::execute`は，Iteration 18の`Database::execute`の状態の遷移をそのまま持ち，`Database`の`begin`，`run`，`commit`，`rollback`を呼ぶ．
`run`は，文の種類でガードを選ぶ．

```rust
    fn run(&self, statement: Statement, txn: &Transaction) -> Result<StatementResult, Error> {
        match statement {
            Statement::Values(values) => Ok(StatementResult::Rows(evaluate_values(&values)?)),
            Statement::Select(select) => {
                let storage = self.read_storage();
                let manager = self.read_transactions();
                let snapshot = manager.snapshot(txn.xid());
                select_rows(&storage, &select, &snapshot, &manager)
            }
            Statement::Explain(select) => explain_plan(&self.read_storage(), &select),
            statement => {
                let mut storage = self.write_storage();
                let manager = self.read_transactions();
                let snapshot = manager.snapshot(txn.xid());
                self.change(&mut storage, statement, &snapshot, &manager)
            }
        }
    }
```

- `TransactionManager`の読み取りのガードは，文ごとに1回だけ取り，`&TransactionManager`として`dml`や`checkpoint`に渡す．`checkpoint`は，Iteration 20では`self.transactions`を使っていたが，引数で受け取るようにした．
- `insert`，`update`，`delete`は`Storage`のメソッドにした．どれも`Storage`の書き込みのガードの中で呼ばれ，`&mut self`で表のヒープファイルを変える．
- `begin`と`commit`は，ログのガードと`TransactionManager`の書き込みのガードを，同時には持たない．ログに書けなかったときは，`drop(wal)`でログのガードを捨ててから，トランザクションを中止する．

トランザクションの中で`Session`を捨てたら，中止する．

```rust
impl Drop for Session {
    fn drop(&mut self) {
        match std::mem::take(&mut self.state) {
            SessionState::Idle => {}
            SessionState::InTransaction(txn) | SessionState::Failed(txn) => {
                self.database.rollback(txn);
            }
        }
    }
}
```

`Drop`がなければ，単体テストは次のように失敗する．

```text
thread 'database::tests::dropping_a_session_in_a_transaction_aborts_the_transaction' (372256) panicked at src/database.rs:802:9:
assertion `left == right` failed
  left: InProgress
 right: Aborted
```

`repl::run_with`は，受け取った`Database`を`Arc`に包んでセッションを作るので，REPLのテストは変わらない．

### 接続ごとのスレッド

受け付けのループを`main`から`server::connection::serve`に移し，結合テストから呼べるようにした．

```rust
pub fn serve(listener: TcpListener, database: Arc<Database>) {
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let database = Arc::clone(&database);
                thread::spawn(move || {
                    if let Err(error) = handle(stream, database) {
                        eprintln!("ferrodb: {error}");
                    }
                });
            }
            Err(error) => eprintln!("ferrodb: {error}"),
        }
    }
}
```

`handle`は`Arc<Database>`を受け取り，接続のセッションを作る．接続が終わって`handle`が返ると，セッションが捨てられ，トランザクションの中なら中止される．

### 複数のスレッドのセッション

```rust
#[test]
fn sessions_in_several_threads_insert_rows_at_the_same_time() {
    let dir = tempfile::tempdir().unwrap();
    let database = Arc::new(Database::open(dir.path()).unwrap());
    Session::new(Arc::clone(&database))
        .execute("CREATE TABLE t (a INTEGER)")
        .unwrap();
    thread::scope(|scope| {
        for n in 0..4 {
            let mut session = Session::new(Arc::clone(&database));
            scope.spawn(move || {
                for i in 0..25 {
                    let sql = format!("INSERT INTO t VALUES ({})", n * 100 + i);
                    session.execute(&sql).unwrap();
                }
            });
        }
    });
    drop(database);
    let mut session = Session::new(Arc::new(Database::open(dir.path()).unwrap()));
    assert_eq!(count(&mut session), Value::BigInt(100));
}
```

データディレクトリで開くので，4つのスレッドのコミットが同じログに書かれ，ページの記録と書き戻しも並行に起こる．開き直してから数え，ログのやり直しも確かめる．

## 21-6 振り返り

1. バッファプールの共有，セッションごとの状態，コミット前の行が見えないこと，セッションを捨てたときの中止，同時の接続を確かめる項目があるかを比べる．
2. Bは，Aの`INSERT`の文が`Storage`の書き込みのガードを持っている間だけ待つ．Aのトランザクションの終わりは待たない．Aの版が見えないのは，スナップショットでAが進行中だからで，ラッチとは関係がない．
3. 1行だった行が2行になる．次は，`N`が0の行を，セッションAが`N = N + 1`，セッションBが`N = N + 10`に書き換えてから，両方がコミットした結果である．

    ```text
     ID | N
    ----+----
      1 |  1
      1 | 10
    (2 rows)
    ```

    Bのスナップショットでは，Aが`xmax`を書いた版はまだ見えるので，Bも同じ版に`xmax`を書き，新しい版を加える．Iteration 22で，進行中のトランザクションが書き換えている行を書き換えるときは，その終わりを待つようにする．

4. 表ごとに`RwLock`を持てば，別の表への変更は同時に動く．ただし，`CREATE TABLE`や`DROP TABLE`は表の一覧も変えるので，一覧のラッチと表のラッチの2段になる．ガードを取る順序を決め，1つの文が複数の表を使う結合で，どの順に取るかも決める必要がある．
5. 問い合わせどうしが同時に動く．`Mutex`では，2つの`SELECT`も1つずつ動く．
6. 模範解答の設計ドキュメントは，照合スクリプトで問題がないことを確かめてある．

## 21-7 発展課題

解答例である．`serve`に上限の数を加え，数を`Arc<Mutex<usize>>`で数える．

```rust
pub fn serve(listener: TcpListener, database: Arc<Database>, max_connections: usize) {
    let active = Arc::new(Mutex::new(0));
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let slot = ConnectionSlot::take(&active, max_connections);
                let database = Arc::clone(&database);
                thread::spawn(move || {
                    let result = match slot {
                        Some(_slot) => handle(stream, database),
                        None => refuse(stream),
                    };
                    if let Err(error) = result {
                        eprintln!("ferrodb: {error}");
                    }
                });
            }
            Err(error) => eprintln!("ferrodb: {error}"),
        }
    }
}
```

接続の数を1つ持つ`ConnectionSlot`は，捨てるときに数を減らす．

```rust
struct ConnectionSlot {
    active: Arc<Mutex<usize>>,
}

impl ConnectionSlot {
    fn take(active: &Arc<Mutex<usize>>, max: usize) -> Option<ConnectionSlot> {
        let mut count = active.lock().expect("a thread panicked while counting");
        if *count >= max {
            return None;
        }
        *count += 1;
        Some(ConnectionSlot {
            active: Arc::clone(active),
        })
    }
}

impl Drop for ConnectionSlot {
    fn drop(&mut self) {
        *self.active.lock().expect("a thread panicked while counting") -= 1;
    }
}
```

- 数えるのは受け付けのスレッドで，減らすのは接続のスレッドである．`take`は，比べて増やすまでを1つのガードの中で行う．
- `Some(_slot)`は，名前を付けて束縛するので，`handle`が返るまで`ConnectionSlot`が生きる．`Some(_)`は束縛しないが，`slot`の中の値はマッチのあとも`slot`に残り，クロージャの終わりに捨てられる．どちらでも数は正しく減るが，名前を付ければ，接続の間だけ持つことが読み取りやすい．
- `refuse`は，起動のメッセージを読んでから`ErrorResponse`を返す．`handle`の起動のメッセージを待つループを`wait_for_startup`に分け，`handle`と`refuse`の両方で使う．
- `main`には`--max-connections`(既定は100)を加え，`serve`に渡す．

結合テストでは，上限を1にしたサーバーへの2つ目の`Client::connect`が，SQLSTATE `53300`で失敗することと，1つ目の接続を閉じたあとで接続できることを確かめた．1つ目の`close`から接続のスレッドが数を減らすまでには少し時間がかかるので，接続できるまで10ミリ秒ずつ待って繰り返す．
