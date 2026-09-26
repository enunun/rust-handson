//! 8192バイトのスロット付きページ．
//!
//! ページの先頭の4バイトはヘッダーで，スロットの数と，タプルを置いた領域の始まりの位置を持つ．
//! ヘッダーのあとにスロット(タプルの位置と長さ)を並べ，タプルはページの末尾から前へ詰める．
//! スロットの並びとタプルの領域の間が，空いている領域である．

use std::fmt;

/// ページの大きさ(バイト)．
pub const PAGE_SIZE: usize = 8192;
/// ヘッダーの大きさ．スロットの数(2バイト)と，タプルの領域の始まり(2バイト)．
const HEADER_SIZE: usize = 4;
/// 1つのスロットの大きさ．タプルの位置(2バイト)と長さ(2バイト)．
const SLOT_SIZE: usize = 4;
/// 1つのページに置けるタプルの最大の大きさ．
pub const MAX_TUPLE_SIZE: usize = PAGE_SIZE - HEADER_SIZE - SLOT_SIZE;

/// ページの中のタプルの番号．スロットの並びの添字である．
pub type SlotId = u16;

/// タプルを置けないときのエラー．
#[derive(Debug, PartialEq)]
pub enum PageError {
    /// このページには空きが足りない．
    PageFull,
    /// どのページにも入らない大きさのタプルである．
    TupleTooLarge { size: usize },
}

/// スロット付きページ．
pub struct Page {
    data: Box<[u8; PAGE_SIZE]>,
}

/// 8192バイトをすべて書くと読めないので，スロットの数と空いている領域の大きさだけを書く．
impl fmt::Debug for Page {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Page")
            .field("slot_count", &self.slot_count())
            .field("free_space", &self.free_space())
            .finish()
    }
}

impl Page {
    /// タプルのない空のページを作る．
    pub fn new() -> Page {
        let mut page = Page {
            data: Box::new([0; PAGE_SIZE]),
        };
        page.set_free_end(PAGE_SIZE);
        page
    }

    /// スロットの数．消したタプルのスロットも数える．
    pub fn slot_count(&self) -> SlotId {
        self.read_u16(0)
    }

    /// タプルを置き，そのスロットの番号を返す．空きが足りなければエラーを返す．
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

    /// スロットのタプルを返す．スロットがないか，タプルを消していれば`None`を返す．
    pub fn get(&self, slot: SlotId) -> Option<&[u8]> {
        if slot >= self.slot_count() {
            return None;
        }
        let (offset, len) = self.slot(slot);
        if offset == 0 {
            return None;
        }
        Some(&self.data[offset..offset + len])
    }

    /// スロットのタプルを消す．消したら`true`を返す．スロットの番号は，ほかのタプルに使わない．
    pub fn delete(&mut self, slot: SlotId) -> bool {
        if self.get(slot).is_none() {
            return false;
        }
        self.set_slot(slot, 0, 0);
        true
    }

    /// スロットのタプルを置き換える．元のタプル以下の大きさならその場所に書き，
    /// 大きければ空いている領域に置き直す．空きが足りなければエラーを返し，元のタプルを残す．
    pub fn update(&mut self, slot: SlotId, tuple: &[u8]) -> Result<(), PageError> {
        check_size(tuple)?;
        let Some(old) = self.get(slot) else {
            return Err(PageError::PageFull);
        };
        let offset = if tuple.len() <= old.len() {
            self.slot(slot).0
        } else if tuple.len() <= self.free_space() {
            let offset = self.free_end() - tuple.len();
            self.set_free_end(offset);
            offset
        } else {
            return Err(PageError::PageFull);
        };
        self.data[offset..offset + tuple.len()].copy_from_slice(tuple);
        self.set_slot(slot, offset, tuple.len());
        Ok(())
    }

    /// スロットの並びの終わりから，タプルの領域の始まりまでのバイト数．
    fn free_space(&self) -> usize {
        self.free_end() - (HEADER_SIZE + usize::from(self.slot_count()) * SLOT_SIZE)
    }

    fn free_end(&self) -> usize {
        usize::from(self.read_u16(2))
    }

    fn set_free_end(&mut self, offset: usize) {
        self.write_u16(2, u16::try_from(offset).expect("offset is within a page"));
    }

    fn slot(&self, slot: SlotId) -> (usize, usize) {
        let position = HEADER_SIZE + usize::from(slot) * SLOT_SIZE;
        (
            usize::from(self.read_u16(position)),
            usize::from(self.read_u16(position + 2)),
        )
    }

    fn set_slot(&mut self, slot: SlotId, offset: usize, len: usize) {
        let position = HEADER_SIZE + usize::from(slot) * SLOT_SIZE;
        self.write_u16(
            position,
            u16::try_from(offset).expect("offset is within a page"),
        );
        self.write_u16(
            position + 2,
            u16::try_from(len).expect("length is within a page"),
        );
    }

    fn read_u16(&self, position: usize) -> u16 {
        let bytes: [u8; 2] = self.data[position..position + 2]
            .try_into()
            .expect("2 bytes");
        u16::from_le_bytes(bytes)
    }

    fn write_u16(&mut self, position: usize, value: u16) {
        self.data[position..position + 2].copy_from_slice(&value.to_le_bytes());
    }
}

/// どのページにも入らない大きさのタプルならエラーを返す．
fn check_size(tuple: &[u8]) -> Result<(), PageError> {
    if tuple.len() > MAX_TUPLE_SIZE {
        return Err(PageError::TupleTooLarge { size: tuple.len() });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserted_tuples_can_be_read_by_their_slots() {
        let mut page = Page::new();
        assert_eq!(page.insert(b"alice"), Ok(0));
        assert_eq!(page.insert(b"bob"), Ok(1));
        assert_eq!(page.get(0), Some(&b"alice"[..]));
        assert_eq!(page.get(1), Some(&b"bob"[..]));
        assert_eq!(page.get(2), None);
        assert_eq!(page.slot_count(), 2);
    }

    #[test]
    fn tuples_are_packed_from_the_end_of_the_page() {
        let mut page = Page::new();
        page.insert(b"ab").unwrap();
        page.insert(b"cde").unwrap();
        assert_eq!(&page.data[0..4], &[2, 0, 0xfb, 0x1f]);
        assert_eq!(&page.data[4..12], &[0xfe, 0x1f, 2, 0, 0xfb, 0x1f, 3, 0]);
        assert_eq!(&page.data[PAGE_SIZE - 5..], b"cdeab");
    }

    #[test]
    fn deleted_tuple_is_gone_and_its_slot_stays() {
        let mut page = Page::new();
        page.insert(b"alice").unwrap();
        page.insert(b"bob").unwrap();
        assert!(page.delete(0));
        assert_eq!(page.get(0), None);
        assert!(!page.delete(0));
        assert_eq!(page.get(1), Some(&b"bob"[..]));
        assert_eq!(page.insert(b"carol"), Ok(2));
    }

    #[test]
    fn update_writes_in_place_or_moves_the_tuple() {
        let mut page = Page::new();
        page.insert(b"alice").unwrap();
        page.insert(b"bob").unwrap();
        assert_eq!(page.update(0, b"al"), Ok(()));
        assert_eq!(page.get(0), Some(&b"al"[..]));
        assert_eq!(page.update(1, b"robert"), Ok(()));
        assert_eq!(page.get(1), Some(&b"robert"[..]));
        assert_eq!(page.get(0), Some(&b"al"[..]));
    }

    #[test]
    fn full_page_rejects_a_tuple() {
        let mut page = Page::new();
        let big = vec![7; 4000];
        page.insert(&big).unwrap();
        page.insert(&big).unwrap();
        assert_eq!(page.insert(&big), Err(PageError::PageFull));
        assert_eq!(page.insert(&[1; 176]), Ok(2));
        assert_eq!(page.update(0, &[1; 4001]), Err(PageError::PageFull));
        assert_eq!(page.get(0), Some(&big[..]));
    }

    #[test]
    fn tuple_larger_than_a_page_is_rejected() {
        let mut page = Page::new();
        assert_eq!(page.insert(&vec![0; MAX_TUPLE_SIZE]), Ok(0));
        let mut page = Page::new();
        assert_eq!(
            page.insert(&vec![0; MAX_TUPLE_SIZE + 1]),
            Err(PageError::TupleTooLarge {
                size: MAX_TUPLE_SIZE + 1
            })
        );
    }
}
