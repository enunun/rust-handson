use std::fmt;

/// SQLの値．`Null`は，型によらず値がないことを表す．
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Integer(i32),
    BigInt(i64),
    Boolean(bool),
    Varchar(String),
    Null,
}

impl Value {
    /// 3値の真理値から値を作る．`None`(不明)は`Null`になる．
    pub fn from_truth(truth: Option<bool>) -> Value {
        match truth {
            Some(b) => Value::Boolean(b),
            None => Value::Null,
        }
    }

    /// 値が`NULL`かどうかを返す．
    pub fn is_null(&self) -> bool {
        match self {
            Value::Null => true,
            Value::Integer(_) | Value::BigInt(_) | Value::Boolean(_) | Value::Varchar(_) => false,
        }
    }

    /// エラーのメッセージに使う，値の型の名前．
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Integer(_) => "integer",
            Value::BigInt(_) => "bigint",
            Value::Boolean(_) => "boolean",
            Value::Varchar(_) => "character varying",
            Value::Null => "unknown",
        }
    }
}

/// 列のデータ型．
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataType {
    Integer,
    BigInt,
    Boolean,
    Varchar(usize),
}

impl fmt::Display for DataType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataType::Integer => f.write_str("integer"),
            DataType::BigInt => f.write_str("bigint"),
            DataType::Boolean => f.write_str("boolean"),
            DataType::Varchar(length) => write!(f, "character varying({length})"),
        }
    }
}
