# Iteration 8：更新，削除，整合性制約

## `UPDATE`と`DELETE`

```sql
UPDATE users SET age = age + 1, name = 'bobby' WHERE id = 2;
DELETE FROM users WHERE age IS NULL;
DROP TABLE users;
```

- `UPDATE`は，`WHERE`の条件が`TRUE`になる行の列に，`SET`の式の値を代入する．`DELETE`は，条件が`TRUE`になる行を消す．
- `WHERE`を省くと，すべての行が対象になる．条件の3値論理は`SELECT`と同じで，`UNKNOWN`の行は対象にならない．
- 結果のコマンドタグは，対象になった行の数を持つ(`UPDATE 2`，`DELETE 0`)．
- `DROP TABLE`は，表の定義と行をまとめて消す．

`SET`の右辺の式は，すべて書き換える前の行で評価する．
次の文は，`a`と`b`の値を入れ替える．左から順に代入するのではない．

```sql
UPDATE t SET a = b, b = a;
```

同じ列に2度代入する文は，どちらの値を使うか決まらないのでエラーになる．

## 整合性制約

整合性制約は，表の行が常に満たすべき条件である．データベースは，制約に違反する変更を拒否する．
`ferrodb`は，列の定義に書く次の3つの制約を扱う．

```sql
CREATE TABLE users (
  id INTEGER PRIMARY KEY,
  name VARCHAR(20) NOT NULL,
  email VARCHAR(40) UNIQUE
);
```

| 制約 | 意味 | 違反のSQLSTATE |
| --- | --- | --- |
| `NOT NULL` | 列の値は`NULL`にならない | `23502` |
| `UNIQUE` | 列の値は，表の中で重ならない | `23505` |
| `PRIMARY KEY` | `NOT NULL`と`UNIQUE`を合わせたもの．表に1つだけ置ける | `23502`か`23505` |

SQLSTATEのクラス`23`は，整合性制約の違反(integrity constraint violation)を表す．

### 一意性制約と`NULL`

`UNIQUE`の列には，`NULL`をいくつでも入れられる．
`NULL`は「値がわからない」ことを表すので，2つの`NULL`が等しいとは言えないからである(`NULL = NULL`は`UNKNOWN`)．
値のない行を1つに限りたいなら，`NOT NULL`も書くか，`PRIMARY KEY`にする．

### 主キー

主キーは，表の行を1つに特定するための列である．
1つの表に主キーは1つだけ置ける．`NULL`では行を特定できないので，主キーの列は`NOT NULL`になる．

### 制約の名前

違反のメッセージには制約の名前が入る．
名前を書かずに作った制約には，PostgreSQLと同じ規則で名前を付ける．

| 制約 | 名前 | 例 |
| --- | --- | --- |
| `PRIMARY KEY` | `表_PKEY` | `USERS_PKEY` |
| `UNIQUE` | `表_列_KEY` | `USERS_EMAIL_KEY` |

PostgreSQLは識別子を小文字に正規化するので`users_pkey`になり，大文字に正規化する`ferrodb`では`USERS_PKEY`になる．

## 文単位の原子性

Iteration 5で，`INSERT`の途中の行でエラーが起きたら，その文で挿入したすべての行を取り消すことを説明した．
`UPDATE`と`DELETE`も同じで，1つの文は全体として成功するか，何も変えないかのどちらかである．

- 式の評価，列の型への変換，制約の検査のどこでエラーになっても，その文による変更はすべて取り消す．
- 3行目の`WHERE`の評価で0による除算が起きたら，1行目と2行目に対する変更も取り消す．

`ferrodb`は，変更したあとの表を別に作り，制約を検査してから，元の表と置き換える．
途中でエラーになれば置き換えないので，元の表はそのまま残る．

## 制約を検査する時点

標準SQLでは，制約は文が終わった時点で検査する．文の途中で一時的に違反しても，終わったときに満たしていればよい．

```sql
-- id が 1 と 2 の行がある
UPDATE users SET id = id + 1;
```

1行目を書き換えた時点では`id`が`2`の行が2つあるが，文が終わると`2`と`3`になり，重なりはない．
`ferrodb`は変更したあとの表全体を検査するので，この文は成功する．

PostgreSQLは，一意性制約を行を書き換えるたびに検査する．そのため，行の並び順によっては，この文が`23505`で失敗する．
標準の振る舞いにするには，制約に`DEFERRABLE`を指定する．

Iteration 17では，一意性の検査を，全行を調べる方法からインデックスを引く方法に替える．
