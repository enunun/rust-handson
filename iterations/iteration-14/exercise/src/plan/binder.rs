//! 構文木の式の名前を解決する．列の名前を行の中の番号にし，リテラルを値にする．

use crate::catalog::{Catalog, Column};
use crate::sql::ast::{
    AggregateFunc, ArithmeticOp, BinaryOp, ComparisonOp, Expr, JoinKind, Limit, Select, SelectItem,
    TableRef, UnaryOp,
};
use crate::value::Value;

/// 名前を解決した式．列は行の中の番号で指す．
#[derive(Debug, Clone, PartialEq)]
pub enum BoundExpr {
    Constant(Value),
    Column(usize),
    Unary {
        op: UnaryOp,
        operand: Box<BoundExpr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<BoundExpr>,
        right: Box<BoundExpr>,
    },
    IsNull {
        operand: Box<BoundExpr>,
        negated: bool,
    },
}

/// 式の中から見える列の並び．`FROM`の表の列を，結合した行の列と同じ順に並べる．
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scope {
    columns: Vec<ScopeColumn>,
}

/// 見える列の1つ．`table`は表の別名で，別名がなければ表の名前である．
#[derive(Debug, Clone, PartialEq)]
pub struct ScopeColumn {
    pub table: String,
    pub name: String,
}

/// 名前を解決した`SELECT`．
#[derive(Debug, PartialEq)]
pub struct BoundSelect {
    pub from: BoundFrom,
    pub input_columns: Vec<String>,
    pub names: Vec<String>,
    pub items: Vec<BoundExpr>,
    pub filter: Option<BoundExpr>,
    pub aggregate: Option<BoundAggregate>,
    pub distinct: bool,
    pub order_by: Vec<BoundOrderBy>,
    pub limit: Limit,
}

/// 集約する問い合わせの，グループのキーと集約関数の呼び出し．
/// 集約した行は，キーの値のあとに集約関数の結果を並べたものである．
/// 選択項目，`having`，`ORDER BY`の式は，集約した行について評価する．
#[derive(Debug, PartialEq)]
pub struct BoundAggregate {
    pub keys: Vec<BoundExpr>,
    pub calls: Vec<AggregateCall>,
    pub having: Option<BoundExpr>,
}

/// 名前を解決した集約関数の呼び出し．`arg`は`COUNT(*)`なら`None`で，`FROM`の行について評価する．
#[derive(Debug, Clone, PartialEq)]
pub struct AggregateCall {
    pub func: AggregateFunc,
    pub arg: Option<BoundExpr>,
    pub distinct: bool,
}

/// 集約する問い合わせで，式を集約した行について解決するための情報．
struct Grouping {
    keys: Vec<BoundExpr>,
    calls: Vec<AggregateCall>,
}

/// 名前を解決した`FROM`．
#[derive(Debug, PartialEq)]
pub enum BoundFrom {
    /// 表．`columns`は修飾した列名(`表.列`)である．
    Table {
        table: String,
        alias: Option<String>,
        columns: Vec<String>,
    },
    /// 2つの表の結合．`condition`は，左右の列を並べた行について評価する．
    Join {
        left: Box<BoundFrom>,
        right: Box<BoundFrom>,
        kind: JoinKind,
        condition: Option<BoundExpr>,
    },
}

/// 名前を解決した`ORDER BY`のキー．
#[derive(Debug, PartialEq)]
pub struct BoundOrderBy {
    pub source: SortSource,
    pub order: SortOrder,
}

/// 並べ替えのキーの値の求め方．
#[derive(Debug, PartialEq)]
pub enum SortSource {
    /// 結果の列の値を使う．
    Output(usize),
    /// `FROM`の行について式を評価する．
    Input(BoundExpr),
}

/// 並べ替えのキーの向きと，`NULL`を置く位置．
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SortOrder {
    pub descending: bool,
    pub nulls_first: bool,
}

impl SortOrder {
    /// `NULLS FIRST`か`NULLS LAST`を書かなければ，`NULL`を最大の値として扱う．
    /// `ASC`では最後に，`DESC`では最初に置く．
    pub fn new(descending: bool, nulls_first: Option<bool>) -> SortOrder {
        SortOrder {
            descending,
            nulls_first: nulls_first.unwrap_or(descending),
        }
    }
}

/// 名前のない式の結果の列名．
const UNNAMED_COLUMN: &str = "?column?";

/// 名前解決のエラー．
#[derive(Debug, PartialEq)]
pub enum BindError {
    UndefinedColumn {
        column: String,
    },
    UndefinedQualifiedColumn {
        table: String,
        column: String,
    },
    AmbiguousColumn {
        column: String,
    },
    UndefinedTable {
        table: String,
    },
    MissingFromEntry {
        table: String,
    },
    DuplicateTableName {
        table: String,
    },
    OrderByNotInSelectList,
    NotGrouped {
        column: String,
    },
    NestedAggregate,
    AggregateNotAllowed {
        clause: &'static str,
    },
    /// 集約関数を書けない場所の集約関数．`bind_in`が`AggregateNotAllowed`に変える．
    MisplacedAggregate,
}

impl ScopeColumn {
    /// 列の名前が`name`で，`table`があればその表の列であるかを返す．
    fn is(&self, table: Option<&str>, name: &str) -> bool {
        match table {
            Some(table) => self.table == table && self.name == name,
            None => self.name == name,
        }
    }
}

impl Scope {
    /// 1つの表の列からなる並び．
    pub fn table(table: &str, columns: &[Column]) -> Scope {
        Scope {
            columns: columns
                .iter()
                .map(|column| ScopeColumn {
                    table: table.to_string(),
                    name: column.name.clone(),
                })
                .collect(),
        }
    }

    /// 2つの並びをつなぐ．結合した行は，左の表の列のあとに右の表の列が並ぶ．
    pub fn join(mut self, right: Scope) -> Scope {
        self.columns.extend(right.columns);
        self
    }

    /// 修飾した列名(`表.列`)の並び．
    pub fn qualified_names(&self) -> Vec<String> {
        self.columns
            .iter()
            .map(|column| format!("{}.{}", column.table, column.name))
            .collect()
    }

    fn has_table(&self, table: &str) -> bool {
        self.columns.iter().any(|column| column.table == table)
    }

    /// 列の名前を，行の中の番号にする．`table`があれば，その表の列だけから探す．
    fn resolve(&self, table: Option<&str>, name: &str) -> Result<usize, BindError> {
        match table {
            Some(table) if !self.has_table(table) => {
                return Err(BindError::MissingFromEntry {
                    table: table.to_string(),
                });
            }
            _ => {}
        }
        let found: Vec<usize> = (0..self.columns.len())
            .filter(|&index| self.columns[index].is(table, name))
            .collect();
        match (found.as_slice(), table) {
            ([index], _) => Ok(*index),
            ([], None) => Err(BindError::UndefinedColumn {
                column: name.to_string(),
            }),
            ([], Some(table)) => Err(BindError::UndefinedQualifiedColumn {
                table: table.to_string(),
                column: name.to_string(),
            }),
            (_, _) => Err(BindError::AmbiguousColumn {
                column: name.to_string(),
            }),
        }
    }
}

/// `SELECT`の`FROM`の表を引き，選択項目，`WHERE`の条件，`ORDER BY`のキーの名前を解決する．
/// `GROUP BY`か`HAVING`があるか，選択項目か`ORDER BY`に集約関数があれば，集約する問い合わせにする．
pub fn bind_select(select: &Select, catalog: &Catalog) -> Result<BoundSelect, BindError> {
    let mut tables = Vec::new();
    let (from, scope) = bind_from(&select.from, catalog, &mut tables)?;
    let filter = bind_filter(&select.filter, &scope, "WHERE")?;
    let aggregated = !select.group_by.is_empty()
        || select.having.is_some()
        || select.items.iter().any(|item| match item {
            SelectItem::Expr { expr, .. } => expr.contains_aggregate(),
            SelectItem::Wildcard => false,
        })
        || select
            .order_by
            .iter()
            .any(|key| key.expr.contains_aggregate());
    let mut grouping = if aggregated {
        let keys = select
            .group_by
            .iter()
            .map(|expr| bind_in(expr, &scope, "GROUP BY"))
            .collect::<Result<Vec<_>, _>>()?;
        Some(Grouping {
            keys,
            calls: Vec::new(),
        })
    } else {
        None
    };
    let (names, items) = bind_select_items(&select.items, &scope, &mut grouping)?;
    let having = match &select.having {
        Some(having) => Some(bind_item(having, &scope, &mut grouping)?),
        None => None,
    };
    let order_by = select
        .order_by
        .iter()
        .map(|key| {
            Ok(BoundOrderBy {
                source: sort_source(
                    &key.expr,
                    &names,
                    &items,
                    &scope,
                    &mut grouping,
                    select.distinct,
                )?,
                order: SortOrder::new(key.descending, key.nulls_first),
            })
        })
        .collect::<Result<Vec<_>, BindError>>()?;
    let (input_columns, aggregate) = match grouping {
        Some(grouping) => {
            let from_columns = scope.qualified_names();
            let mut columns: Vec<String> = grouping
                .keys
                .iter()
                .map(|key| key.display(&from_columns))
                .collect();
            columns.extend(
                grouping
                    .calls
                    .iter()
                    .map(|call| call.display(&from_columns)),
            );
            let aggregate = BoundAggregate {
                keys: grouping.keys,
                calls: grouping.calls,
                having,
            };
            (columns, Some(aggregate))
        }
        None => (scope.qualified_names(), None),
    };
    Ok(BoundSelect {
        from,
        input_columns,
        names,
        items,
        filter,
        aggregate,
        distinct: select.distinct,
        order_by,
        limit: select.limit,
    })
}

/// `FROM`の表をカタログから引き，見える列の並びを作る．`tables`には，それまでに現れた表の
/// 別名(別名がなければ名前)を集め，同じ名前が2度現れたらエラーにする．
fn bind_from(
    table_ref: &TableRef,
    catalog: &Catalog,
    tables: &mut Vec<String>,
) -> Result<(BoundFrom, Scope), BindError> {
    match table_ref {
        TableRef::Table { name, alias } => {
            let schema = catalog.table(name).map_err(|_| BindError::UndefinedTable {
                table: name.clone(),
            })?;
            let qualifier = alias.as_ref().unwrap_or(name);
            if tables.contains(qualifier) {
                return Err(BindError::DuplicateTableName {
                    table: qualifier.clone(),
                });
            }
            tables.push(qualifier.clone());
            let scope = Scope::table(qualifier, &schema.columns);
            let from = BoundFrom::Table {
                table: name.clone(),
                alias: alias.clone(),
                columns: scope.qualified_names(),
            };
            Ok((from, scope))
        }
        TableRef::Join(join) => {
            let (left, left_scope) = bind_from(&join.left, catalog, tables)?;
            let (right, right_scope) = bind_from(&join.right, catalog, tables)?;
            let scope = left_scope.join(right_scope);
            let condition = bind_filter(&join.condition, &scope, "JOIN conditions")?;
            let from = BoundFrom::Join {
                left: Box::new(left),
                right: Box::new(right),
                kind: join.kind,
                condition,
            };
            Ok((from, scope))
        }
    }
}

/// `WHERE`や`ON`の条件があれば，名前を解決する．`clause`は，集約関数を書いたときのエラーに使う．
pub fn bind_filter(
    filter: &Option<Expr>,
    scope: &Scope,
    clause: &'static str,
) -> Result<Option<BoundExpr>, BindError> {
    match filter {
        Some(filter) => Ok(Some(bind_in(filter, scope, clause)?)),
        None => Ok(None),
    }
}

/// 集約関数を書けない句`clause`の式の名前を解決する．
pub fn bind_in(expr: &Expr, scope: &Scope, clause: &'static str) -> Result<BoundExpr, BindError> {
    bind(expr, scope).map_err(|error| match error {
        BindError::MisplacedAggregate => BindError::AggregateNotAllowed { clause },
        other => other,
    })
}

/// 選択項目の名前を解決し，結果の列名と，各列を計算する式を返す．`*`はすべての列に展開する．
fn bind_select_items(
    items: &[SelectItem],
    scope: &Scope,
    grouping: &mut Option<Grouping>,
) -> Result<(Vec<String>, Vec<BoundExpr>), BindError> {
    let mut names = Vec::new();
    let mut exprs = Vec::new();
    for item in items {
        match item {
            SelectItem::Wildcard => {
                for (index, column) in scope.columns.iter().enumerate() {
                    names.push(column.name.clone());
                    exprs.push(match grouping {
                        Some(grouping) => {
                            grouping.key(&BoundExpr::Column(index)).ok_or_else(|| {
                                BindError::NotGrouped {
                                    column: format!("{}.{}", column.table, column.name),
                                }
                            })?
                        }
                        None => BoundExpr::Column(index),
                    });
                }
            }
            SelectItem::Expr { expr, alias } => {
                names.push(column_name(expr, alias));
                exprs.push(bind_item(expr, scope, grouping)?);
            }
        }
    }
    Ok((names, exprs))
}

/// 選択項目，`HAVING`，`ORDER BY`の式の名前を解決する．集約する問い合わせなら，集約した行について解決する．
fn bind_item(
    expr: &Expr,
    scope: &Scope,
    grouping: &mut Option<Grouping>,
) -> Result<BoundExpr, BindError> {
    match grouping {
        Some(grouping) => grouping.bind(expr, scope),
        None => bind(expr, scope),
    }
}

impl Grouping {
    /// `FROM`の行についての式がグループのキーなら，集約した行のその列を返す．
    fn key(&self, expr: &BoundExpr) -> Option<BoundExpr> {
        self.keys
            .iter()
            .position(|key| key == expr)
            .map(BoundExpr::Column)
    }

    /// 式を，集約した行について解決する．集約関数の呼び出しは，集約した行の結果の列にする．
    /// キーと同じ式はキーの列にする．それ以外の列の参照は`NotGrouped`になる．
    fn bind(&mut self, expr: &Expr, scope: &Scope) -> Result<BoundExpr, BindError> {
        if let Expr::Aggregate {
            func,
            arg,
            distinct,
        } = expr
        {
            let arg = match arg {
                Some(arg) => Some(bind(arg, scope).map_err(|error| match error {
                    BindError::MisplacedAggregate => BindError::NestedAggregate,
                    other => other,
                })?),
                None => None,
            };
            let call = AggregateCall {
                func: *func,
                arg,
                distinct: *distinct,
            };
            let index = match self.calls.iter().position(|known| *known == call) {
                Some(index) => index,
                None => {
                    self.calls.push(call);
                    self.calls.len() - 1
                }
            };
            return Ok(BoundExpr::Column(self.keys.len() + index));
        }
        if !expr.contains_aggregate() {
            let bound = bind(expr, scope)?;
            if let Some(key) = self.key(&bound) {
                return Ok(key);
            }
        }
        let bound = match expr {
            Expr::Column(name) => {
                return Err(BindError::NotGrouped {
                    column: name.clone(),
                });
            }
            Expr::QualifiedColumn { table, column } => {
                return Err(BindError::NotGrouped {
                    column: format!("{table}.{column}"),
                });
            }
            Expr::Unary { op, operand } => BoundExpr::Unary {
                op: op.clone(),
                operand: Box::new(self.bind(operand, scope)?),
            },
            Expr::Binary { op, left, right } => BoundExpr::Binary {
                op: op.clone(),
                left: Box::new(self.bind(left, scope)?),
                right: Box::new(self.bind(right, scope)?),
            },
            Expr::IsNull { operand, negated } => BoundExpr::IsNull {
                operand: Box::new(self.bind(operand, scope)?),
                negated: *negated,
            },
            Expr::Integer(_)
            | Expr::Boolean(_)
            | Expr::String(_)
            | Expr::Null
            | Expr::Aggregate { .. } => bind(expr, scope)?,
        };
        Ok(bound)
    }
}

/// 選択項目の結果の列名．別名があれば別名，列そのものなら列名(修飾を除く)，それ以外は`?column?`である．
fn column_name(expr: &Expr, alias: &Option<String>) -> String {
    match (alias, expr) {
        (Some(alias), _) => alias.clone(),
        (None, Expr::Column(name)) => name.clone(),
        (None, Expr::QualifiedColumn { column, .. }) => column.clone(),
        (None, Expr::Aggregate { func, .. }) => func.name().to_string(),
        (None, _) => UNNAMED_COLUMN.to_string(),
    }
}

/// 並べ替えのキーの値の求め方を決める．
/// 名前だけのキーは，まず結果の列名(別名を含む)から探す．見つからなければ，`FROM`の列の式として解決する．
/// 選択項目と同じ式なら，その結果の列の値を使う．
/// `DISTINCT`では，結果の列にないキーで並べ替えられない．
fn sort_source(
    expr: &Expr,
    names: &[String],
    items: &[BoundExpr],
    scope: &Scope,
    grouping: &mut Option<Grouping>,
    distinct: bool,
) -> Result<SortSource, BindError> {
    let output = match expr {
        Expr::Column(name) => names.iter().position(|output| output == name),
        _ => None,
    };
    if let Some(index) = output {
        return Ok(SortSource::Output(index));
    }
    let bound = bind_item(expr, scope, grouping)?;
    if let Some(index) = items.iter().position(|item| *item == bound) {
        return Ok(SortSource::Output(index));
    }
    if distinct {
        return Err(BindError::OrderByNotInSelectList);
    }
    Ok(SortSource::Input(bound))
}

/// 式の中の列の名前を，`scope`の中の番号に解決する．
pub fn bind(expr: &Expr, scope: &Scope) -> Result<BoundExpr, BindError> {
    let bound = match expr {
        Expr::Integer(n) => BoundExpr::Constant(match i32::try_from(*n) {
            Ok(n) => Value::Integer(n),
            Err(_) => Value::BigInt(*n),
        }),
        Expr::Boolean(b) => BoundExpr::Constant(Value::Boolean(*b)),
        Expr::String(s) => BoundExpr::Constant(Value::Varchar(s.clone())),
        Expr::Null => BoundExpr::Constant(Value::Null),
        Expr::Column(name) => BoundExpr::Column(scope.resolve(None, name)?),
        Expr::QualifiedColumn { table, column } => {
            BoundExpr::Column(scope.resolve(Some(table), column)?)
        }
        Expr::Unary { op, operand } => BoundExpr::Unary {
            op: op.clone(),
            operand: Box::new(bind(operand, scope)?),
        },
        Expr::Binary { op, left, right } => BoundExpr::Binary {
            op: op.clone(),
            left: Box::new(bind(left, scope)?),
            right: Box::new(bind(right, scope)?),
        },
        Expr::IsNull { operand, negated } => BoundExpr::IsNull {
            operand: Box::new(bind(operand, scope)?),
            negated: *negated,
        },
        Expr::Aggregate { .. } => return Err(BindError::MisplacedAggregate),
    };
    Ok(bound)
}

impl BoundExpr {
    /// `EXPLAIN`に表示する式の文字列．列は`columns`の名前で表し，演算は括弧で囲む．
    pub fn display(&self, columns: &[String]) -> String {
        match self {
            BoundExpr::Constant(value) => display_value(value),
            BoundExpr::Column(index) => columns[*index].clone(),
            BoundExpr::Unary { op, operand } => {
                let operand = operand.display(columns);
                match op {
                    UnaryOp::Neg => format!("(-{operand})"),
                    UnaryOp::Not => format!("(NOT {operand})"),
                }
            }
            BoundExpr::Binary { op, left, right } => format!(
                "({} {} {})",
                left.display(columns),
                operator(op),
                right.display(columns)
            ),
            BoundExpr::IsNull { operand, negated } => {
                let operand = operand.display(columns);
                if *negated {
                    format!("({operand} IS NOT NULL)")
                } else {
                    format!("({operand} IS NULL)")
                }
            }
        }
    }
}

impl AggregateCall {
    /// `EXPLAIN`に表示する呼び出しの文字列．引数は`columns`の名前で表す．
    pub fn display(&self, columns: &[String]) -> String {
        let name = self.func.name();
        match (&self.arg, self.distinct) {
            (None, _) => format!("{name}(*)"),
            (Some(arg), false) => format!("{name}({})", arg.display(columns)),
            (Some(arg), true) => format!("{name}(DISTINCT {})", arg.display(columns)),
        }
    }
}

/// 定数をSQLのリテラルの形で表す．
fn display_value(value: &Value) -> String {
    match value {
        Value::Integer(n) => n.to_string(),
        Value::BigInt(n) => n.to_string(),
        Value::Boolean(true) => "TRUE".to_string(),
        Value::Boolean(false) => "FALSE".to_string(),
        Value::Varchar(s) => format!("'{}'", s.replace('\'', "''")),
        Value::Null => "NULL".to_string(),
    }
}

/// 2項演算子の記号．
fn operator(op: &BinaryOp) -> &'static str {
    match op {
        BinaryOp::Arithmetic(ArithmeticOp::Add) => "+",
        BinaryOp::Arithmetic(ArithmeticOp::Sub) => "-",
        BinaryOp::Arithmetic(ArithmeticOp::Mul) => "*",
        BinaryOp::Arithmetic(ArithmeticOp::Div) => "/",
        BinaryOp::Comparison(ComparisonOp::Eq) => "=",
        BinaryOp::Comparison(ComparisonOp::NotEq) => "<>",
        BinaryOp::Comparison(ComparisonOp::Lt) => "<",
        BinaryOp::Comparison(ComparisonOp::LtEq) => "<=",
        BinaryOp::Comparison(ComparisonOp::Gt) => ">",
        BinaryOp::Comparison(ComparisonOp::GtEq) => ">=",
        BinaryOp::Concat => "||",
        BinaryOp::And => "AND",
        BinaryOp::Or => "OR",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::TableSchema;
    use crate::sql::ast::Statement;
    use crate::sql::lexer::tokenize;
    use crate::sql::parser::parse;
    use crate::value::DataType;

    fn expr(sql: &str) -> Expr {
        match parse(&tokenize(&format!("VALUES ({sql})")).unwrap()).unwrap() {
            Statement::Values(mut values) => values.rows.remove(0).remove(0),
            other => panic!("not a VALUES statement: {other:?}"),
        }
    }

    fn columns() -> Vec<Column> {
        vec![
            Column {
                name: "ID".to_string(),
                data_type: DataType::Integer,
                nullable: true,
            },
            Column {
                name: "NAME".to_string(),
                data_type: DataType::Varchar(10),
                nullable: true,
            },
        ]
    }

    #[test]
    fn literals_become_constant_values() {
        assert_eq!(
            bind(&expr("1"), &Scope::default()),
            Ok(BoundExpr::Constant(Value::Integer(1)))
        );
        assert_eq!(
            bind(&expr("2147483648"), &Scope::default()),
            Ok(BoundExpr::Constant(Value::BigInt(2147483648)))
        );
        assert_eq!(
            bind(&expr("'a'"), &Scope::default()),
            Ok(BoundExpr::Constant(Value::Varchar("a".to_string())))
        );
        assert_eq!(
            bind(&expr("NULL"), &Scope::default()),
            Ok(BoundExpr::Constant(Value::Null))
        );
    }

    #[test]
    fn column_name_becomes_its_index() {
        assert_eq!(bind(&expr("name"), &scope()), Ok(BoundExpr::Column(1)));
    }

    #[test]
    fn columns_inside_an_expression_are_resolved() {
        assert_eq!(
            bind(&expr("id + 1"), &scope()),
            Ok(BoundExpr::Binary {
                op: BinaryOp::Arithmetic(ArithmeticOp::Add),
                left: Box::new(BoundExpr::Column(0)),
                right: Box::new(BoundExpr::Constant(Value::Integer(1)))
            })
        );
    }

    #[test]
    fn unknown_column_is_undefined() {
        assert_eq!(
            bind(&expr("-age"), &scope()),
            Err(BindError::UndefinedColumn {
                column: "AGE".to_string()
            })
        );
    }

    fn select(sql: &str) -> Select {
        match parse(&tokenize(sql).unwrap()).unwrap() {
            Statement::Select(select) => select,
            other => panic!("not a SELECT statement: {other:?}"),
        }
    }

    fn scope() -> Scope {
        Scope::table("USERS", &columns())
    }

    fn catalog() -> Catalog {
        let mut catalog = Catalog::default();
        catalog
            .create_table(TableSchema {
                name: "USERS".to_string(),
                columns: columns(),
                unique_constraints: vec![],
            })
            .unwrap();
        catalog
    }

    fn order_by_sources(sql: &str) -> Result<Vec<SortSource>, BindError> {
        let bound = bind_select(&select(sql), &catalog())?;
        Ok(bound.order_by.into_iter().map(|key| key.source).collect())
    }

    #[test]
    fn select_items_become_names_and_expressions() {
        let bound =
            bind_select(&select("SELECT *, id + 1 AS next FROM users"), &catalog()).unwrap();
        assert_eq!(bound.names, vec!["ID", "NAME", "NEXT"]);
        assert_eq!(bound.items.len(), 3);
        assert_eq!(bound.input_columns, vec!["USERS.ID", "USERS.NAME"]);
    }

    #[test]
    fn sort_key_prefers_an_output_name_to_a_table_column() {
        assert_eq!(
            order_by_sources("SELECT name AS id FROM users ORDER BY id"),
            Ok(vec![SortSource::Output(0)])
        );
    }

    #[test]
    fn sort_key_equal_to_a_select_item_uses_its_value() {
        assert_eq!(
            order_by_sources("SELECT id + 1 FROM users ORDER BY id + 1"),
            Ok(vec![SortSource::Output(0)])
        );
    }

    #[test]
    fn other_sort_keys_are_evaluated_on_the_table_row() {
        assert_eq!(
            order_by_sources("SELECT name FROM users ORDER BY id"),
            Ok(vec![SortSource::Input(BoundExpr::Column(0))])
        );
        assert_eq!(
            order_by_sources("SELECT DISTINCT name FROM users ORDER BY id"),
            Err(BindError::OrderByNotInSelectList)
        );
    }

    #[test]
    fn expressions_are_displayed_with_column_names_and_parentheses() {
        let names = vec!["ID".to_string(), "NAME".to_string()];
        let display = |sql: &str| bind(&expr(sql), &scope()).unwrap().display(&names);
        assert_eq!(display("id * 10 > 1"), "((ID * 10) > 1)");
        assert_eq!(display("NOT name IS NULL"), "(NOT (NAME IS NULL))");
        assert_eq!(display("-id <> 2147483648"), "((-ID) <> 2147483648)");
        assert_eq!(
            display("name || 'it''s' = TRUE"),
            "((NAME || 'it''s') = TRUE)"
        );
    }

    fn two_tables() -> Scope {
        let column = |name: &str| Column {
            name: name.to_string(),
            data_type: DataType::Integer,
            nullable: true,
        };
        Scope::table("E", &[column("ID"), column("DEPT")])
            .join(Scope::table("D", &[column("ID"), column("CODE")]))
    }

    #[test]
    fn unqualified_name_found_in_one_table_is_resolved() {
        assert_eq!(bind(&expr("code"), &two_tables()), Ok(BoundExpr::Column(3)));
    }

    #[test]
    fn unqualified_name_found_in_two_tables_is_ambiguous() {
        assert_eq!(
            bind(&expr("id"), &two_tables()),
            Err(BindError::AmbiguousColumn {
                column: "ID".to_string()
            })
        );
    }

    #[test]
    fn qualified_name_is_searched_in_its_table() {
        assert_eq!(bind(&expr("d.id"), &two_tables()), Ok(BoundExpr::Column(2)));
        assert_eq!(
            bind(&expr("e.code"), &two_tables()),
            Err(BindError::UndefinedQualifiedColumn {
                table: "E".to_string(),
                column: "CODE".to_string()
            })
        );
        assert_eq!(
            bind(&expr("x.id"), &two_tables()),
            Err(BindError::MissingFromEntry {
                table: "X".to_string()
            })
        );
    }

    #[test]
    fn join_scope_has_the_left_columns_then_the_right_columns() {
        let bound = bind_select(
            &select("SELECT * FROM users a JOIN users b ON a.id = b.id"),
            &catalog(),
        )
        .unwrap();
        assert_eq!(
            bound.input_columns,
            vec!["A.ID", "A.NAME", "B.ID", "B.NAME"]
        );
        assert_eq!(bound.names, vec!["ID", "NAME", "ID", "NAME"]);
        let BoundFrom::Join { condition, .. } = bound.from else {
            panic!("not a join");
        };
        assert_eq!(
            condition,
            Some(BoundExpr::Binary {
                op: BinaryOp::Comparison(ComparisonOp::Eq),
                left: Box::new(BoundExpr::Column(0)),
                right: Box::new(BoundExpr::Column(2))
            })
        );
    }

    #[test]
    fn same_table_name_twice_in_from_is_an_error() {
        assert_eq!(
            bind_select(&select("SELECT * FROM users, users"), &catalog()),
            Err(BindError::DuplicateTableName {
                table: "USERS".to_string()
            })
        );
        assert_eq!(
            bind_select(&select("SELECT * FROM users, t"), &catalog()),
            Err(BindError::UndefinedTable {
                table: "T".to_string()
            })
        );
    }

    fn aggregate_of(sql: &str) -> Result<BoundSelect, BindError> {
        bind_select(&select(sql), &catalog())
    }

    #[test]
    fn grouped_select_items_refer_to_the_aggregated_row() {
        let bound =
            aggregate_of("SELECT name, COUNT(*), COUNT(*) + 1 FROM users GROUP BY name").unwrap();
        let aggregate = bound.aggregate.unwrap();
        assert_eq!(aggregate.keys, vec![BoundExpr::Column(1)]);
        assert_eq!(
            aggregate.calls,
            vec![AggregateCall {
                func: AggregateFunc::Count,
                arg: None,
                distinct: false
            }]
        );
        assert_eq!(bound.items[0], BoundExpr::Column(0));
        assert_eq!(bound.items[1], BoundExpr::Column(1));
        assert_eq!(bound.input_columns, vec!["USERS.NAME", "COUNT(*)"]);
        assert_eq!(bound.names, vec!["NAME", "COUNT", "?column?"]);
    }

    #[test]
    fn aggregate_without_group_by_makes_an_aggregated_query() {
        let bound = aggregate_of("SELECT MAX(id) FROM users").unwrap();
        assert_eq!(bound.aggregate.unwrap().keys, vec![]);
        assert!(
            aggregate_of("SELECT id FROM users")
                .unwrap()
                .aggregate
                .is_none()
        );
    }

    #[test]
    fn column_outside_group_by_and_aggregates_is_an_error() {
        assert_eq!(
            aggregate_of("SELECT id, COUNT(*) FROM users"),
            Err(BindError::NotGrouped {
                column: "ID".to_string()
            })
        );
        assert_eq!(
            aggregate_of("SELECT * FROM users GROUP BY id"),
            Err(BindError::NotGrouped {
                column: "USERS.NAME".to_string()
            })
        );
    }

    #[test]
    fn having_and_order_by_are_resolved_on_the_aggregated_row() {
        let bound = aggregate_of(
            "SELECT name FROM users GROUP BY name HAVING SUM(id) > 1 ORDER BY MIN(id)",
        )
        .unwrap();
        let aggregate = bound.aggregate.unwrap();
        assert_eq!(aggregate.calls.len(), 2);
        assert!(aggregate.having.is_some());
        assert_eq!(
            bound.order_by[0].source,
            SortSource::Input(BoundExpr::Column(2))
        );
    }

    #[test]
    fn aggregates_are_not_allowed_in_where_or_inside_aggregates() {
        assert_eq!(
            aggregate_of("SELECT id FROM users WHERE COUNT(*) > 1"),
            Err(BindError::AggregateNotAllowed { clause: "WHERE" })
        );
        assert_eq!(
            aggregate_of("SELECT SUM(COUNT(*)) FROM users"),
            Err(BindError::NestedAggregate)
        );
    }
}
