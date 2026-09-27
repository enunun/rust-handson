# Iteration 14：ヒープファイルとデータディレクトリ(解説)

演習の各手順の模範解答と，その考え方を説明する．

## 14-1 準備

引き継いだ335個のテストがすべて通り，`cargo run`で起動し直すと表が消えることを確かめれば，準備は終わりである．

## 14-2 文法と概念

課題の解答例である．`tests/`に置いた結合テストで確かめた．

```rust
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

pub fn hex(text: &str) -> String {
    text.bytes().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn tasks() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("blocks");
    fs::write(&path, [1, 2, 3, 4].repeat(4)).unwrap();
    let mut file = OpenOptions::new().read(true).open(&path).unwrap();
    let mut buffer = [0; 4];
    file.seek(SeekFrom::Start(8)).unwrap();
    file.read_exact(&mut buffer).unwrap();
    assert_eq!(buffer, [1, 2, 3, 4]);

    assert_eq!(fs::metadata(&path).unwrap().len(), 16);
    file.seek(SeekFrom::Start(20)).unwrap();
    let error = file.read_exact(&mut buffer).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);

    let heap = Path::new("data").join("users").with_extension("heap");
    assert_eq!(heap.display().to_string(), "data/users.heap");

    assert_eq!(hex("a/b"), "612f62");
}
```

- `[1, 2, 3, 4].repeat(4)`は，スライスを4回つないだ`Vec`である．
- ファイルの終わりより後ろへの`seek`は成功する．その位置から読むと`UnexpectedEof`になる．
- `display()`の値は`Display`を実装するので，`to_string()`で`String`にできる．
- `/`は`0x2f`である．16進数にすれば，表の名前に`/`があってもディレクトリの区切りにならない．

## 14-3 テストリスト

模範解答は[TESTLIST.md](../TESTLIST.md)である．

- `storage::disk`，`storage::heap`，`catalog`，`error`，結合テストの順に並べた．下の層から作れば，上の層のテストで下の層を信用できる．
- `DiskManager`の2つの実装には，同じ手順のテストを使った．手順を`&mut dyn DiskManager`を受け取る関数`write_and_read`にまとめ，メモリー用とファイル用のテストから呼ぶ．
- ファイルに残ることは，値を`drop`してファイルを閉じ，開き直してから読んで確かめた．ヒープファイルの大きさがページの数の8192倍であることも確かめた．
- カタログは，日本語の表の名前，`NOT NULL`の列，一意性制約を持つ表を保存して読み込み，元の定義と比べた．ファイルがないとき，途中で終わるときの項目も書いた．
- 既存テストへの影響は，`storage::heap`と`exec::dml`の単体テストで`HeapFile::new`に`MemoryDiskManager`を渡し，操作の結果を`unwrap`することである．SQLの結合テストは1つも変えていない．
- 結合テストは`tests/persistence.rs`に書いた．`Database::open`で開き，文を実行し，`drop`して開き直す流れである．

## 14-4 設計ドキュメント

| ファイル | 変更 | 理由 |
| --- | --- | --- |
| `c4-container.md` | データディレクトリの`catalog`と`*.heap`を`ContainerDb`で加え，ライブラリからの読み書きを描いた．REPLのバイナリはサブコマンド`repl`にした | 表がプロセスの外のファイルに残るようになった |
| `c4-component.md` | `storage::disk`を加え，`storage::heap`と`database`からの依存，`main`から`crate`(`Database::open`)への依存，`error`から`storage::heap`への依存を加えた | ページの読み書きを`DiskManager`に任せ，起動時にデータディレクトリを開くようになった |
| `code-types.md` | `DiskManager`，`FileDiskManager`，`MemoryDiskManager`，`HeapError`，`PageId`，`PageBytes`を加え，`HeapFile`，`Page`，`Catalog`，`Database`のメソッドとフィールドを更新した | ストレージの型が増え，`HeapFile`の操作が`Result`を返すようになった |
| `layout.md` | ヒープファイルの中のページの並びと，カタログのファイルのバイト配置を加えた | ファイルの形式ができた |
| `code-sequence.md` | 起動の図を加え，REPLの図の`run`を`run_with`にした | `main`がデータディレクトリを開いてからREPLを始めるようになった |

- トレイトと実装の関係は，`classDiagram`の実現の矢印(`DiskManager <|.. FileDiskManager`)で描いた．
- `HeapFile`は`DiskManager`を所有するので合成(`*--`)で，ページは操作のたびに読んで捨てるので依存(`..>`)で描いた．
- データディレクトリの2種類のファイルは，C4のコンテナー図で`ContainerDb`として描いた．プロセスの外にあってデータを保存するものだからである．
- カタログのファイルの`packet`図では，可変長の名前を4バイトの幅で描き，そのことを図の下に書いた．

## 14-5 テスト駆動の実装

### `storage::disk`

```rust
fn write_and_read(disk: &mut dyn DiskManager) {
    assert_eq!(disk.page_count(), 0);
    assert_eq!(disk.allocate_page().unwrap(), 0);
    assert_eq!(disk.allocate_page().unwrap(), 1);
    disk.write_page(1, &page_with(7)).unwrap();
    let mut buffer = Box::new([0; PAGE_SIZE]);
    disk.read_page(1, &mut buffer).unwrap();
    assert_eq!(buffer[0], 7);
    disk.read_page(0, &mut buffer).unwrap();
    assert_eq!(buffer[0], 0);
    assert_eq!(disk.page_count(), 2);
}

#[test]
fn memory_disk_reads_what_was_written() {
    write_and_read(&mut MemoryDiskManager::default());
}

#[test]
fn file_disk_reads_what_was_written() {
    let dir = tempfile::tempdir().unwrap();
    write_and_read(&mut FileDiskManager::open(&dir.path().join("t.heap")).unwrap());
}
```

最初に`MemoryDiskManager`を作った．`Vec<Box<PageBytes>>`を持ち，ないページの読み書きは`UnexpectedEof`のエラーにする．

```rust
impl DiskManager for MemoryDiskManager {
    fn read_page(&self, page_id: PageId, buffer: &mut PageBytes) -> io::Result<()> {
        match self.pages.get(page_id) {
            Some(page) => {
                buffer.copy_from_slice(&page[..]);
                Ok(())
            }
            None => Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!("page {page_id} does not exist"),
            )),
        }
    }

    fn allocate_page(&mut self) -> io::Result<PageId> {
        self.pages.push(Box::new([0; PAGE_SIZE]));
        Ok(self.pages.len() - 1)
    }
    // write_pageとpage_countは省略
}
```

次に，同じ手順のテストを`FileDiskManager`に渡した．

```rust
impl FileDiskManager {
    pub fn open(path: &Path) -> io::Result<FileDiskManager> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        let len = usize::try_from(file.metadata()?.len()).expect("file size fits in usize");
        Ok(FileDiskManager {
            file,
            page_count: len / PAGE_SIZE,
        })
    }

    fn position(page_id: PageId) -> SeekFrom {
        SeekFrom::Start(u64::try_from(page_id * PAGE_SIZE).expect("offset fits in u64"))
    }
}

impl DiskManager for FileDiskManager {
    fn read_page(&self, page_id: PageId, buffer: &mut PageBytes) -> io::Result<()> {
        let mut file = &self.file;
        file.seek(FileDiskManager::position(page_id))?;
        file.read_exact(buffer)
    }

    fn write_page(&mut self, page_id: PageId, buffer: &PageBytes) -> io::Result<()> {
        self.file.seek(FileDiskManager::position(page_id))?;
        self.file.write_all(buffer)
    }

    fn allocate_page(&mut self) -> io::Result<PageId> {
        let page_id = self.page_count;
        self.write_page(page_id, &[0; PAGE_SIZE])?;
        self.page_count += 1;
        Ok(page_id)
    }

    fn page_count(&self) -> usize {
        self.page_count
    }
}
```

- ページの数は，開いたときにファイルの大きさから求め，`allocate_page`で1つずつ増やす．ページの数を聞くたびにファイルの大きさを調べなくて済む．
- `read_page`は`&self`なので，`let mut file = &self.file;`で`&File`を通して読む．
- `create`は`truncate(true)`で開く．`CREATE TABLE`で，同じ名前のファイルが残っていても空の表として始める．
- `file_disk_keeps_pages_after_reopening`は，2ページを加えて`drop`し，開き直して読む．ページの数と内容のほかに，ファイルの大きさが16384バイトであることも確かめる．

### `storage::page`と`storage::heap`

`Page`には，ディスクから読んだバイト列でページを作る`from_bytes`と，書くためのバイト列を返す`bytes`を加えた．どちらもバイト列を複製しない．

`HeapFile`は`Box<dyn DiskManager>`を持つ．Iteration 13で`self.pages[id.page]`を使っていたところを，ページを読む`read`と書き戻す`write`に置き換えた．

```rust
    pub fn insert(&mut self, tuple: &[u8]) -> Result<RowId, HeapError> {
        if let Some(last) = self.disk.page_count().checked_sub(1) {
            let mut page = self.read(last)?;
            match page.insert(tuple) {
                Ok(slot) => {
                    self.write(last, &page)?;
                    return Ok(RowId { page: last, slot });
                }
                Err(PageError::PageFull) => {}
                Err(error) => return Err(error.into()),
            }
        }
        let mut page = Page::new();
        let slot = page.insert(tuple)?;
        let page_id = self.disk.allocate_page()?;
        self.write(page_id, &page)?;
        Ok(RowId {
            page: page_id,
            slot,
        })
    }

    fn read(&self, page_id: PageId) -> Result<Page, HeapError> {
        let mut data = Box::new([0; PAGE_SIZE]);
        self.disk.read_page(page_id, &mut data)?;
        Ok(Page::from_bytes(data))
    }
```

- 最後のページの番号は`page_count() - 1`だが，ページがないと引き算が負になる．`checked_sub(1)`は，そのとき`None`を返す．
- 新しいページには，先にタプルを置いてから`allocate_page`で番号をもらう．大きすぎるタプルは`page.insert`でエラーになるので，空のページがファイルに残らない．
- 操作は，ページの`PageError`，復号の`TupleError`，ファイルの`io::Error`のどれでも失敗しうる．3つをまとめる`HeapError`を作り，それぞれからの`From`を実装して`?`で変換する．
- `tuples`は，読んだページを捨てる前にタプルを`to_vec`で複製して返す．Iteration 13の`&[u8]`は，`HeapFile`の中のページを指していた．

`Database`は`#[derive(Debug)]`を持つので，`HeapFile`にも`Debug`が要る．`HeapFile`に`#[derive(Debug)]`を付けると，次のエラーになる(先頭だけを示す)．

```text
error[E0277]: `(dyn DiskManager + 'static)` doesn't implement `Debug`
  --> src/storage/heap.rs:48:5
   |
46 | #[derive(Debug)]
   |          ----- in this derive macro expansion
47 | pub struct HeapFile {
48 |     disk: Box<dyn DiskManager>,
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^ the trait `Debug` is not implemented for `(dyn DiskManager + 'static)`
```

`dyn DiskManager`は，`DiskManager`の持つメソッドしか呼べない．`Debug`は`DiskManager`のメソッドではないので，導出した`Debug`はフィールドを書けない．
Iteration 13の`Page`と同じく，ページの数だけを書く`Debug`を実装した．`+ 'static`は，Iteration 15で説明するライフタイムの注釈である．

既存の単体テストは，`HeapFile::new()`を`HeapFile::new(Box::new(MemoryDiskManager::default()))`にし，操作の結果を`unwrap`した．`exec::dml`の関数は`?`で`HeapError`を`Error`に変換するので，本体は`heap.delete(*id)?`の1か所だけが変わった．

### `catalog`

```rust
#[test]
fn saved_catalog_is_loaded_with_the_same_tables() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("catalog");
    let mut catalog = Catalog::default();
    catalog.create_table(users()).unwrap();
    catalog.create_table(constrained()).unwrap();
    catalog.save(&path).unwrap();
    let loaded = Catalog::load(&path).unwrap();
    assert_eq!(loaded.table_names(), vec!["USERS", "日本"]);
    assert_eq!(loaded.table("USERS"), Ok(&users()));
    assert_eq!(loaded.table("日本"), Ok(&constrained()));
}
```

形式は[layout.md](../design/layout.md)のとおりである．符号化は`Vec<u8>`の末尾に書き足し，復号は`&mut &[u8]`の入力を読み進める．

```rust
/// 先頭の`len`バイトを取り出し，`input`を残りのバイト列にする．
fn take(input: &mut &[u8], len: usize) -> io::Result<Vec<u8>> {
    if input.len() < len {
        return Err(corrupted());
    }
    let (head, rest) = input.split_at(len);
    let head = head.to_vec();
    *input = rest;
    Ok(head)
}

fn read_string(input: &mut &[u8]) -> io::Result<String> {
    let len = read_u16(input)?;
    String::from_utf8(take(input, len)?).map_err(|_| corrupted())
}
```

- `take`が返すのは複製した`Vec<u8>`である．`&[u8]`を返すと，戻り値が`input`のどの借用に結びつくかを書く必要がある．この書き方(ライフタイム注釈)はIteration 15で扱う．
- 足りないバイト，知らない型の番号，UTF-8でない名前は，どれも`io::ErrorKind::InvalidData`のエラーにした．ファイルを読む関数のエラーとまとめて`io::Result`で返せる．
- `save`は`catalog.tmp`に書いてから`fs::rename`する．`load`は，ファイルがない(`NotFound`)ときだけ空のカタログを返す．
- `table_names`は`HashMap`のキーを並べ替えて返す．同じカタログからは，いつも同じバイト列のファイルができる．

### `error`

`io::Error`を`58030`(`IoError`)の`Error`に変換する．メッセージは`could not access file: 理由`の形である．

```rust
impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Error {
        Error {
            sqlstate: SqlState::IoError,
            message: format!("could not access file: {error}"),
            position: None,
        }
    }
}
```

`HeapError`からの変換は，中のエラーをそれぞれの`From`に渡す．

### `database`，`repl`，`main`

```rust
    pub fn open(dir: &Path) -> Result<Database, Error> {
        fs::create_dir_all(dir)?;
        let catalog = Catalog::load(&dir.join(CATALOG_FILE))?;
        let mut tables = HashMap::new();
        for name in catalog.table_names() {
            let disk = FileDiskManager::open(&heap_path(dir, &name))?;
            tables.insert(name, HeapFile::new(Box::new(disk)));
        }
        Ok(Database {
            catalog,
            tables,
            data_dir: Some(dir.to_path_buf()),
        })
    }
```

- `Database`は`data_dir: Option<PathBuf>`を持つ．`CREATE TABLE`は，`data_dir`があれば`FileDiskManager::create`で，なければ`MemoryDiskManager`でヒープファイルを作る．
- `CREATE TABLE`はヒープファイルを作ってからカタログを保存し，`DROP TABLE`はカタログを保存してからヒープファイルを消す．どちらの順でも，途中で止まったときに残るのは，カタログにない余分なファイルである．カタログにある表のファイルがないという状態は起きない．
- `heap_path`は，表の名前の各バイトを`{byte:02x}`で書いてつなぐ．
- `repl::run_with`は，受け取ったデータベースで文を実行する．`run`は`run_with(Database::new(), ...)`を呼ぶだけになり，引き継いだREPLのテストはそのまま通る．
- `main`は`clap`で引数を読み，`--data-dir`があれば`Database::open`，なければ`Database::new`でデータベースを作る．開けなければ`ferrodb: メッセージ`を標準エラー出力に書き，`ExitCode::FAILURE`を返す．

`tests/persistence.rs`の4つの項目は，ここまでの実装で通る．使用例は次のとおりに動く．

```console
$ cargo run -q -- repl --data-dir ./data
ferrodb> CREATE TABLE t (a INTEGER);
CREATE TABLE
ferrodb> INSERT INTO t VALUES (1);
INSERT 0 1
$ cargo run -q -- repl --data-dir ./data
ferrodb> SELECT * FROM t;
 A
---
 1
(1 row)
```

データディレクトリには，20バイトの`catalog`と，1ページ(8192バイト)の`54.heap`ができる．

```console
$ od -A d -t x1 data/catalog
0000000 01 00 00 00 01 00 54 01 00 01 00 41 00 00 00 00
0000016 00 01 00 00
0000020
```

表の数1，名前の長さ1と`T`(`0x54`)，列の数1，列の名前の長さ1と`A`(`0x41`)，型の番号0(`INTEGER`)，長さ0，`NULL`を許す(1)，一意性制約の数0の順である．

## 14-6 振り返り

1. 境界(ファイルがない，ファイルが途中で終わる，ないページ)の項目と，開き直して確かめる項目があるかを比べる．
2. `HeapFile<D: DiskManager>`にすると，`HeapFile<FileDiskManager>`と`HeapFile<MemoryDiskManager>`は別の型になる．`tables: HashMap<String, HeapFile<...>>`には1つの型しか入らないので，`Database`も`Database<D>`にして，`open`と`new`で違う型を返すことになる．`Box<dyn DiskManager>`なら，どちらのデータベースも同じ`Database`型である．代わりに，ページを読むたびに動的ディスパッチの呼び出しが1回増える．
3. `INSERT`の文は，制約を検査するためにまず表のすべてのページを読み，行ごとに最後のページを1回読んで1回書く．300行を1行ずつ`INSERT`すると，文ごとに表のすべてのページを読み直すので，表が大きくなるほど1文で読むページが増える．Iteration 15のバッファプールは，読んだページをメモリーの枠に残し，2回目からはファイルを読まずに枠のページを使う．
4. カタログを先に保存すると，カタログにある表のヒープファイルのない状態が残りうる．模範解答の`open`は`FileDiskManager::open`でファイルを作るので空の表として開けるものの，行が消えたことに気づけない．ヒープファイルを先に作れば，残るのはカタログにない余分なファイルだけで，次の`CREATE TABLE`が`create`で上書きする．
5. 表の名前には，`"a/b"`や`".."`のように，引用符で囲めばファイル名に使えない文字を入れられる．`"../x"`をそのままファイル名にすると，データディレクトリの外にファイルを作る．16進数なら`0`〜`9`と`a`〜`f`だけになる．大文字と小文字を区別しないファイルシステムでも，`"t"`(`74`)と`T`(`54`)は別のファイルになる．
6. 模範解答の設計ドキュメントは，照合スクリプトで問題がないことを確かめてある．

## 14-7 発展課題

解答例である．`FileDiskManager::open`で，ファイルの大きさが8192の倍数かを調べる．

```rust
    pub fn open(path: &Path) -> io::Result<FileDiskManager> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        let len = usize::try_from(file.metadata()?.len()).expect("file size fits in usize");
        if len % PAGE_SIZE != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "size {len} of {} is not a multiple of {PAGE_SIZE}",
                    path.display()
                ),
            ));
        }
        Ok(FileDiskManager {
            file,
            page_count: len / PAGE_SIZE,
        })
    }
```

```rust
#[test]
fn file_of_a_partial_page_is_invalid_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.heap");
    std::fs::write(&path, vec![0; PAGE_SIZE + 1]).unwrap();
    let error = FileDiskManager::open(&path).err().unwrap();
    assert_eq!(error.kind(), io::ErrorKind::InvalidData);
}
```

テストで`unwrap_err()`を使うと，次のエラーになる．

```text
error[E0277]: `disk::FileDiskManager` doesn't implement `Debug`
   --> src/storage/disk.rs:202:50
    |
202 |         let error = FileDiskManager::open(&path).unwrap_err();
    |                                                  ^^^^^^^^^^ the trait `Debug` is not implemented for `disk::FileDiskManager`
    |
note: required by a bound in `Result::<T, E>::unwrap_err`
```

`unwrap_err`は，`Ok`だったときに中の値を表示してパニックするので，`Ok`の型に`Debug`を求める．`err()`で`Option<io::Error>`にしてから`unwrap`すれば，`FileDiskManager`の`Debug`は要らない．

`Database::open`はこのエラーを`58030`の`Error`に変換し，`main`が表示して終わる．

```console
$ head -c 100 /dev/zero >> data/54.heap
$ echo "SELECT * FROM t;" | cargo run -q -- repl --data-dir ./data
ferrodb: could not access file: size 8292 of ./data/54.heap is not a multiple of 8192
```

大きさの検査を加えても，カタログの項目は変わらない．カタログは`Catalog::load`が復号のときに調べている．
