//! 名前を解決した`SELECT`を，演算子の木(実行計画)にする．

use std::ops::Bound;

use crate::catalog::Catalog;
use crate::plan::binder::{
    AggregateCall, BoundExpr, BoundFrom, BoundSelect, SortOrder, SortSource,
};
use crate::sql::ast::{BinaryOp, ComparisonOp, JoinKind};
use crate::value::Value;

/// 実行計画の演算子．子の演算子が返す行を受け取り，行を返す．
#[derive(Debug, PartialEq)]
pub enum PlanNode {
    /// 表のすべての行を順に返す．
    SeqScan {
        table: String,
        alias: Option<String>,
        columns: Vec<String>,
    },
    /// インデックスで，キーが範囲にある行だけを，キーの順に返す．`conditions`は，範囲を決めた
    /// `WHERE`の条件である．
    IndexScan {
        table: String,
        alias: Option<String>,
        columns: Vec<String>,
        index: String,
        lower: Bound<Value>,
        upper: Bound<Value>,
        conditions: Vec<BoundExpr>,
    },
    /// 外側(左)の行ごとに内側(右)のすべての行と組にし，条件が真の組を返す．
    NestedLoopJoin {
        left: Box<PlanNode>,
        right: Box<PlanNode>,
        kind: JoinKind,
        condition: Option<BoundExpr>,
        columns: Vec<String>,
    },
    /// 条件が真の行だけを返す．`clause`は条件を書いた句(`WHERE`か`HAVING`)である．
    Filter {
        input: Box<PlanNode>,
        predicate: BoundExpr,
        clause: &'static str,
    },
    /// 行をキーの値でグループにまとめ，グループごとにキーの値と集約関数の結果を並べた行を返す．
    HashAggregate {
        input: Box<PlanNode>,
        keys: Vec<BoundExpr>,
        calls: Vec<AggregateCall>,
        columns: Vec<String>,
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
            PlanNode::SeqScan { columns, .. }
            | PlanNode::IndexScan { columns, .. }
            | PlanNode::NestedLoopJoin { columns, .. }
            | PlanNode::HashAggregate { columns, .. } => columns,
            PlanNode::Project { names, .. } => names,
            PlanNode::Filter { input, .. }
            | PlanNode::Distinct { input }
            | PlanNode::Sort { input, .. }
            | PlanNode::Limit { input, .. } => input.columns(),
        }
    }
}

/// `FROM`の表を`SeqScan`に，結合を`NestedLoopJoin`にする．
fn plan_from(from: &BoundFrom) -> PlanNode {
    match from {
        BoundFrom::Table {
            table,
            alias,
            columns,
        } => PlanNode::SeqScan {
            table: table.clone(),
            alias: alias.clone(),
            columns: columns.clone(),
        },
        BoundFrom::Join {
            left,
            right,
            kind,
            condition,
        } => {
            let left = plan_from(left);
            let right = plan_from(right);
            let mut columns = left.columns().to_vec();
            columns.extend_from_slice(right.columns());
            PlanNode::NestedLoopJoin {
                left: Box::new(left),
                right: Box::new(right),
                kind: *kind,
                condition: condition.clone(),
                columns,
            }
        }
    }
}

/// `FROM`を読む演算子と，その上の`Filter`で調べる残りの条件を返す．
/// 1つの表の`WHERE`に，インデックスの列と定数を比べる条件があれば，`IndexScan`で読む．
fn plan_scan(
    from: &BoundFrom,
    filter: Option<&BoundExpr>,
    catalog: &Catalog,
) -> (PlanNode, Option<BoundExpr>) {
    let (
        BoundFrom::Table {
            table,
            alias,
            columns,
        },
        Some(filter),
    ) = (from, filter)
    else {
        return (plan_from(from), filter.cloned());
    };
    let conjuncts = conjuncts(filter);
    let Some(choice) = choose_index(table, &conjuncts, catalog) else {
        return (plan_from(from), Some(filter.clone()));
    };
    let mut used = Vec::new();
    let mut rest = Vec::new();
    for (position, conjunct) in conjuncts.into_iter().enumerate() {
        if choice.used.contains(&position) {
            used.push(conjunct.clone());
        } else {
            rest.push(conjunct.clone());
        }
    }
    let scan = PlanNode::IndexScan {
        table: table.clone(),
        alias: alias.clone(),
        columns: columns.clone(),
        index: choice.index,
        lower: choice.lower,
        upper: choice.upper,
        conditions: used,
    };
    (scan, and_all(rest))
}

/// インデックスで読むときの，インデックスの名前とキーの範囲．`used`は，範囲を決めた条件の位置である．
struct IndexChoice {
    index: String,
    lower: Bound<Value>,
    upper: Bound<Value>,
    used: Vec<usize>,
    equality: bool,
}

/// 表のインデックスのうち，条件で範囲を決められるものを選ぶ．`=`の条件のあるインデックスを優先し，
/// 同じなら名前の順で先のものを選ぶ．
fn choose_index(table: &str, conjuncts: &[&BoundExpr], catalog: &Catalog) -> Option<IndexChoice> {
    let schema = catalog.table(table).ok()?;
    let mut best: Option<IndexChoice> = None;
    for index in catalog.indexes_of(table) {
        let column = &schema.columns[index.column];
        let mut choice = IndexChoice {
            index: index.name.clone(),
            lower: Bound::Unbounded,
            upper: Bound::Unbounded,
            used: Vec::new(),
            equality: false,
        };
        for (position, conjunct) in conjuncts.iter().enumerate() {
            let Some((target, op, value)) = column_comparison(conjunct) else {
                continue;
            };
            if target != index.column || value.is_null() {
                continue;
            }
            let Ok(value) = column.assign(value.clone()) else {
                continue;
            };
            match op {
                ComparisonOp::Eq => {
                    choice.lower = tighter_lower(choice.lower, Bound::Included(value.clone()));
                    choice.upper = tighter_upper(choice.upper, Bound::Included(value));
                    choice.equality = true;
                }
                ComparisonOp::Lt => {
                    choice.upper = tighter_upper(choice.upper, Bound::Excluded(value))
                }
                ComparisonOp::LtEq => {
                    choice.upper = tighter_upper(choice.upper, Bound::Included(value))
                }
                ComparisonOp::Gt => {
                    choice.lower = tighter_lower(choice.lower, Bound::Excluded(value))
                }
                ComparisonOp::GtEq => {
                    choice.lower = tighter_lower(choice.lower, Bound::Included(value))
                }
                ComparisonOp::NotEq => continue,
            }
            choice.used.push(position);
        }
        let better = match &best {
            _ if choice.used.is_empty() => false,
            None => true,
            Some(best) => choice.equality && !best.equality,
        };
        if better {
            best = Some(choice);
        }
    }
    best
}

/// `AND`でつないだ条件を，1つずつの条件に分ける．
fn conjuncts(expr: &BoundExpr) -> Vec<&BoundExpr> {
    match expr {
        BoundExpr::Binary {
            op: BinaryOp::And,
            left,
            right,
        } => {
            let mut list = conjuncts(left);
            list.extend(conjuncts(right));
            list
        }
        other => vec![other],
    }
}

/// 条件を`AND`でつなぐ．条件がなければ`None`を返す．
fn and_all(conditions: Vec<BoundExpr>) -> Option<BoundExpr> {
    conditions
        .into_iter()
        .reduce(|left, right| BoundExpr::Binary {
            op: BinaryOp::And,
            left: Box::new(left),
            right: Box::new(right),
        })
}

/// `列 比較 定数`か`定数 比較 列`の条件を，列の番号，列から見た比較演算子，定数にする．
fn column_comparison(expr: &BoundExpr) -> Option<(usize, ComparisonOp, &Value)> {
    let BoundExpr::Binary {
        op: BinaryOp::Comparison(op),
        left,
        right,
    } = expr
    else {
        return None;
    };
    match (left.as_ref(), right.as_ref()) {
        (BoundExpr::Column(column), BoundExpr::Constant(value)) => {
            Some((*column, op.clone(), value))
        }
        (BoundExpr::Constant(value), BoundExpr::Column(column)) => {
            let flipped = match op {
                ComparisonOp::Lt => ComparisonOp::Gt,
                ComparisonOp::LtEq => ComparisonOp::GtEq,
                ComparisonOp::Gt => ComparisonOp::Lt,
                ComparisonOp::GtEq => ComparisonOp::LtEq,
                other => other.clone(),
            };
            Some((*column, flipped, value))
        }
        _ => None,
    }
}

/// 2つの下限のうち，狭い方を返す．
fn tighter_lower(current: Bound<Value>, new: Bound<Value>) -> Bound<Value> {
    match (&current, &new) {
        (Bound::Unbounded, _) => new,
        (_, Bound::Unbounded) => current,
        (
            Bound::Included(old) | Bound::Excluded(old),
            Bound::Included(value) | Bound::Excluded(value),
        ) => {
            if value > old || (value == old && matches!(new, Bound::Excluded(_))) {
                new
            } else {
                current
            }
        }
    }
}

/// 2つの上限のうち，狭い方を返す．
fn tighter_upper(current: Bound<Value>, new: Bound<Value>) -> Bound<Value> {
    match (&current, &new) {
        (Bound::Unbounded, _) => new,
        (_, Bound::Unbounded) => current,
        (
            Bound::Included(old) | Bound::Excluded(old),
            Bound::Included(value) | Bound::Excluded(value),
        ) => {
            if value < old || (value == old && matches!(new, Bound::Excluded(_))) {
                new
            } else {
                current
            }
        }
    }
}

/// `SELECT`を実行計画にする．演算子は，下から`SeqScan`か`IndexScan`，`Filter`，`HashAggregate`，`Filter`(`HAVING`)，`Project`，`Distinct`，
/// `Sort`，`Limit`の順に重ねる．結果の列にない式で並べ替えるときは，その式を`Project`の
/// 隠れた列として計算し，最後の`Project`で取り除く．
pub fn plan(select: &BoundSelect, catalog: &Catalog) -> PlanNode {
    let (mut node, filter) = plan_scan(&select.from, select.filter.as_ref(), catalog);
    if let Some(predicate) = filter {
        node = PlanNode::Filter {
            input: Box::new(node),
            predicate,
            clause: "WHERE",
        };
    }
    if let Some(aggregate) = &select.aggregate {
        node = PlanNode::HashAggregate {
            input: Box::new(node),
            keys: aggregate.keys.clone(),
            calls: aggregate.calls.clone(),
            columns: select.input_columns.clone(),
        };
        if let Some(having) = &aggregate.having {
            node = PlanNode::Filter {
                input: Box::new(node),
                predicate: having.clone(),
                clause: "HAVING",
            };
        }
    }
    let mut exprs = select.items.clone();
    let mut names = select.names.clone();
    let mut keys = Vec::new();
    for key in &select.order_by {
        let index = match &key.source {
            SortSource::Output(index) => *index,
            SortSource::Input(expr) => {
                names.push(expr.display(&select.input_columns));
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
    use crate::catalog::{Catalog, Column, IndexDef, TableSchema};
    use crate::plan::binder::bind_select;
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
        for (name, column) in [("USERS_ID", 0), ("USERS_NAME", 1)] {
            catalog
                .create_index(IndexDef {
                    name: name.to_string(),
                    table: "USERS".to_string(),
                    column,
                })
                .unwrap();
        }
        catalog
    }

    fn plan_sql(sql: &str) -> PlanNode {
        match parse(&tokenize(sql).unwrap()).unwrap() {
            Statement::Select(select) => {
                let catalog = catalog();
                plan(&bind_select(&select, &catalog).unwrap(), &catalog)
            }
            other => panic!("not a SELECT statement: {other:?}"),
        }
    }

    fn scan() -> Box<PlanNode> {
        Box::new(PlanNode::SeqScan {
            table: "USERS".to_string(),
            alias: None,
            columns: vec!["USERS.ID".to_string(), "USERS.NAME".to_string()],
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
        assert!(matches!(*input, PlanNode::IndexScan { .. }));
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
        assert_eq!(input.columns(), ["NAME", "USERS.ID"]);
    }

    #[test]
    fn join_becomes_a_nested_loop_join_of_two_scans() {
        let plan = plan_sql("SELECT * FROM users a LEFT JOIN users b ON a.id = b.id");
        let PlanNode::Project { input, .. } = plan else {
            panic!("not a Project");
        };
        let PlanNode::NestedLoopJoin {
            left,
            right,
            kind,
            columns,
            ..
        } = *input
        else {
            panic!("not a NestedLoopJoin");
        };
        assert_eq!(kind, JoinKind::Left);
        assert_eq!(columns, vec!["A.ID", "A.NAME", "B.ID", "B.NAME"]);
        assert!(matches!(*left, PlanNode::SeqScan { alias: Some(_), .. }));
        assert_eq!(right.columns(), ["B.ID", "B.NAME"]);
    }

    #[test]
    fn aggregate_and_having_are_between_the_scan_and_the_project() {
        let plan = plan_sql("SELECT name, COUNT(*) FROM users GROUP BY name HAVING COUNT(*) > 1");
        let PlanNode::Project { input, exprs, .. } = plan else {
            panic!("not a Project");
        };
        assert_eq!(exprs, vec![BoundExpr::Column(0), BoundExpr::Column(1)]);
        let PlanNode::Filter { input, clause, .. } = *input else {
            panic!("not a Filter");
        };
        assert_eq!(clause, "HAVING");
        let PlanNode::HashAggregate { input, columns, .. } = *input else {
            panic!("not a HashAggregate");
        };
        assert_eq!(columns, vec!["USERS.NAME", "COUNT(*)"]);
        assert!(matches!(*input, PlanNode::SeqScan { .. }));
    }

    /// 実行計画の`Project`の下の演算子．
    fn below_project(sql: &str) -> PlanNode {
        match plan_sql(sql) {
            PlanNode::Project { input, .. } => *input,
            other => panic!("not a Project: {other:?}"),
        }
    }

    fn integer(n: i32) -> Value {
        Value::Integer(n)
    }

    #[test]
    fn equality_on_an_indexed_column_becomes_an_index_scan() {
        let PlanNode::IndexScan {
            index,
            lower,
            upper,
            conditions,
            ..
        } = below_project("SELECT name FROM users WHERE id = 2")
        else {
            panic!("not an IndexScan");
        };
        assert_eq!(index, "USERS_ID");
        assert_eq!(
            (lower, upper),
            (Bound::Included(integer(2)), Bound::Included(integer(2)))
        );
        assert_eq!(conditions.len(), 1);
    }

    #[test]
    fn range_conditions_on_one_column_are_combined() {
        let PlanNode::IndexScan { lower, upper, .. } =
            below_project("SELECT * FROM users WHERE id > 1 AND id <= 5 AND id > 3")
        else {
            panic!("not an IndexScan");
        };
        assert_eq!(lower, Bound::Excluded(integer(3)));
        assert_eq!(upper, Bound::Included(integer(5)));
    }

    #[test]
    fn constant_on_the_left_is_turned_around() {
        let PlanNode::IndexScan { lower, upper, .. } =
            below_project("SELECT * FROM users WHERE 3 > id")
        else {
            panic!("not an IndexScan");
        };
        assert_eq!(
            (lower, upper),
            (Bound::Unbounded, Bound::Excluded(integer(3)))
        );
    }

    #[test]
    fn other_conditions_remain_in_a_filter() {
        let PlanNode::Filter {
            input, predicate, ..
        } = below_project("SELECT * FROM users WHERE id >= 1 AND name <> 2 AND id + 1 = 3")
        else {
            panic!("not a Filter");
        };
        assert_eq!(
            predicate.display(input.columns()),
            "((USERS.NAME <> 2) AND ((USERS.ID + 1) = 3))"
        );
        assert!(matches!(*input, PlanNode::IndexScan { .. }));
    }

    #[test]
    fn index_with_an_equality_is_preferred() {
        let PlanNode::Filter { input, .. } =
            below_project("SELECT * FROM users WHERE id > 1 AND name = 2")
        else {
            panic!("not a Filter");
        };
        let PlanNode::IndexScan { index, .. } = *input else {
            panic!("not an IndexScan");
        };
        assert_eq!(index, "USERS_NAME");
    }

    #[test]
    fn conditions_the_index_cannot_answer_use_a_seq_scan() {
        for sql in [
            "SELECT * FROM users WHERE id = 1 OR id = 2",
            "SELECT * FROM users WHERE id <> 1",
            "SELECT * FROM users WHERE id = 3000000000",
            "SELECT * FROM users WHERE id = NULL",
            "SELECT * FROM users a JOIN users b ON a.id = b.id WHERE a.id = 1",
        ] {
            let PlanNode::Filter { input, .. } = below_project(sql) else {
                panic!("not a Filter: {sql}");
            };
            assert!(!matches!(*input, PlanNode::IndexScan { .. }), "{sql}");
        }
    }
}
