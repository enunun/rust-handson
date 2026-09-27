//! キーから行の位置を引くインデックス．列の型ごとのB+木を，SQLの値で使えるようにまとめる．

pub mod btree;
pub mod key;

use std::ops::Bound;

use crate::index::btree::{BTree, BTreeError, MAX_KEY_SIZE};
use crate::index::key::IndexKey;
use crate::storage::buffer::BufferPool;
use crate::storage::disk::DiskManager;
use crate::storage::heap::RowId;
use crate::value::{DataType, Value};

/// 列の型ごとのB+木．SQLの値を列の型のキーにして，B+木を使う．
pub enum AnyIndex<'a> {
    Integer(BTree<'a, i32>),
    BigInt(BTree<'a, i64>),
    Boolean(BTree<'a, bool>),
    Varchar(BTree<'a, String>),
}

/// 表の1つの列のインデックス．`column`は表の列の番号である．
pub struct ColumnIndex<'a> {
    pub column: usize,
    pub index: AnyIndex<'a>,
}

impl<'a> AnyIndex<'a> {
    /// 列の型のキーを持つ空の木を，空のプールに作る．
    pub fn create(
        data_type: &DataType,
        pool: &'a BufferPool<Box<dyn DiskManager>>,
    ) -> Result<AnyIndex<'a>, BTreeError> {
        Ok(match data_type {
            DataType::Integer => AnyIndex::Integer(BTree::create(pool)?),
            DataType::BigInt => AnyIndex::BigInt(BTree::create(pool)?),
            DataType::Boolean => AnyIndex::Boolean(BTree::create(pool)?),
            DataType::Varchar(_) => AnyIndex::Varchar(BTree::create(pool)?),
        })
    }

    /// プールにある，列の型のキーを持つ木を開く．
    pub fn open(
        data_type: &DataType,
        pool: &'a BufferPool<Box<dyn DiskManager>>,
    ) -> Result<AnyIndex<'a>, BTreeError> {
        Ok(match data_type {
            DataType::Integer => AnyIndex::Integer(BTree::open(pool)?),
            DataType::BigInt => AnyIndex::BigInt(BTree::open(pool)?),
            DataType::Boolean => AnyIndex::Boolean(BTree::open(pool)?),
            DataType::Varchar(_) => AnyIndex::Varchar(BTree::open(pool)?),
        })
    }

    /// 値と行の位置の組を加える．`NULL`は加えない．
    pub fn insert(&mut self, value: &Value, id: RowId) -> Result<(), BTreeError> {
        match (self, value) {
            (_, Value::Null) => Ok(()),
            (AnyIndex::Integer(tree), Value::Integer(n)) => tree.insert(*n, id),
            (AnyIndex::BigInt(tree), Value::BigInt(n)) => tree.insert(*n, id),
            (AnyIndex::Boolean(tree), Value::Boolean(b)) => tree.insert(*b, id),
            (AnyIndex::Varchar(tree), Value::Varchar(s)) => tree.insert(s.clone(), id),
            (_, value) => unreachable!("{value:?} does not match the type of the index"),
        }
    }

    /// 値と行の位置の組を消す．`NULL`は加えていないので，何もしない．
    pub fn delete(&mut self, value: &Value, id: RowId) -> Result<(), BTreeError> {
        match (self, value) {
            (_, Value::Null) => return Ok(()),
            (AnyIndex::Integer(tree), Value::Integer(n)) => tree.delete(n, id)?,
            (AnyIndex::BigInt(tree), Value::BigInt(n)) => tree.delete(n, id)?,
            (AnyIndex::Boolean(tree), Value::Boolean(b)) => tree.delete(b, id)?,
            (AnyIndex::Varchar(tree), Value::Varchar(s)) => tree.delete(s, id)?,
            (_, value) => unreachable!("{value:?} does not match the type of the index"),
        };
        Ok(())
    }

    /// 値が範囲にある項目の行の位置を，値の順に返す．範囲の値は，列の型の値である．
    pub fn range(
        &self,
        lower: Bound<&Value>,
        upper: Bound<&Value>,
    ) -> Result<Vec<RowId>, BTreeError> {
        match self {
            AnyIndex::Integer(tree) => row_ids(tree, lower.map(as_integer), upper.map(as_integer)),
            AnyIndex::BigInt(tree) => row_ids(tree, lower.map(as_bigint), upper.map(as_bigint)),
            AnyIndex::Boolean(tree) => row_ids(tree, lower.map(as_boolean), upper.map(as_boolean)),
            AnyIndex::Varchar(tree) => row_ids(tree, lower.map(as_varchar), upper.map(as_varchar)),
        }
    }

    /// 値の項目の行の位置をすべて返す．
    pub fn lookup(&self, value: &Value) -> Result<Vec<RowId>, BTreeError> {
        self.range(Bound::Included(value), Bound::Included(value))
    }
}

fn row_ids<K: IndexKey>(
    tree: &BTree<'_, K>,
    lower: Bound<K>,
    upper: Bound<K>,
) -> Result<Vec<RowId>, BTreeError> {
    tree.range((lower, upper))?
        .map(|entry| entry.map(|(_, id)| id))
        .collect()
}

fn as_integer(value: &Value) -> i32 {
    match value {
        Value::Integer(n) => *n,
        other => unreachable!("{other:?} is not an INTEGER"),
    }
}

fn as_bigint(value: &Value) -> i64 {
    match value {
        Value::BigInt(n) => *n,
        other => unreachable!("{other:?} is not a BIGINT"),
    }
}

fn as_boolean(value: &Value) -> bool {
    match value {
        Value::Boolean(b) => *b,
        other => unreachable!("{other:?} is not a BOOLEAN"),
    }
}

fn as_varchar(value: &Value) -> String {
    match value {
        Value::Varchar(s) => s.clone(),
        other => unreachable!("{other:?} is not a VARCHAR"),
    }
}

/// 値をインデックスのキーにできるかを調べる．符号化したキーが大きすぎればエラーを返す．
pub fn check_key(value: &Value) -> Result<(), BTreeError> {
    match value {
        Value::Varchar(s) if s.len() > MAX_KEY_SIZE => {
            Err(BTreeError::KeyTooLarge { size: s.len() })
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::disk::MemoryDiskManager;

    fn pool() -> BufferPool<Box<dyn DiskManager>> {
        BufferPool::new(Box::new(MemoryDiskManager::default()), 8)
    }

    fn id(page: usize) -> RowId {
        RowId { page, slot: 0 }
    }

    fn varchar(s: &str) -> Value {
        Value::Varchar(s.to_string())
    }

    #[test]
    fn index_of_the_column_type_takes_sql_values() {
        let pool = pool();
        let mut index = AnyIndex::create(&DataType::Varchar(10), &pool).unwrap();
        index.insert(&varchar("b"), id(1)).unwrap();
        index.insert(&varchar("a"), id(2)).unwrap();
        index.insert(&varchar("b"), id(3)).unwrap();
        assert_eq!(index.lookup(&varchar("b")).unwrap(), vec![id(1), id(3)]);
        let a = varchar("a");
        assert_eq!(
            index.range(Bound::Included(&a), Bound::Unbounded).unwrap(),
            vec![id(2), id(1), id(3)]
        );
    }

    #[test]
    fn nulls_are_not_indexed() {
        let pool = pool();
        let mut index = AnyIndex::create(&DataType::Integer, &pool).unwrap();
        index.insert(&Value::Null, id(1)).unwrap();
        index.insert(&Value::Integer(5), id(2)).unwrap();
        index.delete(&Value::Null, id(1)).unwrap();
        assert_eq!(
            index.range(Bound::Unbounded, Bound::Unbounded).unwrap(),
            vec![id(2)]
        );
    }

    #[test]
    fn deleted_entry_is_no_longer_found() {
        let pool = pool();
        let mut index = AnyIndex::create(&DataType::BigInt, &pool).unwrap();
        index.insert(&Value::BigInt(7), id(1)).unwrap();
        index.insert(&Value::BigInt(7), id(2)).unwrap();
        index.delete(&Value::BigInt(7), id(1)).unwrap();
        assert_eq!(index.lookup(&Value::BigInt(7)).unwrap(), vec![id(2)]);
    }

    #[test]
    fn reopened_index_has_the_same_entries() {
        let pool = pool();
        let mut index = AnyIndex::create(&DataType::Boolean, &pool).unwrap();
        index.insert(&Value::Boolean(true), id(4)).unwrap();
        let index = AnyIndex::open(&DataType::Boolean, &pool).unwrap();
        assert_eq!(index.lookup(&Value::Boolean(true)).unwrap(), vec![id(4)]);
    }

    #[test]
    fn long_string_cannot_be_a_key() {
        assert!(check_key(&Value::Varchar("x".repeat(MAX_KEY_SIZE))).is_ok());
        assert!(matches!(
            check_key(&Value::Varchar("x".repeat(MAX_KEY_SIZE + 1))),
            Err(BTreeError::KeyTooLarge { size: 2001 })
        ));
    }
}
