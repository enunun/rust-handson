//! 表の行を置くページの列．ページは`DiskManager`で読み書きする．

use std::fmt;
use std::io;

use crate::catalog::TableSchema;
use crate::storage::disk::{DiskManager, PageId};
use crate::storage::page::{PAGE_SIZE, Page, PageError, SlotId};
use crate::storage::tuple::{TupleError, decode_tuple};
use crate::value::Row;

/// 表の行の位置．ページの番号と，ページの中のスロットの番号である．
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RowId {
    pub page: PageId,
    pub slot: SlotId,
}

/// ヒープファイルを読み書きするときのエラー．
#[derive(Debug)]
pub enum HeapError {
    Page(PageError),
    Tuple(TupleError),
    Io(io::Error),
}

impl From<PageError> for HeapError {
    fn from(error: PageError) -> HeapError {
        HeapError::Page(error)
    }
}

impl From<TupleError> for HeapError {
    fn from(error: TupleError) -> HeapError {
        HeapError::Tuple(error)
    }
}

impl From<io::Error> for HeapError {
    fn from(error: io::Error) -> HeapError {
        HeapError::Io(error)
    }
}

/// 1つの表のタプルを置くページの列．操作のたびに，ページを`DiskManager`から読み，変えたら書き戻す．
pub struct HeapFile {
    disk: Box<dyn DiskManager>,
}

/// ページの内容は書かず，ページの数だけを書く．
impl fmt::Debug for HeapFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HeapFile")
            .field("page_count", &self.disk.page_count())
            .finish()
    }
}

impl HeapFile {
    /// `disk`のページを表のページの列として使う．
    pub fn new(disk: Box<dyn DiskManager>) -> HeapFile {
        HeapFile { disk }
    }

    /// タプルを最後のページに置く．空きが足りなければ，新しいページを加えて置く．
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

    /// 位置のタプルを置き換え，新しい位置を返す．同じページに入らなければ，別のページに移す．
    pub fn update(&mut self, id: RowId, tuple: &[u8]) -> Result<RowId, HeapError> {
        let mut page = self.read(id.page)?;
        match page.update(id.slot, tuple) {
            Ok(()) => {
                self.write(id.page, &page)?;
                Ok(id)
            }
            Err(PageError::PageFull) => {
                let new_id = self.insert(tuple)?;
                self.delete(id)?;
                Ok(new_id)
            }
            Err(error) => Err(error.into()),
        }
    }

    /// 位置のタプルを消す．消したら`true`を返す．
    pub fn delete(&mut self, id: RowId) -> Result<bool, HeapError> {
        if id.page >= self.disk.page_count() {
            return Ok(false);
        }
        let mut page = self.read(id.page)?;
        let deleted = page.delete(id.slot);
        if deleted {
            self.write(id.page, &page)?;
        }
        Ok(deleted)
    }

    /// すべてのタプルを，ページとスロットの順に位置と一緒に返す．
    pub fn tuples(&self) -> Result<Vec<(RowId, Vec<u8>)>, HeapError> {
        let mut tuples = Vec::new();
        for page_id in 0..self.disk.page_count() {
            let page = self.read(page_id)?;
            for slot in 0..page.slot_count() {
                if let Some(tuple) = page.get(slot) {
                    tuples.push((
                        RowId {
                            page: page_id,
                            slot,
                        },
                        tuple.to_vec(),
                    ));
                }
            }
        }
        Ok(tuples)
    }

    /// すべての行を，表の列の型に従って復号し，位置と一緒に返す．
    pub fn rows(&self, schema: &TableSchema) -> Result<Vec<(RowId, Row)>, HeapError> {
        self.tuples()?
            .into_iter()
            .map(|(id, tuple)| Ok((id, decode_tuple(&tuple, schema)?)))
            .collect()
    }

    fn read(&self, page_id: PageId) -> Result<Page, HeapError> {
        let mut data = Box::new([0; PAGE_SIZE]);
        self.disk.read_page(page_id, &mut data)?;
        Ok(Page::from_bytes(data))
    }

    fn write(&mut self, page_id: PageId, page: &Page) -> Result<(), HeapError> {
        self.disk.write_page(page_id, page.bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::disk::{FileDiskManager, MemoryDiskManager};
    use crate::storage::page::MAX_TUPLE_SIZE;

    fn memory_heap() -> HeapFile {
        HeapFile::new(Box::new(MemoryDiskManager::default()))
    }

    /// 位置のタプル．なければ`None`である．
    fn get(heap: &HeapFile, id: RowId) -> Option<Vec<u8>> {
        heap.tuples()
            .unwrap()
            .into_iter()
            .find(|(found, _)| *found == id)
            .map(|(_, tuple)| tuple)
    }

    #[test]
    fn tuples_go_to_a_new_page_when_the_last_one_is_full() {
        let mut heap = memory_heap();
        let big = vec![1; 3000];
        assert_eq!(heap.insert(&big).unwrap(), RowId { page: 0, slot: 0 });
        assert_eq!(heap.insert(&big).unwrap(), RowId { page: 0, slot: 1 });
        assert_eq!(heap.insert(&big).unwrap(), RowId { page: 1, slot: 0 });
        assert_eq!(heap.disk.page_count(), 2);
        assert_eq!(get(&heap, RowId { page: 1, slot: 0 }), Some(big));
    }

    #[test]
    fn tuples_are_listed_in_page_and_slot_order_without_deleted_ones() {
        let mut heap = memory_heap();
        let first = heap.insert(b"a").unwrap();
        let second = heap.insert(b"b").unwrap();
        heap.insert(b"c").unwrap();
        assert!(heap.delete(second).unwrap());
        let tuples: Vec<Vec<u8>> = heap
            .tuples()
            .unwrap()
            .into_iter()
            .map(|(_, tuple)| tuple)
            .collect();
        assert_eq!(tuples, vec![b"a".to_vec(), b"c".to_vec()]);
        assert_eq!(heap.tuples().unwrap()[0].0, first);
    }

    #[test]
    fn update_moves_a_tuple_that_no_longer_fits_its_page() {
        let mut heap = memory_heap();
        let id = heap.insert(&[1; 4000]).unwrap();
        heap.insert(&[2; 4000]).unwrap();
        let moved = heap.update(id, &[3; 4100]).unwrap();
        assert_eq!(moved, RowId { page: 1, slot: 0 });
        assert_eq!(get(&heap, id), None);
        assert_eq!(get(&heap, moved), Some(vec![3; 4100]));
        assert_eq!(heap.update(moved, b"x").unwrap(), moved);
    }

    #[test]
    fn tuple_larger_than_a_page_is_rejected() {
        let mut heap = memory_heap();
        assert!(matches!(
            heap.insert(&vec![0; MAX_TUPLE_SIZE + 1]),
            Err(HeapError::Page(PageError::TupleTooLarge { .. }))
        ));
        assert_eq!(heap.disk.page_count(), 0);
    }

    #[test]
    fn tuples_in_a_file_remain_after_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.heap");
        let mut heap = HeapFile::new(Box::new(FileDiskManager::open(&path).unwrap()));
        heap.insert(b"alice").unwrap();
        let bob = heap.insert(b"bob").unwrap();
        heap.update(bob, b"bobby").unwrap();
        drop(heap);
        let heap = HeapFile::new(Box::new(FileDiskManager::open(&path).unwrap()));
        let tuples: Vec<Vec<u8>> = heap
            .tuples()
            .unwrap()
            .into_iter()
            .map(|(_, tuple)| tuple)
            .collect();
        assert_eq!(tuples, vec![b"alice".to_vec(), b"bobby".to_vec()]);
    }
}
