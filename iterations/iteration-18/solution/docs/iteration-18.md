# Iteration 18：トランザクションとMVCC(解説)

演習の各手順の模範解答と，その考え方を説明する．

## 18-1 準備

引き継いだ414個のテストがすべて通れば準備は終わりである．
Iteration 17までの`UPDATE`は，タプルをその場で書き換えるので，`SELECT`の行の順序は変わらない．

## 18-2 文法と概念

課題の解答例である．`tests/`に置いた結合テストで確かめた．

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Port(pub u16);

impl Port {
    pub const HTTP: Port = Port(80);
}

pub fn connect(port: Port) -> String {
    format!("localhost:{}", port.0)
}

pub struct Ticket {
    id: u32,
}

impl Ticket {
    pub fn redeem(self, used: &mut Vec<u32>) {
        used.push(self.id);
    }
}

#[derive(Debug, Default, PartialEq)]
pub enum Light {
    #[default]
    Off,
    On(u8),
}

pub fn toggle(light: &mut Light) -> u8 {
    match std::mem::take(light) {
        Light::Off => {
            *light = Light::On(100);
            0
        }
        Light::On(n) => n,
    }
}
```

`connect(80u16)`は，型が違うのでコンパイルエラーになる．コンパイラーは`Port`で包むよう勧める．

```text
error[E0308]: mismatched types
  --> tests/tasks.rs:54:13
   |
54 |     connect(80u16);
   |     ------- ^^^^^ expected `Port`, found `u16`
   |     |
   |     arguments to this function are incorrect
   |
note: function defined here
  --> tests/tasks.rs:8:8
   |
 8 | pub fn connect(port: Port) -> String {
   |        ^^^^^^^ ----------
help: try wrapping the expression in `Port`
   |
54 |     connect(Port(80u16));
   |             +++++     +
```

同じ`Ticket`で`redeem`を2回呼ぶと，ムーブした値を使うエラーになる．

```text
error[E0382]: use of moved value: `ticket`
  --> tests/tasks.rs:57:5
   |
55 |     let ticket = Ticket { id: 3 };
   |         ------ move occurs because `ticket` has type `Ticket`, which does not implement the `Copy` trait
56 |     ticket.redeem(&mut used);
   |            ----------------- `ticket` moved due to this method call
57 |     ticket.redeem(&mut used);
   |     ^^^^^^ value used here after move
```

`toggle`の`Light::On(n) => n`の腕では`*light`に何も入れないので，`take`が置いた`Light::Off`のままになる．

## 18-3 テストリスト

模範解答は[TESTLIST.md](../TESTLIST.md)である．

- 可視性の規則を`txn`の単体テストに，版の置き方を`storage`と`exec::dml`の単体テストに，セッションの状態を結合テストに分けた．
- `exec::dml`の単体テストでは，表とインデックスのプールとトランザクションの状態を持つ`Table`を作り，操作を1つずつのトランザクションで行う`run`にまとめた．
- 引き継いだ結合テストのうち，次の3つが変わる．
  - `tables.rs`：タプルにヘッダーの8バイトが加わるので，`row is too big`の大きさが9003から9011に，ページに入る最大の文字列が8181文字から8173文字になる．
  - `persistence.rs`：データディレクトリに`xact`が加わる．`UPDATE`した行は表の最後に並ぶので，行を`ORDER BY id`で並べて比べる．
  - `index.rs`：表を消したあとのデータディレクトリに`xact`が残る．

## 18-4 設計ドキュメント

| ファイル | 変更 | 理由 |
| --- | --- | --- |
| `c4-container.md` | データディレクトリに`xact`を加えた | トランザクションの状態をファイルに置く |
| `c4-component.md` | `txn`を加え，`database`，`exec::build`，`exec::dml`，`storage::heap`，`storage::tuple`，`error`との依存を加えた | 版の可視性を判定するモジュールができた |
| `code-types.md` | トランザクションの図を加え，`Database`，`Session`，`TransactionStatus`，`HeapFile`，`StatementResult`，`Statement`を更新した | トランザクションとセッションの型ができた |
| `layout.md` | タプルのヘッダーと，`xact`の形式を加えた | タプルと，新しいファイルの形式が変わった |
| `code-sequence.md` | `UPDATE`が古い版を削除済みにして新しい版を置く流れにし，トランザクションの状態の変化を加えた | 版を残すようになった |

- `storage::tuple`と`txn`は互いに参照する．`TupleHeader`は`TxnId`を持ち，`is_visible`は`TupleHeader`を読む．
- `storage::heap`は`txn`の`TxnId`だけを使い，可視性の判定は呼び出し側から関数で受け取る．

## 18-5 テスト駆動の実装

### 可視性

最初に`txn`を作った．状態は，番号を添字にした`Vec<TxnStatus>`で持つ．

```rust
pub fn is_visible(header: &TupleHeader, snapshot: &Snapshot, manager: &TransactionManager) -> bool {
    if !snapshot.sees(header.xmin, manager) {
        return false;
    }
    header.xmax == TxnId::INVALID || !snapshot.sees(header.xmax, manager)
}

impl Snapshot {
    fn sees(&self, xid: TxnId, manager: &TransactionManager) -> bool {
        if xid == self.xid {
            return true;
        }
        xid < self.xmax
            && !self.active.contains(&xid)
            && manager.status(xid) == TxnStatus::Committed
    }
}
```

`changes_of_transactions_in_progress_are_not_visible`は，スナップショットを作ったあとで書き込んだトランザクションがコミットしても，そのスナップショットでは見えないことを確かめる．`active`に番号が入っているからである．

`Transaction`の`commit`と`rollback`は`self`を受け取る．

```rust
impl Transaction {
    pub fn commit(self, manager: &mut TransactionManager) {
        manager.finish(self.xid, TxnStatus::Committed);
    }
}
```

`txn`のテストで，始めたまま使わない`Transaction`を`drop(second)`で捨てると，`Drop`を実装しない値を捨てても意味がないという`cargo clippy`のエラーになった．変数を`_second`にして，最後まで持つようにした．

### タプルのヘッダーと`HeapFile`

`encode_tuple`と`decode_tuple`は行の値だけを扱うまま残し，その前に置くヘッダーの型を加えた．

```rust
impl TupleHeader {
    pub fn split(bytes: &[u8]) -> Result<(TupleHeader, &[u8]), TupleError> {
        let (header, rest) = split(bytes, TUPLE_HEADER_SIZE)?;
        let xmin = u32::from_le_bytes(header[..4].try_into().expect("4 bytes"));
        let xmax = u32::from_le_bytes(header[4..].try_into().expect("4 bytes"));
        let header = TupleHeader {
            xmin: TxnId(xmin),
            xmax: TxnId(xmax),
        };
        Ok((header, rest))
    }
}
```

`split`の戻り値の`&[u8]`は，引数の`bytes`を借りる．参照の引数が1つなので，ライフタイムの注釈は要らない．

`HeapFile::rows`は，版のヘッダーを受け取る関数`visible`で版を選ぶ．

```rust
    pub fn rows(
        &self,
        schema: &TableSchema,
        visible: impl Fn(&TupleHeader) -> bool,
    ) -> Result<Vec<(RowId, Row)>, HeapError> {
        let mut rows = Vec::new();
        for (id, tuple) in self.tuples()? {
            let (header, data) = TupleHeader::split(&tuple)?;
            if visible(&header) {
                rows.push((id, decode_tuple(data, schema)?));
            }
        }
        Ok(rows)
    }
```

`set_xmax`は，ヘッダーを書き換えたタプルを`update`で書き戻す．長さが同じなので，タプルはその場で書き換わり，`RowId`は変わらない．

### 版を置く`exec::dml`

`update`は，見える版を読み，古い版の`xmax`を書いてから新しい版を加える．

```rust
    for ((id, new_row), tuple) in targets.iter().zip(&tuples) {
        heap.set_xmax(*id, snapshot.xid)?;
        let new_id = heap.insert(tuple)?;
        for index in indexes.iter_mut() {
            index.index.insert(&new_row[index.column], new_id)?;
        }
    }
```

- 古い版は表に残り，別のトランザクションからは見え続ける．インデックスの古い版の項目も消さない．
- `delete`は`set_xmax`だけを行い，インデックスを受け取らなくなった．
- 一意性の検査は，インデックスで見つかった版のうち，見える版だけを重なりとする．版が見えるかを調べる`Versions::sees`は，スナップショットとトランザクションの状態を組にした小さな構造体のメソッドにした．

### `BuildContext`

`build`は，スナップショットとトランザクションの状態も受け取る．引数が6つになり，子の演算子を作るたびに同じ引数を並べることになるので，1つの構造体にまとめた．

```rust
pub struct BuildContext<'a> {
    pub tables: &'a HashMap<String, HeapFile>,
    pub indexes: &'a HashMap<String, BufferPool<Box<dyn DiskManager>>>,
    pub catalog: &'a Catalog,
    pub snapshot: &'a Snapshot,
    pub manager: &'a TransactionManager,
}

pub fn build(plan: &PlanNode, context: &BuildContext<'_>) -> Result<Box<dyn Executor>, Error> {
    let BuildContext {
        tables,
        indexes,
        catalog,
        snapshot,
        manager,
    } = context;
    let visible = |header: &TupleHeader| is_visible(header, snapshot, manager);
    // ...
}
```

`let BuildContext { ... } = context;`は，構造体のパターンでフィールドを変数に取り出す．`IndexScan`も，インデックスで引いた位置の版を`visible`で選ぶ．

### 構文

`START TRANSACTION`，`COMMIT`，`ROLLBACK`は，1つの`alt`にまとめた．文の`alt`はちょうど9個になる．
最初は`literal(keyword(Keyword::Commit)).value(Statement::Commit)`と書き，次のエラーになった(先頭だけを示す)．

```text
error[E0277]: the trait bound `Statement: Clone` is not satisfied
   --> iterations/iteration-18/solution/src/sql/parser.rs:113:20
    |
113 |             .value(Statement::StartTransaction),
    |              ----- ^^^^^^^^^^^^^^^^^^^^^^^^^^^ unsatisfied trait bound
    |              |
    |              required by a bound introduced by this call
    |
help: the trait `Clone` is not implemented for `Statement`
```

`value`は，読むたびに値を複製して返すので`Clone`を求める．`map(|_| Statement::Commit)`なら，読むたびに新しい値を作る．

### セッション

`Database`は`Session`を持つ．`execute`は`std::mem::take`で状態を取り出し，状態ごとに分ける．

```rust
        match std::mem::take(&mut self.session) {
            Session::Idle => match statement? {
                Statement::StartTransaction => {
                    self.session = Session::InTransaction(self.begin()?);
                    Ok(StatementResult::StartTransaction)
                }
                Statement::Commit => Ok(StatementResult::Commit),
                Statement::Rollback => Ok(StatementResult::Rollback),
                statement => {
                    let txn = self.begin()?;
                    let result = self.run(statement, &txn);
                    match result {
                        Ok(_) => txn.commit(&mut self.transactions),
                        Err(_) => txn.rollback(&mut self.transactions),
                    }
                    self.save_transactions()?;
                    result
                }
            },
            Session::InTransaction(txn) => match statement {
                // COMMIT，ROLLBACK，START TRANSACTION，ほかの文，構文のエラー
            },
            Session::Failed(txn) => match statement {
                Ok(Statement::Commit | Statement::Rollback) => {
                    txn.rollback(&mut self.transactions);
                    self.save_transactions()?;
                    Ok(StatementResult::Rollback)
                }
                _ => {
                    self.session = Session::Failed(txn);
                    Err(TransactionError::InFailedTransaction.into())
                }
            },
        }
```

- `txn`は`Session`から取り出したので，`commit(&mut self.transactions)`で`self`のフィールドを借りられる．`self.session`の中に入れたままでは，`self.session`と`self.transactions`を同時に借りることになる．
- どの腕でも，`Transaction`を`commit`か`rollback`で消費するか，`self.session`に入れ直す．入れ直し忘れると，トランザクションが終わらないまま消える．
- `run`は，文ごとに`self.transactions.snapshot(txn.xid())`でスナップショットを作り，`insert`，`update`，`delete`，`select`に渡す．
- `in_block`は，トランザクションの中で表を作る文と消す文を`25001`にする．

結合テストの`only_committed_transactions_remain_after_reopening`は，トランザクションを終えるときだけ状態をファイルに書く実装では，次のように失敗した．

```text
thread 'only_committed_transactions_remain_after_reopening' (164026) panicked at tests/transaction.rs:172:5:
assertion `left == right` failed
  left: [Varchar("x"), Varchar("bob")]
 right: [Varchar("alice")]
```

進行中のトランザクションの番号がファイルになく，開き直したあとの最初のトランザクションに同じ番号を振ったので，止まる前の版を自分の版として見てしまった．トランザクションを始めるときにも状態を書くようにして通した．

### REPL

`write_prompt`に`database.transaction_status()`を渡し，状態に合わせたプロンプトを書く．続きの行のプロンプトは変えない．

## 18-6 振り返り

1. 可視性の組み合わせ(自分，コミット済み，中止，進行中)と，セッションの状態の移り変わりを確かめる項目があるかを比べる．
2. `&self`で受け取ると，コミットしたあとの`Transaction`で文を実行したり，もう一度`rollback`したりするコードもコンパイルでき，誤りは実行するまで見つからない．`self`で受け取れば，コンパイルエラーになる．
3. `UPDATE`と`DELETE`のたびに古い版とそのインデックスの項目が残り，中止したトランザクションの版も残るので，表とインデックスは大きくなり続ける．PostgreSQLは，どのトランザクションからも見えなくなった版を`VACUUM`で片付け，空いた領域を再利用する．
4. 開き直したあとのトランザクションに，進行中だったトランザクションと同じ番号を振り，止まる前の変更が見えてしまう．
5. カタログも表として持ち，表の定義に版を付ければ，`ROLLBACK`で表を作る文を取り消せる．PostgreSQLのシステムカタログは，この形である．
6. 模範解答の設計ドキュメントは，照合スクリプトで問題がないことを確かめてある．

## 18-7 発展課題

解答例である．`START TRANSACTION READ ONLY`を読み，セッションの状態に読み取り専用かを持たせる．

```rust
fn transaction(input: &mut Tokens<'_>) -> ModalResult<Statement> {
    alt((
        (
            literal(keyword(Keyword::Start)),
            cut_err(literal(keyword(Keyword::Transaction))),
            opt((
                literal(keyword(Keyword::Read)),
                cut_err(literal(keyword(Keyword::Only))),
            )),
        )
            .map(|(_, _, read_only)| Statement::StartTransaction {
                read_only: read_only.is_some(),
            }),
        // COMMIT，ROLLBACK
    ))
    .parse_next(input)
}
```

```rust
enum Session {
    #[default]
    Idle,
    InTransaction { txn: Transaction, read_only: bool },
    Failed(Transaction),
}
```

トランザクションの中の文は，表を作る文の検査のあとに，読み取り専用の検査をしてから実行する．

```rust
                    let result = in_block(&statement)
                        .and_then(|()| check_access(&statement, read_only))
                        .and_then(|()| self.run(statement, &txn));
```

```rust
fn check_access(statement: &Statement, read_only: bool) -> Result<(), Error> {
    let name = match statement {
        Statement::Insert(_) => "INSERT",
        Statement::Update(_) => "UPDATE",
        Statement::Delete(_) => "DELETE",
        _ => return Ok(()),
    };
    if read_only {
        return Err(TransactionError::ReadOnly { statement: name }.into());
    }
    Ok(())
}
```

`TransactionError::ReadOnly`は，`SqlState::ReadOnlySqlTransaction`(`25006`)と`cannot execute INSERT in a read-only transaction`の形のメッセージに変換する．
`and_then`でつないだ検査は，どれかがエラーを返すと残りを実行しない．
