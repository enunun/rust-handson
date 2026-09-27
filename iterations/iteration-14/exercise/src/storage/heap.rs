//! 表の行を置くページの列．

use crate::catalog::TableSchema;
use crate::storage::page::{Page, PageError, SlotId};
use crate::storage::tuple::{TupleError, decode_tuple};
use crate::value::Row;

/// 表の行の位置．ページの番号と，ページの中のスロットの番号である．
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RowId {
    pub page: usize,
    pub slot: SlotId,
}

/// 1つの表のタプルを置くページの列．
#[derive(Debug, Default)]
pub struct HeapFile {
    pages: Vec<Page>,
}

impl HeapFile {
    /// ページのない空の表を作る．
    pub fn new() -> HeapFile {
        HeapFile::default()
    }

    /// タプルを最後のページに置く．空きが足りなければ，新しいページを加えて置く．
    pub fn insert(&mut self, tuple: &[u8]) -> Result<RowId, PageError> {
        if let Some(last) = self.pages.last_mut() {
            match last.insert(tuple) {
                Ok(slot) => {
                    return Ok(RowId {
                        page: self.pages.len() - 1,
                        slot,
                    });
                }
                Err(PageError::PageFull) => {}
                Err(error) => return Err(error),
            }
        }
        let mut page = Page::new();
        let slot = page.insert(tuple)?;
        self.pages.push(page);
        Ok(RowId {
            page: self.pages.len() - 1,
            slot,
        })
    }

    /// 位置のタプルを置き換え，新しい位置を返す．同じページに入らなければ，別のページに移す．
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

    /// 位置のタプルを消す．消したら`true`を返す．
    pub fn delete(&mut self, id: RowId) -> bool {
        match self.pages.get_mut(id.page) {
            Some(page) => page.delete(id.slot),
            None => false,
        }
    }

    /// すべてのタプルを，ページとスロットの順に位置と一緒に返す．
    pub fn tuples(&self) -> Vec<(RowId, &[u8])> {
        let mut tuples = Vec::new();
        for (page_number, page) in self.pages.iter().enumerate() {
            for slot in 0..page.slot_count() {
                if let Some(tuple) = page.get(slot) {
                    tuples.push((
                        RowId {
                            page: page_number,
                            slot,
                        },
                        tuple,
                    ));
                }
            }
        }
        tuples
    }

    /// すべての行を，表の列の型に従って復号し，位置と一緒に返す．
    pub fn rows(&self, schema: &TableSchema) -> Result<Vec<(RowId, Row)>, TupleError> {
        self.tuples()
            .into_iter()
            .map(|(id, tuple)| Ok((id, decode_tuple(tuple, schema)?)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::page::MAX_TUPLE_SIZE;

    /// 位置のタプル．なければ`None`である．
    fn get(heap: &HeapFile, id: RowId) -> Option<Vec<u8>> {
        heap.tuples()
            .into_iter()
            .find(|(found, _)| *found == id)
            .map(|(_, tuple)| tuple.to_vec())
    }

    #[test]
    fn tuples_go_to_a_new_page_when_the_last_one_is_full() {
        let mut heap = HeapFile::new();
        let big = vec![1; 3000];
        assert_eq!(heap.insert(&big), Ok(RowId { page: 0, slot: 0 }));
        assert_eq!(heap.insert(&big), Ok(RowId { page: 0, slot: 1 }));
        assert_eq!(heap.insert(&big), Ok(RowId { page: 1, slot: 0 }));
        assert_eq!(heap.pages.len(), 2);
        assert_eq!(get(&heap, RowId { page: 1, slot: 0 }), Some(big));
    }

    #[test]
    fn tuples_are_listed_in_page_and_slot_order_without_deleted_ones() {
        let mut heap = HeapFile::new();
        let first = heap.insert(b"a").unwrap();
        let second = heap.insert(b"b").unwrap();
        heap.insert(b"c").unwrap();
        assert!(heap.delete(second));
        let tuples: Vec<&[u8]> = heap.tuples().into_iter().map(|(_, tuple)| tuple).collect();
        assert_eq!(tuples, vec![&b"a"[..], &b"c"[..]]);
        assert_eq!(heap.tuples()[0].0, first);
    }

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

    #[test]
    fn tuple_larger_than_a_page_is_rejected() {
        let mut heap = HeapFile::new();
        assert_eq!(
            heap.insert(&vec![0; MAX_TUPLE_SIZE + 1]),
            Err(PageError::TupleTooLarge {
                size: MAX_TUPLE_SIZE + 1
            })
        );
        assert_eq!(heap.pages.len(), 0);
    }
}
