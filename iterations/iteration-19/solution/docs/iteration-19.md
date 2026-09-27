# Iteration 19：WALとクラッシュリカバリ(解説)

演習の各手順の模範解答と，その考え方を説明する．

## 19-1 準備

引き継いだ442個のテストがすべて通れば準備は終わりである．
Iteration 18の`ferrodb`で`kill -9`すると，枠に残っていたページの変更は失われる．前にREPLを終えたあとに加えた行は，コミットしていても見えなくなる．

## 19-2 文法と概念

課題の解答例である．`tests/`に置いた結合テストで確かめた．

```rust
use std::cell::RefCell;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::rc::Rc;

pub struct Numbered<W: Write> {
    out: W,
    count: usize,
}

impl<W: Write> Numbered<W> {
    pub fn new(out: W) -> Numbered<W> {
        Numbered { out, count: 0 }
    }

    pub fn line(&mut self, text: &str) -> io::Result<()> {
        self.count += 1;
        writeln!(self.out, "{}: {text}", self.count)
    }
}

pub struct Speaker {
    log: Rc<RefCell<Vec<String>>>,
}

impl Speaker {
    pub fn say(&self, text: &str) {
        self.log.borrow_mut().push(text.to_string());
    }
}

#[test]
fn tasks() {
    let mut numbered = Numbered::new(Vec::new());
    numbered.line("a").unwrap();
    numbered.line("b").unwrap();
    assert_eq!(numbered.out, b"1: a\n2: b\n");

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out");
    let mut writer = BufWriter::new(File::create(&path).unwrap());
    writer.write_all(b"hello").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"");
    writer.flush().unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"hello");

    let log = Rc::new(RefCell::new(Vec::new()));
    let first = Speaker { log: Rc::clone(&log) };
    let second = Speaker { log: Rc::clone(&log) };
    first.say("hi");
    second.say("bye");
    assert_eq!(Rc::strong_count(&log), 3);
    assert_eq!(*log.borrow(), vec!["hi", "bye"]);

    assert_ne!(crc32fast::hash(b"abc"), crc32fast::hash(b"abd"));
}
```

- `writeln!`は，`Write`を実装するどの型にも書ける．
- `BufWriter`に5バイトを書いても，`flush`するまでファイルは空である．
- `Rc::strong_count`は，元の`log`と，2つの`Speaker`が持つ複製を合わせて3である．

## 19-3 テストリスト

模範解答は[TESTLIST.md](../TESTLIST.md)である．

- レコードの形式とリカバリは`wal`の単体テストで，WALの規則は`storage::buffer`の単体テストで，強制終了のあとのことは結合テストで確かめた．
- `recovery_redoes_pages_after_the_last_checkpoint`は，チェックポイントの前のページの記録(やり直さない)，あとのページの記録(やり直す)，ないファイルの記録(飛ばす)，コミットしたトランザクションと進行中のトランザクションを1つのログに並べた．
- 強制終了の結合テストは，`crash_after`という補助関数にまとめた．環境変数があれば子プロセスとして文を実行して`abort`し，なければ同じテストを子プロセスとして起動して待つ．
- 引き継いだ結合テストのうち，データディレクトリのファイルの一覧を確かめる2つに`wal`が加わる．

## 19-4 設計ドキュメント

| ファイル | 変更 | 理由 |
| --- | --- | --- |
| `c4-container.md` | ログのファイル`wal`を加え，`xact`をチェックポイントで書くようにした | 変更をまずログに書く |
| `c4-component.md` | `wal`を加え，`database`と`storage::buffer`から`wal`への依存と，`wal`から`storage::disk`，`storage::page`，`txn`への依存を加えた | ログを書くモジュールと，リカバリでページとトランザクションの状態を書くモジュールができた |
| `code-types.md` | ログの図を加え，`BufferPool`，`Frame`，`PageGuard`，`PageLog`，`DiskManager`，`HeapFile`，`Database`，`TransactionManager`を更新した | ページの変更を記録し，ディスクに届けるようになった |
| `layout.md` | ログのレコードの配置を加え，`xact`を書く時点を直した | 新しいファイルの形式ができた |
| `code-sequence.md` | コミットとリカバリの流れを加えた | コミットでログを待ち，開くときにやり直す |

- `page_lsn`は，ページの先頭でなく枠に持つ．B+木のノードはページのバイト列をすべて使うので，ページの先頭に場所がない．ページを丸ごと記録すれば，やり直しはどのページにも何度でも同じ結果になるので，`page_lsn`はWALの規則のためだけに要る．

## 19-5 テスト駆動の実装

### レコードと`WalWriter`

```rust
    pub fn append(&mut self, record: &WalRecord) -> io::Result<Lsn> {
        let payload = record.encode();
        let mut bytes = Vec::with_capacity(RECORD_HEADER_SIZE + payload.len());
        let len = u32::try_from(payload.len()).expect("record fits in u32");
        bytes.extend_from_slice(&len.to_le_bytes());
        bytes.extend_from_slice(&crc32fast::hash(&payload).to_le_bytes());
        bytes.extend_from_slice(&payload);
        if let Err(error) = self.out.write_all(&bytes) {
            self.failed = true;
            return Err(error);
        }
        self.next = Lsn(self.next.0 + u64::try_from(bytes.len()).expect("usize fits in u64"));
        Ok(self.next)
    }
```

`read_records`は，次のどれかのレコードで止まり，それまでのレコードと終わりの位置を返す．バイト数の分のバイトがないレコード，CRC-32が合わないレコード，種類を読めないレコードである．

```rust
impl WalWriter<File> {
    pub fn flush_to(&mut self, lsn: Lsn) -> io::Result<()> {
        self.check()?;
        if lsn <= self.flushed {
            return Ok(());
        }
        self.out.flush()?;
        self.out.get_ref().sync_data()?;
        self.flushed = self.next;
        Ok(())
    }
}
```

`open`は，リカバリで正しく読めた終わりの位置より後ろを`set_len`で切り捨ててから書く．切り捨てないと，壊れた末尾の後ろに新しいレコードを書き，次のリカバリがそこまで読めない．

### リカバリ

```rust
pub fn recover(dir: &Path, wal: &Path, manager: &mut TransactionManager) -> io::Result<Lsn> {
    // ログのファイルを読む(なければ空)
    let (records, end) = read_records(&bytes);
    let start = records
        .iter()
        .rposition(|(record, _)| *record == WalRecord::Checkpoint)
        .map_or(0, |position| position + 1);
    for (record, _) in &records[start..] {
        match record {
            WalRecord::Begin { xid } => manager.set_status(*xid, TxnStatus::InProgress),
            WalRecord::Commit { xid } => manager.set_status(*xid, TxnStatus::Committed),
            WalRecord::Abort { xid } => manager.set_status(*xid, TxnStatus::Aborted),
            WalRecord::PageImage { file, page, bytes } => redo_page(&dir.join(file), *page, bytes)?,
            WalRecord::Checkpoint => {}
        }
    }
    manager.abort_unfinished();
    Ok(end)
}
```

- `rposition`は，後ろから探して，条件を満たす最初の要素の位置を返す．`map_or(0, ...)`は，見つからなければ0を返す．
- `set_status`は，知らない番号なら，そこまでの状態を`Vec`に加える．`begin`は次に`Vec`の長さを番号に使うので，ログにあった番号を再び振らない．Iteration 18で`xact`をトランザクションの開始のたびに書いたのは，この番号を覚えるためだった．ログに`Begin`を記録するので，`xact`はチェックポイントで書けばよい．

### バッファプールのログ

`PageGuard`は，`write`で書き換えたかを`Cell<bool>`で覚え，捨てるときに記録する．

```rust
impl Drop for PageGuard<'_> {
    fn drop(&mut self) {
        if let Some(log) = self.log
            && self.written.get()
        {
            let record = WalRecord::PageImage {
                file: log.file.clone(),
                page: self.page_id(),
                bytes: Box::new(*self.frame.page.borrow().bytes()),
            };
            if let Ok(lsn) = log.wal.borrow_mut().append(&record) {
                self.frame.page_lsn.set(lsn);
            }
        }
        self.frame.pin_count.set(self.frame.pin_count.get() - 1);
    }
}
```

- 最初は`if self.written.get() { if let Some(log) = self.log { ... } }`と書き，`cargo clippy`の`collapsible_if`で，`if let`をつなぐ形にするよう求められた．
- `append`が失敗すると，`WalWriter`が失敗を覚える．`Drop`からエラーを返せないので，次のコミットの`flush_to`でエラーにする．

書き戻すときは，枠の`page_lsn`までログを届けてからファイルに書く．

```rust
        if frame.dirty.get() {
            if let Some(log) = &self.log {
                log.wal.borrow_mut().flush_to(frame.page_lsn.get())?;
            }
            self.disk
                .borrow_mut()
                .write_page(page_id, frame.page.borrow().bytes())?;
            frame.dirty.set(false);
        }
```

`written_page_goes_to_the_log_before_the_disk`は，最初はページを追い出す前のログのファイルが空であることも確かめていた．レコードは8192バイトより大きく，`BufWriter`はためずにファイルへ書くので，この確かめは誤りだった．追い出したあとにログのファイルから`PageImage`を読めることを確かめる形にした．

### `Database`

`Database`は`Option<Rc<RefCell<WalWriter<File>>>>`を持ち，表とインデックスのプールを作るときに`Rc::clone`で`PageLog`に渡す．

```rust
    fn commit(&mut self, txn: Transaction) -> Result<(), Error> {
        if let Some(wal) = &self.wal {
            let mut wal = wal.borrow_mut();
            let written = wal
                .append(&WalRecord::Commit { xid: txn.xid() })
                .and_then(|lsn| wal.flush_to(lsn));
            if let Err(error) = written {
                drop(wal);
                txn.rollback(&mut self.transactions);
                return Err(error.into());
            }
        }
        txn.commit(&mut self.transactions);
        Ok(())
    }
```

- `wal`は`self.wal`の中の`RefCell`を借りた`RefMut`である．`drop(wal)`で先に返してから，トランザクションを中止する．
- `checkpoint`は，すべての表とインデックスのプールの`flush_all`でページを書いて`sync`し，`xact`を書いてから，`Checkpoint`を記録して`flush_to`する．
- `open`は，`xact`を読み，`wal::recover`でやり直す．そのあとでカタログと表とインデックスを開き，チェックポイントを取る．
- `DiskManager`に`sync`を加え，`FileDiskManager`は`sync_data`，`MemoryDiskManager`は何もしない．`xact`とカタログも，名前を変える前に`sync_data`する．

表とインデックスを作る文と消す文のあとにもチェックポイントを取る．取らないと，表を消して同じ名前で作り直したあとに止まったとき，消した表のページの記録を新しい表のファイルにやり直してしまう．

### 強制終了のテスト

```rust
fn crash_after(test: &str, statements: &[&str]) -> tempfile::TempDir {
    if let Ok(dir) = std::env::var(CHILD_DIR) {
        let mut db = Database::open(Path::new(&dir)).unwrap();
        for sql in statements {
            db.execute(sql).unwrap();
        }
        std::process::abort();
    }
    let dir = tempfile::tempdir().unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args([test, "--exact", "--nocapture"])
        .env(CHILD_DIR, dir.path())
        .status()
        .unwrap();
    assert!(!status.success());
    dir
}
```

ページをやり直さないリカバリでは，`committed_rows_survive_a_crash`は次のように失敗する．加えた行のページは枠に残ったまま，プロセスが止まったからである．

```text
thread 'committed_rows_survive_a_crash' (220239) panicked at tests/recovery.rs:56:5:
assertion `left == right` failed
  left: []
 right: [Integer(1), Integer(2)]
```

## 19-6 振り返り

1. レコードの形式，壊れた末尾，チェックポイントの前後，WALの規則，強制終了のあとを確かめる項目があるかを比べる．
2. ページを丸ごと記録すると，1行の変更でも8192バイト以上を書くが，やり直すときにページの今の内容を読まなくてよい．タプルの追加や`xmax`の書き換えを記録すると，ログは小さくなる．その代わり，やり直すときにページの状態を知る必要がある．同じ変更を2回しないように，どのレコードまで反映したか(LSN)をページに書く．
3. 複数の行を1つのトランザクションにまとめれば，`sync_data`はコミットの1回だけになる．PostgreSQLは，同時にコミットする複数のトランザクションのログを1回の書き込みでまとめて届ける(グループコミット)．
4. 表とインデックスのファイル，`xact`，カタログを，チェックポイントのレコードより先にディスクへ届ける．模範解答は`DiskManager::sync`と`sync_data`で届ける．
5. 消した表のページの記録を新しい表のファイルにやり直し，新しい表に古い行が現れる．表を作る文と消す文のあとのチェックポイントは，これを防ぐ．
6. 模範解答の設計ドキュメントは，照合スクリプトで問題がないことを確かめてある．

## 19-7 発展課題

解答例である．`WalWriter<File>`に，ファイルを空にする`truncate`を加える．

```rust
    pub fn truncate(&mut self) -> io::Result<()> {
        self.check()?;
        self.out.flush()?;
        let file = self.out.get_mut();
        file.set_len(0)?;
        file.seek(SeekFrom::Start(0))?;
        self.next = Lsn(0);
        self.flushed = Lsn(0);
        Ok(())
    }
```

`checkpoint`で，ページと`xact`を書いたあと，`Checkpoint`を記録する前に呼ぶ．

```rust
        let mut wal = wal.borrow_mut();
        wal.truncate()?;
        let lsn = wal.append(&WalRecord::Checkpoint)?;
        wal.flush_to(lsn)?;
```

- 位置は0から数え直すので，枠に残った`page_lsn`は，ログの今の終わりより大きいことがある．チェックポイントのあとの枠のページはどれも書き戻し済みで，書き換えれば新しい位置が入るので，大きな`page_lsn`で`flush_to`を呼ぶのは，書き戻し済みのページを書き戻すときだけである．そのときもログを届けるだけで，誤りにはならない．
- `xact`を書いてからログを空にする．その間に止まっても，古いログのやり直しは同じ結果になるので，状態は正しい．

`CHECKPOINT`の文は，自分のトランザクションの中で実行されるので，文のあとのログには，チェックポイントのレコード(9バイト)とコミットのレコード(13バイト)が残る．
