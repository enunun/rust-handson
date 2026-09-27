use ferrodb::repl;

fn run(input: &str, interactive: bool) -> String {
    let mut output = Vec::new();
    repl::run(input.as_bytes(), &mut output, interactive).unwrap();
    String::from_utf8(output).unwrap()
}

#[test]
fn executes_statements_and_shows_results() {
    let input = "CREATE TABLE users (id INTEGER, name VARCHAR(20));\n\
                 INSERT INTO users VALUES (1, 'alice'),\n  (2, 'bob');\n\
                 SELECT * FROM users;\n";
    assert_eq!(
        run(input, false),
        "CREATE TABLE\n\
         INSERT 0 2\n \
         ID | NAME\n\
         ----+-------\n  \
         1 | alice\n  \
         2 | bob\n\
         (2 rows)\n\
         \n"
    );
}

#[test]
fn shows_an_error_and_continues() {
    assert_eq!(
        run("VALUES (1 +);\nVALUES (2);\n", false),
        "ERROR:  syntax error at or near \")\"\n COLUMN1\n---------\n       2\n(1 row)\n\n"
    );
}

#[test]
fn executes_an_unterminated_statement_at_the_end_of_input() {
    assert_eq!(
        run("VALUES (1)", false),
        " COLUMN1\n---------\n       1\n(1 row)\n\n"
    );
}

#[test]
fn ignores_empty_statements() {
    assert_eq!(run(";\n  ;\n", false), "");
}

#[test]
fn shows_prompts_when_interactive() {
    assert_eq!(
        run("VALUES (1,\n2);\n", true),
        "ferrodb>       ->  COLUMN1 | COLUMN2\n---------+---------\n       1 |       2\n(1 row)\n\nferrodb> \n"
    );
}

#[test]
fn shows_a_query_plan() {
    let input = "CREATE TABLE users (id INTEGER, name VARCHAR(20));\n\
                 EXPLAIN SELECT name FROM users WHERE id > 1 ORDER BY name;\n";
    assert_eq!(
        run(input, false),
        "CREATE TABLE\n        \
         QUERY PLAN\n\
         ---------------------------\n \
         Sort [NAME]\n   \
         Project [USERS.NAME]\n     \
         Filter (USERS.ID > 1)\n       \
         SeqScan USERS\n\
         (4 rows)\n\
         \n"
    );
}

#[test]
fn prompt_shows_the_transaction_state() {
    let input = "START TRANSACTION;\nVALUES (1 / 0);\nROLLBACK;\n";
    assert_eq!(
        run(input, true),
        "ferrodb> START TRANSACTION\n\
         ferrodb*> ERROR:  division by zero\n\
         ferrodb!> ROLLBACK\n\
         ferrodb> \n"
    );
}
