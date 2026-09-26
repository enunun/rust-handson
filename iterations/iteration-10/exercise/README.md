# Iteration 10：実行計画とEXPLAIN(演習)

`SELECT`を実行計画(演算子の木)に変換してから実行し，`EXPLAIN`で実行計画を表示する．

## 進め方

[docs/iteration-10.md](docs/iteration-10.md)の手順に従って，次の順に進める．

1. 準備：引き継いだテストが通ることを確かめる．
2. 文法と概念：ノートを読み，小さな課題で確かめる．
3. テストリスト：`TESTLIST.md`に，確かめる振る舞いを書き出す．
4. 設計ドキュメント：`design/`の図を更新する．
5. テスト駆動の実装：テストリストの項目を1つずつ実装する．
6. 振り返り：模範解答と比べ，設計ドキュメントと実装を見比べる．
7. 発展課題：`FETCH FIRST`の下の並べ替えを，先頭の行だけを残す演算子`TopN`にする．

## ディレクトリの構成

```text
README.md             このファイル
Cargo.toml            パッケージferrodb
TESTLIST.md           テストリスト(見出しだけのひな形)
docs/iteration-10.md  演習の手順
design/               設計ドキュメント(Iteration 9の模範解答)
src/                  Iteration 9の模範解答のコード
tests/                Iteration 9の結合テスト
```

模範解答は[../solution/](../solution/README.md)にある．
