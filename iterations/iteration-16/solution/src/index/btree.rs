//! キーから行の位置を引くB+木．ノードはバッファプールのページに置く．

use std::marker::PhantomData;
use std::ops::{Bound, RangeBounds};

use crate::index::key::IndexKey;
use crate::storage::buffer::{BufferError, BufferPool, PageGuard};
use crate::storage::disk::{DiskManager, PageId};
use crate::storage::heap::RowId;
use crate::storage::page::{PAGE_SIZE, Page};

/// 符号化したキーの最大のバイト数．1つのノードに，最大の大きさの項目が4つ入る．
pub const MAX_KEY_SIZE: usize = 2000;

/// 根のページ番号を書くページ．
const META_PAGE: PageId = 0;
/// ノードの種類，項目の数，ページ番号(次の葉か，最初の子)の7バイト．
const HEADER_SIZE: usize = 7;
const LEAF: u8 = 0;
const INTERNAL: u8 = 1;
/// 次の葉がないことを表すページ番号．
const NO_PAGE: u32 = u32::MAX;

/// B+木を使うときのエラー．
#[derive(Debug)]
pub enum BTreeError {
    /// 符号化したキーが`MAX_KEY_SIZE`を超える．
    KeyTooLarge {
        size: usize,
    },
    /// ページの内容がノードとして読めない．
    Corrupted,
    Buffer(BufferError),
}

impl From<BufferError> for BTreeError {
    fn from(error: BufferError) -> BTreeError {
        BTreeError::Buffer(error)
    }
}

/// 木の項目．符号化したキーと行の位置の組で並べる．同じキーの項目は，行の位置の順に並ぶ．
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Entry {
    key: Vec<u8>,
    id: RowId,
}

/// 1つのページに置くノード．
#[derive(Debug, PartialEq)]
enum Node {
    /// 葉．項目を順に並べ，右隣の葉のページ番号を持つ．
    Leaf {
        entries: Vec<Entry>,
        next: Option<PageId>,
    },
    /// 内部ノード．`first`は最初の区切りより小さい項目の子，`children`は区切りと，
    /// 区切り以上で次の区切りより小さい項目の子の組である．
    Internal {
        first: PageId,
        children: Vec<(Entry, PageId)>,
    },
}

/// キーから行の位置を引くB+木．木のページは，借りたバッファプールに置く．
pub struct BTree<'a, K: IndexKey> {
    pool: &'a BufferPool<Box<dyn DiskManager>>,
    key: PhantomData<K>,
}

/// キーの範囲にある項目を，キーの順に返すイテレーター．
pub struct RangeIter<'a, K: IndexKey> {
    pool: &'a BufferPool<Box<dyn DiskManager>>,
    entries: std::vec::IntoIter<Entry>,
    next_leaf: Option<PageId>,
    start: Bound<Vec<u8>>,
    end: Bound<Vec<u8>>,
    key: PhantomData<K>,
}

impl<'a, K: IndexKey> BTree<'a, K> {
    /// 空のプールに，空の木を作る．ページ0に根のページ番号を書き，ページ1を空の葉にする．
    pub fn create(pool: &'a BufferPool<Box<dyn DiskManager>>) -> Result<BTree<'a, K>, BTreeError> {
        let meta = pool.new_page()?;
        let root = pool.new_page()?;
        write_page(&meta, &encode_meta(root.page_id()));
        let empty = Node::Leaf {
            entries: Vec::new(),
            next: None,
        };
        write_page(&root, &empty.encode());
        Ok(BTree {
            pool,
            key: PhantomData,
        })
    }

    /// キーと行の位置の組を加える．葉がいっぱいになれば分割し，区切りを親に加える．
    pub fn insert(&mut self, key: K, id: RowId) -> Result<(), BTreeError> {
        let entry = Entry {
            key: encode_key(&key)?,
            id,
        };
        let (leaf, mut path) = self.find_leaf(&entry)?;
        let Node::Leaf { mut entries, next } = read_node(self.pool, leaf)? else {
            return Err(BTreeError::Corrupted);
        };
        let position = entries.partition_point(|e| e < &entry);
        entries.insert(position, entry);
        let mut split = self.write_or_split(leaf, Node::Leaf { entries, next })?;
        while let Some((separator, right)) = split {
            match path.pop() {
                Some(parent) => {
                    let Node::Internal {
                        first,
                        mut children,
                    } = read_node(self.pool, parent)?
                    else {
                        return Err(BTreeError::Corrupted);
                    };
                    let position = children.partition_point(|(s, _)| s < &separator);
                    children.insert(position, (separator, right));
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
        Ok(())
    }

    /// キーの項目の行の位置を1つ返す．同じキーの項目が複数あれば，行の位置が最も小さいものを返す．
    pub fn get(&self, key: &K) -> Result<Option<RowId>, BTreeError> {
        let mut range = self.range((Bound::Included(key), Bound::Included(key)))?;
        match range.next() {
            Some(Ok((_, id))) => Ok(Some(id)),
            Some(Err(error)) => Err(error),
            None => Ok(None),
        }
    }

    /// キーと行の位置の組を消す．消したら`true`を返す．ノードは併合しない．
    pub fn delete(&mut self, key: &K, id: RowId) -> Result<bool, BTreeError> {
        let entry = Entry {
            key: key.encode(),
            id,
        };
        let (leaf, _) = self.find_leaf(&entry)?;
        let Node::Leaf { mut entries, next } = read_node(self.pool, leaf)? else {
            return Err(BTreeError::Corrupted);
        };
        let Ok(position) = entries.binary_search(&entry) else {
            return Ok(false);
        };
        entries.remove(position);
        let page = self.pool.fetch_page(leaf)?;
        write_page(&page, &Node::Leaf { entries, next }.encode());
        Ok(true)
    }

    /// 範囲にあるキーの項目を，キーと行の位置の順に返すイテレーターを作る．
    pub fn range(&self, bounds: impl RangeBounds<K>) -> Result<RangeIter<'a, K>, BTreeError> {
        let start = bounds.start_bound().map(|key| key.encode());
        let end = bounds.end_bound().map(|key| key.encode());
        let lowest = match &start {
            Bound::Included(key) | Bound::Excluded(key) => key.clone(),
            Bound::Unbounded => Vec::new(),
        };
        let (leaf, _) = self.find_leaf(&Entry {
            key: lowest,
            id: RowId { page: 0, slot: 0 },
        })?;
        Ok(RangeIter {
            pool: self.pool,
            entries: Vec::new().into_iter(),
            next_leaf: Some(leaf),
            start,
            end,
            key: PhantomData,
        })
    }

    fn root(&self) -> Result<PageId, BTreeError> {
        let page = self.pool.fetch_page(META_PAGE)?;
        let bytes: [u8; 4] = page.read().bytes()[..4].try_into().expect("4 bytes");
        Ok(page_id(u32::from_le_bytes(bytes)))
    }

    /// `target`の項目を置くべき葉と，根からその葉の親までの内部ノードのページ番号を返す．
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

    /// ノードがページに入れば書き，入らなければ2つに分ける．分けたら，右のノードの最初の
    /// 区切りと，右のノードのページ番号を返す．
    fn write_or_split(
        &self,
        page: PageId,
        node: Node,
    ) -> Result<Option<(Entry, PageId)>, BTreeError> {
        if node.encoded_len() <= PAGE_SIZE {
            write_page(&self.pool.fetch_page(page)?, &node.encode());
            return Ok(None);
        }
        let (left, separator, right) = match node {
            Node::Leaf { mut entries, next } => {
                let sizes: Vec<usize> = entries.iter().map(|e| e.encoded_len()).collect();
                let right_entries = entries.split_off(split_point(&sizes));
                let separator = right_entries[0].clone();
                let right = self.allocate(&Node::Leaf {
                    entries: right_entries,
                    next,
                })?;
                let left = Node::Leaf {
                    entries,
                    next: Some(right),
                };
                (left, separator, right)
            }
            Node::Internal {
                first,
                mut children,
            } => {
                let sizes: Vec<usize> = children.iter().map(|(s, _)| s.encoded_len() + 4).collect();
                let mut right_children = children.split_off(split_point(&sizes));
                let (separator, right_first) = right_children.remove(0);
                let right = self.allocate(&Node::Internal {
                    first: right_first,
                    children: right_children,
                })?;
                (Node::Internal { first, children }, separator, right)
            }
        };
        write_page(&self.pool.fetch_page(page)?, &left.encode());
        Ok(Some((separator, right)))
    }

    /// 新しいページにノードを書き，そのページ番号を返す．
    fn allocate(&self, node: &Node) -> Result<PageId, BTreeError> {
        let page = self.pool.new_page()?;
        write_page(&page, &node.encode());
        Ok(page.page_id())
    }
}

impl<K: IndexKey> Iterator for RangeIter<'_, K> {
    type Item = Result<(K, RowId), BTreeError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(entry) = self.entries.next() {
                let after_start = match &self.start {
                    Bound::Included(start) => &entry.key >= start,
                    Bound::Excluded(start) => &entry.key > start,
                    Bound::Unbounded => true,
                };
                if !after_start {
                    continue;
                }
                let before_end = match &self.end {
                    Bound::Included(end) => &entry.key <= end,
                    Bound::Excluded(end) => &entry.key < end,
                    Bound::Unbounded => true,
                };
                if !before_end {
                    self.next_leaf = None;
                    self.entries = Vec::new().into_iter();
                    return None;
                }
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

/// キーを符号化する．大きすぎればエラーを返す．
fn encode_key<K: IndexKey>(key: &K) -> Result<Vec<u8>, BTreeError> {
    let bytes = key.encode();
    if bytes.len() > MAX_KEY_SIZE {
        return Err(BTreeError::KeyTooLarge { size: bytes.len() });
    }
    Ok(bytes)
}

/// 大きさが`sizes`の項目の並びを，バイト数がほぼ半分になる位置で分ける．
/// どちらの側にも1つ以上の項目を残す．
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

fn read_node(pool: &BufferPool<Box<dyn DiskManager>>, page: PageId) -> Result<Node, BTreeError> {
    let guard = pool.fetch_page(page)?;
    let page = guard.read();
    Node::decode(page.bytes()).ok_or(BTreeError::Corrupted)
}

/// ページの内容を，`bytes`で始まり残りが0のバイト列に置き換える．
fn write_page(page: &PageGuard<'_>, bytes: &[u8]) {
    let mut data = Box::new([0; PAGE_SIZE]);
    data[..bytes.len()].copy_from_slice(bytes);
    *page.write() = Page::from_bytes(data);
}

fn encode_meta(root: PageId) -> Vec<u8> {
    page_number(root).to_le_bytes().to_vec()
}

fn page_number(page: PageId) -> u32 {
    u32::try_from(page).expect("page number fits in u32")
}

fn page_id(number: u32) -> PageId {
    usize::try_from(number).expect("u32 fits in usize")
}

impl Entry {
    /// キーのバイト数(2バイト)，キー，ページ番号(4バイト)，スロット番号(2バイト)の大きさ．
    fn encoded_len(&self) -> usize {
        2 + self.key.len() + 4 + 2
    }

    fn encode(&self, bytes: &mut Vec<u8>) {
        let len = u16::try_from(self.key.len()).expect("key fits in u16");
        bytes.extend_from_slice(&len.to_le_bytes());
        bytes.extend_from_slice(&self.key);
        bytes.extend_from_slice(&page_number(self.id.page).to_le_bytes());
        bytes.extend_from_slice(&self.id.slot.to_le_bytes());
    }

    fn decode(input: &mut &[u8]) -> Option<Entry> {
        let len = usize::from(read_u16(input)?);
        let key = take(input, len)?.to_vec();
        let page = page_id(read_u32(input)?);
        let slot = read_u16(input)?;
        Some(Entry {
            key,
            id: RowId { page, slot },
        })
    }
}

impl Node {
    fn encoded_len(&self) -> usize {
        match self {
            Node::Leaf { entries, .. } => {
                HEADER_SIZE + entries.iter().map(Entry::encoded_len).sum::<usize>()
            }
            Node::Internal { children, .. } => {
                HEADER_SIZE
                    + children
                        .iter()
                        .map(|(s, _)| s.encoded_len() + 4)
                        .sum::<usize>()
            }
        }
    }

    fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.encoded_len());
        match self {
            Node::Leaf { entries, next } => {
                bytes.push(LEAF);
                write_count(&mut bytes, entries.len());
                let next = next.map_or(NO_PAGE, page_number);
                bytes.extend_from_slice(&next.to_le_bytes());
                for entry in entries {
                    entry.encode(&mut bytes);
                }
            }
            Node::Internal { first, children } => {
                bytes.push(INTERNAL);
                write_count(&mut bytes, children.len());
                bytes.extend_from_slice(&page_number(*first).to_le_bytes());
                for (separator, child) in children {
                    separator.encode(&mut bytes);
                    bytes.extend_from_slice(&page_number(*child).to_le_bytes());
                }
            }
        }
        bytes
    }

    fn decode(mut bytes: &[u8]) -> Option<Node> {
        let input = &mut bytes;
        let kind = take(input, 1)?[0];
        let count = read_u16(input)?;
        let page = read_u32(input)?;
        match kind {
            LEAF => {
                let mut entries = Vec::new();
                for _ in 0..count {
                    entries.push(Entry::decode(input)?);
                }
                let next = if page == NO_PAGE {
                    None
                } else {
                    Some(page_id(page))
                };
                Some(Node::Leaf { entries, next })
            }
            INTERNAL => {
                let mut children = Vec::new();
                for _ in 0..count {
                    let separator = Entry::decode(input)?;
                    children.push((separator, page_id(read_u32(input)?)));
                }
                Some(Node::Internal {
                    first: page_id(page),
                    children,
                })
            }
            _ => None,
        }
    }
}

fn write_count(bytes: &mut Vec<u8>, count: usize) {
    let count = u16::try_from(count).expect("count fits in u16");
    bytes.extend_from_slice(&count.to_le_bytes());
}

/// 先頭の`len`バイトを返し，`input`を残りのバイト列にする．
fn take<'a>(input: &mut &'a [u8], len: usize) -> Option<&'a [u8]> {
    if input.len() < len {
        return None;
    }
    let (head, rest) = input.split_at(len);
    *input = rest;
    Some(head)
}

fn read_u16(input: &mut &[u8]) -> Option<u16> {
    Some(u16::from_le_bytes(take(input, 2)?.try_into().ok()?))
}

fn read_u32(input: &mut &[u8]) -> Option<u32> {
    Some(u32::from_le_bytes(take(input, 4)?.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::disk::MemoryDiskManager;

    fn pool(frames: usize) -> BufferPool<Box<dyn DiskManager>> {
        BufferPool::new(Box::new(MemoryDiskManager::default()), frames)
    }

    fn id(n: usize) -> RowId {
        RowId {
            page: n,
            slot: u16::try_from(n % 100).unwrap(),
        }
    }

    fn keys<K: IndexKey>(tree: &BTree<K>, bounds: impl RangeBounds<K>) -> Vec<K> {
        tree.range(bounds)
            .unwrap()
            .map(|entry| entry.unwrap().0)
            .collect()
    }

    /// 根のノード．
    fn root_node<K: IndexKey>(tree: &BTree<K>) -> Node {
        read_node(tree.pool, tree.root().unwrap()).unwrap()
    }

    #[test]
    fn empty_tree_has_no_keys() {
        let pool = pool(4);
        let tree = BTree::<i32>::create(&pool).unwrap();
        assert_eq!(tree.get(&1).unwrap(), None);
        assert_eq!(keys(&tree, ..), Vec::<i32>::new());
    }

    #[test]
    fn inserted_key_can_be_found() {
        let pool = pool(4);
        let mut tree = BTree::create(&pool).unwrap();
        tree.insert(42, id(7)).unwrap();
        tree.insert(-5, id(8)).unwrap();
        assert_eq!(tree.get(&42).unwrap(), Some(id(7)));
        assert_eq!(tree.get(&-5).unwrap(), Some(id(8)));
        assert_eq!(tree.get(&0).unwrap(), None);
    }

    #[test]
    fn keys_are_returned_in_order() {
        let pool = pool(4);
        let mut tree = BTree::create(&pool).unwrap();
        for key in [5, -3, 9, 0, 7] {
            tree.insert(key, id(1)).unwrap();
        }
        assert_eq!(keys(&tree, ..), vec![-3, 0, 5, 7, 9]);
    }

    #[test]
    fn full_leaf_is_split_under_a_new_root() {
        let pool = pool(8);
        let mut tree = BTree::create(&pool).unwrap();
        for key in 0..1000 {
            tree.insert(key, id(1)).unwrap();
        }
        assert!(matches!(root_node(&tree), Node::Internal { .. }));
        assert_eq!(keys(&tree, ..), (0..1000).collect::<Vec<i32>>());
        assert_eq!(tree.get(&999).unwrap(), Some(id(1)));
    }

    #[test]
    fn full_internal_node_is_split_too() {
        let pool = pool(8);
        let mut tree = BTree::create(&pool).unwrap();
        let key = |n: usize| format!("{n:05}{}", "x".repeat(1000));
        for n in (0..300).rev() {
            tree.insert(key(n), id(n)).unwrap();
        }
        let Node::Internal { first, .. } = root_node(&tree) else {
            panic!("root is a leaf");
        };
        assert!(matches!(
            read_node(&pool, first).unwrap(),
            Node::Internal { .. }
        ));
        for n in 0..300 {
            assert_eq!(tree.get(&key(n)).unwrap(), Some(id(n)));
        }
        assert_eq!(keys(&tree, ..).len(), 300);
    }

    #[test]
    fn range_respects_its_bounds() {
        let pool = pool(8);
        let mut tree = BTree::create(&pool).unwrap();
        for key in 0..500 {
            tree.insert(key * 2, id(1)).unwrap();
        }
        assert_eq!(keys(&tree, 10..20), vec![10, 12, 14, 16, 18]);
        assert_eq!(keys(&tree, 11..=20), vec![12, 14, 16, 18, 20]);
        assert_eq!(keys(&tree, ..4), vec![0, 2]);
        assert_eq!(keys(&tree, 995..), vec![996, 998]);
        let after_ten = (Bound::Excluded(10), Bound::Included(14));
        assert_eq!(keys(&tree, after_ten), vec![12, 14]);
        assert_eq!(keys(&tree, 2000..), Vec::<i32>::new());
    }

    #[test]
    fn duplicate_keys_are_kept_in_row_order() {
        let pool = pool(8);
        let mut tree = BTree::create(&pool).unwrap();
        for n in [3, 1, 2] {
            tree.insert(true, id(n)).unwrap();
        }
        tree.insert(false, id(9)).unwrap();
        let ids: Vec<RowId> = tree
            .range(true..=true)
            .unwrap()
            .map(|entry| entry.unwrap().1)
            .collect();
        assert_eq!(ids, vec![id(1), id(2), id(3)]);
        assert_eq!(tree.get(&true).unwrap(), Some(id(1)));
    }

    #[test]
    fn delete_removes_only_the_given_entry() {
        let pool = pool(8);
        let mut tree = BTree::create(&pool).unwrap();
        tree.insert(1i64, id(1)).unwrap();
        tree.insert(1i64, id(2)).unwrap();
        tree.insert(2i64, id(3)).unwrap();
        assert!(tree.delete(&1, id(1)).unwrap());
        assert!(!tree.delete(&1, id(1)).unwrap());
        assert!(!tree.delete(&5, id(1)).unwrap());
        assert_eq!(tree.get(&1).unwrap(), Some(id(2)));
        assert_eq!(keys(&tree, ..), vec![1, 2]);
    }

    #[test]
    fn delete_after_splits_keeps_the_other_keys() {
        let pool = pool(8);
        let mut tree = BTree::create(&pool).unwrap();
        for key in 0..1000 {
            tree.insert(key, id(1)).unwrap();
        }
        for key in (0..1000).filter(|k| k % 3 != 0) {
            assert!(tree.delete(&key, id(1)).unwrap());
        }
        let expected: Vec<i32> = (0..1000).filter(|k| k % 3 == 0).collect();
        assert_eq!(keys(&tree, ..), expected);
    }

    #[test]
    fn too_large_key_is_rejected() {
        let pool = pool(4);
        let mut tree = BTree::create(&pool).unwrap();
        let key = "x".repeat(MAX_KEY_SIZE + 1);
        assert!(matches!(
            tree.insert(key, id(1)),
            Err(BTreeError::KeyTooLarge { size: 2001 })
        ));
        tree.insert("x".repeat(MAX_KEY_SIZE), id(1)).unwrap();
    }

    #[test]
    fn tree_works_with_few_frames() {
        let pool = pool(3);
        let mut tree = BTree::create(&pool).unwrap();
        for key in (0..3000).rev() {
            tree.insert(key, id(1)).unwrap();
        }
        assert_eq!(keys(&tree, 1000..1003), vec![1000, 1001, 1002]);
        assert!(pool.page_count() > 3);
    }

    #[test]
    fn node_is_encoded_with_a_header_and_entries() {
        let node = Node::Leaf {
            entries: vec![Entry {
                key: vec![0xab],
                id: RowId { page: 2, slot: 3 },
            }],
            next: None,
        };
        let bytes = node.encode();
        assert_eq!(
            bytes,
            vec![
                0, 1, 0, 0xff, 0xff, 0xff, 0xff, 1, 0, 0xab, 2, 0, 0, 0, 3, 0
            ]
        );
        assert_eq!(Node::decode(&bytes), Some(node));
    }

    #[test]
    fn split_point_divides_bytes_in_half() {
        assert_eq!(split_point(&[10, 10, 10, 10]), 2);
        assert_eq!(split_point(&[100, 1, 1, 1]), 1);
        assert_eq!(split_point(&[1, 1, 1, 100]), 3);
    }
}
