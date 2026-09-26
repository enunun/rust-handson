/// SQLの値．`Null`は，型によらず値がないことを表す．
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Integer(i32),
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
            Value::Integer(_) | Value::Boolean(_) | Value::Varchar(_) => false,
        }
    }
}
