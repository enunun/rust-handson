# Iteration 0：プロジェクトの作成と字句解析(演習)

`cargo init`で`ferrodb`のパッケージを作り，SQLの文字列`VALUES (1, 2 + 3)`をトークンの列に分ける字句解析器を作る．

## 進め方

[docs/iteration-00.md](docs/iteration-00.md)の手順に従って，次の順に進める．

1. 準備：`cargo init`でパッケージを作り，winnowを依存に加える．
2. 文法と概念：ノートを読み，小さなテストで確かめる．
3. テストリスト：`TESTLIST.md`に，確かめる振る舞いを書き出す．
4. 設計ドキュメント：`design/`の5つの図を描く．
5. テスト駆動の実装：テストリストの項目を1つずつ実装する．
6. 振り返り：模範解答と比べ，設計ドキュメントと実装を見比べる．
7. 発展課題：SQLのコメントを読み飛ばす．

## ディレクトリの構成

```text
README.md             このファイル
TESTLIST.md           テストリスト(見出しだけのひな形)
docs/iteration-00.md  演習の手順
design/               設計ドキュメント(描くものを説明するコメントだけのひな形)
```

`Cargo.toml`，`src/`，`tests/`は，演習の中で作る．
模範解答は[../solution/](../solution/README.md)にある．
