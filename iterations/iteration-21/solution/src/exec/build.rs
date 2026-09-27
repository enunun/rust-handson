//! 実行計画から演算子の木を作り，実行する．

use std::collections::HashMap;

use crate::catalog::Catalog;
use crate::error::Error;
use crate::exec::Executor;
use crate::exec::aggregate::HashAggregate;
use crate::exec::distinct::Distinct;
use crate::exec::filter::Filter;
use crate::exec::index_scan::IndexScan;
use crate::exec::join::NestedLoopJoin;
use crate::exec::limit::Limit;
use crate::exec::project::Project;
use crate::exec::scan::SeqScan;
use crate::exec::sort::Sort;
use crate::index::AnyIndex;
use crate::plan::planner::PlanNode;
use crate::storage::buffer::BufferPool;
use crate::storage::disk::DiskManager;
use crate::storage::heap::HeapFile;
use crate::storage::tuple::{TupleHeader, decode_tuple};
use crate::txn::{Snapshot, TransactionManager, is_visible};
use crate::value::Row;

/// 演算子の木を作るときに読む，表，インデックス，カタログと，行の版を選ぶスナップショット．
/// `tables`は表の名前ごとのページの列，`indexes`はインデックスの名前ごとのバッファプールである．
pub struct BuildContext<'a> {
    pub tables: &'a HashMap<String, HeapFile>,
    pub indexes: &'a HashMap<String, BufferPool<Box<dyn DiskManager>>>,
    pub catalog: &'a Catalog,
    pub snapshot: &'a Snapshot,
    pub manager: &'a TransactionManager,
}

/// 実行計画の演算子ごとに`Executor`を作り，子の演算子をつなぐ．
/// `SeqScan`と`IndexScan`は，スナップショットから見える版のタプルを，カタログの表の定義で行に戻して持つ．
pub fn build(plan: &PlanNode, context: &BuildContext<'_>) -> Result<Box<dyn Executor>, Error> {
    let BuildContext {
        tables,
        indexes,
        catalog,
        snapshot,
        manager,
    } = context;
    let visible = |header: &TupleHeader| is_visible(header, snapshot, manager);
    let executor: Box<dyn Executor> = match plan {
        PlanNode::SeqScan { table, .. } => {
            let schema = catalog.table(table)?;
            let rows = tables[table]
                .rows(schema, visible)?
                .into_iter()
                .map(|(_, row)| row)
                .collect();
            Box::new(SeqScan::new(rows))
        }
        PlanNode::IndexScan {
            table,
            index,
            lower,
            upper,
            ..
        } => {
            let schema = catalog.table(table)?;
            let column = catalog.index(index)?.column;
            let tree = AnyIndex::open(&schema.columns[column].data_type, &indexes[index])?;
            let mut rows = Vec::new();
            for id in tree.range(lower.as_ref(), upper.as_ref())? {
                let Some(tuple) = tables[table].get(id)? else {
                    continue;
                };
                let (header, data) = TupleHeader::split(&tuple)?;
                if visible(&header) {
                    rows.push(decode_tuple(data, schema)?);
                }
            }
            Box::new(IndexScan::new(rows))
        }
        PlanNode::NestedLoopJoin {
            left,
            right,
            kind,
            condition,
            ..
        } => Box::new(NestedLoopJoin::new(
            build(left, context)?,
            build(right, context)?,
            *kind,
            condition.clone(),
            right.columns().len(),
        )),
        PlanNode::Filter {
            input,
            predicate,
            clause,
        } => Box::new(Filter::new(
            build(input, context)?,
            predicate.clone(),
            clause,
        )),
        PlanNode::HashAggregate {
            input, keys, calls, ..
        } => Box::new(HashAggregate::new(
            build(input, context)?,
            keys.clone(),
            calls.clone(),
        )),
        PlanNode::Project { input, exprs, .. } => {
            Box::new(Project::new(build(input, context)?, exprs.clone()))
        }
        PlanNode::Distinct { input } => Box::new(Distinct::new(build(input, context)?)),
        PlanNode::Sort { input, keys } => Box::new(Sort::new(build(input, context)?, keys.clone())),
        PlanNode::Limit {
            input,
            offset,
            fetch,
        } => Box::new(Limit::new(build(input, context)?, *offset, *fetch)),
    };
    Ok(executor)
}

/// 演算子が返す行を，なくなるまで集める．
pub fn collect_rows(executor: &mut dyn Executor) -> Result<Vec<Row>, Error> {
    let mut rows = Vec::new();
    while let Some(row) = executor.next()? {
        rows.push(row);
    }
    Ok(rows)
}
