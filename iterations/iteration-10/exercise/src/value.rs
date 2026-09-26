use std::cmp::Ordering;
use std::fmt;

/// 表の1行．列の順に値を並べる．
pub type Row = Vec<Value>;

/// SQLの値．`Null`は，型によらず値がないことを表す．
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

/// 並べ替えに使う値の順序．`NULL`はどの値よりも大きい．
/// `INTEGER`と`BIGINT`は数として比べ，数が等しければ`INTEGER`を先にする．
/// 型の違う値は，整数，真偽値，文字列，`NULL`の順に並べる．
impl Ord for Value {
    fn cmp(&self, other: &Value) -> Ordering {
        match (self, other) {
            (Value::Integer(a), Value::Integer(b)) => a.cmp(b),
            (Value::BigInt(a), Value::BigInt(b)) => a.cmp(b),
            (Value::Integer(a), Value::BigInt(b)) => i64::from(*a).cmp(b).then(Ordering::Less),
            (Value::BigInt(a), Value::Integer(b)) => a.cmp(&i64::from(*b)).then(Ordering::Greater),
            (Value::Boolean(a), Value::Boolean(b)) => a.cmp(b),
            (Value::Varchar(a), Value::Varchar(b)) => a.cmp(b),
            _ => self.type_rank().cmp(&other.type_rank()),
        }
    }
}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Value) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Value {
    /// 型の違う値を並べるときの，型の順番．
    fn type_rank(&self) -> u8 {
        match self {
            Value::Integer(_) | Value::BigInt(_) => 0,
            Value::Boolean(_) => 1,
            Value::Varchar(_) => 2,
            Value::Null => 3,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_of_the_same_type_are_ordered_naturally() {
        assert!(Value::Integer(1) < Value::Integer(2));
        assert!(Value::Boolean(false) < Value::Boolean(true));
        assert!(Value::Varchar("a".to_string()) < Value::Varchar("b".to_string()));
    }

    #[test]
    fn null_is_greater_than_any_value() {
        assert!(Value::Integer(i32::MAX) < Value::Null);
        assert!(Value::Varchar("z".to_string()) < Value::Null);
        assert_eq!(Value::Null.cmp(&Value::Null), Ordering::Equal);
    }

    #[test]
    fn integer_and_bigint_are_compared_as_numbers() {
        assert!(Value::Integer(2) < Value::BigInt(10));
        assert!(Value::BigInt(-1) < Value::Integer(0));
        assert!(Value::Integer(1) < Value::BigInt(1));
    }
}
