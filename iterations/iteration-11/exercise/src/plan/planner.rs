//! 名前を解決した`SELECT`を，演算子の木(実行計画)にする．

use crate::plan::binder::{BoundExpr, BoundSelect, SortOrder, SortSource};

/// 実行計画の演算子．子の演算子が返す行を受け取り，行を返す．
#[derive(Debug, PartialEq)]
pub enum PlanNode {
    /// 表のすべての行を順に返す．
    SeqScan { table: String, columns: Vec<String> },
    /// 条件が真の行だけを返す．
    Filter {
        input: Box<PlanNode>,
        predicate: BoundExpr,
    },
    /// 各行から式の値を計算した行を返す．
    Project {
        input: Box<PlanNode>,
        exprs: Vec<BoundExpr>,
        names: Vec<String>,
    },
    /// 同じ値の行を1つだけ返す．
    Distinct { input: Box<PlanNode> },
    /// すべての行を読み，キーで並べ替えて返す．
    Sort {
        input: Box<PlanNode>,
        keys: Vec<SortKey>,
    },
    /// 先頭の`offset`行を飛ばし，`fetch`行まで返す．
    Limit {
        input: Box<PlanNode>,
        offset: usize,
        fetch: Option<usize>,
    },
}

/// 並べ替えのキー．`expr`は子の演算子が返す行について評価する．
#[derive(Debug, Clone, PartialEq)]
pub struct SortKey {
    pub expr: BoundExpr,
    pub order: SortOrder,
}

impl PlanNode {
    /// この演算子が返す行の列名．
    pub fn columns(&self) -> &[String] {
        match self {
            PlanNode::SeqScan { columns, .. } => columns,
            PlanNode::Project { names, .. } => names,
            PlanNode::Filter { input, .. }
            | PlanNode::Distinct { input }
            | PlanNode::Sort { input, .. }
            | PlanNode::Limit { input, .. } => input.columns(),
        }
    }
}

/// `SELECT`を実行計画にする．演算子は，下から`SeqScan`，`Filter`，`Project`，`Distinct`，
/// `Sort`，`Limit`の順に重ねる．結果の列にない式で並べ替えるときは，その式を`Project`の
/// 隠れた列として計算し，最後の`Project`で取り除く．
pub fn plan(select: &BoundSelect) -> PlanNode {
    let mut node = PlanNode::SeqScan {
        table: select.table.clone(),
        columns: select.table_columns.clone(),
    };
    if let Some(predicate) = &select.filter {
        node = PlanNode::Filter {
            input: Box::new(node),
            predicate: predicate.clone(),
        };
    }
    let mut exprs = select.items.clone();
    let mut names = select.names.clone();
    let mut keys = Vec::new();
    for key in &select.order_by {
        let index = match &key.source {
            SortSource::Output(index) => *index,
            SortSource::Input(expr) => {
                names.push(expr.display(&select.table_columns));
                exprs.push(expr.clone());
                exprs.len() - 1
            }
        };
        keys.push(SortKey {
            expr: BoundExpr::Column(index),
            order: key.order,
        });
    }
    let hidden_columns = exprs.len() > select.items.len();
    node = PlanNode::Project {
        input: Box::new(node),
        exprs,
        names,
    };
    if select.distinct {
        node = PlanNode::Distinct {
            input: Box::new(node),
        };
    }
    if !keys.is_empty() {
        node = PlanNode::Sort {
            input: Box::new(node),
            keys,
        };
    }
    if select.limit.offset > 0 || select.limit.fetch.is_some() {
        node = PlanNode::Limit {
            input: Box::new(node),
            offset: select.limit.offset,
            fetch: select.limit.fetch,
        };
    }
    if hidden_columns {
        node = PlanNode::Project {
            input: Box::new(node),
            exprs: (0..select.items.len()).map(BoundExpr::Column).collect(),
            names: select.names.clone(),
        };
    }
    node
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{Column, TableSchema};
    use crate::plan::binder::bind_select;
    use crate::sql::ast::Statement;
    use crate::sql::lexer::tokenize;
    use crate::sql::parser::parse;
    use crate::value::DataType;

    fn users() -> TableSchema {
        let column = |name: &str| Column {
            name: name.to_string(),
            data_type: DataType::Integer,
            nullable: true,
        };
        TableSchema {
            name: "USERS".to_string(),
            columns: vec![column("ID"), column("NAME")],
            unique_constraints: vec![],
        }
    }

    fn plan_sql(sql: &str) -> PlanNode {
        match parse(&tokenize(sql).unwrap()).unwrap() {
            Statement::Select(select) => plan(&bind_select(&select, &users()).unwrap()),
            other => panic!("not a SELECT statement: {other:?}"),
        }
    }

    fn scan() -> Box<PlanNode> {
        Box::new(PlanNode::SeqScan {
            table: "USERS".to_string(),
            columns: vec!["ID".to_string(), "NAME".to_string()],
        })
    }

    #[test]
    fn select_is_a_project_over_a_scan() {
        assert_eq!(
            plan_sql("SELECT name FROM users"),
            PlanNode::Project {
                input: scan(),
                exprs: vec![BoundExpr::Column(1)],
                names: vec!["NAME".to_string()],
            }
        );
    }

    #[test]
    fn operators_are_stacked_in_the_order_of_processing() {
        let plan =
            plan_sql("SELECT DISTINCT name FROM users WHERE id > 1 ORDER BY name OFFSET 1 ROWS");
        let PlanNode::Limit { input, offset, .. } = plan else {
            panic!("not a Limit: {plan:?}");
        };
        assert_eq!(offset, 1);
        let PlanNode::Sort { input, keys } = *input else {
            panic!("not a Sort");
        };
        assert_eq!(keys[0].expr, BoundExpr::Column(0));
        let PlanNode::Distinct { input } = *input else {
            panic!("not a Distinct");
        };
        let PlanNode::Project { input, .. } = *input else {
            panic!("not a Project");
        };
        assert!(matches!(*input, PlanNode::Filter { .. }));
    }

    #[test]
    fn sort_key_outside_the_select_list_is_a_hidden_column() {
        let plan = plan_sql("SELECT name FROM users ORDER BY id");
        assert_eq!(plan.columns(), ["NAME"]);
        let PlanNode::Project { input, exprs, .. } = plan else {
            panic!("not a Project");
        };
        assert_eq!(exprs, vec![BoundExpr::Column(0)]);
        let PlanNode::Sort { input, keys } = *input else {
            panic!("not a Sort");
        };
        assert_eq!(keys[0].expr, BoundExpr::Column(1));
        assert_eq!(input.columns(), ["NAME", "ID"]);
    }
}
