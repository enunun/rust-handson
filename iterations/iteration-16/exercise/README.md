# Iteration 16：B+木(演習)

キーから行の位置を引くB+木を，バッファプールのページの上に作る．

## 進め方

[docs/iteration-16.md](docs/iteration-16.md)の手順に従って，次の順に進める．

1. 準備：引き継いだテストが通ることを確かめる．
2. 文法と概念：ノートを読み，小さな課題で確かめる．
3. テストリスト：`TESTLIST.md`に，確かめる振る舞いを書き出す．
4. 設計ドキュメント：`design/`の図を更新する．
5. テスト駆動の実装：テストリストの項目を1つずつ実装する．
6. 振り返り：模範解答と比べ，設計ドキュメントと実装を見比べる．
7. 発展課題：ファイルにある木を開き直す．

## ディレクトリの構成

```text
README.md             このファイル
Cargo.toml            パッケージferrodb
TESTLIST.md           テストリスト(見出しだけのひな形)
docs/iteration-16.md  演習の手順
design/               設計ドキュメント(Iteration 15の模範解答)
src/                  Iteration 15の模範解答のコード
tests/                Iteration 15の結合テスト
```

模範解答は[../solution/](../solution/README.md)にある．
