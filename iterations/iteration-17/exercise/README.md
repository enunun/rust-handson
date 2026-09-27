# Iteration 17：インデックスの利用(演習)

`CREATE INDEX`でB+木のインデックスを作り，`WHERE`の条件に合う行をインデックスで読む．一意性制約もインデックスで検査する．

## 進め方

[docs/iteration-17.md](docs/iteration-17.md)の手順に従って，次の順に進める．

1. 準備：引き継いだテストが通ることを確かめる．
2. 文法と概念：ノートを読み，小さな課題で確かめる．
3. テストリスト：`TESTLIST.md`に，確かめる振る舞いを書き出す．
4. 設計ドキュメント：`design/`の図を更新する．
5. テスト駆動の実装：テストリストの項目を1つずつ実装する．
6. 振り返り：模範解答と比べ，設計ドキュメントと実装を見比べる．
7. 発展課題：インデックスの順で済む並べ替えを省く．

## ディレクトリの構成

```text
README.md             このファイル
Cargo.toml            パッケージferrodb
TESTLIST.md           テストリスト(見出しだけのひな形)
docs/iteration-17.md  演習の手順
design/               設計ドキュメント(Iteration 16の模範解答)
src/                  Iteration 16の模範解答のコード
tests/                Iteration 16の結合テスト
```

模範解答は[../solution/](../solution/README.md)にある．
