# Iteration 13：ページとタプルのバイト表現(解説)

演習の各手順の模範解答と，その考え方を説明する．

## 13-1 準備

引き継いだ318個のテストがすべて通れば準備は終わりである．

## 13-2 文法と概念

課題の解答例である．

```rust
pub fn encode_numbers(values: &[i32]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

pub fn decode_numbers(mut bytes: &[u8]) -> Option<Vec<i32>> {
    let mut values = Vec::new();
    while !bytes.is_empty() {
        if bytes.len() < 4 {
            return None;
        }
        let (value, rest) = bytes.split_at(4);
        values.push(i32::from_le_bytes(value.try_into().ok()?));
        bytes = rest;
    }
    Some(values)
}

pub fn bitmap(flags: &[bool]) -> Vec<u8> {
    let mut bytes = vec![0; flags.len().div_ceil(8)];
    for (index, &flag) in flags.iter().enumerate() {
        if flag {
            bytes[index / 8] |= 1 << (index % 8);
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t() {
        let bytes = encode_numbers(&[1, -1, 256]);
        assert_eq!(bytes, vec![1, 0, 0, 0, 255, 255, 255, 255, 0, 1, 0, 0]);
        assert_eq!(decode_numbers(&bytes), Some(vec![1, -1, 256]));
        assert_eq!(decode_numbers(&bytes[..5]), None);
        assert!(u8::try_from(300).is_err());
        assert_eq!(u8::try_from(255), Ok(255));
        assert_eq!(
            bitmap(&[true, false, false, true, false, false, false, false, true]),
            vec![0b0000_1001, 0b0000_0001]
        );
    }
}
```

- `decode_numbers`の引数を`mut bytes: &[u8]`にし，読んだ残りのスライスを`bytes`に入れ直す．スライスそのものは複製せず，指す範囲だけが変わる．
- `try_into()`の結果の`Result`は，`.ok()?`で`Option`の`?`に渡せる．
- `256`は`[0, 1, 0, 0]`になる．リトルエンディアンでは下位のバイトが先に来る．

## 13-3 テストリスト

模範解答は[TESTLIST.md](../TESTLIST.md)である．

- タプル，ページ，ヒープファイル，行の変更，結合テストの順に並べた．下の層から順に作れば，上の層のテストで下の層を信用できる．
- タプルの単体テストでは，4つの型の値のバイト列を1つずつ期待値に書いた．`日本`は6バイトなので，長さの2バイトは`[6, 0]`である．
- ページの単体テストでは，タプルを2つ置いたページの先頭の12バイトと末尾の5バイトを比べ，ヘッダーとスロットの書き方を確かめた．
- 空きの境界は，4000バイトのタプルを2つ置いたページで確かめた．残りは8192 - 4 - 8 - 8000 = 180バイトで，スロットの4バイトを除くと176バイトのタプルまで置ける．
- 既存テストへの影響は，`exec::dml`の単体テストで表を`HeapFile`で渡すことである．SQLの結合テストは1つも変えていない．

## 13-4 設計ドキュメント

| ファイル | 変更 | 理由 |
| --- | --- | --- |
| `layout.md` | 新たに作り，ページの先頭とタプルのバイト配置を`packet`図で描いた | バイト配置は，型の図や処理の流れの図では表せない |
| `c4-component.md` | `storage`の境界と，`storage::heap`，`storage::page`，`storage::tuple`を加えた．`database`，`exec::build`，`exec::dml`，`error`からの依存を加えた | 表の行をページに置く層ができた |
| `code-types.md` | 3つ目の図に`HeapFile`，`RowId`，`Page`，`SlotId`，`PageError`，`TupleError`を描き，`Database`のフィールドを`tables: HashMap<String, HeapFile>`にした | ストレージの型ができた |
| `code-sequence.md` | `UPDATE`の図を，行と位置を読み，検査してから位置のタプルを書き換える流れにした | 表の行の持ち方が変わった |

- `SlotId`は`u16`の別名なので，型の図では`<<type>>`で描いた．
- `packet`図の目盛りはビットなので，2バイトの数は16ビットの幅で描いた．ページ全体(8192バイト)は図に収まらないので，先頭の12バイトだけを描き，全体の形は文字の図で示した．

## 13-5 テスト駆動の実装

### タプル

```rust
#[test]
fn null_sets_a_bit_and_takes_no_bytes() {
    let row = vec![Value::Null, Value::BigInt(3), Value::Null, Value::Null];
    let bytes = encode_tuple(&row, &all_types());
    assert_eq!(bytes[0], 0b0000_1101);
    assert_eq!(bytes.len(), 1 + 8);
}
```

```rust
pub fn encode_tuple(row: &[Value], schema: &TableSchema) -> Vec<u8> {
    let mut bytes = vec![0; bitmap_len(schema.columns.len())];
    for (index, value) in row.iter().enumerate() {
        match value {
            Value::Null => bytes[index / 8] |= 1 << (index % 8),
            Value::Integer(n) => bytes.extend_from_slice(&n.to_le_bytes()),
            Value::BigInt(n) => bytes.extend_from_slice(&n.to_le_bytes()),
            Value::Boolean(b) => bytes.push(u8::from(*b)),
            Value::Varchar(s) => {
                let len = u16::try_from(s.len()).expect("VARCHAR fits in a page");
                bytes.extend_from_slice(&len.to_le_bytes());
                bytes.extend_from_slice(s.as_bytes());
            }
        }
    }
    bytes
}
```

- ビットマップの分の0を並べてから，値のバイトを末尾に加える．`u8::from(true)`は`1`である．
- `VARCHAR`の長さは，文字の数でなく`s.len()`(UTF-8のバイト数)である．
- 長さが`u16`に入らない文字列は，ページにも入らない．その場合は`expect`で止まるが，`Column::assign`が`VARCHAR(n)`の長さを調べているので，`n`が65535以下なら起きない．

`decode_tuple`は，列の型に従って`split_at`で値のバイトを切り出す．足りなければ`TupleError::Truncated`を返す小さな関数`split`を作った．

```rust
fn split(bytes: &[u8], len: usize) -> Result<(&[u8], &[u8]), TupleError> {
    if bytes.len() < len {
        return Err(TupleError::Truncated);
    }
    Ok(bytes.split_at(len))
}
```

`split`は1つの`&[u8]`を受け取り，2つの`&[u8]`を返す．どちらも引数と同じバイト列を指すので，参照を1つ受け取る関数の戻り値の参照は，その引数の参照と同じだけ有効である．
9列の表のテストで，`schema(&[DataType::Boolean; 9])`と書くと，次のエラーになった．

```text
error[E0277]: the trait bound `value::DataType: Copy` is not satisfied
   --> src/storage/tuple.rs:174:29
    |
174 |         let nine = schema(&[DataType::Boolean; 9]);
    |                             ^^^^^^^^^^^^^^^^^ the trait `Copy` is not implemented for `value::DataType`
    |
    = note: the `Copy` trait is required because this value will be copied for each element of the array
```

`DataType`は`VARCHAR(n)`の`n`を持つので`Copy`ではない．`vec![DataType::Boolean; 9]`は`Clone`で複製するので，こちらを使った．

### ページ

```rust
#[test]
fn tuples_are_packed_from_the_end_of_the_page() {
    let mut page = Page::new();
    page.insert(b"ab").unwrap();
    page.insert(b"cde").unwrap();
    assert_eq!(&page.data[0..4], &[2, 0, 0xfb, 0x1f]);
    assert_eq!(&page.data[4..12], &[0xfe, 0x1f, 2, 0, 0xfb, 0x1f, 3, 0]);
    assert_eq!(&page.data[PAGE_SIZE - 5..], b"cdeab");
}
```

`0x1ffb`は8187で，2つ目のタプル`cde`の位置である．

```rust
    pub fn insert(&mut self, tuple: &[u8]) -> Result<SlotId, PageError> {
        check_size(tuple)?;
        if self.free_space() < tuple.len() + SLOT_SIZE {
            return Err(PageError::PageFull);
        }
        let slot = self.slot_count();
        let offset = self.free_end() - tuple.len();
        self.data[offset..offset + tuple.len()].copy_from_slice(tuple);
        self.set_free_end(offset);
        self.write_u16(0, slot + 1);
        self.set_slot(slot, offset, tuple.len());
        Ok(slot)
    }

    fn read_u16(&self, position: usize) -> u16 {
        let bytes: [u8; 2] = self.data[position..position + 2]
            .try_into()
            .expect("2 bytes");
        u16::from_le_bytes(bytes)
    }
```

- ヘッダーとスロットは2バイトの数なので，`read_u16`と`write_u16`だけで読み書きする．
- `get`は，消したスロット(位置が0)なら`None`を返す．位置0はヘッダーなので，タプルの位置にはならない．
- `update`は，元のタプル以下の大きさならその場所に書き，大きければ空いている領域に置き直す．どちらもできなければ，元のタプルを残して`PageFull`を返す．
- `Page`は8192バイトを持つので，`#[derive(Debug)]`の代わりに，スロットの数と空きだけを書く`Debug`を実装した．`Database`が`Debug`を導出しているので，`HeapFile`と`Page`にも`Debug`が要る．

### ヒープファイル

```rust
#[test]
fn update_moves_a_tuple_that_no_longer_fits_its_page() {
    let mut heap = HeapFile::new();
    let id = heap.insert(&[1; 4000]).unwrap();
    heap.insert(&[2; 4000]).unwrap();
    let moved = heap.update(id, &[3; 4100]).unwrap();
    assert_eq!(moved, RowId { page: 1, slot: 0 });
    assert_eq!(get(&heap, id), None);
    assert_eq!(get(&heap, moved), Some(vec![3; 4100]));
    assert_eq!(heap.update(moved, b"x"), Ok(moved));
}
```

`HeapFile::update`は，ページの`update`が`PageFull`を返したら，新しい場所に置いてから元のタプルを消す．

```rust
    pub fn update(&mut self, id: RowId, tuple: &[u8]) -> Result<RowId, PageError> {
        match self.pages[id.page].update(id.slot, tuple) {
            Ok(()) => Ok(id),
            Err(PageError::PageFull) => {
                let new_id = self.insert(tuple)?;
                self.pages[id.page].delete(id.slot);
                Ok(new_id)
            }
            Err(error) => Err(error),
        }
    }
```

先に消してから置くと，置けなかった場合に行がなくなる．置いてから消せば，どちらの場合も行は1つ残る．
`tuples`は`Vec<(RowId, &[u8])>`を返す．タプルのバイト列はページの中を指すので，`HeapFile`を借りている間だけ使える．

### `exec::dml`と`Database`

`exec::dml`の関数は，`&mut Vec<Row>`の代わりに`&mut HeapFile`を受け取る．単体テストは，行をタプルにして置いた`HeapFile`を作る補助関数`heap_of`と，`HeapFile`の行を読む`rows_of`で書き直した．

```rust
pub fn update(
    schema: &TableSchema,
    heap: &mut HeapFile,
    assignments: &[(usize, BoundExpr)],
    filter: &Option<BoundExpr>,
) -> Result<usize, Error> {
    let rows = heap.rows(schema)?;
    let changes = rows
        .iter()
        .map(|(_, row)| updated_row(schema, row, assignments, filter))
        .collect::<Result<Vec<Option<Row>>, Error>>()?;
    let mut after: Vec<Row> = rows.iter().map(|(_, row)| row.clone()).collect();
    for (row, change) in after.iter_mut().zip(&changes) {
        if let Some(new_row) = change {
            *row = new_row.clone();
        }
    }
    check_constraints(schema, &after)?;
    let mut writes = Vec::new();
    for ((id, _), change) in rows.iter().zip(&changes) {
        if let Some(new_row) = change {
            writes.push((*id, encode(new_row, schema)?));
        }
    }
    for (id, tuple) in &writes {
        heap.update(*id, tuple)?;
    }
    Ok(writes.len())
}
```

計算(新しい行)，検査(制約とタプルの大きさ)，変更(ページの書き換え)の3つの段階に分けた．すべての検査を終えてから変更の段階に入るので，文の途中で失敗しても表は変わらない．
`encode`は，タプルが`MAX_TUPLE_SIZE`を超えたら`PageError::TupleTooLarge`を返す．`error`で`54000`に変換する．

`Database`は，表の名前ごとの`HeapFile`を持つ．`build`は，`SeqScan`を作るときにカタログから表の定義を引き，`HeapFile::rows`で行に戻して渡す．復号に失敗しうるので，`build`は`Result`を返すようにした．
`tables.rs`の2つの新しい項目は，ここまでの実装で通る．

## 13-6 振り返り

1. タプル，ページ，ヒープファイル，行の変更のそれぞれに，境界(空き，最大の大きさ，壊れたバイト列)の項目があるかを比べる．
2. 型の印を書くと，値ごとに1バイト大きくなる．その代わり，表の定義がなくても復号でき，列の型を変えても古いタプルを読める．PostgreSQLはタプルに型を書かず，列の型を変える`ALTER TABLE`では表のタプルを書き直す．
3. インデックスが覚えている`RowId`の行が別のページに移ると，インデックスは古い位置を指したままになる．行を移すたびにインデックスも更新する必要がある．Iteration 17では，`UPDATE`で行を書き換えたあとに，インデックスの位置も更新する．
4. ページを書き換えながら検査すると，違反を見つけた時点で，それまでに書き換えたタプルを元に戻す必要がある．元の値を覚えておく仕組みが要る．Iteration 19では，変更を先にWALへ書き，障害のあとでWALから変更をやり直す(REDO)仕組みを作る．
5. 模範解答の設計ドキュメントは，照合スクリプトで問題がないことを確かめてある．

## 13-7 発展課題

解答例である．残っているタプルを複製してから，ページの末尾へ順に書き直す．

```rust
    pub fn compact(&mut self) {
        let live: Vec<(SlotId, Vec<u8>)> = (0..self.slot_count())
            .filter_map(|slot| self.get(slot).map(|tuple| (slot, tuple.to_vec())))
            .collect();
        let mut end = PAGE_SIZE;
        for (slot, tuple) in &live {
            end -= tuple.len();
            self.data[end..end + tuple.len()].copy_from_slice(tuple);
            self.set_slot(*slot, end, tuple.len());
        }
        self.set_free_end(end);
    }
```

`filter_map`は，クロージャが`Some`を返した要素だけを集める．`self.get`で借りたタプルは`to_vec`で複製しておく．借りたまま`self.data`に書こうとすると，借用の衝突になる．

`update`も，最初に`self.get(slot)`で得た古いタプルを持ったまま`compact`を呼ぶと，次のエラーになる．

```text
error[E0502]: cannot borrow `*self` as mutable because it is also borrowed as immutable
   --> src/storage/page.rs:110:17
    |
103 |         let Some(old) = self.get(slot) else {
    |                         ---- immutable borrow occurs here
...
110 |                 self.set_slot(slot, 0, 0);
    |                 ^^^^^^^^^^^^^^^^^^^^^^^^^ mutable borrow occurs here
...
113 |                     let old = old.to_vec();
    |                               --- immutable borrow later used here
```

古いタプルを先に複製してから，ページを書き換える．

```rust
    pub fn update(&mut self, slot: SlotId, tuple: &[u8]) -> Result<(), PageError> {
        check_size(tuple)?;
        let Some(old) = self.get(slot).map(|old| old.to_vec()) else {
            return Err(PageError::PageFull);
        };
        if tuple.len() <= old.len() {
            let offset = self.slot(slot).0;
            self.data[offset..offset + tuple.len()].copy_from_slice(tuple);
            self.set_slot(slot, offset, tuple.len());
            return Ok(());
        }
        if tuple.len() > self.free_space() {
            self.set_slot(slot, 0, 0);
            self.compact();
        }
        let (bytes, result) = if tuple.len() <= self.free_space() {
            (tuple, Ok(()))
        } else {
            (&old[..], Err(PageError::PageFull))
        };
        let offset = self.free_end() - bytes.len();
        self.data[offset..offset + bytes.len()].copy_from_slice(bytes);
        self.set_free_end(offset);
        self.set_slot(slot, offset, bytes.len());
        result
    }
```

- 空きが足りなければ，古いタプルのスロットを空にしてから詰める．古いタプルの領域も空きに戻る．
- 詰めても入らなければ，複製しておいた古いタプルを置き直して`PageFull`を返す．
- `insert`も，空きが足りなければ`compact`してからもう一度調べる．

```rust
#[test]
fn compact_reclaims_the_space_of_deleted_tuples() {
    let mut page = Page::new();
    page.insert(&[1; 4000]).unwrap();
    page.insert(&[2; 4000]).unwrap();
    assert!(page.delete(0));
    assert_eq!(page.insert(&[3; 3900]), Ok(2));
    assert_eq!(page.get(1), Some(&[2; 4000][..]));
    assert_eq!(page.get(2), Some(&[3; 3900][..]));
}
```

詰めるようにすると，4000バイトから4100バイトへの更新も同じページに収まる．ヒープファイルの`update_moves_a_tuple_that_no_longer_fits_its_page`は，詰めても入らない4200バイトに変える．
