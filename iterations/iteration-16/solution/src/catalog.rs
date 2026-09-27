use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

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
    pub unique_constraints: Vec<UniqueConstraint>,
}

/// 列の定義．`nullable`が偽なら，列に`NULL`を入れられない(`NOT NULL`)．
#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub name: String,
    pub data_type: DataType,
    pub nullable: bool,
}

/// 一意性制約．`column`の列の値は，`NULL`を除いて表の中で重ならない．
#[derive(Debug, Clone, PartialEq)]
pub struct UniqueConstraint {
    pub name: String,
    pub column: usize,
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
    MultiplePrimaryKeys {
        table: String,
    },
    UndefinedTableToDrop {
        table: String,
    },
    DuplicateAssignment {
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

    /// 表の定義を消す．
    pub fn drop_table(&mut self, name: &str) -> Result<(), SchemaError> {
        match self.tables.remove(name) {
            Some(_) => Ok(()),
            None => Err(SchemaError::UndefinedTableToDrop {
                table: name.to_string(),
            }),
        }
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

/// カタログのファイルの形式．表の定義を，表の名前の順に並べる．
/// 数値はリトルエンディアンで，文字列は2バイトのバイト数とUTF-8のバイト列で書く．
impl Catalog {
    /// 表の名前を，名前の順に返す．
    pub fn table_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.tables.keys().cloned().collect();
        names.sort();
        names
    }

    /// 表の定義をファイルに書く．別の名前のファイルに書いてから名前を変えるので，
    /// 書いている途中で止まっても，前の内容のファイルが残る．
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let temporary = path.with_extension("tmp");
        fs::write(&temporary, self.encode())?;
        fs::rename(&temporary, path)
    }

    /// ファイルから表の定義を読む．ファイルがなければ，表のないカタログを返す．
    pub fn load(path: &Path) -> io::Result<Catalog> {
        match fs::read(path) {
            Ok(bytes) => Catalog::decode(&bytes),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Catalog::default()),
            Err(error) => Err(error),
        }
    }

    fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        let names = self.table_names();
        write_u32(&mut bytes, names.len());
        for name in names {
            let schema = &self.tables[&name];
            write_string(&mut bytes, &schema.name);
            write_u16(&mut bytes, schema.columns.len());
            for column in &schema.columns {
                write_string(&mut bytes, &column.name);
                let (tag, length) = match column.data_type {
                    DataType::Integer => (0, 0),
                    DataType::BigInt => (1, 0),
                    DataType::Boolean => (2, 0),
                    DataType::Varchar(length) => (3, length),
                };
                bytes.push(tag);
                write_u32(&mut bytes, length);
                bytes.push(u8::from(column.nullable));
            }
            write_u16(&mut bytes, schema.unique_constraints.len());
            for constraint in &schema.unique_constraints {
                write_string(&mut bytes, &constraint.name);
                write_u16(&mut bytes, constraint.column);
            }
        }
        bytes
    }

    fn decode(mut bytes: &[u8]) -> io::Result<Catalog> {
        let input = &mut bytes;
        let mut catalog = Catalog::default();
        for _ in 0..read_u32(input)? {
            let name = read_string(input)?;
            let mut columns = Vec::new();
            for _ in 0..read_u16(input)? {
                let column_name = read_string(input)?;
                let tag = take(input, 1)?[0];
                let length = read_u32(input)?;
                let data_type = match tag {
                    0 => DataType::Integer,
                    1 => DataType::BigInt,
                    2 => DataType::Boolean,
                    3 => DataType::Varchar(length),
                    _ => return Err(corrupted()),
                };
                let nullable = take(input, 1)?[0] != 0;
                columns.push(Column {
                    name: column_name,
                    data_type,
                    nullable,
                });
            }
            let mut unique_constraints = Vec::new();
            for _ in 0..read_u16(input)? {
                unique_constraints.push(UniqueConstraint {
                    name: read_string(input)?,
                    column: read_u16(input)?,
                });
            }
            catalog.tables.insert(
                name.clone(),
                TableSchema {
                    name,
                    columns,
                    unique_constraints,
                },
            );
        }
        Ok(catalog)
    }
}

fn write_u16(bytes: &mut Vec<u8>, value: usize) {
    let value = u16::try_from(value).expect("value fits in u16");
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn write_u32(bytes: &mut Vec<u8>, value: usize) {
    let value = u32::try_from(value).expect("value fits in u32");
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn write_string(bytes: &mut Vec<u8>, text: &str) {
    write_u16(bytes, text.len());
    bytes.extend_from_slice(text.as_bytes());
}

/// 先頭の`len`バイトを取り出し，`input`を残りのバイト列にする．
fn take(input: &mut &[u8], len: usize) -> io::Result<Vec<u8>> {
    if input.len() < len {
        return Err(corrupted());
    }
    let (head, rest) = input.split_at(len);
    let head = head.to_vec();
    *input = rest;
    Ok(head)
}

fn read_u16(input: &mut &[u8]) -> io::Result<usize> {
    let bytes: [u8; 2] = take(input, 2)?.try_into().expect("2 bytes");
    Ok(usize::from(u16::from_le_bytes(bytes)))
}

fn read_u32(input: &mut &[u8]) -> io::Result<usize> {
    let bytes: [u8; 4] = take(input, 4)?.try_into().expect("4 bytes");
    let value = u32::from_le_bytes(bytes);
    Ok(usize::try_from(value).expect("u32 fits in usize"))
}

fn read_string(input: &mut &[u8]) -> io::Result<String> {
    let len = read_u16(input)?;
    String::from_utf8(take(input, len)?).map_err(|_| corrupted())
}

fn corrupted() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid catalog file")
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
            nullable: true,
        }
    }

    fn users() -> TableSchema {
        TableSchema {
            name: "USERS".to_string(),
            columns: vec![
                column("ID", DataType::Integer),
                column("NAME", DataType::Varchar(5)),
            ],
            unique_constraints: vec![],
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
    fn dropped_table_can_no_longer_be_found() {
        let mut catalog = Catalog::default();
        catalog.create_table(users()).unwrap();
        assert_eq!(catalog.drop_table("USERS"), Ok(()));
        assert_eq!(
            catalog.table("USERS"),
            Err(SchemaError::UndefinedTable {
                table: "USERS".to_string()
            })
        );
    }

    #[test]
    fn dropping_an_unknown_table_is_an_error() {
        let mut catalog = Catalog::default();
        assert_eq!(
            catalog.drop_table("USERS"),
            Err(SchemaError::UndefinedTableToDrop {
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
            unique_constraints: vec![],
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

    fn constrained() -> TableSchema {
        TableSchema {
            name: "日本".to_string(),
            columns: vec![
                Column {
                    name: "ID".to_string(),
                    data_type: DataType::BigInt,
                    nullable: false,
                },
                column("FLAG", DataType::Boolean),
            ],
            unique_constraints: vec![UniqueConstraint {
                name: "日本_PKEY".to_string(),
                column: 0,
            }],
        }
    }

    #[test]
    fn saved_catalog_is_loaded_with_the_same_tables() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("catalog");
        let mut catalog = Catalog::default();
        catalog.create_table(users()).unwrap();
        catalog.create_table(constrained()).unwrap();
        catalog.save(&path).unwrap();
        let loaded = Catalog::load(&path).unwrap();
        assert_eq!(loaded.table_names(), vec!["USERS", "日本"]);
        assert_eq!(loaded.table("USERS"), Ok(&users()));
        assert_eq!(loaded.table("日本"), Ok(&constrained()));
    }

    #[test]
    fn missing_catalog_file_is_an_empty_catalog() {
        let dir = tempfile::tempdir().unwrap();
        let catalog = Catalog::load(&dir.path().join("catalog")).unwrap();
        assert_eq!(catalog.table_names(), Vec::<String>::new());
    }

    #[test]
    fn catalog_file_lists_tables_in_name_order() {
        let mut catalog = Catalog::default();
        catalog.create_table(users()).unwrap();
        let bytes = catalog.encode();
        assert_eq!(&bytes[..4], &[1, 0, 0, 0]);
        assert_eq!(&bytes[4..11], &[5, 0, b'U', b'S', b'E', b'R', b'S']);
        assert_eq!(&bytes[11..13], &[2, 0]);
    }

    #[test]
    fn broken_catalog_file_is_invalid_data() {
        let mut catalog = Catalog::default();
        catalog.create_table(users()).unwrap();
        let bytes = catalog.encode();
        let error = Catalog::decode(&bytes[..bytes.len() - 1]).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }
}
