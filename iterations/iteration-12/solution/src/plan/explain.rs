//! 実行計画を，`EXPLAIN`で表示する行にする．

use crate::plan::binder::SortOrder;
use crate::plan::planner::{PlanNode, SortKey};
use crate::sql::ast::JoinKind;

/// 実行計画の演算子を1行に1つずつ表す．子の演算子は，親より2文字深く字下げする．
pub fn explain(plan: &PlanNode) -> Vec<String> {
    let mut lines = Vec::new();
    explain_node(plan, 0, &mut lines);
    lines
}

fn explain_node(node: &PlanNode, depth: usize, lines: &mut Vec<String>) {
    let indent = "  ".repeat(depth);
    lines.push(format!("{indent}{}", label(node)));
    match node {
        PlanNode::SeqScan { .. } => {}
        PlanNode::NestedLoopJoin { left, right, .. } => {
            explain_node(left, depth + 1, lines);
            explain_node(right, depth + 1, lines);
        }
        PlanNode::Filter { input, .. }
        | PlanNode::HashAggregate { input, .. }
        | PlanNode::Project { input, .. }
        | PlanNode::Distinct { input }
        | PlanNode::Sort { input, .. }
        | PlanNode::Limit { input, .. } => explain_node(input, depth + 1, lines),
    }
}

/// 演算子の名前と，その引数．式は子の演算子の列名で表す．
fn label(node: &PlanNode) -> String {
    match node {
        PlanNode::SeqScan {
            table, alias: None, ..
        } => format!("SeqScan {table}"),
        PlanNode::SeqScan {
            table,
            alias: Some(alias),
            ..
        } => format!("SeqScan {table} {alias}"),
        PlanNode::NestedLoopJoin {
            kind,
            condition,
            columns,
            ..
        } => {
            let kind = match kind {
                JoinKind::Cross => "CROSS",
                JoinKind::Inner => "INNER",
                JoinKind::Left => "LEFT",
            };
            match condition {
                Some(condition) => {
                    format!("NestedLoopJoin {kind} {}", condition.display(columns))
                }
                None => format!("NestedLoopJoin {kind}"),
            }
        }
        PlanNode::Filter {
            input, predicate, ..
        } => format!("Filter {}", predicate.display(input.columns())),
        PlanNode::HashAggregate {
            input, keys, calls, ..
        } => {
            let calls: Vec<String> = calls
                .iter()
                .map(|call| call.display(input.columns()))
                .collect();
            let mut label = format!("HashAggregate [{}]", calls.join(", "));
            if !keys.is_empty() {
                let keys: Vec<String> = keys
                    .iter()
                    .map(|key| key.display(input.columns()))
                    .collect();
                label.push_str(&format!(" GROUP BY [{}]", keys.join(", ")));
            }
            label
        }
        PlanNode::Project { input, exprs, .. } => {
            let exprs: Vec<String> = exprs
                .iter()
                .map(|expr| expr.display(input.columns()))
                .collect();
            format!("Project [{}]", exprs.join(", "))
        }
        PlanNode::Distinct { .. } => "Distinct".to_string(),
        PlanNode::Sort { input, keys } => {
            let keys: Vec<String> = keys
                .iter()
                .map(|key| sort_key(key, input.columns()))
                .collect();
            format!("Sort [{}]", keys.join(", "))
        }
        PlanNode::Limit { offset, fetch, .. } => {
            let mut label = "Limit".to_string();
            if *offset > 0 {
                label.push_str(&format!(" OFFSET {offset}"));
            }
            if let Some(fetch) = fetch {
                label.push_str(&format!(" FETCH FIRST {fetch}"));
            }
            label
        }
    }
}

/// 並べ替えのキー．`DESC`と，既定と違う`NULLS`の位置を書き添える．
fn sort_key(key: &SortKey, columns: &[String]) -> String {
    let mut text = key.expr.display(columns);
    if key.order.descending {
        text.push_str(" DESC");
    }
    if key.order != SortOrder::new(key.order.descending, None) {
        if key.order.nulls_first {
            text.push_str(" NULLS FIRST");
        } else {
            text.push_str(" NULLS LAST");
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{Catalog, Column, TableSchema};
    use crate::plan::binder::bind_select;
    use crate::plan::planner::plan;
    use crate::sql::ast::Statement;
    use crate::sql::lexer::tokenize;
    use crate::sql::parser::parse;
    use crate::value::DataType;

    fn catalog() -> Catalog {
        let column = |name: &str| Column {
            name: name.to_string(),
            data_type: DataType::Integer,
            nullable: true,
        };
        let mut catalog = Catalog::default();
        catalog
            .create_table(TableSchema {
                name: "USERS".to_string(),
                columns: vec![column("ID"), column("NAME")],
                unique_constraints: vec![],
            })
            .unwrap();
        catalog
    }

    fn plan_sql(sql: &str) -> PlanNode {
        match parse(&tokenize(sql).unwrap()).unwrap() {
            Statement::Select(select) => plan(&bind_select(&select, &catalog()).unwrap()),
            other => panic!("not a SELECT statement: {other:?}"),
        }
    }

    #[test]
    fn each_operator_is_a_line_indented_under_its_parent() {
        assert_eq!(
            explain(&plan_sql(
                "SELECT name FROM users WHERE id > 1 ORDER BY name"
            )),
            vec![
                "Sort [NAME]",
                "  Project [USERS.NAME]",
                "    Filter (USERS.ID > 1)",
                "      SeqScan USERS",
            ]
        );
    }

    #[test]
    fn sort_keys_show_desc_and_non_default_nulls() {
        assert_eq!(
            explain(&plan_sql(
                "SELECT id FROM users ORDER BY id DESC, name NULLS FIRST, id DESC NULLS FIRST"
            ))[1],
            "  Sort [ID DESC, USERS.NAME NULLS FIRST, ID DESC]"
        );
    }

    #[test]
    fn limit_shows_offset_and_fetch() {
        assert_eq!(
            explain(&plan_sql(
                "SELECT id FROM users OFFSET 2 ROWS FETCH FIRST 3 ROWS ONLY"
            ))[0],
            "Limit OFFSET 2 FETCH FIRST 3"
        );
        assert_eq!(
            explain(&plan_sql("SELECT id FROM users FETCH FIRST 3 ROWS ONLY"))[0],
            "Limit FETCH FIRST 3"
        );
    }

    #[test]
    fn join_shows_its_kind_condition_and_both_children() {
        assert_eq!(
            explain(&plan_sql(
                "SELECT a.name FROM users a JOIN users b ON a.id = b.id CROSS JOIN users"
            )),
            vec![
                "Project [A.NAME]",
                "  NestedLoopJoin CROSS",
                "    NestedLoopJoin INNER (A.ID = B.ID)",
                "      SeqScan USERS A",
                "      SeqScan USERS B",
                "    SeqScan USERS",
            ]
        );
    }

    #[test]
    fn hash_aggregate_shows_its_calls_and_keys() {
        assert_eq!(
            explain(&plan_sql(
                "SELECT COUNT(DISTINCT name), SUM(id) FROM users GROUP BY name, id + 1"
            )),
            vec![
                "Project [COUNT(DISTINCT USERS.NAME), SUM(USERS.ID)]",
                "  HashAggregate [COUNT(DISTINCT USERS.NAME), SUM(USERS.ID)] GROUP BY [USERS.NAME, (USERS.ID + 1)]",
                "    SeqScan USERS",
            ]
        );
        assert_eq!(
            explain(&plan_sql("SELECT COUNT(*) FROM users"))[1],
            "  HashAggregate [COUNT(*)]"
        );
    }
}
