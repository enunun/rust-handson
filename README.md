# claude-docker-template

Claude Code for VSCode + Docker(mise) + rtkで開発するときの，最小構成のテンプレート．
言語や作るものは特に決めず，devcontainer・mise・rtk・lefthookの土台だけを提供する．

## 構成

``` text
.devcontainer/
  devcontainer.json  VSCode Dev Containersの設定．claude-home/rtk-homeを
                      ホストにバインドマウントし，資格情報や履歴をコンテナの
                      再作成後も保つ．
  Dockerfile          mise公式イメージをベースに，rtk/lefthookをmiseで入れる．
                      プロジェクト固有のパッケージ・ツールチェーンはここに追加する．
  compose.yml         コンテナを起動したままにする(sleep infinity)だけの設定．
.claude/
  settings.json        Bashツール呼び出しをrtk経由に書き換えるフック．
                        enunun/system-development-skillsを参照するプラグイン設定も含む．
.rtk/
  filters.toml          プロジェクト固有のrtkフィルタ(雛形のみ)．
mise.toml               ツールの版とタスク(install/fmt/lint/test/check/setup)の雛形．
lefthook.yml             コミット時の検査の雛形．
CLAUDE.md                プロジェクト向けのClaude Code指示の雛形．
.gitignore
```

## 使い方

1. このフォルダの中身を，新しいプロジェクトのリポジトリのルートにコピーする．
2. `PROJECT_NAME`という文字列を，プロジェクト名に置き換える(`devcontainer.json`，`compose.yml`，`CLAUDE.md`)．
3. `mise.toml`の`[tools]`に，プロジェクトが使う言語・ツールを追加する．
4. `mise.toml`の各タスク(`install`/`fmt`/`lint`/`test`)と，`lefthook.yml`の`format`コマンドを，実際のコマンドに置き換える．
5. `Dockerfile`に，プロジェクトのビルドに必要なシステムパッケージがあれば追加する．
6. VSCodeで「Reopen in Container」を実行する．初回は`mise run setup`が走る．
7. `.gitignore`から`pnpm-lock.yaml`を削除し，lockファイルがコミットされるようにする．
8. `mise.toml`の`[settings]`と`lockfile = true`の行のコメントを解除し，lockファイルを使用するようにする．

## rtk(Rust Token Killer)について

シェルコマンドの出力を絞り込み，トークン消費を抑えるCLIプロキシ．
`.claude/settings.json`のフックが，Claude CodeのBashツール呼び出しを自動的に`rtk`経由に書き換える．
コマンドの詳しい対応表は[rtkのリポジトリ](https://github.com/rtk-ai/rtk)を参照．
`~/.claude/CLAUDE.md`からrtkの使い方を読み込ませておくと，全プロジェクトで効く．

## 共有スキルについて

`.claude/settings.json`は，[enunun/system-development-skills](https://github.com/enunun/system-development-skills)をプラグインのマーケットプレイスとして参照する設定を含む．成果物を仕上げる`finalize-artifacts`スキルなど，プロジェクトを問わず使うスキルはそちらに集約されている．

## claude-home / rtk-home について

`.devcontainer/claude-home/`と`.devcontainer/rtk-home/`は，コンテナ作成時に
`initializeCommand`が自動生成し，
コンテナ内の`/root/.claude`や`/root/.config/rtk`などにバインドマウントされる．
資格情報や履歴を含むため，`.gitignore`で除外している．
