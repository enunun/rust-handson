//! インデックスのキーを，バイト列の比較で順序が保たれるように符号化する．

/// インデックスのキーにできる型．`encode`したバイト列を辞書式に比べた順序は，
/// 元の値の順序と同じになる．
pub trait IndexKey: Sized {
    /// 順序を保つバイト列にする．
    fn encode(&self) -> Vec<u8>;
    /// `encode`したバイト列から値に戻す．バイト列が正しくなければ`None`を返す．
    fn decode(bytes: &[u8]) -> Option<Self>;
}

/// 符号ビットを反転してビッグエンディアンで書く．負の数は0x00から，正の数は0x80から始まる．
impl IndexKey for i32 {
    fn encode(&self) -> Vec<u8> {
        (self.cast_unsigned() ^ 0x8000_0000).to_be_bytes().to_vec()
    }

    fn decode(bytes: &[u8]) -> Option<i32> {
        let bytes: [u8; 4] = bytes.try_into().ok()?;
        Some((u32::from_be_bytes(bytes) ^ 0x8000_0000).cast_signed())
    }
}

impl IndexKey for i64 {
    fn encode(&self) -> Vec<u8> {
        (self.cast_unsigned() ^ 0x8000_0000_0000_0000)
            .to_be_bytes()
            .to_vec()
    }

    fn decode(bytes: &[u8]) -> Option<i64> {
        let bytes: [u8; 8] = bytes.try_into().ok()?;
        Some((u64::from_be_bytes(bytes) ^ 0x8000_0000_0000_0000).cast_signed())
    }
}

/// 偽を`0`，真を`1`の1バイトで書く．
impl IndexKey for bool {
    fn encode(&self) -> Vec<u8> {
        vec![u8::from(*self)]
    }

    fn decode(bytes: &[u8]) -> Option<bool> {
        match bytes {
            [0] => Some(false),
            [1] => Some(true),
            _ => None,
        }
    }
}

/// UTF-8のバイト列をそのまま使う．UTF-8のバイト列の順序は，文字の番号の順序と同じである．
impl IndexKey for String {
    fn encode(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }

    fn decode(bytes: &[u8]) -> Option<String> {
        String::from_utf8(bytes.to_vec()).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 並べた値を符号化したバイト列も，同じ順に並ぶことを確かめる．
    fn assert_order_kept<K: IndexKey + std::fmt::Debug + PartialEq>(sorted: &[K]) {
        for pair in sorted.windows(2) {
            assert!(
                pair[0].encode() < pair[1].encode(),
                "{:?} < {:?}",
                pair[0],
                pair[1]
            );
        }
        for value in sorted {
            assert_eq!(K::decode(&value.encode()).as_ref(), Some(value));
        }
    }

    #[test]
    fn integers_keep_their_order() {
        assert_order_kept(&[i32::MIN, -256, -1, 0, 1, 255, 256, i32::MAX]);
        assert_order_kept(&[i64::MIN, -1, 0, 1, 1 << 40, i64::MAX]);
    }

    #[test]
    fn integer_is_big_endian_with_the_sign_bit_flipped() {
        assert_eq!(1i32.encode(), vec![0x80, 0, 0, 1]);
        assert_eq!((-1i32).encode(), vec![0x7f, 0xff, 0xff, 0xff]);
    }

    #[test]
    fn booleans_keep_their_order() {
        assert_order_kept(&[false, true]);
        assert_eq!(bool::decode(&[2]), None);
    }

    #[test]
    fn strings_keep_their_order() {
        let sorted: Vec<String> = ["", "a", "ab", "b", "z", "é", "日本"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_order_kept(&sorted);
        assert_eq!(String::decode(&[0xff]), None);
    }

    #[test]
    fn keys_of_the_wrong_length_are_rejected() {
        assert_eq!(i32::decode(&[0, 0, 0]), None);
        assert_eq!(i64::decode(&[0; 4]), None);
    }
}
