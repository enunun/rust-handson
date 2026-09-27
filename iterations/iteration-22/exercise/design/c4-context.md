# システムコンテキスト

`ferrodb`と，それを使う人やプログラムの関係を示す．
利用者はREPLか`psql`などのPostgreSQLのクライアントでSQLを実行し，開発者はRustのプログラムからライブラリとして使う．

```mermaid
C4Context
  title ferrodbのシステムコンテキスト
  Person(user, "利用者", "SQLを入力して結果を見る人")
  Person(developer, "開発者", "Rustのプログラムからferrodbを使う人")
  System_Ext(client, "PostgreSQLのクライアント", "psql や postgres クレートなど")
  System(ferrodb, "ferrodb", "SQLを処理するデータベース")
  Rel(user, ferrodb, "REPLでSQLの文を入力し，結果の表を見る")
  Rel(user, client, "SQLの文を入力する")
  Rel(client, ferrodb, "問い合わせを送り，結果を受け取る", "TCP，プロトコル3.0")
  Rel(developer, ferrodb, "表を作り，行を挿入し，問い合わせる")
```
