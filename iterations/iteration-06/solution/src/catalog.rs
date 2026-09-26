use std::collections::HashMap;

use crate::value::{DataType, Value};

/// 表の定義を名前で引く．
#[derive(Debug, Default)]
pub struct Catalog {
    tables: HashMap<String, TableSchema>,
}

/// 表の定義．
#[derive(Debug, Clone, PartialEq)]
pub struct TableSchema {
    pub name: String,
    pub columns: Vec<Column>,
}

/// 列の定義．
#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub name: String,
    pub data_type: DataType,
}

/// 文を表の定義と照らし合わせたときのエラー．
#[derive(Debug, PartialEq)]
pub enum SchemaError {
    UndefinedTable {
        table: String,
    },
    DuplicateTable {
        table: String,
    },
    UndefinedColumn {
        table: String,
        column: String,
    },
    DuplicateColumn {
        column: String,
    },
    TypeMismatch {
        column: String,
        expected: DataType,
        found: &'static str,
    },
    ValueTooLong {
        data_type: DataType,
    },
    OutOfRange,
    MoreValuesThanColumns,
    MoreColumnsThanValues,
}

impl Catalog {
    /// 表の定義を加える．同じ名前の表や，表の中で同じ名前の列があればエラーを返す．
    pub fn create_table(&mut self, schema: TableSchema) -> Result<(), SchemaError> {
        if self.tables.contains_key(&schema.name) {
            return Err(SchemaError::DuplicateTable { table: schema.name });
        }
        for (index, column) in schema.columns.iter().enumerate() {
            for earlier in &schema.columns[..index] {
                if earlier.name == column.name {
                    return Err(SchemaError::DuplicateColumn {
                        column: column.name.clone(),
                    });
                }
            }
        }
        self.tables.insert(schema.name.clone(), schema);
        Ok(())
    }

    /// 名前で表の定義を引く．
    pub fn table(&self, name: &str) -> Result<&TableSchema, SchemaError> {
        match self.tables.get(name) {
            Some(schema) => Ok(schema),
            None => Err(SchemaError::UndefinedTable {
                table: name.to_string(),
            }),
        }
    }
}

impl TableSchema {
    /// 列の名前から，その列の番号を返す．
    pub fn column_index(&self, name: &str) -> Result<usize, SchemaError> {
        for (index, column) in self.columns.iter().enumerate() {
            if column.name == name {
                return Ok(index);
            }
        }
        Err(SchemaError::UndefinedColumn {
            table: self.name.clone(),
            column: name.to_string(),
        })
    }
}

impl Column {
    /// 値を，この列の型に合わせて格納できる形にする．
    /// `INTEGER`と`BIGINT`は範囲に収まれば互いに変換し，`VARCHAR(n)`は文字の数を調べる．
    pub fn assign(&self, value: Value) -> Result<Value, SchemaError> {
        match (&self.data_type, value) {
            (_, Value::Null) => Ok(Value::Null),
            (DataType::Integer, Value::Integer(n)) => Ok(Value::Integer(n)),
            (DataType::Integer, Value::BigInt(n)) => match i32::try_from(n) {
                Ok(n) => Ok(Value::Integer(n)),
                Err(_) => Err(SchemaError::OutOfRange),
            },
            (DataType::BigInt, Value::Integer(n)) => Ok(Value::BigInt(i64::from(n))),
            (DataType::BigInt, Value::BigInt(n)) => Ok(Value::BigInt(n)),
            (DataType::Boolean, Value::Boolean(b)) => Ok(Value::Boolean(b)),
            (DataType::Varchar(length), Value::Varchar(s)) => {
                if s.chars().count() > *length {
                    Err(SchemaError::ValueTooLong {
                        data_type: self.data_type.clone(),
                    })
                } else {
                    Ok(Value::Varchar(s))
                }
            }
            (expected, value) => Err(SchemaError::TypeMismatch {
                column: self.name.clone(),
                expected: expected.clone(),
                found: value.type_name(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column(name: &str, data_type: DataType) -> Column {
        Column {
            name: name.to_string(),
            data_type,
        }
    }

    fn users() -> TableSchema {
        TableSchema {
            name: "USERS".to_string(),
            columns: vec![
                column("ID", DataType::Integer),
                column("NAME", DataType::Varchar(5)),
            ],
        }
    }

    #[test]
    fn created_table_can_be_found_by_name() {
        let mut catalog = Catalog::default();
        catalog.create_table(users()).unwrap();
        assert_eq!(catalog.table("USERS"), Ok(&users()));
    }

    #[test]
    fn unknown_table_is_undefined() {
        let catalog = Catalog::default();
        assert_eq!(
            catalog.table("USERS"),
            Err(SchemaError::UndefinedTable {
                table: "USERS".to_string()
            })
        );
    }

    #[test]
    fn creating_the_same_table_twice_is_an_error() {
        let mut catalog = Catalog::default();
        catalog.create_table(users()).unwrap();
        assert_eq!(
            catalog.create_table(users()),
            Err(SchemaError::DuplicateTable {
                table: "USERS".to_string()
            })
        );
    }

    #[test]
    fn column_names_must_be_unique_in_a_table() {
        let mut catalog = Catalog::default();
        let schema = TableSchema {
            name: "T".to_string(),
            columns: vec![
                column("A", DataType::Integer),
                column("A", DataType::Boolean),
            ],
        };
        assert_eq!(
            catalog.create_table(schema),
            Err(SchemaError::DuplicateColumn {
                column: "A".to_string()
            })
        );
    }

    #[test]
    fn column_index_by_name() {
        assert_eq!(users().column_index("NAME"), Ok(1));
        assert_eq!(
            users().column_index("AGE"),
            Err(SchemaError::UndefinedColumn {
                table: "USERS".to_string(),
                column: "AGE".to_string()
            })
        );
    }

    #[test]
    fn null_can_be_assigned_to_any_column() {
        assert_eq!(
            column("A", DataType::Boolean).assign(Value::Null),
            Ok(Value::Null)
        );
    }

    #[test]
    fn integers_are_converted_to_the_column_type() {
        assert_eq!(
            column("A", DataType::BigInt).assign(Value::Integer(1)),
            Ok(Value::BigInt(1))
        );
        assert_eq!(
            column("A", DataType::Integer).assign(Value::BigInt(1)),
            Ok(Value::Integer(1))
        );
        assert_eq!(
            column("A", DataType::Integer).assign(Value::BigInt(2147483648)),
            Err(SchemaError::OutOfRange)
        );
    }

    #[test]
    fn varchar_length_is_counted_in_characters() {
        let name = column("NAME", DataType::Varchar(3));
        assert_eq!(
            name.assign(Value::Varchar("日本語".to_string())),
            Ok(Value::Varchar("日本語".to_string()))
        );
        assert_eq!(
            name.assign(Value::Varchar("abcd".to_string())),
            Err(SchemaError::ValueTooLong {
                data_type: DataType::Varchar(3)
            })
        );
    }

    #[test]
    fn value_of_another_type_is_a_type_mismatch() {
        assert_eq!(
            column("ID", DataType::Integer).assign(Value::Boolean(true)),
            Err(SchemaError::TypeMismatch {
                column: "ID".to_string(),
                expected: DataType::Integer,
                found: "boolean"
            })
        );
    }
}
