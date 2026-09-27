//! 表の行を加え，書き換え，消す．変更したあとの行が制約を満たすかを検査する．

use std::collections::HashSet;

use crate::catalog::TableSchema;
use crate::error::Error;
use crate::exec::eval::{eval, matches_filter};
use crate::plan::binder::BoundExpr;
use crate::storage::heap::HeapFile;
use crate::storage::page::{MAX_TUPLE_SIZE, PageError};
use crate::storage::tuple::encode_tuple;
use crate::value::{Row, Value};

/// 行が制約に違反したときのエラー．
#[derive(Debug, PartialEq)]
pub enum ConstraintError {
    NotNull { table: String, column: String },
    Unique { constraint: String },
}

/// 行`new_rows`を表に加え，加えた行の数を返す．制約に違反するか，ページに入らない行があれば，
/// 1行も加えない．
pub fn insert(
    schema: &TableSchema,
    heap: &mut HeapFile,
    new_rows: Vec<Row>,
) -> Result<usize, Error> {
    let mut after: Vec<Row> = heap.rows(schema)?.into_iter().map(|(_, row)| row).collect();
    after.extend(new_rows.iter().cloned());
    check_constraints(schema, &after)?;
    let tuples = new_rows
        .iter()
        .map(|row| encode(row, schema))
        .collect::<Result<Vec<_>, _>>()?;
    for tuple in &tuples {
        heap.insert(tuple)?;
    }
    Ok(tuples.len())
}

/// 条件を満たす行の列に，式の値を代入する．`assignments`は，列の番号と式の組である．
/// 書き換えた行の数を返す．途中でエラーになるか，制約に違反すれば，1行も書き換えない．
pub fn update(
    schema: &TableSchema,
    heap: &mut HeapFile,
    assignments: &[(usize, BoundExpr)],
    filter: &Option<BoundExpr>,
) -> Result<usize, Error> {
    let rows = heap.rows(schema)?;
    let changes = rows
        .iter()
        .map(|(_, row)| updated_row(schema, row, assignments, filter))
        .collect::<Result<Vec<Option<Row>>, Error>>()?;
    let mut after: Vec<Row> = rows.iter().map(|(_, row)| row.clone()).collect();
    for (row, change) in after.iter_mut().zip(&changes) {
        if let Some(new_row) = change {
            *row = new_row.clone();
        }
    }
    check_constraints(schema, &after)?;
    let mut writes = Vec::new();
    for ((id, _), change) in rows.iter().zip(&changes) {
        if let Some(new_row) = change {
            writes.push((*id, encode(new_row, schema)?));
        }
    }
    for (id, tuple) in &writes {
        heap.update(*id, tuple)?;
    }
    Ok(writes.len())
}

/// 条件を満たす行を消し，消した行の数を返す．途中でエラーになれば，1行も消さない．
pub fn delete(
    schema: &TableSchema,
    heap: &mut HeapFile,
    filter: &Option<BoundExpr>,
) -> Result<usize, Error> {
    let mut targets = Vec::new();
    for (id, row) in heap.rows(schema)? {
        if matches_filter(filter, &row)? {
            targets.push(id);
        }
    }
    for id in &targets {
        heap.delete(*id)?;
    }
    Ok(targets.len())
}

/// 行をタプルにする．ページに入らない大きさならエラーを返す．
fn encode(row: &[Value], schema: &TableSchema) -> Result<Vec<u8>, Error> {
    let tuple = encode_tuple(row, schema);
    if tuple.len() > MAX_TUPLE_SIZE {
        return Err(PageError::TupleTooLarge { size: tuple.len() }.into());
    }
    Ok(tuple)
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

/// 表のすべての行が，`NOT NULL`と一意性制約を満たすかを調べる．
fn check_constraints(schema: &TableSchema, rows: &[Row]) -> Result<(), ConstraintError> {
    for (index, column) in schema.columns.iter().enumerate() {
        if !column.nullable && rows.iter().any(|row| row[index].is_null()) {
            return Err(ConstraintError::NotNull {
                table: schema.name.clone(),
                column: column.name.clone(),
            });
        }
    }
    for constraint in &schema.unique_constraints {
        let mut seen = HashSet::new();
        for row in rows {
            let value = &row[constraint.column];
            if !value.is_null() && !seen.insert(value) {
                return Err(ConstraintError::Unique {
                    constraint: constraint.name.clone(),
                });
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
    use crate::sql::ast::{ArithmeticOp, BinaryOp, ComparisonOp};
    use crate::storage::disk::MemoryDiskManager;
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

    /// 行をタプルにして置いたページの列．
    fn heap_of(rows: &[Row]) -> HeapFile {
        let mut heap = HeapFile::new(Box::new(MemoryDiskManager::default()));
        for row in rows {
            heap.insert(&encode_tuple(row, &users())).unwrap();
        }
        heap
    }

    /// ページの列の行を，置いた順に返す．
    fn rows_of(heap: &HeapFile) -> Vec<Row> {
        heap.rows(&users())
            .unwrap()
            .into_iter()
            .map(|(_, row)| row)
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
            check_constraints(&users(), &rows),
            Err(ConstraintError::NotNull {
                table: "USERS".to_string(),
                column: "NAME".to_string()
            })
        );
    }

    #[test]
    fn duplicate_value_violates_the_unique_constraint() {
        let mut rows = table();
        rows.push(row(9, "dave", Some("a@x")));
        assert_eq!(
            check_constraints(&users(), &rows),
            Err(ConstraintError::Unique {
                constraint: "USERS_EMAIL_KEY".to_string()
            })
        );
    }

    #[test]
    fn unique_column_may_have_many_nulls() {
        assert_eq!(check_constraints(&users(), &table()), Ok(()));
    }

    #[test]
    fn insert_adds_rows_and_returns_their_count() {
        let mut heap = heap_of(&table());
        let new_rows = vec![row(4, "dave", None), row(5, "eve", None)];
        assert_eq!(insert(&users(), &mut heap, new_rows), Ok(2));
        assert_eq!(rows_of(&heap).len(), 5);
        assert_eq!(rows_of(&heap)[4], row(5, "eve", None));
    }

    #[test]
    fn insert_violating_a_constraint_adds_no_rows() {
        let mut heap = heap_of(&table());
        let new_rows = vec![row(4, "dave", None), row(1, "eve", None)];
        let error = insert(&users(), &mut heap, new_rows).unwrap_err();
        assert_eq!(error.sqlstate(), SqlState::UniqueViolation);
        assert_eq!(rows_of(&heap), table());
    }

    #[test]
    fn update_assigns_to_matching_rows_and_returns_their_count() {
        let mut heap = heap_of(&table());
        let assignments = vec![(1, constant(Value::Varchar("zed".to_string())))];
        assert_eq!(update(&users(), &mut heap, &assignments, &id_is(2)), Ok(1));
        assert_eq!(rows_of(&heap)[1], row(2, "zed", None));
        assert_eq!(rows_of(&heap)[0], row(1, "alice", Some("a@x")));
    }

    #[test]
    fn update_evaluates_assignments_on_the_row_before_the_update() {
        let mut heap = heap_of(&[row(1, "a", Some("b"))]);
        let assignments = vec![(1, column(2)), (2, column(1))];
        update(&users(), &mut heap, &assignments, &None).unwrap();
        assert_eq!(rows_of(&heap), vec![row(1, "b", Some("a"))]);
    }

    #[test]
    fn update_violating_a_constraint_changes_no_rows() {
        let mut heap = heap_of(&table());
        let assignments = vec![(0, constant(Value::Integer(1)))];
        let error = update(&users(), &mut heap, &assignments, &None).unwrap_err();
        assert_eq!(error.sqlstate(), SqlState::UniqueViolation);
        assert_eq!(rows_of(&heap), table());
    }

    #[test]
    fn unique_values_are_checked_after_all_rows_are_updated() {
        let mut heap = heap_of(&table());
        let id_plus_1 = binary(
            BinaryOp::Arithmetic(ArithmeticOp::Add),
            column(0),
            constant(Value::Integer(1)),
        );
        assert_eq!(update(&users(), &mut heap, &[(0, id_plus_1)], &None), Ok(3));
        assert_eq!(rows_of(&heap)[0][0], Value::Integer(2));
    }

    #[test]
    fn delete_removes_matching_rows_and_returns_their_count() {
        let mut heap = heap_of(&table());
        assert_eq!(delete(&users(), &mut heap, &id_is(2)), Ok(1));
        assert_eq!(
            rows_of(&heap),
            vec![row(1, "alice", Some("a@x")), row(3, "carol", None)]
        );
    }

    #[test]
    fn error_in_the_condition_leaves_the_table_unchanged() {
        let mut heap = heap_of(&table());
        let assignments = vec![(1, constant(Value::Varchar("zed".to_string())))];
        let filter = divides_by_zero_at_id_2();
        let error = update(&users(), &mut heap, &assignments, &filter).unwrap_err();
        assert_eq!(error.sqlstate(), SqlState::DivisionByZero);
        let error = delete(&users(), &mut heap, &filter).unwrap_err();
        assert_eq!(error.sqlstate(), SqlState::DivisionByZero);
        assert_eq!(rows_of(&heap), table());
    }
}
