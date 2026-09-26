# システムコンテキスト

`ferrodb`と，それを使う人の関係を示す．
`ferrodb`は，Rustのプログラムから呼び出すライブラリである．

```mermaid
C4Context
  title ferrodbのシステムコンテキスト
  Person(developer, "開発者", "Rustのプログラムからferrodbを使う人")
  System(ferrodb, "ferrodb", "SQLを処理するデータベース")
  Rel(developer, ferrodb, "SQLの文を渡し，結果の表を受け取る")
```
