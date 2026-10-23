#!/usr/bin/env bash
# 以发布快照启动原型服务。
# 用法：scripts/serve.sh <release-id> [previous-release-id]
# 需要 SPIKE_DATABASE_URL 指向一次性数据库副本。
set -euo pipefail
cd "$(dirname "$0")/.."

id=${1:?用法：scripts/serve.sh <release-id> [previous-release-id]}
dir="target/releases/$id"
prev=${2:-}

export LEPTOS_OUTPUT_NAME=ygg_spike
export LEPTOS_SITE_ROOT="$dir/site"
export LEPTOS_SITE_PKG_DIR=pkg
export LEPTOS_SITE_ADDR="${LEPTOS_SITE_ADDR:-127.0.0.1:3100}"
export LEPTOS_HASH_FILES=true
export LEPTOS_ENV=PROD
# 浏览器库产物（yggdrasil-core、tiptap…）取自主应用 dx 发布目录，与对照服务完全同一份。
export SPIKE_PUBLIC_DIR="${SPIKE_PUBLIC_DIR:-$(cd ../.. && pwd)/target/dx/yggdrasil/release/web/public}"
export SPIKE_UPLOADS_DIR="${SPIKE_UPLOADS_DIR:-$(cd ../.. && pwd)/uploads}"
: "${SPIKE_DATABASE_URL:?需要 SPIKE_DATABASE_URL}"
if [[ -n "$prev" ]]; then
  export SPIKE_PREVIOUS_SITE_ROOT="target/releases/$prev/site"
else
  unset SPIKE_PREVIOUS_SITE_ROOT
fi
exec "$dir/ygg_leptos_spike"
