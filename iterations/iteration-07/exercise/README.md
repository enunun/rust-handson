# Iteration 7：WHERE，列の選択，別名(演習)

`SELECT`で列と式を選んで別名を付け，`WHERE`で行を絞り込む．列の名前を解決する段階を作る．

## 進め方

[docs/iteration-07.md](docs/iteration-07.md)の手順に従って，次の順に進める．

1. 準備：引き継いだテストが通ることを確かめる．
2. 文法と概念：ノートを読み，小さな課題で確かめる．
3. テストリスト：`TESTLIST.md`に，確かめる振る舞いを書き出す．
4. 設計ドキュメント：`design/`の図を更新する．
5. テスト駆動の実装：テストリストの項目を1つずつ実装する．
6. 振り返り：模範解答と比べ，設計ドキュメントと実装を見比べる．
7. 発展課題：`IN (式, ...)`に対応する．

## ディレクトリの構成

```text
README.md             このファイル
Cargo.toml            パッケージferrodb
TESTLIST.md           テストリスト(見出しだけのひな形)
docs/iteration-07.md  演習の手順
design/               設計ドキュメント(Iteration 6の模範解答)
src/                  Iteration 6の模範解答のコード
tests/                Iteration 6の結合テスト
```

模範解答は[../solution/](../solution/README.md)にある．
