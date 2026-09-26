# miseの公式Debianイメージ．git，curl，CA証明書とmiseを含み，
# PATHに/mise/shims，MISE_DATA_DIRに/miseが設定済み．
FROM jdxcode/mise:2026.9.12-debian

RUN apt-get update \
	&& apt-get install -y --no-install-recommends \
	openssh-client \
	procps \
	jq \
	libatomic1 \
	&& rm -rf /var/lib/apt/lists/*

# ここにプロジェクト固有のシステムパッケージ・ツールチェーンのインストールを追加する
# (例: ネイティブビルドのリンカ，画像/PDF変換ツール，各言語のランタイムなど)．

# ツール(rtk，lefthookなど)はmise.tomlのとおりにイメージへ入れる．
# 版を固定したい場合は，先に`mise lock`でmise.lockを作り，
# COPYの対象に加えたうえで`mise install --locked`にする．
WORKDIR /tmp/mise-bootstrap
COPY mise.toml ./
RUN mise trust \
	&& mise ls --current \
	&& mise install --verbose

# 起動後は，リポジトリのmise.tomlをグローバル設定として読む．
# これにより，/workspacesの外のディレクトリでも同じツールを使える．
# グローバル設定のルートも/workspacesにして，タスクをリポジトリ直下で実行させる
# (未指定だとホームディレクトリで実行される)．
ENV MISE_GLOBAL_CONFIG_FILE=/workspaces/mise.toml \
	MISE_GLOBAL_CONFIG_ROOT=/workspaces \
	MISE_TRUSTED_CONFIG_PATHS=/workspaces

WORKDIR /workspaces
