#!/bin/sh
# 构建代码运行器所需的沙箱镜像：base → python / node / go / rust / bun。
#
# 依赖：本机已安装 docker。构建顺序固定（python/node/go/rust/bun 均为 FROM base）。
# 镜像 tag 与 src/api/code_runner/languages.rs 的注册项严格对应：
#   yggdrasil-runner-base:latest
#   yggdrasil-runner-python:latest
#   yggdrasil-runner-node:latest
#   yggdrasil-runner-go:latest
#   yggdrasil-runner-rust:latest
#   yggdrasil-runner-bun:latest
set -e

usage() {
    echo "用法: $0 [--cn-mirror]"
    echo "  --cn-mirror  使用清华 Alpine 镜像源（默认使用官方源）"
    echo "  -h, --help   显示帮助"
}

CN_MIRROR=false
while [ "$#" -gt 0 ]; do
    case "$1" in
        --cn-mirror) CN_MIRROR=true ;;
        -h|--help) usage; exit 0 ;;
        *) echo "未知参数: $1" >&2; usage >&2; exit 1 ;;
    esac
    shift
done

SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)

echo "==> Building yggdrasil-runner-base:latest"
# 后续语言镜像继承 base 中的 /etc/apk/repositories，只需在这里传入换源开关。
docker build --build-arg "CN_MIRROR=$CN_MIRROR" \
    -t yggdrasil-runner-base:latest "$SCRIPT_DIR/runner-base"

echo "==> Building yggdrasil-runner-python:latest"
docker build -t yggdrasil-runner-python:latest "$SCRIPT_DIR/runner-python"

echo "==> Building yggdrasil-runner-node:latest"
docker build -t yggdrasil-runner-node:latest "$SCRIPT_DIR/runner-node"

echo "==> Building yggdrasil-runner-go:latest"
docker build -t yggdrasil-runner-go:latest "$SCRIPT_DIR/runner-go"

echo "==> Building yggdrasil-runner-rust:latest"
docker build -t yggdrasil-runner-rust:latest "$SCRIPT_DIR/runner-rust"

echo "==> Building yggdrasil-runner-bun:latest"
docker build -t yggdrasil-runner-bun:latest "$SCRIPT_DIR/runner-bun"

echo "==> Done. Images:"
docker images --filter "reference=yggdrasil-runner-*" \
    --format "table {{.Repository}}\t{{.Tag}}\t{{.Size}}"
