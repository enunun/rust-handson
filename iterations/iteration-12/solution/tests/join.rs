use ferrodb::{Database, QueryResult, SqlState, StatementResult, Value};

fn database() -> Database {
    let mut db = Database::new();
    db.execute("CREATE TABLE dept (code VARCHAR(10) PRIMARY KEY, title VARCHAR(20) NOT NULL)")
        .unwrap();
    db.execute(
        "CREATE TABLE emp (id INTEGER PRIMARY KEY, name VARCHAR(20) NOT NULL, dept VARCHAR(10), salary INTEGER)",
    )
    .unwrap();
    db.execute("INSERT INTO dept VALUES ('dev', 'Development'), ('ops', 'Operations')")
        .unwrap();
    db.execute(
        "INSERT INTO emp VALUES (1, 'Sato', 'dev', 500), (2, 'Suzuki', 'dev', 450), (3, 'Tanaka', 'ops', 400), (4, 'Ito', NULL, 300)",
    )
    .unwrap();
    db
}

fn query(db: &mut Database, sql: &str) -> QueryResult {
    match db.execute(sql).unwrap() {
        StatementResult::Rows(result) => result,
        other => panic!("not a query: {other:?}"),
    }
}

fn varchar(s: &str) -> Value {
    Value::Varchar(s.to_string())
}

#[test]
fn left_join_keeps_rows_without_a_match() {
    let result = query(
        &mut database(),
        "SELECT e.name, d.title FROM emp e LEFT JOIN dept d ON e.dept = d.code",
    );
    assert_eq!(result.columns, vec!["NAME", "TITLE"]);
    assert_eq!(
        result.rows,
        vec![
            vec![varchar("Sato"), varchar("Development")],
            vec![varchar("Suzuki"), varchar("Development")],
            vec![varchar("Tanaka"), varchar("Operations")],
            vec![varchar("Ito"), Value::Null],
        ]
    );
}

#[test]
fn inner_join_returns_only_matching_rows() {
    let result = query(
        &mut database(),
        "SELECT name, title FROM emp INNER JOIN dept ON dept = code ORDER BY name",
    );
    assert_eq!(
        result.rows,
        vec![
            vec![varchar("Sato"), varchar("Development")],
            vec![varchar("Suzuki"), varchar("Development")],
            vec![varchar("Tanaka"), varchar("Operations")],
        ]
    );
}

#[test]
fn comma_and_cross_join_return_every_pair() {
    let mut db = database();
    assert_eq!(query(&mut db, "SELECT * FROM emp, dept").rows.len(), 8);
    let result = query(&mut db, "SELECT * FROM dept CROSS JOIN emp");
    assert_eq!(
        result.columns,
        vec!["CODE", "TITLE", "ID", "NAME", "DEPT", "SALARY"]
    );
    assert_eq!(result.rows.len(), 8);
}

#[test]
fn where_filters_the_joined_rows() {
    let result = query(
        &mut database(),
        "SELECT e.id FROM emp e, dept d WHERE e.dept = d.code AND d.title = 'Operations'",
    );
    assert_eq!(result.rows, vec![vec![Value::Integer(3)]]);
}

#[test]
fn a_table_can_be_joined_with_itself_under_different_aliases() {
    let result = query(
        &mut database(),
        "SELECT a.name, b.name FROM emp a JOIN emp b ON a.salary > b.salary AND a.dept = b.dept",
    );
    assert_eq!(result.rows, vec![vec![varchar("Sato"), varchar("Suzuki")]]);
}

#[test]
fn column_in_both_tables_is_ambiguous() {
    let mut db = database();
    db.execute("CREATE TABLE t (id INTEGER)").unwrap();
    let err = db.execute("SELECT id FROM emp, t").unwrap_err();
    assert_eq!(err.sqlstate(), SqlState::AmbiguousColumn);
    assert_eq!(err.sqlstate().code(), "42702");
    assert_eq!(err.to_string(), "column reference \"ID\" is ambiguous");
}

#[test]
fn qualified_names_must_refer_to_a_table_in_from() {
    let mut db = database();
    let err = db.execute("SELECT x.id FROM emp e").unwrap_err();
    assert_eq!(err.sqlstate().code(), "42P01");
    assert_eq!(err.to_string(), "missing FROM-clause entry for table \"X\"");
    let err = db.execute("SELECT emp.id FROM emp e").unwrap_err();
    assert_eq!(
        err.to_string(),
        "missing FROM-clause entry for table \"EMP\""
    );
    let err = db.execute("SELECT e.code FROM emp e").unwrap_err();
    assert_eq!(err.sqlstate().code(), "42703");
    assert_eq!(err.to_string(), "column E.CODE does not exist");
}

#[test]
fn same_table_name_twice_is_an_error() {
    let err = database().execute("SELECT * FROM emp, emp").unwrap_err();
    assert_eq!(err.sqlstate().code(), "42712");
    assert_eq!(
        err.to_string(),
        "table name \"EMP\" specified more than once"
    );
}

#[test]
fn join_condition_must_be_boolean() {
    let err = database()
        .execute("SELECT * FROM emp JOIN dept ON 1")
        .unwrap_err();
    assert_eq!(err.sqlstate(), SqlState::DatatypeMismatch);
    assert_eq!(
        err.to_string(),
        "argument of JOIN/ON must be type boolean, not type integer"
    );
}

#[test]
fn explain_shows_the_join_with_qualified_names() {
    let result = query(
        &mut database(),
        "EXPLAIN SELECT e.name, d.title FROM emp e LEFT JOIN dept d ON e.dept = d.code",
    );
    assert_eq!(
        result.rows,
        vec![
            vec![varchar("Project [E.NAME, D.TITLE]")],
            vec![varchar("  NestedLoopJoin LEFT (E.DEPT = D.CODE)")],
            vec![varchar("    SeqScan EMP E")],
            vec![varchar("    SeqScan DEPT D")],
        ]
    );
}
