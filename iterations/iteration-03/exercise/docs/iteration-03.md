# Iteration 3：文字列

このIterationでは，文字列リテラル，連結`||`，文字列の比較を扱い，エラーの位置を文字の数で数える．
文字列の値を通して，Rustの所有権と借用，`String`と`&str`の違いを学ぶ．

## 3-1 準備

`cargo test`を実行し，引き継いだテストがすべて通ることを確かめる．次は結果の要約である．

```text
     Running unittests src/lib.rs (target/debug/deps/ferrodb-fcec4bc47a054b29)
test result: ok. 58 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/tokenize.rs (target/debug/deps/tokenize-8d7c29559361cdcd)
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
     Running tests/values.rs (target/debug/deps/values-920a463a58df66c3)
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

## 3-2 文法と概念

次の2つのノートを読む．

- [Rustのノート](../../../../docs/rust/iteration-03.md)：所有権とムーブ，`Clone`，借用の規則，`String`と`&str`，UTF-8と文字の数
- [データベースのノート](../../../../docs/db/iteration-03.md)：文字列リテラル，連結，照合順序，エラーの位置

読み終えたら，一時的なモジュールにテストを書いて，次の課題を確かめる．

1. 名と姓(`&str`)を受け取り，それぞれの先頭の1文字をつないだ`String`を返す関数`initials`を書く．`initials("Ada", "Lovelace")`は`"AL"`になる．
2. `&Vec<String>`を受け取り，文字の数が最も多い要素の複製を返す関数`longest`を書く．呼んだあとも，元の`Vec`を使えることを確かめる．
3. `"héllo"`の`len()`と`chars().count()`の値を確かめ，違う理由を説明する．
4. ノートの`twice`関数をそのまま書いてコンパイルし，エラーを読む．`take`の引数を`&str`に変えて直す．

## 3-3 テストリスト

### 要件

- 文字列リテラル`'abc'`を扱う．`'it''s'`のように，引用符を2つ重ねると1つの引用符になる．
- 文字列の連結`||`と，文字列どうしの比較を扱う．
- 文字列の比較は，Unicodeのコードポイント順とする．
- 文字列に日本語などが含まれても，エラーの位置はバイトではなく文字の番号で数える．

### 使用例

```rust
let result = ferrodb::execute("VALUES ('it''s' || ' ok', 'a' < 'b')").unwrap();
assert_eq!(result.rows, vec![vec![Value::Varchar("it's ok".to_string()), Value::Boolean(true)]]);
```

### 作るもの

| モジュール | 作るもの |
| --- | --- |
| `token` | `Token::String(String)`，`Token::Concat` |
| `lexer` | 文字列リテラルを読む．エラーの位置を文字の数で数える |
| `ast` | `Expr::String(String)`，`BinaryOp::Concat` |
| `value` | `Value::Varchar(String)` |

### 書くときに考えること

- 文字列リテラルの境界を考える．空の文字列，引用符を含む文字列，閉じていない文字列はどうなるか．
- 閉じていない文字列のエラーは，どの位置を指すべきか．
- エラーの位置をバイトで数えるか文字で数えるかによって結果が変わるのは，どんな入力か．
- `||`の優先順位を，隣り合う演算子(加減算と比較)との組で確かめる．
- 文字列の比較で，大文字と小文字，長さの違い，日本語の順序はどうなるか．

## 3-4 設計ドキュメント

- `c4-component.md`：新しいモジュールはない．依存が変わるかを確かめる．
- `code-types.md`：文字列を持つ列挙子を加える．どの型が文字列を所有するか．
- `code-sequence.md`：流れは変わらない．図の下の説明に，連結の優先順位と評価の規則を書く．

## 3-5 テスト駆動の実装

### 実装のヒント

- 文字列リテラルは，`delimited('\'', repeat(0.., 1文字を読むパーサー), '\'')`で読める．`repeat`で集めた`char`は`String`として受け取れる．
- 1文字を読むパーサーは，`''`を`'`として読むか，`none_of('\'')`で`'`以外の1文字を読む．
- 閉じていない文字列では，文字列のパーサーが失敗して入力が開きの引用符の前に戻る．そのあとどうなるかを考える．
- エラーの位置は，`offset()`までの部分`&sql[..offset]`の文字を`chars().count()`で数える．
- 構文解析器は`&Token`から構文木を作る．トークンの`String`を構文木に持たせるには`clone`する．
- 連結は，左辺の`String`の所有権を受け取り，`push_str`で右辺を足すと，新しい`String`を作らずに済む．`match`のパターンの変数に`mut`を付ける．
- `Value`に列挙子を加えると，コンパイラーが直す場所(`is_null`，`eval_unary`，`truth`など)を教えてくれる．

### ツールの操作

- 1つのテストだけを，名前を指定して実行する．
- コンパイルエラーの`help:`の行は，直し方の候補を示す．所有権のエラーでは，参照に変える方法や`clone`する方法が提案される．

## 3-6 振り返り

1. 自分の`TESTLIST.md`を[模範解答のテストリスト](../../solution/TESTLIST.md)と比べる．
2. `constant`は`&Token`を受け取り，文字列を`clone`する．`Token`の所有権を受け取る設計にすると，`clone`をなくせるか．winnowの`any`が返す値との関係を考える．
3. 連結で`a.push_str(&b)`の`&b`を`b`にすると，どうなるか．`push_str`の引数の型から考える．
4. `Value::Varchar(String)`を`Value::Varchar(&str)`にできない理由は何か．
5. 設計ドキュメントと実装を見比べ，違うところがあれば設計ドキュメントを直す．

## 3-7 発展課題

標準SQLでは，改行を含む区切りだけをはさんで隣り合う文字列リテラルは，1つの文字列リテラルになる．

```sql
VALUES ('abc'
        'def')   -- abcdef
```

`tokenize`を，この規則に従って文字列をつなぐように変える．改行を含まない空白だけで区切られた`'abc' 'def'`は，2つの文字列のトークンのままである．
これまでと同じく，テストリスト，設計ドキュメント，実装の順に進める．
