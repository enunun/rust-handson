# Iteration 2：真偽値，比較，NULLと3値論理(演習)

真偽値と`NULL`を値に加え，比較演算子，`AND`，`OR`，`NOT`，`IS NULL`を3値論理で評価する．

## 進め方

[docs/iteration-02.md](docs/iteration-02.md)の手順に従って，次の順に進める．

1. 準備：引き継いだテストが通ることを確かめる．
2. 文法と概念：ノートを読み，小さな課題で確かめる．
3. テストリスト：`TESTLIST.md`に，確かめる振る舞いを書き出す．
4. 設計ドキュメント：`design/`の図を更新する．
5. テスト駆動の実装：テストリストの項目を1つずつ実装する．
6. 振り返り：模範解答と比べ，設計ドキュメントと実装を見比べる．
7. 発展課題：`IS TRUE`と`IS FALSE`に対応する．

## ディレクトリの構成

```text
README.md             このファイル
Cargo.toml            パッケージferrodb
TESTLIST.md           テストリスト(見出しだけのひな形)
docs/iteration-02.md  演習の手順
design/               設計ドキュメント(Iteration 1の模範解答)
src/                  Iteration 1の模範解答のコード
tests/                Iteration 1の結合テスト
```

模範解答は[../solution/](../solution/README.md)にある．
