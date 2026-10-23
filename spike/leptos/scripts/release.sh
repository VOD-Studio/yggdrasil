#!/usr/bin/env bash
# 构建一份发布快照：cargo leptos --split 产物 + brotli 旁车文件 + 服务端二进制。
# 用法：scripts/release.sh <release-id>   → target/releases/<release-id>/
set -euo pipefail
cd "$(dirname "$0")/.."

id=${1:?用法：scripts/release.sh <release-id>}
out="target/releases/$id"

SPIKE_BUILD_ID="$id" cargo leptos build --release --split

rm -rf "$out"
mkdir -p "$out"
cp -r target/site "$out/site"
# leptos 运行期从可执行文件同目录读取 hash.txt，二者必须一起搬。
cp target/release/ygg_leptos_spike target/release/hash.txt "$out/"
find "$out/site/pkg" -type f \( -name '*.wasm' -o -name '*.js' -o -name '*.json' -o -name '*.css' \) \
  -exec brotli -q 11 -k -f {} \;
echo "发布快照：$out"
