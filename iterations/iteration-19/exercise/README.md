# Iteration 19：WALとクラッシュリカバリ(演習)

変更を先にログ(WAL)へ書き，プロセスが止まったあとで，ログからコミットした変更をやり直す．

## 進め方

[docs/iteration-19.md](docs/iteration-19.md)の手順に従って，次の順に進める．

1. 準備：引き継いだテストが通ることを確かめる．
2. 文法と概念：ノートを読み，小さな課題で確かめる．
3. テストリスト：`TESTLIST.md`に，確かめる振る舞いを書き出す．
4. 設計ドキュメント：`design/`の図を更新する．
5. テスト駆動の実装：テストリストの項目を1つずつ実装する．
6. 振り返り：模範解答と比べ，設計ドキュメントと実装を見比べる．
7. 発展課題：チェックポイントでログを切り詰める．

## ディレクトリの構成

```text
README.md             このファイル
Cargo.toml            パッケージferrodb
TESTLIST.md           テストリスト(見出しだけのひな形)
docs/iteration-19.md  演習の手順
design/               設計ドキュメント(Iteration 18の模範解答)
src/                  Iteration 18の模範解答のコード
tests/                Iteration 18の結合テスト
```

模範解答は[../solution/](../solution/README.md)にある．
