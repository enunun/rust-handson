# Iteration 16：B+木(解説)

演習の各手順の模範解答と，その考え方を説明する．

## 16-1 準備

引き継いだ362個のテストがすべて通れば準備は終わりである．

## 16-2 文法と概念

課題の解答例である．`tests/`に置いた結合テストで確かめた．

```rust
use std::ops::{Bound, RangeBounds};

pub fn encode_i16(n: i16) -> [u8; 2] {
    (n.cast_unsigned() ^ 0x8000).to_be_bytes()
}

pub struct Squares {
    current: u64,
    last: u64,
}

impl Squares {
    pub fn new(last: u64) -> Squares {
        Squares { current: 0, last }
    }
}

impl Iterator for Squares {
    type Item = u64;

    fn next(&mut self) -> Option<u64> {
        if self.current == self.last {
            return None;
        }
        self.current += 1;
        Some(self.current * self.current)
    }
}

pub fn in_range(sorted: &[i32], bounds: impl RangeBounds<i32>) -> Vec<i32> {
    let start = match bounds.start_bound() {
        Bound::Included(n) => sorted.partition_point(|x| x < n),
        Bound::Excluded(n) => sorted.partition_point(|x| x <= n),
        Bound::Unbounded => 0,
    };
    sorted[start..]
        .iter()
        .take_while(|x| bounds.contains(x))
        .copied()
        .collect()
}

#[test]
fn tasks() {
    let encoded: Vec<[u8; 2]> = [-300, -1, 0, 1, 300].iter().map(|&n| encode_i16(n)).collect();
    assert!(encoded.windows(2).all(|w| w[0] < w[1]));
    assert_eq!(Squares::new(4).collect::<Vec<u64>>(), vec![1, 4, 9, 16]);
    let sorted = vec![1, 3, 5, 7, 9];
    assert_eq!(in_range(&sorted, 3..7), vec![3, 5]);
    assert_eq!(in_range(&sorted, 3..=7), vec![3, 5, 7]);
    assert_eq!(in_range(&sorted, ..), sorted);
}
```

- 配列`[u8; 2]`どうしも，`<`で辞書式に比べられる．
- `take_while`は，条件を満たさない要素に会ったところで止まる．整列しているので，範囲の終わりを超えた先を調べなくて済む．

## 16-3 テストリスト

模範解答は[TESTLIST.md](../TESTLIST.md)である．

- キーの符号化，B+木の順に並べた．B+木の項目は，1つの葉，葉の分割，内部ノードの分割，範囲，同じキー，削除，境界の順に難しくした．
- 符号化の単体テストは，並べた値を符号化したバイト列が同じ順に並ぶことを`windows(2)`で確かめる補助関数`assert_order_kept`にまとめた．型引数`K: IndexKey`で，4つの型に同じ手順を使う．
- `INTEGER`のキーの葉の項目は12バイトなので，1つの葉に680ほど入る．1000個入れれば葉が分割される．
- 内部ノードの項目は16バイトで，1つの内部ノードに510ほどの子が入る．`INTEGER`のキーで内部ノードを分割するには，数十万個の項目が要る．1000バイトのキーなら，葉と内部ノードのどちらも8個ほどしか入らないので，300個で3段になる．
- 3枠のバッファプールの項目で，枠が少なくても木が動くことを確かめる．模範解答は，ノードを1つ読み書きするたびにピンを外すので，同時にピン留めするのは`create`の2ページまでである．
- 既存テストへの影響はない．

## 16-4 設計ドキュメント

| ファイル | 変更 | 理由 |
| --- | --- | --- |
| `c4-component.md` | `index`の境界と`index::btree`，`index::key`を加え，`index::btree`から`index::key`，`storage::buffer`，`storage::disk`，`storage::heap`，`storage::page`への依存を加えた | B+木のモジュールができた |
| `code-types.md` | インデックスの図を加え，`IndexKey`，`BTree`，`RangeIter`，`Entry`，`Node`，`BTreeError`を描いた | B+木の型ができた |
| `layout.md` | メタページ，ノードのヘッダー，葉の項目，内部ノードの項目の配置を加えた | ノードをページに置く形式ができた |
| `code-sequence.md` | `insert`で葉を分割し，区切りを親に送る流れを加えた | 分割は親へ，根へと伝わる |

- `index`へ向かう矢印は，Iteration 17で`database`から加わる．このIterationでは，`index`を結合テストから使う．
- `IndexKey::decode`は`self`を受け取らない関連関数なので，型の図では`$`を付けた．
- `Entry`と`Node`は非公開の型だが，ノードの形を示すために描いた．

## 16-5 テスト駆動の実装

### `index::key`

```rust
#[test]
fn integers_keep_their_order() {
    assert_order_kept(&[i32::MIN, -256, -1, 0, 1, 255, 256, i32::MAX]);
    assert_order_kept(&[i64::MIN, -1, 0, 1, 1 << 40, i64::MAX]);
}
```

```rust
impl IndexKey for i32 {
    fn encode(&self) -> Vec<u8> {
        (self.cast_unsigned() ^ 0x8000_0000).to_be_bytes().to_vec()
    }

    fn decode(bytes: &[u8]) -> Option<i32> {
        let bytes: [u8; 4] = bytes.try_into().ok()?;
        Some((u32::from_be_bytes(bytes) ^ 0x8000_0000).cast_signed())
    }
}
```

`String`はUTF-8のバイト列をそのまま使い，`decode`は`String::from_utf8(...).ok()`で戻す．

### ノード

最初に，1つの葉だけの木で`inserted_key_can_be_found`を通した．ノードは`enum`で表し，ページのバイト列と相互に変換する．

```rust
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Entry {
    key: Vec<u8>,
    id: RowId,
}

enum Node {
    Leaf {
        entries: Vec<Entry>,
        next: Option<PageId>,
    },
    Internal {
        first: PageId,
        children: Vec<(Entry, PageId)>,
    },
}
```

`Entry`は`Ord`を導出するので，キー，`RowId`の順に比べる．そのために`RowId`にも`PartialOrd`と`Ord`を導出した．

復号では，Iteration 14のカタログと同じく入力を読み進める．Iteration 15でライフタイム注釈を学んだので，`take`は複製せずに入力の一部の`&[u8]`を返す．

```rust
fn take<'a>(input: &mut &'a [u8], len: usize) -> Option<&'a [u8]> {
    if input.len() < len {
        return None;
    }
    let (head, rest) = input.split_at(len);
    *input = rest;
    Some(head)
}
```

戻り値の`&'a [u8]`は，`&mut`の借用でなく，元のバイト列を借りる．

ノードは`Page::from_bytes`でページごと置き換える．

```rust
fn write_page(page: &PageGuard<'_>, bytes: &[u8]) {
    let mut data = Box::new([0; PAGE_SIZE]);
    data[..bytes.len()].copy_from_slice(bytes);
    *page.write() = Page::from_bytes(data);
}
```

`create`は，空のプールにメタページ(ページ0)と空の葉(ページ1)を作る．

### 挿入と分割

`full_leaf_is_split_under_a_new_root`を通すために，下りる途中の内部ノードを積む`find_leaf`と，入らないノードを分ける`write_or_split`を作った．

```rust
    fn find_leaf(&self, target: &Entry) -> Result<(PageId, Vec<PageId>), BTreeError> {
        let mut page = self.root()?;
        let mut path = Vec::new();
        loop {
            match read_node(self.pool, page)? {
                Node::Leaf { .. } => return Ok((page, path)),
                Node::Internal { first, children } => {
                    path.push(page);
                    let position = children.partition_point(|(s, _)| s <= target);
                    page = match position {
                        0 => first,
                        _ => children[position - 1].1,
                    };
                }
            }
        }
    }
```

`insert`は，葉に項目を入れ，分割で区切りが返る間は親に加え続ける．

```rust
        let mut split = self.write_or_split(leaf, Node::Leaf { entries, next })?;
        while let Some((separator, right)) = split {
            match path.pop() {
                Some(parent) => {
                    // 親に(separator, right)を加え，親も分割する
                    split = self.write_or_split(parent, Node::Internal { first, children })?;
                }
                None => {
                    let old_root = self.root()?;
                    let new_root = self.allocate(&Node::Internal {
                        first: old_root,
                        children: vec![(separator, right)],
                    })?;
                    write_page(&self.pool.fetch_page(META_PAGE)?, &encode_meta(new_root));
                    split = None;
                }
            }
        }
```

`write_or_split`は，葉なら右の葉の最初の項目の複製を，内部ノードなら右から取り除いた真ん中の区切りを返す．
分ける位置を項目の数の半分にすると，長いキーが片側に偏ったとき，その側がページに入らない．項目のバイト数がほぼ半分になる位置で分ける`split_point`を作り，単体テストを加えた．

```rust
fn split_point(sizes: &[usize]) -> usize {
    let half = sizes.iter().sum::<usize>() / 2;
    let mut total = 0;
    let mut point = sizes.len() - 1;
    for (index, size) in sizes.iter().enumerate() {
        if total + size > half {
            point = index;
            break;
        }
        total += size;
    }
    point.clamp(1, sizes.len() - 1)
}
```

キーを2000バイトまでにしたので，項目は最大で2008バイトである．分けたあとの片方は，全体の半分に項目1つを足した大きさを超えないので，ページに入る．

### 範囲の走査

`RangeIter`は，範囲の始まりの葉のページ番号から始める．葉の項目を`Vec`に読み出して`IntoIter`に入れ，1つずつ返す．

```rust
impl<K: IndexKey> Iterator for RangeIter<'_, K> {
    type Item = Result<(K, RowId), BTreeError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(entry) = self.entries.next() {
                // 始まりより前なら飛ばし，終わりより後ろなら止まる
                return Some(match K::decode(&entry.key) {
                    Some(key) => Ok((key, entry.id)),
                    None => Err(BTreeError::Corrupted),
                });
            }
            let leaf = self.next_leaf.take()?;
            match read_node(self.pool, leaf) {
                Ok(Node::Leaf { entries, next }) => {
                    self.entries = entries.into_iter();
                    self.next_leaf = next;
                }
                Ok(Node::Internal { .. }) => return Some(Err(BTreeError::Corrupted)),
                Err(error) => return Some(Err(error)),
            }
        }
    }
}
```

- 範囲は`bounds.start_bound().map(|key| key.encode())`で，符号化したキーの`Bound<Vec<u8>>`にして持つ．
- 始まりのキーで葉まで下りるときは，`RowId { page: 0, slot: 0 }`と組にした項目を探す．同じキーの項目がいくつあっても，その最初の項目がある葉に着く．
- `self.next_leaf.take()?`は，次の葉がなければ`None`を返して終わる．
- `get`は，`(Bound::Included(key), Bound::Included(key))`の範囲の最初の項目である．

### 削除とエラー

`delete`は，キーと`RowId`の組で葉まで下り，`binary_search`で項目を探して消す．
`too_large_key_is_rejected`は，`insert`の最初にキーの大きさを調べて通した．

### 公開

`BTree`はまだ`database`から使わないので，`index`を`mod index;`で宣言すると，すべての項目が使われていないというエラーになる．

```text
error: constant `MAX_KEY_SIZE` is never used
  --> src/index/btree.rs:13:11
   |
13 | pub const MAX_KEY_SIZE: usize = 2000;
   |           ^^^^^^^^^^^^
   |
   = note: `-D dead-code` implied by `-D warnings`
   = help: to override `-D warnings` add `#[expect(dead_code)]` or `#[allow(dead_code)]`
```

`pub mod index;`と`pub mod storage;`で公開し，`tests/btree.rs`から使った．`storage`を公開すると，`cargo clippy`が`Page::new`に対応する`Default`を求めるので，`impl Default for Page`を加えた．
結合テストでは，`drop(tree)`と書くと，`Drop`を実装しない値を捨てても意味がないという`cargo clippy`のエラーになった．`tree`を最後に使ったところで借用が終わるので，そのまま`drop(pool)`できる．

## 16-6 振り返り

1. 分割(葉，根，内部ノード)，範囲の境界，同じキー，枠の少ないプールの項目があるかを比べる．
2. ノードを読み出す設計は，`Vec`の`insert`と`split_off`で項目を扱えるので，分割と並べ替えを簡単に書ける．その代わり，1つの項目を加えるたびにページ全体を復号し，符号化し直す．PostgreSQLは，ページの中のスロット(Iteration 13と同じ形)で項目を指し，ページの中で直接書き換える．
3. `Value`で受け取ると，`INTEGER`の木に文字列を入れる誤りは，実行したときに初めて見つかる．`BTree<i32>`なら`insert`の引数の型が合わず，コンパイルで見つかる．Iteration 17では，列の型ごとの`BTree`を`enum`でまとめ，SQLの値から型を選ぶ．
4. `RangeIter`が葉の`PageGuard`を持ち続けると，その間はページの`Ref`も持つので，同じページを書き換えられない．また，走査の数だけ枠がピン留めされ，枠が足りなくなりうる．模範解答は葉の項目を複製してピンを外すので，走査の途中で木を書き換えても`RefCell`のパニックにはならない．ただし，書き換えで葉が分割されると，走査は古い内容のまま進む．
5. 項目の少ない葉が増え，同じ数の項目により多くのページを使う．高さは減らないので，検索で読むページも減らない．
6. 模範解答の設計ドキュメントは，照合スクリプトで問題がないことを確かめてある．

## 16-7 発展課題

解答例である．木の状態はページにあるので，`open`はプールが空でないことを確かめるだけでよい．

```rust
    pub fn open(pool: &'a BufferPool<Box<dyn DiskManager>>) -> Result<BTree<'a, K>, BTreeError> {
        if pool.page_count() == 0 {
            return Err(BTreeError::Corrupted);
        }
        Ok(BTree {
            pool,
            key: PhantomData,
        })
    }
```

```rust
#[test]
fn reopened_tree_has_the_same_keys() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("t.index");
    let first = pool(&path);
    let mut tree = BTree::create(&first).unwrap();
    for key in 0..1000 {
        tree.insert(key, RowId { page: 0, slot: 1 }).unwrap();
    }
    drop(first);
    let second = pool(&path);
    let tree = BTree::<i32>::open(&second).unwrap();
    assert_eq!(tree.get(&999).unwrap(), Some(RowId { page: 0, slot: 1 }));
    assert_eq!(tree.range(..).unwrap().count(), 1000);
    let empty = pool(&dir.path().join("empty.index"));
    assert!(BTree::<i32>::open(&empty).is_err());
}
```

`pool`は，ファイルを`FileDiskManager::open`で開いて4枠のプールを作る補助関数である．`drop(first)`でプールを捨てると，変更の印のあるページがファイルに書き戻される．
