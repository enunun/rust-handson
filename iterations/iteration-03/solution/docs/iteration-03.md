# Iteration 3：文字列(解説)

演習の各手順の模範解答と，その考え方を説明する．

## 3-1 準備

引き継いだ66個のテストがすべて通れば準備は終わりである．

## 3-2 文法と概念

課題1〜3の解答例である．

```rust
pub fn initials(first: &str, last: &str) -> String {
    let mut result = String::new();
    result.push_str(&first[0..1]);
    result.push_str(&last[0..1]);
    result
}

pub fn longest(words: &Vec<String>) -> String {
    let mut best = String::new();
    for word in words {
        if word.chars().count() > best.chars().count() {
            best = word.clone();
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t() {
        assert_eq!(initials("Ada", "Lovelace"), "AL");
        let words = vec!["a".to_string(), "日本語".to_string(), "ab".to_string()];
        assert_eq!(longest(&words), "日本語");
        assert_eq!(words.len(), 3);
        assert_eq!("héllo".len(), 6);
        assert_eq!("héllo".chars().count(), 5);
    }
}
```

- `initials`の`&first[0..1]`はバイトの位置で切り出すので，先頭が日本語の名前ではパニックする．どんな名前でも動かすには，`chars()`で最初の文字を取り出す．
- `longest`は`&Vec<String>`を借りるだけなので，呼んだあとも`words`を使える．`best`に入れるときは，借りている要素を`clone`して所有する．
- `é`はUTF-8で2バイトなので，`len()`は6，`chars().count()`は5になる．
- 課題4の`twice`は，`take`が`a`の所有権を受け取るのでコンパイルエラーになる．`take(s: &str)`にして`take(&a)`と呼べば，`a`を貸すだけになり，そのあとも`a.len()`を呼べる．

## 3-3 テストリスト

模範解答は[TESTLIST.md](../TESTLIST.md)である．

- 字句解析は，文字列リテラルのいろいろな形を先に並べ，連結演算子，文字で数える位置の順にした．文字列リテラルの形は，通常の文字列，空の文字列，引用符を含む文字列，空白と日本語を含む文字列，閉じていない文字列の5つである．
- 文字で数える位置の項目には，エラーの前にマルチバイトの文字を含む入力が要る．Iteration 0の文法ではそのような入力を書けなかった．文字列リテラルによって書けるようになった．
- 構文解析は，連結の結合性と，隣り合う強さの演算子との組を並べた．
- 評価は，定数，連結，`NULL`との連結，型の不一致，比較の順にした．比較は，照合順序の性質(大文字と小文字，空の文字列，長さ，日本語)を1つの項目にまとめた．

## 3-4 設計ドキュメント

| ファイル | 変更 | 理由 |
| --- | --- | --- |
| `c4-component.md` | 変更なし | 新しいモジュールはなく，依存も変わらない |
| `code-types.md` | `Token::String`，`Token::Concat`，`Expr::String`，`BinaryOp::Concat`，`Value::Varchar`を加えた．どの型が文字列を所有するかを説明に書いた | トークン，構文木，値のそれぞれが文字列を所有する．構文解析器は借りたトークンから構文木を作るので，文字列を複製する |
| `code-sequence.md` | 優先順位に連結を加え，連結の評価を説明に書いた | 呼び出しの順序は変わらない |

## 3-5 テスト駆動の実装

### 文字列リテラル

```rust
#[test]
fn string_literal() {
    assert_eq!(tokenize("'abc'"), Ok(vec![Token::String("abc".to_string())]));
}
```

`Token`に`String(String)`を加え，文字列リテラルを読む`string`を`token`に加えた．
最初は，`none_of('\'')`で`'`以外の文字を集めるだけで通る．

```rust
fn token(input: &mut &str) -> winnow::Result<Token> {
    alt((keyword, integer, string, comparison, symbol)).parse_next(input)
}

fn string(input: &mut &str) -> winnow::Result<Token> {
    delimited('\'', repeat(0.., none_of('\'')), '\'')
        .map(Token::String)
        .parse_next(input)
}
```

空の文字列の項目は，`repeat(0.., ...)`が0回を許すので書いた時点で通る．

### `'it''s'`の`''`は，1つの引用符になる

```rust
#[test]
fn doubled_quote_is_one_quote() {
    assert_eq!(
        tokenize("'it''s'"),
        Ok(vec![Token::String("it's".to_string())])
    );
}
```

1文字を読む部分を`string_char`に分け，`''`を`'`として読む選択肢を先に置いた．

```rust
fn string(input: &mut &str) -> winnow::Result<Token> {
    delimited('\'', repeat(0.., string_char), '\'')
        .map(Token::String)
        .parse_next(input)
}

fn string_char(input: &mut &str) -> winnow::Result<char> {
    alt(("''".value('\''), none_of('\''))).parse_next(input)
}
```

空白と日本語を含む文字列，閉じていない文字列の項目は，書いた時点で通る．
閉じていない`'abc`では`string`が失敗して入力が`'`の前に戻り，`'`はどのトークンにも当てはまらないので，その位置で字句解析が止まる．

### 連結演算子のトークン

```rust
#[test]
fn concatenation_operator() {
    assert_eq!(
        tokenize("'a'||'b'"),
        Ok(vec![
            Token::String("a".to_string()),
            Token::Concat,
            Token::String("b".to_string())
        ])
    );
}
```

`Token::Concat`を加え，`symbol`に`"||".value(Token::Concat)`を加えた．

### エラーの位置を文字で数える

```rust
#[test]
fn position_counts_characters_not_bytes() {
    assert_eq!(tokenize("'あ' ?"), Err(LexError { position: 5 }));
}
```

`あ`は3バイトなので，バイトで数えると2つずれる．

```text
assertion `left == right` failed
  left: Err(LexError { position: 7 })
 right: Err(LexError { position: 5 })
```

`offset()`までの部分を切り出し，文字の数を数える．

```rust
        Err(error) => Err(LexError {
            position: sql[..error.offset()].chars().count() + 1,
        }),
```

### 文字列の定数と連結の構文

```rust
#[test]
fn concatenation_binds_tighter_than_comparison() {
    assert_eq!(
        parse_sql("VALUES ('a' || 'b' = 'ab')"),
        Ok(Values {
            exprs: vec![comparison(
                ComparisonOp::Eq,
                binary(BinaryOp::Concat, string("a"), string("b")),
                string("ab")
            )]
        })
    );
}
```

`Expr::String(String)`と`BinaryOp::Concat`を加えた．
`constant`は`&Token`を受け取るので，中の文字列は借りたものである．構文木に持たせる文字列は`clone`で複製する．

```rust
        Token::String(s) => Some(Expr::String(s.clone())),
```

連結の強さは7とした．比較(5)より強く，加減算(10)より弱い．結合性は左結合である．

```rust
const CONCAT: i64 = 7;

            literal(Token::Concat).value(Infix::Left(CONCAT, concat)),

fn concat(_: &mut Tokens<'_>, left: Expr, right: Expr) -> winnow::Result<Expr> {
    Ok(binary(BinaryOp::Concat, left, right))
}
```

### 文字列の値と連結の評価

```rust
#[test]
fn concatenation() {
    assert_eq!(eval_sql("'it''s' || ' ok'"), Ok(varchar("it's ok")));
}
```

`Value::Varchar(String)`を加えると，`Value`を網羅する`match`がコンパイルエラーになる．`is_null`，`eval_unary`，`truth`に`Varchar`の腕を加えた．
連結は，左辺と右辺の所有権を受け取る．左辺の`String`をそのまま使い，右辺を借りて末尾に足す．

```rust
fn concat(left: Value, right: Value) -> Result<Value, EvalError> {
    match (left, right) {
        (Value::Varchar(mut a), Value::Varchar(b)) => {
            a.push_str(&b);
            Ok(Value::Varchar(a))
        }
        (Value::Null, Value::Varchar(_) | Value::Null) | (Value::Varchar(_), Value::Null) => {
            Ok(Value::Null)
        }
        _ => Err(EvalError::DatatypeMismatch),
    }
}
```

`NULL`との連結と型の不一致の項目は，この`match`の2つ目と3つ目の腕で通る．

### 文字列の比較

```rust
#[test]
fn strings_compare_by_code_point() {
    assert_eq!(eval_sql("'abc' = 'abc'"), Ok(Value::Boolean(true)));
    assert_eq!(eval_sql("'a' < 'b'"), Ok(Value::Boolean(true)));
    assert_eq!(eval_sql("'B' < 'a'"), Ok(Value::Boolean(true)));
    assert_eq!(eval_sql("'' < 'a'"), Ok(Value::Boolean(true)));
    assert_eq!(eval_sql("'ab' < 'b'"), Ok(Value::Boolean(true)));
    assert_eq!(eval_sql("'z' < 'あ'"), Ok(Value::Boolean(true)));
}
```

`comparison`に文字列どうしの腕を加えた．`String`の`cmp`はUTF-8のバイト列を先頭から比べる．UTF-8では，バイト列の順とコードポイントの順が一致する．

```rust
        (Value::Varchar(a), Value::Varchar(b)) => a.cmp(&b),
```

残りの型の不一致の項目は，既存の`_`の腕で通る．

### 結合テスト

使用例と，文字で数える位置を`tests/values.rs`に加えた．どちらも書いた時点で通る．

```rust
#[test]
fn reports_a_lexical_error_position_in_characters() {
    assert_eq!(
        execute("VALUES ('日本語' ? 1)"),
        Err(Error::Lex(LexError { position: 15 }))
    );
}
```

## 3-6 振り返り

1. 字句解析の項目が，文字列リテラルの境界を網羅しているかを比べる．
2. `constant`が受け取るのは，`any`が`TokenSlice`から読んだ`&Token`である．`TokenSlice`はトークンの列を借りているので，中の`Token`の所有権を取り出せない．`clone`をなくすには，トークンの列そのものを所有して消費する入力を使う必要がある．構文解析で文字列を複製する費用は小さいので，借用のままにした．
3. `push_str`は`&str`を受け取る．`b`(`String`)をそのまま渡すと型の不一致のエラーになる．`&b`は`&String`で，`&str`として渡せる．
4. `&str`は借りた文字列なので，どこかにある文字列を指す．評価の結果の値は，元のSQLの文字列やトークンの列より長く使われるので，値が文字列を所有する必要がある．借りた文字列を値に持たせるには，ライフタイム(Iteration 15)で「何より長く生きないか」を型に書く必要がある．
5. 模範解答の設計ドキュメントは，照合スクリプトで問題がないことを確かめてある．

## 3-7 発展課題

解答例である．`string`を，1つ目の文字列と，改行をはさんで続く文字列の並びに分けて読み，つなぐ．

```rust
use winnow::ascii::{alpha1, digit1, line_ending, multispace0, space0};

fn string(input: &mut &str) -> winnow::Result<Token> {
    (string_part, repeat(0.., preceded(line_break, string_part)))
        .map(join_parts)
        .parse_next(input)
}

fn string_part(input: &mut &str) -> winnow::Result<String> {
    delimited('\'', repeat(0.., string_char), '\'').parse_next(input)
}

fn line_break(input: &mut &str) -> winnow::Result<()> {
    (space0, line_ending, multispace0).void().parse_next(input)
}

fn join_parts((first, rest): (String, Vec<String>)) -> Token {
    let mut text = first;
    for part in rest {
        text.push_str(&part);
    }
    Token::String(text)
}
```

```rust
#[test]
fn adjacent_strings_separated_by_a_newline_are_joined() {
    assert_eq!(
        tokenize("'abc'\n  'def'"),
        Ok(vec![Token::String("abcdef".to_string())])
    );
    assert_eq!(
        tokenize("'abc' 'def'"),
        Ok(vec![
            Token::String("abc".to_string()),
            Token::String("def".to_string())
        ])
    );
}
```

- `join_parts`の引数`(first, rest)`は，タプルを受け取って2つの変数に分解するパターンである．
- `for part in rest`は`rest`の所有権を受け取り，各要素の`String`を順に取り出す．
- `line_break`は，行末までの空白(`space0`)，改行(`line_ending`)，次の行の先頭の空白(`multispace0`)を読む．改行のない空白だけでは`line_ending`が失敗する．
