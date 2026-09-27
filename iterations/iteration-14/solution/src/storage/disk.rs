//! ページをページ番号で読み書きする．ファイルに保存する実装と，メモリーに置く実装がある．

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;

use crate::storage::page::{PAGE_SIZE, PageBytes};

/// ページの番号．ファイルの先頭から数えて何番目のページかを表す．
pub type PageId = usize;

/// ページを読み書きするもの．
pub trait DiskManager {
    /// ページを読んで`buffer`に書き写す．
    fn read_page(&self, page_id: PageId, buffer: &mut PageBytes) -> io::Result<()>;
    /// `buffer`をページに書く．
    fn write_page(&mut self, page_id: PageId, buffer: &PageBytes) -> io::Result<()>;
    /// 末尾に0で埋めたページを加え，その番号を返す．
    fn allocate_page(&mut self) -> io::Result<PageId>;
    /// ページの数．
    fn page_count(&self) -> usize;
}

/// ページを1つのファイルに順に並べて保存する．`page_id`番目のページは，ファイルの
/// `page_id * PAGE_SIZE`バイト目から始まる．
pub struct FileDiskManager {
    file: File,
    page_count: usize,
}

impl FileDiskManager {
    /// ファイルを開く．なければ空のファイルを作る．
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

    /// 空のファイルを作る．同じ名前のファイルがあれば，中身を捨てる．
    pub fn create(path: &Path) -> io::Result<FileDiskManager> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        Ok(FileDiskManager {
            file,
            page_count: 0,
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

/// ページをメモリーに置く．プロセスが終わると消える．
#[derive(Default)]
pub struct MemoryDiskManager {
    pages: Vec<Box<PageBytes>>,
}

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

    fn write_page(&mut self, page_id: PageId, buffer: &PageBytes) -> io::Result<()> {
        match self.pages.get_mut(page_id) {
            Some(page) => {
                page.copy_from_slice(buffer);
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

    fn page_count(&self) -> usize {
        self.pages.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 先頭のバイトだけを`value`にしたページ．
    fn page_with(value: u8) -> Box<PageBytes> {
        let mut page = Box::new([0; PAGE_SIZE]);
        page[0] = value;
        page
    }

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

    #[test]
    fn file_disk_keeps_pages_after_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.heap");
        let mut disk = FileDiskManager::open(&path).unwrap();
        disk.allocate_page().unwrap();
        disk.allocate_page().unwrap();
        disk.write_page(1, &page_with(9)).unwrap();
        drop(disk);
        let disk = FileDiskManager::open(&path).unwrap();
        assert_eq!(disk.page_count(), 2);
        let mut buffer = Box::new([0; PAGE_SIZE]);
        disk.read_page(1, &mut buffer).unwrap();
        assert_eq!(buffer[0], 9);
        assert_eq!(
            std::fs::metadata(&path).unwrap().len(),
            u64::try_from(2 * PAGE_SIZE).unwrap()
        );
    }

    #[test]
    fn reading_a_missing_page_is_an_error() {
        let disk = MemoryDiskManager::default();
        let mut buffer = Box::new([0; PAGE_SIZE]);
        assert!(disk.read_page(0, &mut buffer).is_err());
    }
}
