//! 変更を先に書くログ(WAL)．ページの変更とトランザクションの開始，コミット，中止を記録し，
//! 障害のあとにログから変更をやり直す．

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Seek, SeekFrom, Write};
use std::path::Path;

use crate::storage::disk::{DiskManager, FileDiskManager, PageId};
use crate::storage::page::PageBytes;
use crate::txn::{TransactionManager, TxnId, TxnStatus};

/// ログの中の位置．ログのファイルの先頭からのバイト数で，レコードの終わりの位置を表す．
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Lsn(pub u64);

/// ログのレコード．
#[derive(Debug, Clone, PartialEq)]
pub enum WalRecord {
    Begin {
        xid: TxnId,
    },
    Commit {
        xid: TxnId,
    },
    Abort {
        xid: TxnId,
    },
    /// ページを書き換えたあとの内容．`file`はデータディレクトリの中のファイルの名前である．
    PageImage {
        file: String,
        page: PageId,
        bytes: Box<PageBytes>,
    },
    /// ここまでの変更は，表とインデックスのファイルとトランザクションの状態のファイルに書いてある．
    Checkpoint,
}

const BEGIN: u8 = 1;
const COMMIT: u8 = 2;
const ABORT: u8 = 3;
const PAGE_IMAGE: u8 = 4;
const CHECKPOINT: u8 = 5;

/// レコードの前に置く，内容のバイト数(4バイト)とCRC-32(4バイト)．
const RECORD_HEADER_SIZE: usize = 8;

/// ログを書く．書いたレコードは`BufWriter`にためて，まとめて`W`に書く．
pub struct WalWriter<W: Write> {
    out: BufWriter<W>,
    /// 次のレコードを書く位置．
    next: Lsn,
    /// ここまでのレコードは，ディスクに書いた．
    flushed: Lsn,
    /// 書き込みに失敗したら真にする．そのあとのコミットは失敗する．
    failed: bool,
}

/// ためているレコードは書かず，次のレコードを書く位置とディスクに書いた位置だけを書く．
impl<W: Write> std::fmt::Debug for WalWriter<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WalWriter")
            .field("next", &self.next)
            .field("flushed", &self.flushed)
            .finish()
    }
}

impl<W: Write> WalWriter<W> {
    /// `start`の位置から`out`にログを書く．
    pub fn new(out: W, start: Lsn) -> WalWriter<W> {
        WalWriter {
            out: BufWriter::new(out),
            next: start,
            flushed: start,
            failed: false,
        }
    }

    /// レコードを書き，その終わりの位置を返す．
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

    /// 次のレコードを書く位置．
    pub fn next_lsn(&self) -> Lsn {
        self.next
    }

    /// 書き込みに失敗していたら，エラーを返す．
    fn check(&self) -> io::Result<()> {
        if self.failed {
            return Err(io::Error::other("an earlier write to the log failed"));
        }
        Ok(())
    }
}

impl WalWriter<File> {
    /// データディレクトリのログのファイルを開き，`end`より後ろを切り捨てて，`end`から書く．
    pub fn open(path: &Path, end: Lsn) -> io::Result<WalWriter<File>> {
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        file.set_len(end.0)?;
        file.seek(SeekFrom::Start(end.0))?;
        Ok(WalWriter::new(file, end))
    }

    /// `lsn`までのレコードをファイルに書き，ディスクに届くまで待つ．
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

impl WalRecord {
    fn encode(&self) -> Vec<u8> {
        match self {
            WalRecord::Begin { xid } => encode_xid(BEGIN, *xid),
            WalRecord::Commit { xid } => encode_xid(COMMIT, *xid),
            WalRecord::Abort { xid } => encode_xid(ABORT, *xid),
            WalRecord::PageImage { file, page, bytes } => {
                let mut out = vec![PAGE_IMAGE];
                let len = u16::try_from(file.len()).expect("file name fits in u16");
                out.extend_from_slice(&len.to_le_bytes());
                out.extend_from_slice(file.as_bytes());
                let page = u32::try_from(*page).expect("page number fits in u32");
                out.extend_from_slice(&page.to_le_bytes());
                out.extend_from_slice(&bytes[..]);
                out
            }
            WalRecord::Checkpoint => vec![CHECKPOINT],
        }
    }

    fn decode(payload: &[u8]) -> Option<WalRecord> {
        let (&kind, mut rest) = payload.split_first()?;
        let record = match kind {
            BEGIN => WalRecord::Begin {
                xid: read_xid(rest)?,
            },
            COMMIT => WalRecord::Commit {
                xid: read_xid(rest)?,
            },
            ABORT => WalRecord::Abort {
                xid: read_xid(rest)?,
            },
            PAGE_IMAGE => {
                let len = usize::from(u16::from_le_bytes(take(&mut rest, 2)?.try_into().ok()?));
                let file = String::from_utf8(take(&mut rest, len)?.to_vec()).ok()?;
                let page = u32::from_le_bytes(take(&mut rest, 4)?.try_into().ok()?);
                let bytes: Box<PageBytes> = Box::new(rest.try_into().ok()?);
                WalRecord::PageImage {
                    file,
                    page: usize::try_from(page).ok()?,
                    bytes,
                }
            }
            CHECKPOINT => WalRecord::Checkpoint,
            _ => return None,
        };
        Some(record)
    }
}

fn encode_xid(kind: u8, xid: TxnId) -> Vec<u8> {
    let mut out = vec![kind];
    out.extend_from_slice(&xid.0.to_le_bytes());
    out
}

fn read_xid(bytes: &[u8]) -> Option<TxnId> {
    Some(TxnId(u32::from_le_bytes(bytes.try_into().ok()?)))
}

fn take<'a>(input: &mut &'a [u8], len: usize) -> Option<&'a [u8]> {
    if input.len() < len {
        return None;
    }
    let (head, rest) = input.split_at(len);
    *input = rest;
    Some(head)
}

/// ログのバイト列からレコードを順に読む．レコードと，その終わりの位置の組を返す．
/// 途中で終わるレコードや，CRC-32の合わないレコードに会ったら，そこで読むのをやめる．
/// 最後の要素は，正しく読めたレコードの終わりの位置である．
pub fn read_records(bytes: &[u8]) -> (Vec<(WalRecord, Lsn)>, Lsn) {
    let mut records = Vec::new();
    let mut position = 0;
    while let Some(header) = bytes.get(position..position + RECORD_HEADER_SIZE) {
        let len = u32::from_le_bytes(header[..4].try_into().expect("4 bytes"));
        let crc = u32::from_le_bytes(header[4..].try_into().expect("4 bytes"));
        let start = position + RECORD_HEADER_SIZE;
        let Some(payload) = usize::try_from(len)
            .ok()
            .and_then(|len| bytes.get(start..start + len))
        else {
            break;
        };
        if crc32fast::hash(payload) != crc {
            break;
        }
        let Some(record) = WalRecord::decode(payload) else {
            break;
        };
        position = start + payload.len();
        records.push((record, lsn_of(position)));
    }
    (records, lsn_of(position))
}

fn lsn_of(position: usize) -> Lsn {
    Lsn(u64::try_from(position).expect("usize fits in u64"))
}

/// データディレクトリのログを，最後のチェックポイントのあとからやり直す．
/// ページの内容をファイルに書き，トランザクションの開始と終了を`manager`に記録する．
/// コミットも中止も記録されていないトランザクションは中止したものとみなす．
/// 正しく読めたログの終わりの位置を返す．
pub fn recover(dir: &Path, wal: &Path, manager: &mut TransactionManager) -> io::Result<Lsn> {
    let bytes = match fs::read(wal) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error),
    };
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

/// ファイルがあれば，ページの内容を書く．ファイルがページまで届いていなければ，ページを加える．
fn redo_page(path: &Path, page: PageId, bytes: &PageBytes) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let mut disk = FileDiskManager::open(path)?;
    while disk.page_count() <= page {
        disk.allocate_page()?;
    }
    disk.write_page(page, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::page::PAGE_SIZE;

    fn page_of(byte: u8) -> Box<PageBytes> {
        Box::new([byte; PAGE_SIZE])
    }

    fn image(file: &str, page: PageId, byte: u8) -> WalRecord {
        WalRecord::PageImage {
            file: file.to_string(),
            page,
            bytes: page_of(byte),
        }
    }

    #[test]
    fn records_are_read_back_in_order() {
        let mut writer = WalWriter::new(Vec::new(), Lsn(0));
        let records = vec![
            WalRecord::Begin { xid: TxnId(1) },
            image("54.heap", 2, 7),
            WalRecord::Commit { xid: TxnId(1) },
            WalRecord::Abort { xid: TxnId(2) },
            WalRecord::Checkpoint,
        ];
        let mut ends = Vec::new();
        for record in &records {
            ends.push(writer.append(record).unwrap());
        }
        let bytes = writer.out.into_inner().unwrap();
        let (read, end) = read_records(&bytes);
        assert_eq!(
            read,
            records.into_iter().zip(ends.clone()).collect::<Vec<_>>()
        );
        assert_eq!(end, ends[4]);
        assert_eq!(ends[0], Lsn(8 + 5));
    }

    #[test]
    fn record_has_its_length_and_checksum_first() {
        let mut writer = WalWriter::new(Vec::new(), Lsn(0));
        writer.append(&WalRecord::Commit { xid: TxnId(3) }).unwrap();
        let bytes = writer.out.into_inner().unwrap();
        let payload = [COMMIT, 3, 0, 0, 0];
        assert_eq!(&bytes[..4], &[5, 0, 0, 0]);
        assert_eq!(&bytes[4..8], &crc32fast::hash(&payload).to_le_bytes());
        assert_eq!(&bytes[8..], &payload);
    }

    #[test]
    fn torn_or_corrupted_tail_is_ignored() {
        let mut writer = WalWriter::new(Vec::new(), Lsn(0));
        let first = writer.append(&WalRecord::Begin { xid: TxnId(1) }).unwrap();
        writer.append(&WalRecord::Commit { xid: TxnId(1) }).unwrap();
        let bytes = writer.out.into_inner().unwrap();
        let (records, end) = read_records(&bytes[..bytes.len() - 1]);
        assert_eq!((records.len(), end), (1, first));
        let mut corrupted = bytes.clone();
        let last = corrupted.len() - 1;
        corrupted[last] ^= 0xff;
        let (records, end) = read_records(&corrupted);
        assert_eq!((records.len(), end), (1, first));
    }

    #[test]
    fn recovery_redoes_pages_after_the_last_checkpoint() {
        let dir = tempfile::tempdir().unwrap();
        let heap = dir.path().join("54.heap");
        FileDiskManager::create(&heap).unwrap();
        let wal_path = dir.path().join("wal");
        let mut writer = WalWriter::open(&wal_path, Lsn(0)).unwrap();
        writer.append(&image("54.heap", 0, 1)).unwrap();
        writer.append(&WalRecord::Checkpoint).unwrap();
        writer.append(&WalRecord::Begin { xid: TxnId(1) }).unwrap();
        writer.append(&image("54.heap", 1, 9)).unwrap();
        writer.append(&image("gone.heap", 0, 9)).unwrap();
        writer.append(&WalRecord::Commit { xid: TxnId(1) }).unwrap();
        writer.append(&WalRecord::Begin { xid: TxnId(2) }).unwrap();
        let end = writer.next_lsn();
        writer.flush_to(end).unwrap();

        let mut manager = TransactionManager::default();
        assert_eq!(recover(dir.path(), &wal_path, &mut manager).unwrap(), end);
        let disk = FileDiskManager::open(&heap).unwrap();
        assert_eq!(disk.page_count(), 2);
        let mut buffer = Box::new([0; PAGE_SIZE]);
        disk.read_page(0, &mut buffer).unwrap();
        assert_eq!(buffer[0], 0);
        disk.read_page(1, &mut buffer).unwrap();
        assert_eq!(buffer[0], 9);
        assert_eq!(manager.status(TxnId(1)), TxnStatus::Committed);
        assert_eq!(manager.status(TxnId(2)), TxnStatus::Aborted);
        assert_eq!(manager.begin().xid(), TxnId(3));
        assert!(!dir.path().join("gone.heap").exists());
    }

    #[test]
    fn opening_the_log_drops_the_bytes_after_the_end() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wal");
        std::fs::write(&path, [1, 2, 3, 4, 5]).unwrap();
        let mut writer = WalWriter::open(&path, Lsn(2)).unwrap();
        writer.append(&WalRecord::Checkpoint).unwrap();
        let end = writer.next_lsn();
        writer.flush_to(end).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..2], &[1, 2]);
        assert_eq!(bytes.len(), 2 + 8 + 1);
    }
}
