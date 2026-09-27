//! 表の行を加え，書き換え，消す．変更する行が制約を満たすかを，インデックスで検査する．

use std::collections::HashSet;

use crate::catalog::TableSchema;
use crate::error::Error;
use crate::exec::eval::{eval, matches_filter};
use crate::index::{ColumnIndex, check_key};
use crate::plan::binder::BoundExpr;
use crate::storage::heap::{HeapFile, RowId};
use crate::storage::page::{MAX_TUPLE_SIZE, PageError};
use crate::storage::tuple::encode_tuple;
use crate::value::{Row, Value};

/// 行が制約に違反したときのエラー．
#[derive(Debug, PartialEq)]
pub enum ConstraintError {
    NotNull { table: String, column: String },
    Unique { constraint: String },
}

/// 行`new_rows`を表に加え，表のインデックスにも加える．加えた行の数を返す．
/// 制約に違反するか，ページやインデックスに入らない行があれば，1行も加えない．
pub fn insert(
    schema: &TableSchema,
    heap: &mut HeapFile,
    indexes: &mut [ColumnIndex<'_>],
    new_rows: Vec<Row>,
) -> Result<usize, Error> {
    check_not_null(schema, &new_rows)?;
    check_unique(schema, indexes, &new_rows, &HashSet::new())?;
    let tuples = encode_all(schema, indexes, &new_rows)?;
    for (row, tuple) in new_rows.iter().zip(&tuples) {
        let id = heap.insert(tuple)?;
        for index in indexes.iter_mut() {
            index.index.insert(&row[index.column], id)?;
        }
    }
    Ok(tuples.len())
}

/// 条件を満たす行の列に，式の値を代入する．`assignments`は，列の番号と式の組である．
/// 書き換えた行の数を返す．途中でエラーになるか，制約に違反すれば，1行も書き換えない．
pub fn update(
    schema: &TableSchema,
    heap: &mut HeapFile,
    indexes: &mut [ColumnIndex<'_>],
    assignments: &[(usize, BoundExpr)],
    filter: &Option<BoundExpr>,
) -> Result<usize, Error> {
    let mut targets = Vec::new();
    for (id, row) in heap.rows(schema)? {
        if let Some(new_row) = updated_row(schema, &row, assignments, filter)? {
            targets.push((id, row, new_row));
        }
    }
    let new_rows: Vec<Row> = targets.iter().map(|(_, _, new)| new.clone()).collect();
    let replaced: HashSet<RowId> = targets.iter().map(|(id, _, _)| *id).collect();
    check_not_null(schema, &new_rows)?;
    check_unique(schema, indexes, &new_rows, &replaced)?;
    let tuples = encode_all(schema, indexes, &new_rows)?;
    for ((id, old_row, new_row), tuple) in targets.iter().zip(&tuples) {
        let new_id = heap.update(*id, tuple)?;
        for index in indexes.iter_mut() {
            index.index.delete(&old_row[index.column], *id)?;
            index.index.insert(&new_row[index.column], new_id)?;
        }
    }
    Ok(targets.len())
}

/// 条件を満たす行を，表とインデックスから消す．消した行の数を返す．
/// 途中でエラーになれば，1行も消さない．
pub fn delete(
    schema: &TableSchema,
    heap: &mut HeapFile,
    indexes: &mut [ColumnIndex<'_>],
    filter: &Option<BoundExpr>,
) -> Result<usize, Error> {
    let mut targets = Vec::new();
    for (id, row) in heap.rows(schema)? {
        if matches_filter(filter, &row)? {
            targets.push((id, row));
        }
    }
    for (id, row) in &targets {
        heap.delete(*id)?;
        for index in indexes.iter_mut() {
            index.index.delete(&row[index.column], *id)?;
        }
    }
    Ok(targets.len())
}

/// 行をタプルにする．タプルがページに入らないか，インデックスのキーが大きすぎればエラーを返す．
fn encode_all(
    schema: &TableSchema,
    indexes: &[ColumnIndex<'_>],
    rows: &[Row],
) -> Result<Vec<Vec<u8>>, Error> {
    let mut tuples = Vec::new();
    for row in rows {
        let tuple = encode_tuple(row, schema);
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
/// `rows`の中で値が重なるか，`replaced`(書き換える前の行)でない行に同じ値があれば違反である．
fn check_unique(
    schema: &TableSchema,
    indexes: &[ColumnIndex<'_>],
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
            let others = index.index.lookup(value)?;
            if others.iter().any(|id| !replaced.contains(id)) {
                return Err(violation.into());
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
    use crate::value::DataType;

    /// `ID INTEGER PRIMARY KEY, NAME VARCHAR(10) NOT NULL, EMAIL VARCHAR(10) UNIQUE`の表．
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

    fn table() -> Vec<Row> {
        vec![
            row(1, "alice", Some("a@x")),
            row(2, "bob", None),
            row(3, "carol", None),
        ]
    }

    type Pool = BufferPool<Box<dyn DiskManager>>;

    /// 行をタプルにして置いたページの列と，`ID`と`EMAIL`の列のインデックスのプール．
    fn table_of(rows: &[Row]) -> (HeapFile, Vec<(usize, Pool)>) {
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
        let mut heap = HeapFile::new(Box::new(MemoryDiskManager::default()));
        let mut indexes = indexes(&pools);
        for row in rows {
            let id = heap.insert(&encode_tuple(row, &users())).unwrap();
            for index in &mut indexes {
                index.index.insert(&row[index.column], id).unwrap();
            }
        }
        drop(indexes);
        (heap, pools)
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

    /// ページの列の行を，置いた順に返す．
    fn rows_of(heap: &HeapFile) -> Vec<Row> {
        heap.rows(&users())
            .unwrap()
            .into_iter()
            .map(|(_, row)| row)
            .collect()
    }

    /// インデックスで値を引き，その位置の行を返す．
    fn lookup(heap: &HeapFile, pools: &[(usize, Pool)], column: usize, value: Value) -> Vec<Row> {
        let indexes = indexes(pools);
        let index = indexes.iter().find(|i| i.column == column).unwrap();
        index
            .index
            .lookup(&value)
            .unwrap()
            .into_iter()
            .map(|id| decode_tuple(&heap.get(id).unwrap().unwrap(), &users()).unwrap())
            .collect()
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
        let mut rows = table();
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
        let (mut heap, pools) = table_of(&table());
        let new_rows = vec![row(9, "dave", Some("a@x"))];
        let error = insert(&users(), &mut heap, &mut indexes(&pools), new_rows).unwrap_err();
        assert_eq!(error.sqlstate(), SqlState::UniqueViolation);
        assert_eq!(
            error.message(),
            "duplicate key value violates unique constraint \"USERS_EMAIL_KEY\""
        );
    }

    #[test]
    fn same_value_twice_among_new_rows_violates_the_unique_constraint() {
        let (mut heap, pools) = table_of(&table());
        let new_rows = vec![row(4, "dave", None), row(4, "eve", None)];
        let error = insert(&users(), &mut heap, &mut indexes(&pools), new_rows).unwrap_err();
        assert_eq!(error.sqlstate(), SqlState::UniqueViolation);
    }

    #[test]
    fn unique_column_may_have_many_nulls() {
        let (mut heap, pools) = table_of(&table());
        let new_rows = vec![row(4, "dave", None), row(5, "eve", None)];
        assert_eq!(
            insert(&users(), &mut heap, &mut indexes(&pools), new_rows),
            Ok(2)
        );
    }

    #[test]
    fn insert_adds_rows_to_the_table_and_its_indexes() {
        let (mut heap, pools) = table_of(&table());
        let new_rows = vec![row(4, "dave", Some("d@x")), row(5, "eve", None)];
        assert_eq!(
            insert(&users(), &mut heap, &mut indexes(&pools), new_rows),
            Ok(2)
        );
        assert_eq!(rows_of(&heap).len(), 5);
        assert_eq!(rows_of(&heap)[4], row(5, "eve", None));
        assert_eq!(
            lookup(&heap, &pools, 2, Value::Varchar("d@x".to_string())),
            vec![row(4, "dave", Some("d@x"))]
        );
    }

    #[test]
    fn insert_violating_a_constraint_adds_no_rows() {
        let (mut heap, pools) = table_of(&table());
        let new_rows = vec![row(4, "dave", None), row(1, "eve", None)];
        let error = insert(&users(), &mut heap, &mut indexes(&pools), new_rows).unwrap_err();
        assert_eq!(error.sqlstate(), SqlState::UniqueViolation);
        assert_eq!(rows_of(&heap), table());
        assert_eq!(
            lookup(&heap, &pools, 0, Value::Integer(4)),
            Vec::<Row>::new()
        );
    }

    #[test]
    fn update_assigns_to_matching_rows_and_returns_their_count() {
        let (mut heap, pools) = table_of(&table());
        let assignments = vec![(1, constant(Value::Varchar("zed".to_string())))];
        assert_eq!(
            update(
                &users(),
                &mut heap,
                &mut indexes(&pools),
                &assignments,
                &id_is(2)
            ),
            Ok(1)
        );
        assert_eq!(rows_of(&heap)[1], row(2, "zed", None));
        assert_eq!(rows_of(&heap)[0], row(1, "alice", Some("a@x")));
    }

    #[test]
    fn update_moves_index_entries_to_the_new_values() {
        let (mut heap, pools) = table_of(&table());
        let assignments = vec![(0, constant(Value::Integer(20)))];
        update(
            &users(),
            &mut heap,
            &mut indexes(&pools),
            &assignments,
            &id_is(2),
        )
        .unwrap();
        assert_eq!(
            lookup(&heap, &pools, 0, Value::Integer(2)),
            Vec::<Row>::new()
        );
        assert_eq!(
            lookup(&heap, &pools, 0, Value::Integer(20)),
            vec![row(20, "bob", None)]
        );
    }

    #[test]
    fn update_evaluates_assignments_on_the_row_before_the_update() {
        let (mut heap, pools) = table_of(&[row(1, "a", Some("b"))]);
        let assignments = vec![(1, column(2)), (2, column(1))];
        update(
            &users(),
            &mut heap,
            &mut indexes(&pools),
            &assignments,
            &None,
        )
        .unwrap();
        assert_eq!(rows_of(&heap), vec![row(1, "b", Some("a"))]);
    }

    #[test]
    fn update_violating_a_constraint_changes_no_rows() {
        let (mut heap, pools) = table_of(&table());
        let assignments = vec![(0, constant(Value::Integer(1)))];
        let error = update(
            &users(),
            &mut heap,
            &mut indexes(&pools),
            &assignments,
            &None,
        )
        .unwrap_err();
        assert_eq!(error.sqlstate(), SqlState::UniqueViolation);
        assert_eq!(rows_of(&heap), table());
    }

    #[test]
    fn unique_values_are_checked_after_all_rows_are_updated() {
        let (mut heap, pools) = table_of(&table());
        let id_plus_1 = binary(
            BinaryOp::Arithmetic(ArithmeticOp::Add),
            column(0),
            constant(Value::Integer(1)),
        );
        assert_eq!(
            update(
                &users(),
                &mut heap,
                &mut indexes(&pools),
                &[(0, id_plus_1)],
                &None
            ),
            Ok(3)
        );
        assert_eq!(rows_of(&heap)[0][0], Value::Integer(2));
        assert_eq!(
            lookup(&heap, &pools, 0, Value::Integer(4)),
            vec![row(4, "carol", None)]
        );
    }

    #[test]
    fn delete_removes_matching_rows_and_their_index_entries() {
        let (mut heap, pools) = table_of(&table());
        assert_eq!(
            delete(&users(), &mut heap, &mut indexes(&pools), &id_is(2)),
            Ok(1)
        );
        assert_eq!(
            rows_of(&heap),
            vec![row(1, "alice", Some("a@x")), row(3, "carol", None)]
        );
        assert_eq!(
            lookup(&heap, &pools, 0, Value::Integer(2)),
            Vec::<Row>::new()
        );
    }

    #[test]
    fn error_in_the_condition_leaves_the_table_unchanged() {
        let (mut heap, pools) = table_of(&table());
        let assignments = vec![(1, constant(Value::Varchar("zed".to_string())))];
        let filter = divides_by_zero_at_id_2();
        let error = update(
            &users(),
            &mut heap,
            &mut indexes(&pools),
            &assignments,
            &filter,
        )
        .unwrap_err();
        assert_eq!(error.sqlstate(), SqlState::DivisionByZero);
        let error = delete(&users(), &mut heap, &mut indexes(&pools), &filter).unwrap_err();
        assert_eq!(error.sqlstate(), SqlState::DivisionByZero);
        assert_eq!(rows_of(&heap), table());
    }
}
