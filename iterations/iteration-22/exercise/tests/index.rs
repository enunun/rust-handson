use std::sync::Arc;

use ferrodb::{Database, Session, SqlState, StatementResult, Value};

fn database() -> Session {
    let mut db = Session::new(Arc::new(Database::new()));
    db.execute(
        "CREATE TABLE emp (id INTEGER PRIMARY KEY, name VARCHAR(20) NOT NULL, dept VARCHAR(10), salary INTEGER)",
    )
    .unwrap();
    db.execute(
        "INSERT INTO emp VALUES (1, 'alice', 'dev', 500), (2, 'bob', 'dev', 400), \
         (3, 'carol', 'ops', 450), (4, 'dave', NULL, NULL)",
    )
    .unwrap();
    db
}

fn rows(db: &mut Session, sql: &str) -> Vec<Vec<Value>> {
    match db.execute(sql).unwrap() {
        StatementResult::Rows(result) => result.rows,
        other => panic!("not a query: {other:?}"),
    }
}

fn plan_lines(db: &mut Session, sql: &str) -> Vec<String> {
    rows(db, &format!("EXPLAIN {sql}"))
        .into_iter()
        .map(|row| match &row[0] {
            Value::Varchar(line) => line.clone(),
            other => panic!("not a line: {other:?}"),
        })
        .collect()
}

fn names(db: &mut Session, sql: &str) -> Vec<String> {
    rows(db, sql)
        .into_iter()
        .map(|row| match &row[0] {
            Value::Varchar(name) => name.clone(),
            other => panic!("not a name: {other:?}"),
        })
        .collect()
}

#[test]
fn created_index_is_used_for_a_range_condition() {
    let mut db = database();
    assert_eq!(
        db.execute("CREATE INDEX emp_salary ON emp (salary)"),
        Ok(StatementResult::CreateIndex)
    );
    assert_eq!(
        plan_lines(&mut db, "SELECT name FROM emp WHERE salary >= 450"),
        vec![
            "Project [EMP.NAME]",
            "  IndexScan EMP USING EMP_SALARY (EMP.SALARY >= 450)",
        ]
    );
    assert_eq!(
        names(&mut db, "SELECT name FROM emp WHERE salary >= 450"),
        vec!["carol", "alice"]
    );
}

#[test]
fn primary_key_has_an_index() {
    let mut db = database();
    assert_eq!(
        plan_lines(
            &mut db,
            "SELECT name FROM emp WHERE id = 3 AND dept = 'ops'"
        ),
        vec![
            "Project [EMP.NAME]",
            "  Filter (EMP.DEPT = 'ops')",
            "    IndexScan EMP USING EMP_PKEY (EMP.ID = 3)",
        ]
    );
    assert_eq!(
        names(
            &mut db,
            "SELECT name FROM emp WHERE id = 3 AND dept = 'ops'"
        ),
        vec!["carol"]
    );
}

#[test]
fn index_scan_returns_the_same_rows_as_a_seq_scan() {
    let mut db = database();
    let sql = "SELECT id FROM emp WHERE salary > 400 AND salary <= 500 ORDER BY id";
    let before = rows(&mut db, sql);
    db.execute("CREATE INDEX emp_salary ON emp (salary)")
        .unwrap();
    assert!(
        plan_lines(&mut db, sql)
            .iter()
            .any(|line| line.contains("IndexScan"))
    );
    assert_eq!(rows(&mut db, sql), before);
    assert_eq!(
        before,
        vec![vec![Value::Integer(1)], vec![Value::Integer(3)]]
    );
}

#[test]
fn index_follows_insert_update_and_delete() {
    let mut db = database();
    db.execute("CREATE INDEX emp_salary ON emp (salary)")
        .unwrap();
    db.execute("INSERT INTO emp VALUES (5, 'eve', 'ops', 450)")
        .unwrap();
    db.execute("UPDATE emp SET salary = 300 WHERE id = 3")
        .unwrap();
    db.execute("DELETE FROM emp WHERE id = 1").unwrap();
    assert_eq!(
        names(&mut db, "SELECT name FROM emp WHERE salary >= 400"),
        vec!["bob", "eve"]
    );
    assert_eq!(
        names(&mut db, "SELECT name FROM emp WHERE salary = 300"),
        vec!["carol"]
    );
}

#[test]
fn dropped_index_is_no_longer_used() {
    let mut db = database();
    db.execute("CREATE INDEX emp_salary ON emp (salary)")
        .unwrap();
    assert_eq!(
        db.execute("DROP INDEX emp_salary"),
        Ok(StatementResult::DropIndex)
    );
    assert_eq!(
        plan_lines(&mut db, "SELECT name FROM emp WHERE salary >= 450"),
        vec![
            "Project [EMP.NAME]",
            "  Filter (EMP.SALARY >= 450)",
            "    SeqScan EMP",
        ]
    );
}

#[test]
fn index_statements_report_errors() {
    let mut db = database();
    db.execute("CREATE INDEX emp_salary ON emp (salary)")
        .unwrap();
    let cases = [
        (
            "CREATE INDEX emp_salary ON emp (dept)",
            "42P07",
            "relation \"EMP_SALARY\" already exists",
        ),
        (
            "CREATE INDEX x ON nowhere (a)",
            "42P01",
            "relation \"NOWHERE\" does not exist",
        ),
        (
            "CREATE INDEX x ON emp (age)",
            "42703",
            "column \"AGE\" of relation \"EMP\" does not exist",
        ),
        ("DROP INDEX x", "42704", "index \"X\" does not exist"),
        (
            "DROP INDEX emp_pkey",
            "2BP01",
            "cannot drop index EMP_PKEY because constraint EMP_PKEY on table EMP requires it",
        ),
    ];
    for (sql, code, message) in cases {
        let error = db.execute(sql).unwrap_err();
        assert_eq!(
            (error.sqlstate().code(), error.message()),
            (code, message),
            "{sql}"
        );
    }
}

#[test]
fn unique_constraint_is_checked_with_the_index() {
    let mut db = database();
    let error = db
        .execute("INSERT INTO emp VALUES (3, 'x', NULL, NULL)")
        .unwrap_err();
    assert_eq!(error.sqlstate(), SqlState::UniqueViolation);
    assert_eq!(
        error.message(),
        "duplicate key value violates unique constraint \"EMP_PKEY\""
    );
    db.execute("UPDATE emp SET id = id + 10").unwrap();
    assert_eq!(
        names(&mut db, "SELECT name FROM emp WHERE id = 13"),
        vec!["carol"]
    );
}

#[test]
fn index_on_values_too_long_for_a_key_is_not_created() {
    let mut db = Session::new(Arc::new(Database::new()));
    db.execute("CREATE TABLE t (body VARCHAR(3000))").unwrap();
    db.execute(&format!("INSERT INTO t VALUES ('{}')", "x".repeat(2001)))
        .unwrap();
    let error = db.execute("CREATE INDEX t_body ON t (body)").unwrap_err();
    assert_eq!(error.sqlstate(), SqlState::ProgramLimitExceeded);
    assert_eq!(error.message(), "index key size 2001 exceeds maximum 2000");
    let error = db.execute("DROP INDEX t_body").unwrap_err();
    assert_eq!(error.sqlstate().code(), "42704");
}

#[test]
fn indexes_remain_after_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let mut db = Session::new(Arc::new(Database::open(dir.path()).unwrap()));
    db.execute("CREATE TABLE t (id INTEGER PRIMARY KEY, n INTEGER)")
        .unwrap();
    db.execute("CREATE INDEX t_n ON t (n)").unwrap();
    for id in 0..500 {
        db.execute(&format!("INSERT INTO t VALUES ({id}, {})", id % 7))
            .unwrap();
    }
    drop(db);

    let mut db = Session::new(Arc::new(Database::open(dir.path()).unwrap()));
    assert_eq!(
        plan_lines(&mut db, "SELECT id FROM t WHERE n = 3"),
        vec!["Project [T.ID]", "  IndexScan T USING T_N (T.N = 3)"]
    );
    assert_eq!(rows(&mut db, "SELECT id FROM t WHERE n = 3").len(), 71);
    let error = db.execute("INSERT INTO t VALUES (499, 0)").unwrap_err();
    assert_eq!(error.sqlstate(), SqlState::UniqueViolation);
    db.execute("DROP TABLE t").unwrap();
    let mut files: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    files.sort();
    assert_eq!(files, vec!["catalog", "wal", "xact"]);
}
