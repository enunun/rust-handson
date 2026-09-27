//! トランザクションと，タプルの版がどのトランザクションから見えるかの判定．

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;
use std::sync::{Condvar, Mutex};
use std::time::Duration;

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
    /// スナップショットのあとにほかのトランザクションが書き換えてコミットした行を，書き換えようとした．
    SerializationFailure,
    /// ほかのトランザクションの終わりを待つ時間が，上限を超えた．
    LockTimeout,
    /// `SERIALIZABLE`の分離レベルを選んだ．
    SerializableNotSupported,
}

/// トランザクションの分離レベル．スナップショットを取る時点が違う．
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Isolation {
    /// 文ごとに新しいスナップショットを取る．
    ReadCommitted,
    /// トランザクションの最初の文で取ったスナップショットを使い続ける．
    RepeatableRead,
}

/// 始めたトランザクション．`commit`か`rollback`で値を消費して終える．
#[derive(Debug, PartialEq)]
pub struct Transaction {
    xid: TxnId,
    isolation: Isolation,
    /// `RepeatableRead`で，最初の文で取ったスナップショット．
    snapshot: Option<Snapshot>,
}

/// 書き換えようとする版を，ほかのトランザクションが先に書き換えていること．
#[derive(Debug, PartialEq)]
pub enum WriteConflict {
    /// 進行中のトランザクションが書き換えている．その終わりを待つ．
    InProgress(TxnId),
    /// スナップショットのあとにコミットしたトランザクションが書き換えた．
    Committed,
}

/// トランザクションが終わったことを，終わりを待つスレッドに知らせる．
#[derive(Debug, Default)]
pub struct EndSignal {
    lock: Mutex<()>,
    ended: Condvar,
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
    /// `READ COMMITTED`の新しいトランザクションを始める．
    pub fn begin(&mut self) -> Transaction {
        self.begin_with(Isolation::ReadCommitted)
    }

    /// 分離レベルを選んで，新しいトランザクションを始める．
    pub fn begin_with(&mut self, isolation: Isolation) -> Transaction {
        let xid = TxnId(u32::try_from(self.statuses.len()).expect("fewer than 2^32 transactions"));
        self.statuses.push(TxnStatus::InProgress);
        Transaction {
            xid,
            isolation,
            snapshot: None,
        }
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

    /// ログのやり直しで，トランザクションの状態を記録する．知らない番号なら，そこまでの番号を加える．
    pub fn set_status(&mut self, xid: TxnId, status: TxnStatus) {
        let index = usize::try_from(xid.0).expect("u32 fits in usize");
        while self.statuses.len() <= index {
            self.statuses.push(TxnStatus::Aborted);
        }
        self.statuses[index] = status;
    }

    /// 進行中のトランザクションを，すべて中止したものとみなす．ログのやり直しの最後に使う．
    pub fn abort_unfinished(&mut self) {
        for status in &mut self.statuses {
            if *status == TxnStatus::InProgress {
                *status = TxnStatus::Aborted;
            }
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
        let mut file = File::create(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_data()?;
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

    /// トランザクションの分離レベル．
    pub fn isolation(&self) -> Isolation {
        self.isolation
    }

    /// 次の文で使うスナップショット．`ReadCommitted`は文ごとに新しく作り，
    /// `RepeatableRead`は最初の文で作ったものを使い続ける．
    pub fn snapshot(&mut self, manager: &TransactionManager) -> Snapshot {
        match self.isolation {
            Isolation::ReadCommitted => manager.snapshot(self.xid),
            Isolation::RepeatableRead => self
                .snapshot
                .get_or_insert_with(|| manager.snapshot(self.xid))
                .clone(),
        }
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

/// スナップショットから見える版を書き換えられるかを返す．ほかのトランザクションが先に`xmax`を書いていれば，
/// 衝突を返す．中止したトランザクションの`xmax`は，書いていないものとみなす．
pub fn write_conflict(
    header: &TupleHeader,
    snapshot: &Snapshot,
    manager: &TransactionManager,
) -> Option<WriteConflict> {
    if header.xmax == TxnId::INVALID || header.xmax == snapshot.xid {
        return None;
    }
    match manager.status(header.xmax) {
        TxnStatus::Aborted => None,
        TxnStatus::InProgress => Some(WriteConflict::InProgress(header.xmax)),
        TxnStatus::Committed => Some(WriteConflict::Committed),
    }
}

impl EndSignal {
    /// `ended`が真を返すまで待つ．`timeout`を過ぎても真にならなければ偽を返す．
    /// `ended`は，`notify`で起こされるたびに，`Mutex`を持ったまま呼ばれる．
    pub fn wait_until(&self, timeout: Duration, mut ended: impl FnMut() -> bool) -> bool {
        let guard = self.lock.lock().expect(POISONED);
        let (_guard, result) = self
            .ended
            .wait_timeout_while(guard, timeout, |_| !ended())
            .expect(POISONED);
        !result.timed_out()
    }

    /// 待っているスレッドをすべて起こす．トランザクションの状態を変えたあとに呼ぶ．
    /// `Mutex`を取ってから起こすので，待つスレッドが状態を調べてから眠るまでの間に，知らせが失われない．
    pub fn notify(&self) {
        let _guard = self.lock.lock().expect(POISONED);
        self.ended.notify_all();
    }
}

/// `Mutex`を取れないときのメッセージ．ほかのスレッドが`Mutex`を持ったままパニックしていた．
const POISONED: &str = "a thread panicked while holding the latch";

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

    #[test]
    fn repeatable_read_keeps_the_first_snapshot() {
        let mut manager = TransactionManager::default();
        let mut read_committed = manager.begin();
        let mut repeatable_read = manager.begin_with(Isolation::RepeatableRead);
        let first = repeatable_read.snapshot(&manager);
        let before = read_committed.snapshot(&manager);
        manager.begin().commit(&mut manager);
        assert_eq!(repeatable_read.snapshot(&manager), first);
        assert_ne!(read_committed.snapshot(&manager), before);
    }

    #[test]
    fn version_updated_by_another_transaction_is_a_write_conflict() {
        let mut manager = TransactionManager::default();
        let me = manager.begin();
        let other = manager.begin();
        let aborted = manager.begin();
        aborted.rollback(&mut manager);
        let snapshot = manager.snapshot(me.xid());
        assert_eq!(write_conflict(&header(1, 0), &snapshot, &manager), None);
        assert_eq!(write_conflict(&header(1, 3), &snapshot, &manager), None);
        assert_eq!(
            write_conflict(&header(1, 2), &snapshot, &manager),
            Some(WriteConflict::InProgress(TxnId(2)))
        );
        other.commit(&mut manager);
        assert_eq!(
            write_conflict(&header(1, 2), &snapshot, &manager),
            Some(WriteConflict::Committed)
        );
    }

    #[test]
    fn waiting_ends_when_notified_or_times_out() {
        let signal = EndSignal::default();
        let ended = std::sync::Mutex::new(false);
        std::thread::scope(|scope| {
            scope.spawn(|| {
                std::thread::sleep(Duration::from_millis(20));
                *ended.lock().unwrap() = true;
                signal.notify();
            });
            assert!(signal.wait_until(Duration::from_secs(5), || *ended.lock().unwrap()));
        });
        assert!(!signal.wait_until(Duration::from_millis(10), || false));
    }
}
