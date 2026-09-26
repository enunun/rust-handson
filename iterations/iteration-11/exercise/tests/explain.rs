use ferrodb::{Database, SqlState, StatementResult, Value};

fn database() -> Database {
    let mut db = Database::new();
    db.execute("CREATE TABLE users (id INTEGER, name VARCHAR(20))")
        .unwrap();
    db.execute("INSERT INTO users VALUES (1, 'alice'), (2, 'bob'), (3, 'carol')")
        .unwrap();
    db
}

fn plan_lines(db: &mut Database, sql: &str) -> Vec<String> {
    match db.execute(sql).unwrap() {
        StatementResult::Rows(result) => {
            assert_eq!(result.columns, vec!["QUERY PLAN"]);
            result
                .rows
                .into_iter()
                .map(|row| match &row[0] {
                    Value::Varchar(line) => line.clone(),
                    other => panic!("not a line: {other:?}"),
                })
                .collect()
        }
        other => panic!("not a query: {other:?}"),
    }
}

#[test]
fn explain_shows_the_plan_as_an_indented_tree() {
    let mut db = database();
    assert_eq!(
        plan_lines(
            &mut db,
            "EXPLAIN SELECT name FROM users WHERE id > 1 ORDER BY name"
        ),
        vec![
            "Sort [NAME]",
            "  Project [NAME]",
            "    Filter (ID > 1)",
            "      SeqScan USERS",
        ]
    );
}

#[test]
fn explain_shows_distinct_and_limit() {
    let mut db = database();
    assert_eq!(
        plan_lines(
            &mut db,
            "EXPLAIN SELECT DISTINCT name FROM users ORDER BY name DESC NULLS LAST OFFSET 1 ROWS"
        ),
        vec![
            "Limit OFFSET 1",
            "  Sort [NAME DESC NULLS LAST]",
            "    Distinct",
            "      Project [NAME]",
            "        SeqScan USERS",
        ]
    );
}

#[test]
fn hidden_sort_column_is_removed_by_the_top_project() {
    let mut db = database();
    assert_eq!(
        plan_lines(
            &mut db,
            "EXPLAIN SELECT name, id * 10 AS score FROM users ORDER BY id DESC FETCH FIRST 2 ROWS ONLY"
        ),
        vec![
            "Project [NAME, SCORE]",
            "  Limit FETCH FIRST 2",
            "    Sort [ID DESC]",
            "      Project [NAME, (ID * 10), ID]",
            "        SeqScan USERS",
        ]
    );
    match db
        .execute(
            "SELECT name, id * 10 AS score FROM users ORDER BY id DESC FETCH FIRST 2 ROWS ONLY",
        )
        .unwrap()
    {
        StatementResult::Rows(result) => {
            assert_eq!(result.columns, vec!["NAME", "SCORE"]);
            assert_eq!(
                result.rows,
                vec![
                    vec![Value::Varchar("carol".to_string()), Value::Integer(30)],
                    vec![Value::Varchar("bob".to_string()), Value::Integer(20)],
                ]
            );
        }
        other => panic!("not a query: {other:?}"),
    }
}

#[test]
fn explain_reports_the_same_errors_as_select() {
    let mut db = database();
    let err = db.execute("EXPLAIN SELECT email FROM users").unwrap_err();
    assert_eq!(err.sqlstate(), SqlState::UndefinedColumn);
    let err = db.execute("EXPLAIN SELECT * FROM t").unwrap_err();
    assert_eq!(err.sqlstate(), SqlState::UndefinedTable);
}

#[test]
fn explain_does_not_evaluate_the_query() {
    let mut db = database();
    assert_eq!(
        plan_lines(&mut db, "EXPLAIN SELECT 1 / 0 FROM users"),
        vec!["Project [(1 / 0)]", "  SeqScan USERS"]
    );
}
