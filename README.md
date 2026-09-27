# Rustでリレーショナルデータベースを作るハンズオン

PostgreSQLのクライアントから接続できるリレーショナルデータベース`ferrodb`を，Rustで一から作るハンズオンである．
23回のIterationで1つのプログラムを育てながら，Rustの言語機能とデータベースの内部の仕組みを学ぶ．

```console
$ psql -h 127.0.0.1 -p 5433 -U alice ferro
ferro=> SELECT d.title, COUNT(*) AS n FROM emp e LEFT JOIN dept d ON e.dept = d.code GROUP BY d.title;
```

## 学ぶこと

- Rust：所有権と借用，代数的データ型(`struct`と`enum`)による型のモデリング，トレイトとジェネリクス，`Result`によるエラー処理，ライフタイム，ファイルとネットワークの入出力，スレッド
- データベース：SQLの字句解析と構文解析，実行計画，スロット付きページ，バッファプール，B+木，MVCC，WAL，PostgreSQLのプロトコル
- 開発の進め方：テストリストから始めるテスト駆動開発と，C4モデルによる設計ドキュメント

## 前提

- Rust以外のプログラミング言語(Python，JavaScript，Go，Javaなど)で，プログラムを書いた経験がある．
- 単体テストを書いたことがあり，テスト駆動開発の考え方を知っている．
- Rustとデータベースの内部は知らなくてよい．SQLは`SELECT`と`INSERT`を書いたことがあれば十分である．

## 環境の準備

VS CodeのDev Containersで開くと，必要なものがそろう．

1. このリポジトリをクローンし，VS Codeで開く．
2. コマンドパレットで「Dev Containers: Reopen in Container」を実行する．初回は`mise run setup`が実行される．
3. ターミナルで`cargo --version`を実行し，`cargo 1.98.1`と表示されることを確かめる．

Dev Containersを使わない場合は，次を用意する．

- Rust 1.98.1(`rustup`または`mise`で入れる)
- CのリンカーとCの標準ライブラリ(Debian系では`gcc`と`libc6-dev`)
- Node.jsとpnpm(`pnpm install`を実行する．Mermaidの図の検査に使う)
- `psql`(Iteration 20から使う．Debian系では`postgresql-client`)

## 進め方

各Iterationは`iterations/iteration-NN/`にある．

- `exercise/`：受講者が作業する場所．`exercise/README.md`から始める．
- `solution/`：演習を終えた状態と模範解答．自分の答えと見比べる．

どのIterationも，テストリスト → 設計ドキュメント → テスト駆動の実装 → 設計レビューの順に進める．

- [ロードマップ](docs/ROADMAP.md)：各Iterationで作る機能と学ぶこと
- [テスト駆動開発とテストリスト](docs/tdd.md)
- [設計ドキュメントの書き方](docs/design.md)
- [Rustのノート](docs/rust/README.md)：各Iterationで初めて使う文法と概念
- [データベースのノート](docs/db/README.md)：各Iterationで初めて扱う理論

## Iterationの一覧

| # | 作る機能 |
| --- | --- |
| [0](iterations/iteration-00/exercise/README.md) | プロジェクトの作成と，`VALUES (1, 2 + 3)`の字句解析 |
| [1](iterations/iteration-01/exercise/README.md) | `VALUES`の算術式を構文解析して評価する |
| [2](iterations/iteration-02/exercise/README.md) | 真偽値，比較，`NULL`と3値論理 |
| [3](iterations/iteration-03/exercise/README.md) | 文字列 |
| [4](iterations/iteration-04/exercise/README.md) | SQLSTATEとエラー位置 |
| [5](iterations/iteration-05/exercise/README.md) | `CREATE TABLE`，`INSERT`，`SELECT * FROM` |
| [6](iterations/iteration-06/exercise/README.md) | REPL |
| [7](iterations/iteration-07/exercise/README.md) | `WHERE`，列の選択，別名 |
| [8](iterations/iteration-08/exercise/README.md) | `UPDATE`，`DELETE`，`DROP TABLE`，制約 |
| [9](iterations/iteration-09/exercise/README.md) | `ORDER BY`，`OFFSET`，`FETCH FIRST`，`DISTINCT` |
| [10](iterations/iteration-10/exercise/README.md) | 実行計画と`EXPLAIN` |
| [11](iterations/iteration-11/exercise/README.md) | 結合(`CROSS`，`INNER`，`LEFT JOIN`) |
| [12](iterations/iteration-12/exercise/README.md) | 集約(`GROUP BY`，`HAVING`，集約関数) |
| [13](iterations/iteration-13/exercise/README.md) | ページとタプルのバイト表現 |
| [14](iterations/iteration-14/exercise/README.md) | ヒープファイルとデータディレクトリ |
| [15](iterations/iteration-15/exercise/README.md) | バッファプール |
| [16](iterations/iteration-16/exercise/README.md) | B+木 |
