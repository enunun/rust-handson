//! 実行計画から演算子の木を作り，実行する．

use std::collections::HashMap;

use crate::error::Error;
use crate::exec::Executor;
use crate::exec::aggregate::HashAggregate;
use crate::exec::distinct::Distinct;
use crate::exec::filter::Filter;
use crate::exec::join::NestedLoopJoin;
use crate::exec::limit::Limit;
use crate::exec::project::Project;
use crate::exec::scan::SeqScan;
use crate::exec::sort::Sort;
use crate::plan::planner::PlanNode;
use crate::value::Row;

/// 実行計画の演算子ごとに`Executor`を作り，子の演算子をつなぐ．`tables`は表の名前ごとの行である．
pub fn build(plan: &PlanNode, tables: &HashMap<String, Vec<Row>>) -> Box<dyn Executor> {
    match plan {
        PlanNode::SeqScan { table, .. } => Box::new(SeqScan::new(tables[table].clone())),
        PlanNode::NestedLoopJoin {
            left,
            right,
            kind,
            condition,
            ..
        } => Box::new(NestedLoopJoin::new(
            build(left, tables),
            build(right, tables),
            *kind,
            condition.clone(),
            right.columns().len(),
        )),
        PlanNode::Filter {
            input,
            predicate,
            clause,
        } => Box::new(Filter::new(build(input, tables), predicate.clone(), clause)),
        PlanNode::HashAggregate {
            input, keys, calls, ..
        } => Box::new(HashAggregate::new(
            build(input, tables),
            keys.clone(),
            calls.clone(),
        )),
        PlanNode::Project { input, exprs, .. } => {
            Box::new(Project::new(build(input, tables), exprs.clone()))
        }
        PlanNode::Distinct { input } => Box::new(Distinct::new(build(input, tables))),
        PlanNode::Sort { input, keys } => Box::new(Sort::new(build(input, tables), keys.clone())),
        PlanNode::Limit {
            input,
            offset,
            fetch,
        } => Box::new(Limit::new(build(input, tables), *offset, *fetch)),
    }
}

/// 演算子が返す行を，なくなるまで集める．
pub fn collect_rows(executor: &mut dyn Executor) -> Result<Vec<Row>, Error> {
    let mut rows = Vec::new();
    while let Some(row) = executor.next()? {
        rows.push(row);
    }
    Ok(rows)
}
