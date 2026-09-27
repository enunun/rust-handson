//! トランザクションと，タプルの版がどのトランザクションから見えるかの判定．

use std::fs;
use std::io;
use std::path::Path;

use crate::storage::tuple::TupleHeader;

/// トランザクションの番号．1から順に振る．0はどのトランザクションでもないことを表す．
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TxnId(pub u32);

impl TxnId {
    /// どのトランザクションでもないことを表す番号．削除されていないタプルの`xmax`に使う．
    pub const INVALID: TxnId = TxnId(0);
}

/// トランザクションの状態．
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxnStatus {
    InProgress,
    Committed,
    Aborted,
}

/// トランザクションの状態に合わない文を実行したときのエラー．
#[derive(Debug, PartialEq)]
pub enum TransactionError {
    /// 失敗したトランザクションの中で，`COMMIT`と`ROLLBACK`のほかの文を実行した．
    InFailedTransaction,
    /// トランザクションの中で`START TRANSACTION`を実行した．
    AlreadyInProgress,
    /// トランザクションの中で，表やインデックスを作る文か消す文を実行した．
    NotInTransactionBlock { statement: &'static str },
}

/// 始めたトランザクション．`commit`か`rollback`で値を消費して終える．
#[derive(Debug, PartialEq)]
pub struct Transaction {
    xid: TxnId,
}

/// 文を実行する時点で，どのトランザクションの変更が見えるかを表す．
/// `xmax`以上の番号と，`active`の番号のトランザクションは，まだ終わっていないものとみなす．
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub xid: TxnId,
    pub xmax: TxnId,
    pub active: Vec<TxnId>,
}

/// トランザクションの番号を振り，状態を覚える．
#[derive(Debug)]
pub struct TransactionManager {
    /// `statuses[n]`は番号`n`のトランザクションの状態である．番号0は使わない．
    statuses: Vec<TxnStatus>,
}

impl Default for TransactionManager {
    fn default() -> TransactionManager {
        TransactionManager {
            statuses: vec![TxnStatus::Aborted],
        }
    }
}

impl TransactionManager {
    /// 新しいトランザクションを始める．
    pub fn begin(&mut self) -> Transaction {
        let xid = TxnId(u32::try_from(self.statuses.len()).expect("fewer than 2^32 transactions"));
        self.statuses.push(TxnStatus::InProgress);
        Transaction { xid }
    }

    /// トランザクションの状態．
    pub fn status(&self, xid: TxnId) -> TxnStatus {
        self.statuses[usize::try_from(xid.0).expect("u32 fits in usize")]
    }

    /// `xid`のトランザクションが今の時点で使うスナップショットを作る．
    pub fn snapshot(&self, xid: TxnId) -> Snapshot {
        let active = (1..self.statuses.len())
            .map(|n| TxnId(u32::try_from(n).expect("fewer than 2^32 transactions")))
            .filter(|&other| other != xid && self.status(other) == TxnStatus::InProgress)
            .collect();
        Snapshot {
            xid,
            xmax: TxnId(u32::try_from(self.statuses.len()).expect("fewer than 2^32 transactions")),
            active,
        }
    }

    fn finish(&mut self, xid: TxnId, status: TxnStatus) {
        self.statuses[usize::try_from(xid.0).expect("u32 fits in usize")] = status;
    }

    /// 状態をファイルに書く．トランザクションごとに1バイト(0は進行中，1はコミット済み，2は中止)である．
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let bytes: Vec<u8> = self
            .statuses
            .iter()
            .map(|status| match status {
                TxnStatus::InProgress => 0,
                TxnStatus::Committed => 1,
                TxnStatus::Aborted => 2,
            })
            .collect();
        let temporary = path.with_extension("tmp");
        fs::write(&temporary, bytes)?;
        fs::rename(&temporary, path)
    }

    /// ファイルから状態を読む．ファイルがなければ，トランザクションのない状態を返す．
    /// 進行中のまま終わったトランザクションは，中止したものとみなす．
    pub fn load(path: &Path) -> io::Result<TransactionManager> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(TransactionManager::default());
            }
            Err(error) => return Err(error),
        };
        let mut statuses = Vec::new();
        for byte in bytes {
            statuses.push(match byte {
                1 => TxnStatus::Committed,
                0 | 2 => TxnStatus::Aborted,
                _ => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "invalid transaction status file",
                    ));
                }
            });
        }
        if statuses.is_empty() {
            return Ok(TransactionManager::default());
        }
        Ok(TransactionManager { statuses })
    }
}

impl Transaction {
    /// トランザクションの番号．
    pub fn xid(&self) -> TxnId {
        self.xid
    }

    /// コミットする．変更は，これから作るスナップショットで見えるようになる．
    pub fn commit(self, manager: &mut TransactionManager) {
        manager.finish(self.xid, TxnStatus::Committed);
    }

    /// 中止する．変更は，どのスナップショットでも見えなくなる．
    pub fn rollback(self, manager: &mut TransactionManager) {
        manager.finish(self.xid, TxnStatus::Aborted);
    }
}

impl Snapshot {
    /// `xid`のトランザクションの変更が，このスナップショットで見えるかを返す．
    /// 自分の変更と，スナップショットを作る前にコミットしたトランザクションの変更が見える．
    fn sees(&self, xid: TxnId, manager: &TransactionManager) -> bool {
        if xid == self.xid {
            return true;
        }
        xid < self.xmax
            && !self.active.contains(&xid)
            && manager.status(xid) == TxnStatus::Committed
    }
}

/// タプルの版がスナップショットから見えるかを返す．作ったトランザクションが見えて，
/// 削除したトランザクションが見えなければ，見える．
pub fn is_visible(header: &TupleHeader, snapshot: &Snapshot, manager: &TransactionManager) -> bool {
    if !snapshot.sees(header.xmin, manager) {
        return false;
    }
    header.xmax == TxnId::INVALID || !snapshot.sees(header.xmax, manager)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(xmin: u32, xmax: u32) -> TupleHeader {
        TupleHeader {
            xmin: TxnId(xmin),
            xmax: TxnId(xmax),
        }
    }

    #[test]
    fn transactions_are_numbered_from_one() {
        let mut manager = TransactionManager::default();
        let first = manager.begin();
        let second = manager.begin();
        assert_eq!((first.xid(), second.xid()), (TxnId(1), TxnId(2)));
        assert_eq!(manager.status(TxnId(1)), TxnStatus::InProgress);
        first.commit(&mut manager);
        second.rollback(&mut manager);
        assert_eq!(manager.status(TxnId(1)), TxnStatus::Committed);
        assert_eq!(manager.status(TxnId(2)), TxnStatus::Aborted);
    }

    #[test]
    fn snapshot_lists_other_transactions_in_progress() {
        let mut manager = TransactionManager::default();
        let first = manager.begin();
        let _second = manager.begin();
        first.commit(&mut manager);
        let third = manager.begin();
        assert_eq!(
            manager.snapshot(third.xid()),
            Snapshot {
                xid: TxnId(3),
                xmax: TxnId(4),
                active: vec![TxnId(2)],
            }
        );
    }

    #[test]
    fn own_changes_are_visible() {
        let mut manager = TransactionManager::default();
        let txn = manager.begin();
        let snapshot = manager.snapshot(txn.xid());
        assert!(is_visible(&header(1, 0), &snapshot, &manager));
        assert!(!is_visible(&header(1, 1), &snapshot, &manager));
    }

    #[test]
    fn committed_changes_are_visible_and_aborted_ones_are_not() {
        let mut manager = TransactionManager::default();
        manager.begin().commit(&mut manager);
        manager.begin().rollback(&mut manager);
        let reader = manager.begin();
        let snapshot = manager.snapshot(reader.xid());
        assert!(is_visible(&header(1, 0), &snapshot, &manager));
        assert!(!is_visible(&header(2, 0), &snapshot, &manager));
        assert!(!is_visible(&header(1, 1), &snapshot, &manager));
        assert!(is_visible(&header(1, 2), &snapshot, &manager));
    }

    #[test]
    fn changes_of_transactions_in_progress_are_not_visible() {
        let mut manager = TransactionManager::default();
        manager.begin().commit(&mut manager);
        let writer = manager.begin();
        let reader = manager.begin();
        let snapshot = manager.snapshot(reader.xid());
        assert!(!is_visible(&header(2, 0), &snapshot, &manager));
        assert!(is_visible(&header(1, 2), &snapshot, &manager));
        writer.commit(&mut manager);
        assert!(!is_visible(&header(2, 0), &snapshot, &manager));
        let later = manager.snapshot(reader.xid());
        assert!(is_visible(&header(2, 0), &later, &manager));
    }

    #[test]
    fn saved_statuses_are_loaded_with_unfinished_ones_aborted() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("xact");
        let mut manager = TransactionManager::default();
        manager.begin().commit(&mut manager);
        let unfinished = manager.begin();
        manager.save(&path).unwrap();
        let loaded = TransactionManager::load(&path).unwrap();
        assert_eq!(loaded.status(TxnId(1)), TxnStatus::Committed);
        assert_eq!(loaded.status(unfinished.xid()), TxnStatus::Aborted);
        assert_eq!(std::fs::read(&path).unwrap(), vec![2, 1, 0]);
    }

    #[test]
    fn missing_status_file_means_no_transactions() {
        let dir = tempfile::tempdir().unwrap();
        let mut manager = TransactionManager::load(&dir.path().join("xact")).unwrap();
        assert_eq!(manager.begin().xid(), TxnId(1));
    }
}
