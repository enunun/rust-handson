# Iteration 8：更新，削除，制約(演習)

`UPDATE`と`DELETE`で表の行を書き換え，消す．`DROP TABLE`で表を消す．
列制約`NOT NULL`，`PRIMARY KEY`，`UNIQUE`を検査し，違反した文による変更をすべて取り消す．

## 進め方

[docs/iteration-08.md](docs/iteration-08.md)の手順に従って，次の順に進める．

1. 準備：引き継いだテストが通ることを確かめる．
2. 文法と概念：ノートを読み，小さな課題で確かめる．
3. テストリスト：`TESTLIST.md`に，確かめる振る舞いを書き出す．
4. 設計ドキュメント：`design/`の図を更新する．
5. テスト駆動の実装：テストリストの項目を1つずつ実装する．
6. 振り返り：模範解答と比べ，設計ドキュメントと実装を見比べる．
7. 発展課題：表制約`PRIMARY KEY (列)`と`UNIQUE (列)`に対応する．

## ディレクトリの構成

```text
README.md             このファイル
Cargo.toml            パッケージferrodb
TESTLIST.md           テストリスト(見出しだけのひな形)
docs/iteration-08.md  演習の手順
design/               設計ドキュメント(Iteration 7の模範解答)
src/                  Iteration 7の模範解答のコード
tests/                Iteration 7の結合テスト
```

模範解答は[../solution/](../solution/README.md)にある．
