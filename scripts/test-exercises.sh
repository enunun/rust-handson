#!/usr/bin/env bash
# Cargo.tomlのあるすべての演習パッケージをビルドしてテストする．
# 演習はワークスペースに含めない単独のパッケージなので，1つずつ実行する．
# 演習のパッケージ名はどれもferrodbなので，ビルドの成果物がぶつからないように，演習ごとにターゲットディレクトリを分ける．
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
found=0
for manifest in "$root"/iterations/*/exercise/Cargo.toml; do
	[ -e "$manifest" ] || continue
	found=1
	iteration="$(basename "$(dirname "$(dirname "$manifest")")")"
	echo "== ${manifest#"$root"/}"
	CARGO_TARGET_DIR="$root/target/exercises/$iteration" cargo test --manifest-path "$manifest" --quiet
done

if [ "$found" -eq 0 ]; then
	echo "Cargo.tomlのある演習パッケージはない"
fi
