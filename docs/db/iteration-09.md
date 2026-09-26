# Iteration 9：並べ替え，件数の制限，重複の除去

## `SELECT`の処理の順序

Iteration 7で説明した順序に，`DISTINCT`，`ORDER BY`，`OFFSET`と`FETCH FIRST`が加わる．

```sql
SELECT DISTINCT name, age   -- 3. 選択項目を計算する  4. 重複を除く
FROM users                  -- 1. 表の行を読む
WHERE age >= 20             -- 2. 条件で行を絞り込む
ORDER BY age DESC           -- 5. 並べ替える
OFFSET 1 ROWS               -- 6. 先頭の行を飛ばす
FETCH FIRST 2 ROWS ONLY;    -- 7. 返す行の数を制限する
```

`ORDER BY`は選択項目の計算より後なので，選択項目の別名で並べ替えられる．
`OFFSET`と`FETCH FIRST`は並べ替えた後なので，「上位の何件」を取り出せる．

## `ORDER BY`

```sql
ORDER BY age DESC, name ASC NULLS FIRST
```

- キーを`,`で区切って並べる．前のキーが等しい行だけを，次のキーで並べる．
- `ASC`は昇順，`DESC`は降順で，省くと昇順である．
- キーには式を書ける．選択していない列でも並べ替えられる．

### 名前の解決

`ORDER BY`の名前だけのキーは，まず結果の列名(別名を含む)から探し，なければ表の列名として探す．

```sql
SELECT id, 0 - id AS rev FROM users ORDER BY rev;
```

`rev`は表の列ではないが，結果の列名なので並べ替えのキーになる．

`WHERE`は選択項目より前に処理するので，`WHERE`では別名を使えない．
`ORDER BY`では使える．この違いは，処理の順序から来ている．

### 並べ替えの安定性

すべてのキーが等しい行をどの順に返すかは，SQLでは決まっていない．
順序を確実に決めたいなら，主キーのように重ならない列を最後のキーに加える(`ORDER BY age, id`)．
`ferrodb`は安定な並べ替えを使うので，キーが等しい行は表の行の順に並ぶ．

## `NULL`の並び順

`NULL`を並べ替えたとき，ほかの値の前に置くか後に置くかは，標準SQLでは実装が決める．
PostgreSQLと`ferrodb`は，`NULL`をどの値よりも大きい値として扱う．

| 指定 | `NULL`の位置 |
| --- | --- |
| `ASC` | 最後 |
| `DESC` | 最初 |
| `NULLS FIRST` | 向きによらず最初 |
| `NULLS LAST` | 向きによらず最後 |

`WHERE`の比較では`NULL`は`UNKNOWN`を返すが，並べ替えでは`NULL`にも位置を決める．並べ替えは3値論理を使わない．

## `OFFSET`と`FETCH FIRST`

```sql
OFFSET 1 ROWS FETCH FIRST 2 ROWS ONLY
```

- `OFFSET n ROWS`は先頭の`n`行を飛ばし，`FETCH FIRST n ROWS ONLY`は`n`行までを返す．
- 行の数より大きい`OFFSET`や，`FETCH FIRST 0 ROWS ONLY`は，0行を返す．
- 並べ替えずに使うと，どの行が返るかは決まらない．ページ分けには`ORDER BY`を組み合わせる．

PostgreSQLやMySQLの`LIMIT n OFFSET m`は，標準SQLにない構文である．
標準SQLでは`OFFSET`と`FETCH FIRST`を使う．

## `DISTINCT`

`SELECT DISTINCT`は，選択項目の値がすべて等しい行を1つにまとめる．

- 比べるのは結果の列の値で，表のほかの列は関係ない．
- `NULL`どうしは同じ値とみなす．`WHERE`の`NULL = NULL`は`UNKNOWN`だが，重複の判定では「区別しない(not distinct)」と扱う．

`DISTINCT`と`ORDER BY`を組み合わせるとき，並べ替えのキーは選択項目になければならない．

```sql
SELECT DISTINCT name FROM users ORDER BY id;   -- 42P10
```

同じ`name`の行が複数あると，まとめた1行の`id`が決まらないからである．
