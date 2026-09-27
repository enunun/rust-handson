//! 行(タプル)をバイト列に符号化し，元に戻す．
//!
//! ページに置くタプルは，8バイトのヘッダー(`TupleHeader`)のあとに，行の値のバイト列を並べる．
//! 行の値のバイト列は，`NULL`の列を表すビットマップのあとに，`NULL`でない列の値を列の順に並べる．
//! ビットマップは列ごとに1ビットで，`i`番目の列が`NULL`なら`i / 8`バイト目の`i % 8`ビットを1にする．
//! 値は，`INTEGER`を4バイト，`BIGINT`を8バイトのリトルエンディアン，`BOOLEAN`を1バイト，
//! `VARCHAR`を2バイトの長さとUTF-8のバイト列で表す．

use crate::catalog::TableSchema;
use crate::txn::TxnId;
use crate::value::{DataType, Row, Value};

/// タプルのヘッダーのバイト数．
pub const TUPLE_HEADER_SIZE: usize = 8;

/// タプルのヘッダー．その版を作ったトランザクション(`xmin`)と，削除したトランザクション(`xmax`)を持つ．
/// 削除されていなければ，`xmax`は`TxnId::INVALID`である．
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TupleHeader {
    pub xmin: TxnId,
    pub xmax: TxnId,
}

/// タプルを復号できないときのエラー．
#[derive(Debug, PartialEq)]
pub enum TupleError {
    Truncated,
    InvalidBoolean,
    InvalidUtf8,
}

impl TupleHeader {
    /// `xmin`のトランザクションが作った，削除されていない版のヘッダー．
    pub fn new(xmin: TxnId) -> TupleHeader {
        TupleHeader {
            xmin,
            xmax: TxnId::INVALID,
        }
    }

    /// `xmin`と`xmax`を，4バイトずつリトルエンディアンで並べる．
    pub fn encode(&self) -> [u8; TUPLE_HEADER_SIZE] {
        let mut bytes = [0; TUPLE_HEADER_SIZE];
        bytes[..4].copy_from_slice(&self.xmin.0.to_le_bytes());
        bytes[4..].copy_from_slice(&self.xmax.0.to_le_bytes());
        bytes
    }

    /// ページに置いたタプルを，ヘッダーと行の値のバイト列に分ける．
    pub fn split(bytes: &[u8]) -> Result<(TupleHeader, &[u8]), TupleError> {
        let (header, rest) = split(bytes, TUPLE_HEADER_SIZE)?;
        let xmin = u32::from_le_bytes(header[..4].try_into().expect("4 bytes"));
        let xmax = u32::from_le_bytes(header[4..].try_into().expect("4 bytes"));
        let header = TupleHeader {
            xmin: TxnId(xmin),
            xmax: TxnId(xmax),
        };
        Ok((header, rest))
    }
}

/// ヘッダーと行を，ページに置くタプルのバイト列にする．
pub fn encode_version(header: &TupleHeader, row: &[Value], schema: &TableSchema) -> Vec<u8> {
    let mut bytes = header.encode().to_vec();
    bytes.extend(encode_tuple(row, schema));
    bytes
}

/// 行をタプルのバイト列にする．値は，`Column::assign`で列の型に合わせたものとする．
pub fn encode_tuple(row: &[Value], schema: &TableSchema) -> Vec<u8> {
    let mut bytes = vec![0; bitmap_len(schema.columns.len())];
    for (index, value) in row.iter().enumerate() {
        match value {
            Value::Null => bytes[index / 8] |= 1 << (index % 8),
            Value::Integer(n) => bytes.extend_from_slice(&n.to_le_bytes()),
            Value::BigInt(n) => bytes.extend_from_slice(&n.to_le_bytes()),
            Value::Boolean(b) => bytes.push(u8::from(*b)),
            Value::Varchar(s) => {
                let len = u16::try_from(s.len()).expect("VARCHAR fits in a page");
                bytes.extend_from_slice(&len.to_le_bytes());
                bytes.extend_from_slice(s.as_bytes());
            }
        }
    }
    bytes
}

/// タプルのバイト列を，表の列の型に従って行に戻す．
pub fn decode_tuple(bytes: &[u8], schema: &TableSchema) -> Result<Row, TupleError> {
    let columns = &schema.columns;
    let (bitmap, mut rest) = split(bytes, bitmap_len(columns.len()))?;
    let mut row = Vec::with_capacity(columns.len());
    for (index, column) in columns.iter().enumerate() {
        if bitmap[index / 8] & (1 << (index % 8)) != 0 {
            row.push(Value::Null);
            continue;
        }
        let value = match column.data_type {
            DataType::Integer => {
                let (value, next) = split(rest, 4)?;
                rest = next;
                Value::Integer(i32::from_le_bytes(value.try_into().expect("4 bytes")))
            }
            DataType::BigInt => {
                let (value, next) = split(rest, 8)?;
                rest = next;
                Value::BigInt(i64::from_le_bytes(value.try_into().expect("8 bytes")))
            }
            DataType::Boolean => {
                let (value, next) = split(rest, 1)?;
                rest = next;
                match value[0] {
                    0 => Value::Boolean(false),
                    1 => Value::Boolean(true),
                    _ => return Err(TupleError::InvalidBoolean),
                }
            }
            DataType::Varchar(_) => {
                let (len, next) = split(rest, 2)?;
                let (text, next) = split(
                    next,
                    usize::from(u16::from_le_bytes(len.try_into().expect("2 bytes"))),
                )?;
                rest = next;
                let text = String::from_utf8(text.to_vec()).map_err(|_| TupleError::InvalidUtf8)?;
                Value::Varchar(text)
            }
        };
        row.push(value);
    }
    Ok(row)
}

/// 列の数から，ビットマップのバイト数を求める．
fn bitmap_len(columns: usize) -> usize {
    columns.div_ceil(8)
}

/// バイト列を，先頭の`len`バイトと残りに分ける．足りなければエラーを返す．
fn split(bytes: &[u8], len: usize) -> Result<(&[u8], &[u8]), TupleError> {
    if bytes.len() < len {
        return Err(TupleError::Truncated);
    }
    Ok(bytes.split_at(len))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Column;

    fn schema(types: &[DataType]) -> TableSchema {
        TableSchema {
            name: "T".to_string(),
            columns: types
                .iter()
                .enumerate()
                .map(|(index, data_type)| Column {
                    name: format!("C{index}"),
                    data_type: data_type.clone(),
                    nullable: true,
                })
                .collect(),
            unique_constraints: vec![],
        }
    }

    fn all_types() -> TableSchema {
        schema(&[
            DataType::Integer,
            DataType::BigInt,
            DataType::Boolean,
            DataType::Varchar(10),
        ])
    }

    #[test]
    fn values_are_laid_out_after_the_null_bitmap() {
        let row = vec![
            Value::Integer(1),
            Value::BigInt(-2),
            Value::Boolean(true),
            Value::Varchar("日本".to_string()),
        ];
        assert_eq!(
            encode_tuple(&row, &all_types()),
            vec![
                0b0000_0000,
                1,
                0,
                0,
                0,
                0xfe,
                0xff,
                0xff,
                0xff,
                0xff,
                0xff,
                0xff,
                0xff,
                1,
                6,
                0,
                0xe6,
                0x97,
                0xa5,
                0xe6,
                0x9c,
                0xac
            ]
        );
    }

    #[test]
    fn null_sets_a_bit_and_takes_no_bytes() {
        let row = vec![Value::Null, Value::BigInt(3), Value::Null, Value::Null];
        let bytes = encode_tuple(&row, &all_types());
        assert_eq!(bytes[0], 0b0000_1101);
        assert_eq!(bytes.len(), 1 + 8);
    }

    #[test]
    fn bitmap_has_one_byte_for_each_eight_columns() {
        let nine = schema(&vec![DataType::Boolean; 9]);
        let mut row = vec![Value::Boolean(false); 9];
        row[8] = Value::Null;
        let bytes = encode_tuple(&row, &nine);
        assert_eq!(&bytes[..2], &[0b0000_0000, 0b0000_0001]);
        assert_eq!(decode_tuple(&bytes, &nine), Ok(row));
    }

    #[test]
    fn decoding_an_encoded_row_returns_the_row() {
        let rows = [
            vec![
                Value::Integer(i32::MIN),
                Value::BigInt(i64::MAX),
                Value::Boolean(false),
                Value::Varchar(String::new()),
            ],
            vec![Value::Null, Value::Null, Value::Null, Value::Null],
        ];
        for row in rows {
            assert_eq!(
                decode_tuple(&encode_tuple(&row, &all_types()), &all_types()),
                Ok(row)
            );
        }
    }

    #[test]
    fn broken_tuples_are_errors() {
        let integer = schema(&[DataType::Integer]);
        assert_eq!(
            decode_tuple(&[0, 1, 2], &integer),
            Err(TupleError::Truncated)
        );
        assert_eq!(
            decode_tuple(&[0, 2], &schema(&[DataType::Boolean])),
            Err(TupleError::InvalidBoolean)
        );
        assert_eq!(
            decode_tuple(&[0, 1, 0, 0xff], &schema(&[DataType::Varchar(1)])),
            Err(TupleError::InvalidUtf8)
        );
    }

    #[test]
    fn header_holds_xmin_and_xmax_before_the_values() {
        let header = TupleHeader {
            xmin: TxnId(1),
            xmax: TxnId(258),
        };
        let bytes = encode_version(
            &header,
            &[Value::Boolean(true)],
            &schema(&[DataType::Boolean]),
        );
        assert_eq!(bytes, vec![1, 0, 0, 0, 2, 1, 0, 0, 0, 1]);
        assert_eq!(TupleHeader::split(&bytes), Ok((header, &bytes[8..])));
        assert_eq!(TupleHeader::new(TxnId(3)).xmax, TxnId::INVALID);
        assert_eq!(TupleHeader::split(&bytes[..7]), Err(TupleError::Truncated));
    }
}
