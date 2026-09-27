//! 表の行を加え，書き換え，消す．行を書き換えるときは古い版を削除済みにして，新しい版を加える．
//! 変更する行が制約を満たすかを，インデックスで検査する．

use std::collections::HashSet;

use crate::catalog::TableSchema;
use crate::error::Error;
use crate::exec::eval::{eval, matches_filter};
use crate::index::{ColumnIndex, check_key};
use crate::plan::binder::BoundExpr;
use crate::storage::heap::{HeapFile, RowId};
use crate::storage::page::{MAX_TUPLE_SIZE, PageError};
use crate::storage::tuple::{TupleHeader, encode_version};
use crate::txn::{
    Snapshot, TransactionError, TransactionManager, TxnId, WriteConflict, is_visible,
    write_conflict,
};
use crate::value::{Row, Value};

/// 行が制約に違反したときのエラー．
#[derive(Debug, PartialEq)]
pub enum ConstraintError {
    NotNull { table: String, column: String },
    Unique { constraint: String },
}

/// 行`new_rows`を，スナップショットのトランザクションが作った版として表に加え，インデックスにも加える．
/// 加えた行の数を返す．制約に違反するか，ページやインデックスに入らない行があれば，1行も加えない．
pub fn insert(
    schema: &TableSchema,
    heap: &mut HeapFile,
    indexes: &mut [ColumnIndex<'_>],
    snapshot: &Snapshot,
    manager: &TransactionManager,
    new_rows: Vec<Row>,
) -> Result<usize, Error> {
    check_not_null(schema, &new_rows)?;
    let versions = Versions { snapshot, manager };
    check_unique(schema, heap, indexes, &versions, &new_rows, &HashSet::new())?;
    let tuples = encode_all(schema, indexes, snapshot, &new_rows)?;
    for (row, tuple) in new_rows.iter().zip(&tuples) {
        let id = heap.insert(tuple)?;
        for index in indexes.iter_mut() {
            index.index.insert(&row[index.column], id)?;
        }
    }
    Ok(tuples.len())
}

/// スナップショットから見える行のうち，条件を満たす行の列に式の値を代入する．
/// 古い版を削除済みにして，新しい版を加える．書き換えた行の数を返す．
/// 途中でエラーになるか，制約に違反すれば，1行も書き換えない．
/// 書き換える行を進行中のほかのトランザクションが書き換えていれば，何も変えずに`WaitFor`を返す．
pub fn update(
    schema: &TableSchema,
    heap: &mut HeapFile,
    indexes: &mut [ColumnIndex<'_>],
    snapshot: &Snapshot,
    manager: &TransactionManager,
    assignments: &[(usize, BoundExpr)],
    filter: &Option<BoundExpr>,
) -> Result<Outcome, Error> {
    let mut targets = Vec::new();
    for (id, row) in heap.rows(schema, |header| is_visible(header, snapshot, manager))? {
        if let Some(new_row) = updated_row(schema, &row, assignments, filter)? {
            targets.push((id, new_row));
        }
    }
    if let Some(xid) = first_conflict(heap, targets.iter().map(|(id, _)| *id), snapshot, manager)? {
        return Ok(Outcome::WaitFor(xid));
    }
    let new_rows: Vec<Row> = targets.iter().map(|(_, new)| new.clone()).collect();
    let replaced: HashSet<RowId> = targets.iter().map(|(id, _)| *id).collect();
    check_not_null(schema, &new_rows)?;
    let versions = Versions { snapshot, manager };
    check_unique(schema, heap, indexes, &versions, &new_rows, &replaced)?;
    let tuples = encode_all(schema, indexes, snapshot, &new_rows)?;
    for ((id, new_row), tuple) in targets.iter().zip(&tuples) {
        heap.set_xmax(*id, snapshot.xid)?;
        let new_id = heap.insert(tuple)?;
        for index in indexes.iter_mut() {
            index.index.insert(&new_row[index.column], new_id)?;
        }
    }
    Ok(Outcome::Done(targets.len()))
}

/// スナップショットから見える行のうち，条件を満たす行の版を削除済みにする．消した行の数を返す．
/// 途中でエラーになれば，1行も消さない．インデックスの項目は残す．
/// 消す行を進行中のほかのトランザクションが書き換えていれば，何も消さずに`WaitFor`を返す．
pub fn delete(
    schema: &TableSchema,
    heap: &mut HeapFile,
    snapshot: &Snapshot,
    manager: &TransactionManager,
    filter: &Option<BoundExpr>,
) -> Result<Outcome, Error> {
    let mut targets = Vec::new();
    for (id, row) in heap.rows(schema, |header| is_visible(header, snapshot, manager))? {
        if matches_filter(filter, &row)? {
            targets.push(id);
        }
    }
    if let Some(xid) = first_conflict(heap, targets.iter().copied(), snapshot, manager)? {
        return Ok(Outcome::WaitFor(xid));
    }
    for id in &targets {
        heap.set_xmax(*id, snapshot.xid)?;
    }
    Ok(Outcome::Done(targets.len()))
}

/// 版を書き換える文の結果．
#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// 書き換えた行の数．
    Done(usize),
    /// 書き換える行を進行中のトランザクションが書き換えていたので，何も変えなかった．
    /// そのトランザクションの終わりを待ってから，文をやり直す．
    WaitFor(TxnId),
}

/// 書き換える版`ids`のうち，ほかのトランザクションが先に書き換えた最初の版を調べる．
/// 進行中のトランザクションならその番号を返し，スナップショットのあとにコミットしたトランザクションなら，
/// `SerializationFailure`のエラーを返す．
fn first_conflict(
    heap: &HeapFile,
    ids: impl Iterator<Item = RowId>,
    snapshot: &Snapshot,
    manager: &TransactionManager,
) -> Result<Option<TxnId>, Error> {
    for id in ids {
        let tuple = heap.get(id)?.expect("a visible version is in the heap");
        let (header, _) = TupleHeader::split(&tuple)?;
        match write_conflict(&header, snapshot, manager) {
            None => {}
            Some(WriteConflict::InProgress(xid)) => return Ok(Some(xid)),
            Some(WriteConflict::Committed) => {
                return Err(TransactionError::SerializationFailure.into());
            }
        }
    }
    Ok(None)
}

/// 版が見えるかを判定するためのスナップショットとトランザクションの状態．
struct Versions<'a> {
    snapshot: &'a Snapshot,
    manager: &'a TransactionManager,
}

impl Versions<'_> {
    /// 位置の版が見えるかを返す．
    fn sees(&self, heap: &HeapFile, id: RowId) -> Result<bool, Error> {
        let Some(tuple) = heap.get(id)? else {
            return Ok(false);
        };
        let (header, _) = TupleHeader::split(&tuple)?;
        Ok(is_visible(&header, self.snapshot, self.manager))
    }
}

/// 行を，スナップショットのトランザクションが作った版のタプルにする．
/// タプルがページに入らないか，インデックスのキーが大きすぎればエラーを返す．
fn encode_all(
    schema: &TableSchema,
    indexes: &[ColumnIndex<'_>],
    snapshot: &Snapshot,
    rows: &[Row],
) -> Result<Vec<Vec<u8>>, Error> {
    let header = TupleHeader::new(snapshot.xid);
    let mut tuples = Vec::new();
    for row in rows {
        let tuple = encode_version(&header, row, schema);
        if tuple.len() > MAX_TUPLE_SIZE {
            return Err(PageError::TupleTooLarge { size: tuple.len() }.into());
        }
        for index in indexes {
            check_key(&row[index.column])?;
        }
        tuples.push(tuple);
    }
    Ok(tuples)
}

/// 行が条件を満たせば，代入したあとの新しい行を返す．満たさなければ`None`を返す．
/// 代入の式は，すべて書き換える前の行で評価する．
fn updated_row(
    schema: &TableSchema,
    row: &[Value],
    assignments: &[(usize, BoundExpr)],
    filter: &Option<BoundExpr>,
) -> Result<Option<Row>, Error> {
    if !matches_filter(filter, row)? {
        return Ok(None);
    }
    let mut new_row = row.to_vec();
    for (index, expr) in assignments {
        new_row[*index] = schema.columns[*index].assign(eval(expr, row)?)?;
    }
    Ok(Some(new_row))
}

/// 加える行や書き換えたあとの行が，`NOT NULL`を満たすかを調べる．
fn check_not_null(schema: &TableSchema, rows: &[Row]) -> Result<(), ConstraintError> {
    for (index, column) in schema.columns.iter().enumerate() {
        if !column.nullable && rows.iter().any(|row| row[index].is_null()) {
            return Err(ConstraintError::NotNull {
                table: schema.name.clone(),
                column: column.name.clone(),
            });
        }
    }
    Ok(())
}

/// 加える行や書き換えたあとの行`rows`が，一意性制約を満たすかをインデックスで調べる．
/// `rows`の中で値が重なるか，`replaced`(書き換える前の版)でない見える版に同じ値があれば違反である．
fn check_unique(
    schema: &TableSchema,
    heap: &HeapFile,
    indexes: &[ColumnIndex<'_>],
    versions: &Versions<'_>,
    rows: &[Row],
    replaced: &HashSet<RowId>,
) -> Result<(), Error> {
    for constraint in &schema.unique_constraints {
        let index = indexes
            .iter()
            .find(|index| index.column == constraint.column)
            .expect("every unique constraint has an index");
        let violation = ConstraintError::Unique {
            constraint: constraint.name.clone(),
        };
        let mut seen = HashSet::new();
        for row in rows {
            let value = &row[constraint.column];
            if value.is_null() {
                continue;
            }
            if !seen.insert(value) {
                return Err(violation.into());
            }
            for id in index.index.lookup(value)? {
                if !replaced.contains(&id) && versions.sees(heap, id)? {
                    return Err(violation.into());
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{Column, UniqueConstraint};
    use crate::error::SqlState;
    use crate::index::AnyIndex;
    use crate::sql::ast::{ArithmeticOp, BinaryOp, ComparisonOp};
    use crate::storage::buffer::BufferPool;
    use crate::storage::disk::{DiskManager, MemoryDiskManager};
    use crate::storage::tuple::decode_tuple;
    use crate::txn::TxnId;
    use crate::value::DataType;

    fn users() -> TableSchema {
        TableSchema {
            name: "USERS".to_string(),
            columns: vec![
                Column {
                    name: "ID".to_string(),
                    data_type: DataType::Integer,
                    nullable: false,
                },
                Column {
                    name: "NAME".to_string(),
                    data_type: DataType::Varchar(10),
                    nullable: false,
                },
                Column {
                    name: "EMAIL".to_string(),
                    data_type: DataType::Varchar(10),
                    nullable: true,
                },
            ],
            unique_constraints: vec![
                UniqueConstraint {
                    name: "USERS_PKEY".to_string(),
                    column: 0,
                },
                UniqueConstraint {
                    name: "USERS_EMAIL_KEY".to_string(),
                    column: 2,
                },
            ],
        }
    }

    fn row(id: i32, name: &str, email: Option<&str>) -> Row {
        let email = match email {
            Some(email) => Value::Varchar(email.to_string()),
            None => Value::Null,
        };
        vec![Value::Integer(id), Value::Varchar(name.to_string()), email]
    }

    fn initial_rows() -> Vec<Row> {
        vec![
            row(1, "alice", Some("a@x")),
            row(2, "bob", None),
            row(3, "carol", None),
        ]
    }

    type Pool = BufferPool<Box<dyn DiskManager>>;

    /// テストの表．ページの列，`ID`と`EMAIL`の列のインデックスのプール，トランザクションの状態を持つ．
    struct Table {
        heap: HeapFile,
        pools: Vec<(usize, Pool)>,
        manager: TransactionManager,
    }

    /// `rows`を，1つのトランザクションで加えてコミットした表．
    fn table_of(rows: &[Row]) -> Table {
        let pools: Vec<(usize, Pool)> = [0, 2]
            .into_iter()
            .map(|column| {
                let disk: Box<dyn DiskManager> = Box::new(MemoryDiskManager::default());
                (column, BufferPool::new(disk, 8))
            })
            .collect();
        for (column, pool) in &pools {
            AnyIndex::create(&users().columns[*column].data_type, pool).unwrap();
        }
        let mut table = Table {
            heap: HeapFile::new(Box::new(MemoryDiskManager::default())),
            pools,
            manager: TransactionManager::default(),
        };
        table
            .run(|heap, indexes, snapshot, manager| {
                insert(&users(), heap, indexes, snapshot, manager, rows.to_vec())
            })
            .unwrap();
        table
    }

    fn indexes(pools: &[(usize, Pool)]) -> Vec<ColumnIndex<'_>> {
        pools
            .iter()
            .map(|(column, pool)| ColumnIndex {
                column: *column,
                index: AnyIndex::open(&users().columns[*column].data_type, pool).unwrap(),
            })
            .collect()
    }

    /// 書き換えた行の数．ほかのトランザクションを待つ結果なら，パニックする．
    fn done(outcome: Outcome) -> usize {
        match outcome {
            Outcome::Done(count) => count,
            Outcome::WaitFor(xid) => panic!("unexpected wait for {xid:?}"),
        }
    }

    impl Table {
        /// 新しいトランザクションで`operation`を実行し，成功すればコミット，失敗すれば中止する．
        fn run(
            &mut self,
            operation: impl FnOnce(
                &mut HeapFile,
                &mut [ColumnIndex<'_>],
                &Snapshot,
                &TransactionManager,
            ) -> Result<usize, Error>,
        ) -> Result<usize, Error> {
            let txn = self.manager.begin();
            let snapshot = self.manager.snapshot(txn.xid());
            let result = {
                let mut indexes = indexes(&self.pools);
                operation(&mut self.heap, &mut indexes, &snapshot, &self.manager)
            };
            match result {
                Ok(_) => txn.commit(&mut self.manager),
                Err(_) => txn.rollback(&mut self.manager),
            }
            result
        }

        fn insert(&mut self, rows: Vec<Row>) -> Result<usize, Error> {
            self.run(|heap, indexes, snapshot, manager| {
                insert(&users(), heap, indexes, snapshot, manager, rows)
            })
        }

        fn update(
            &mut self,
            assignments: &[(usize, BoundExpr)],
            filter: &Option<BoundExpr>,
        ) -> Result<usize, Error> {
            self.run(|heap, indexes, snapshot, manager| {
                update(
                    &users(),
                    heap,
                    indexes,
                    snapshot,
                    manager,
                    assignments,
                    filter,
                )
                .map(done)
            })
        }

        fn delete(&mut self, filter: &Option<BoundExpr>) -> Result<usize, Error> {
            self.run(|heap, _, snapshot, manager| {
                delete(&users(), heap, snapshot, manager, filter).map(done)
            })
        }

        /// コミット済みのトランザクションの変更だけが見えるスナップショット．
        fn reader(&self) -> Snapshot {
            self.manager.snapshot(TxnId::INVALID)
        }

        /// 見える行を，置いた順に返す．
        fn rows(&self) -> Vec<Row> {
            let snapshot = self.reader();
            self.heap
                .rows(&users(), |header| {
                    is_visible(header, &snapshot, &self.manager)
                })
                .unwrap()
                .into_iter()
                .map(|(_, row)| row)
                .collect()
        }

        /// インデックスで値を引き，見える版の行を返す．
        fn lookup(&self, column: usize, value: Value) -> Vec<Row> {
            let snapshot = self.reader();
            let indexes = indexes(&self.pools);
            let index = indexes.iter().find(|i| i.column == column).unwrap();
            let mut rows = Vec::new();
            for id in index.index.lookup(&value).unwrap() {
                let tuple = self.heap.get(id).unwrap().unwrap();
                let (header, data) = TupleHeader::split(&tuple).unwrap();
                if is_visible(&header, &snapshot, &self.manager) {
                    rows.push(decode_tuple(data, &users()).unwrap());
                }
            }
            rows
        }
    }

    fn column(index: usize) -> BoundExpr {
        BoundExpr::Column(index)
    }

    fn constant(value: Value) -> BoundExpr {
        BoundExpr::Constant(value)
    }

    fn binary(op: BinaryOp, left: BoundExpr, right: BoundExpr) -> BoundExpr {
        BoundExpr::Binary {
            op,
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    /// `ID = n`の条件．
    fn id_is(n: i32) -> Option<BoundExpr> {
        Some(binary(
            BinaryOp::Comparison(ComparisonOp::Eq),
            column(0),
            constant(Value::Integer(n)),
        ))
    }

    /// `10 / (ID - 2) > 0`の条件．`ID`が2の行で0で割る．
    fn divides_by_zero_at_id_2() -> Option<BoundExpr> {
        let id_minus_2 = binary(
            BinaryOp::Arithmetic(ArithmeticOp::Sub),
            column(0),
            constant(Value::Integer(2)),
        );
        Some(binary(
            BinaryOp::Comparison(ComparisonOp::Gt),
            binary(
                BinaryOp::Arithmetic(ArithmeticOp::Div),
                constant(Value::Integer(10)),
                id_minus_2,
            ),
            constant(Value::Integer(0)),
        ))
    }

    #[test]
    fn null_in_a_not_null_column_violates_the_constraint() {
        let mut rows = initial_rows();
        rows[1][1] = Value::Null;
        assert_eq!(
            check_not_null(&users(), &rows),
            Err(ConstraintError::NotNull {
                table: "USERS".to_string(),
                column: "NAME".to_string()
            })
        );
    }

    #[test]
    fn value_in_the_index_violates_the_unique_constraint() {
        let mut table = table_of(&initial_rows());
        let error = table.insert(vec![row(9, "dave", Some("a@x"))]).unwrap_err();
        assert_eq!(error.sqlstate(), SqlState::UniqueViolation);
        assert_eq!(
            error.message(),
            "duplicate key value violates unique constraint \"USERS_EMAIL_KEY\""
        );
    }

    #[test]
    fn same_value_twice_among_new_rows_violates_the_unique_constraint() {
        let mut table = table_of(&initial_rows());
        let error = table
            .insert(vec![row(4, "dave", None), row(4, "eve", None)])
            .unwrap_err();
        assert_eq!(error.sqlstate(), SqlState::UniqueViolation);
    }

    #[test]
    fn unique_column_may_have_many_nulls() {
        let mut table = table_of(&initial_rows());
        let new_rows = vec![row(4, "dave", None), row(5, "eve", None)];
        assert_eq!(table.insert(new_rows), Ok(2));
    }

    #[test]
    fn insert_adds_rows_to_the_table_and_its_indexes() {
        let mut table = table_of(&initial_rows());
        let new_rows = vec![row(4, "dave", Some("d@x")), row(5, "eve", None)];
        assert_eq!(table.insert(new_rows), Ok(2));
        assert_eq!(table.rows().len(), 5);
        assert_eq!(table.rows()[4], row(5, "eve", None));
        assert_eq!(
            table.lookup(2, Value::Varchar("d@x".to_string())),
            vec![row(4, "dave", Some("d@x"))]
        );
    }

    #[test]
    fn new_version_records_the_transaction_that_made_it() {
        let table = table_of(&initial_rows());
        let tuples = table.heap.tuples().unwrap();
        let (header, _) = TupleHeader::split(&tuples[0].1).unwrap();
        assert_eq!(header, TupleHeader::new(TxnId(1)));
    }

    #[test]
    fn insert_violating_a_constraint_adds_no_rows() {
        let mut table = table_of(&initial_rows());
        let new_rows = vec![row(4, "dave", None), row(1, "eve", None)];
        let error = table.insert(new_rows).unwrap_err();
        assert_eq!(error.sqlstate(), SqlState::UniqueViolation);
        assert_eq!(table.rows(), initial_rows());
        assert_eq!(table.lookup(0, Value::Integer(4)), Vec::<Row>::new());
    }

    #[test]
    fn update_assigns_to_matching_rows_and_returns_their_count() {
        let mut table = table_of(&initial_rows());
        let assignments = vec![(1, constant(Value::Varchar("zed".to_string())))];
        assert_eq!(table.update(&assignments, &id_is(2)), Ok(1));
        assert_eq!(
            table.rows(),
            vec![
                row(1, "alice", Some("a@x")),
                row(3, "carol", None),
                row(2, "zed", None),
            ]
        );
    }

    #[test]
    fn update_keeps_the_old_version_as_deleted() {
        let mut table = table_of(&initial_rows());
        let assignments = vec![(1, constant(Value::Varchar("zed".to_string())))];
        table.update(&assignments, &id_is(2)).unwrap();
        let tuples = table.heap.tuples().unwrap();
        assert_eq!(tuples.len(), 4);
        let (old, _) = TupleHeader::split(&tuples[1].1).unwrap();
        let (new, _) = TupleHeader::split(&tuples[3].1).unwrap();
        assert_eq!(
            (old.xmin, old.xmax, new.xmin, new.xmax),
            (TxnId(1), TxnId(2), TxnId(2), TxnId::INVALID)
        );
    }

    #[test]
    fn index_finds_only_the_new_version_after_an_update() {
        let mut table = table_of(&initial_rows());
        let assignments = vec![(0, constant(Value::Integer(20)))];
        table.update(&assignments, &id_is(2)).unwrap();
        assert_eq!(table.lookup(0, Value::Integer(2)), Vec::<Row>::new());
        assert_eq!(
            table.lookup(0, Value::Integer(20)),
            vec![row(20, "bob", None)]
        );
    }

    #[test]
    fn update_evaluates_assignments_on_the_row_before_the_update() {
        let mut table = table_of(&[row(1, "a", Some("b"))]);
        let assignments = vec![(1, column(2)), (2, column(1))];
        table.update(&assignments, &None).unwrap();
        assert_eq!(table.rows(), vec![row(1, "b", Some("a"))]);
    }

    #[test]
    fn update_violating_a_constraint_changes_no_rows() {
        let mut table = table_of(&initial_rows());
        let assignments = vec![(0, constant(Value::Integer(1)))];
        let error = table.update(&assignments, &None).unwrap_err();
        assert_eq!(error.sqlstate(), SqlState::UniqueViolation);
        assert_eq!(table.rows(), initial_rows());
    }

    #[test]
    fn unique_values_are_checked_after_all_rows_are_updated() {
        let mut table = table_of(&initial_rows());
        let id_plus_1 = binary(
            BinaryOp::Arithmetic(ArithmeticOp::Add),
            column(0),
            constant(Value::Integer(1)),
        );
        assert_eq!(table.update(&[(0, id_plus_1)], &None), Ok(3));
        assert_eq!(table.rows()[0][0], Value::Integer(2));
        assert_eq!(
            table.lookup(0, Value::Integer(4)),
            vec![row(4, "carol", None)]
        );
    }

    #[test]
    fn delete_makes_matching_rows_invisible() {
        let mut table = table_of(&initial_rows());
        assert_eq!(table.delete(&id_is(2)), Ok(1));
        assert_eq!(
            table.rows(),
            vec![row(1, "alice", Some("a@x")), row(3, "carol", None)]
        );
        assert_eq!(table.lookup(0, Value::Integer(2)), Vec::<Row>::new());
        assert_eq!(table.heap.tuples().unwrap().len(), 3);
    }

    #[test]
    fn deleted_value_can_be_inserted_again() {
        let mut table = table_of(&initial_rows());
        table.delete(&id_is(1)).unwrap();
        assert_eq!(table.insert(vec![row(1, "zoe", Some("a@x"))]), Ok(1));
        assert_eq!(
            table.lookup(0, Value::Integer(1)),
            vec![row(1, "zoe", Some("a@x"))]
        );
    }

    #[test]
    fn rows_of_an_aborted_transaction_are_invisible() {
        let mut table = table_of(&initial_rows());
        let txn = table.manager.begin();
        let snapshot = table.manager.snapshot(txn.xid());
        {
            let mut indexes = indexes(&table.pools);
            insert(
                &users(),
                &mut table.heap,
                &mut indexes,
                &snapshot,
                &table.manager,
                vec![row(4, "dave", None)],
            )
            .unwrap();
        }
        txn.rollback(&mut table.manager);
        assert_eq!(table.rows(), initial_rows());
        assert_eq!(table.insert(vec![row(4, "eve", None)]), Ok(1));
    }

    #[test]
    fn error_in_the_condition_leaves_the_table_unchanged() {
        let mut table = table_of(&initial_rows());
        let assignments = vec![(1, constant(Value::Varchar("zed".to_string())))];
        let filter = divides_by_zero_at_id_2();
        let error = table.update(&assignments, &filter).unwrap_err();
        assert_eq!(error.sqlstate(), SqlState::DivisionByZero);
        let error = table.delete(&filter).unwrap_err();
        assert_eq!(error.sqlstate(), SqlState::DivisionByZero);
        assert_eq!(table.rows(), initial_rows());
    }

    #[test]
    fn row_changed_by_a_transaction_in_progress_is_left_for_later() {
        let mut table = table_of(&initial_rows());
        let other = table.manager.begin();
        let other_snapshot = table.manager.snapshot(other.xid());
        delete(
            &users(),
            &mut table.heap,
            &other_snapshot,
            &table.manager,
            &id_is(2),
        )
        .unwrap();
        let me = table.manager.begin();
        let snapshot = table.manager.snapshot(me.xid());
        let assignments = vec![(1, constant(Value::Varchar("zed".to_string())))];
        let mut indexes = indexes(&table.pools);
        let outcome = update(
            &users(),
            &mut table.heap,
            &mut indexes,
            &snapshot,
            &table.manager,
            &assignments,
            &id_is(2),
        );
        assert_eq!(outcome, Ok(Outcome::WaitFor(other.xid())));
        drop(indexes);
        other.commit(&mut table.manager);
        let error = delete(
            &users(),
            &mut table.heap,
            &snapshot,
            &table.manager,
            &id_is(2),
        );
        assert_eq!(
            error.unwrap_err().sqlstate(),
            SqlState::SerializationFailure
        );
    }
}
