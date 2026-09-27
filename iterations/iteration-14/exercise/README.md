# Iteration 14：ヒープファイルとデータディレクトリ(演習)

表をデータディレクトリのファイルに保存する．`ferrodb repl --data-dir DIR`で起動し直しても，表と行が残る．

## 進め方

[docs/iteration-14.md](docs/iteration-14.md)の手順に従って，次の順に進める．

1. 準備：引き継いだテストが通ることを確かめる．
2. 文法と概念：ノートを読み，小さな課題で確かめる．
3. テストリスト：`TESTLIST.md`に，確かめる振る舞いを書き出す．
4. 設計ドキュメント：`design/`の図を更新する．
5. テスト駆動の実装：テストリストの項目を1つずつ実装する．
6. 振り返り：模範解答と比べ，設計ドキュメントと実装を見比べる．
7. 発展課題：大きさが8192の倍数でないヒープファイルを，開くときに見つける．

## ディレクトリの構成

```text
README.md             このファイル
Cargo.toml            パッケージferrodb
TESTLIST.md           テストリスト(見出しだけのひな形)
docs/iteration-14.md  演習の手順
design/               設計ドキュメント(Iteration 13の模範解答)
src/                  Iteration 13の模範解答のコード
tests/                Iteration 13の結合テスト
```

模範解答は[../solution/](../solution/README.md)にある．
