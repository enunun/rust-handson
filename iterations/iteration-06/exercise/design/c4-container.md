# コンテナ

`ferrodb`を構成する，実行される単位と保存される単位を示す．
現在はライブラリクレートが1つだけある．

```mermaid
C4Container
  title ferrodbのコンテナ
  Person(developer, "開発者")
  System_Boundary(system, "ferrodb") {
    Container(library, "ferrodb", "Rust library", "SQLの文を実行し，表と行をメモリーに持つ")
  }
  Rel(developer, library, "Database::execute を呼ぶ")
```
