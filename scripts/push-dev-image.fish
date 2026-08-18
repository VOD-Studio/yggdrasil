#!/usr/bin/env fish

# 构建并推送 Yggdrasil 多架构开发镜像。
#
# 用法:
#   docker login hub.rua.plus
#   fish scripts/push-dev-image.fish
#
# 可选环境变量:
#   YGG_VERSION=v0.11.0  覆盖自动读取的最近 Git tag
#   PLATFORMS=linux/amd64,linux/arm64  覆盖构建平台
#   CN_MIRROR=false       关闭国内依赖镜像

set -l REGISTRY hub.rua.plus
set -l REPOSITORY yggdrasil
set -l BUILDER yggdrasil-multiarch

set -l version
if set -q YGG_VERSION[1]
    set version "$YGG_VERSION"
else
    set version (git describe --tags --abbrev=0 2>/dev/null)
end

if test (count $version) -eq 0
    echo "无法确定版本号，请设置 YGG_VERSION，例如: set -x YGG_VERSION v0.11.0" >&2
    exit 1
end

set -l platforms linux/amd64,linux/arm64
if set -q PLATFORMS[1]
    set platforms "$PLATFORMS"
end

set -l mirror true
if set -q CN_MIRROR[1]
    set mirror "$CN_MIRROR"
end

set -l image "$REGISTRY/$REPOSITORY:$version-dev"

if not type -q docker
    echo "缺少 docker 命令" >&2
    exit 1
end

if not type -q make
    echo "缺少 make 命令" >&2
    exit 1
end

if not docker info >/dev/null 2>&1
    echo "Docker daemon 不可用" >&2
    exit 1
end

if not docker buildx version >/dev/null 2>&1
    echo "Docker Buildx 不可用" >&2
    exit 1
end

echo "==> 启用 arm64 构建支持"
docker run --privileged --rm tonistiigi/binfmt --install arm64; or exit 1

echo "==> 准备 BuildKit builder: $BUILDER"
if not docker buildx inspect "$BUILDER" >/dev/null 2>&1
    docker buildx create --name "$BUILDER" --driver docker-container --use; or exit 1
else
    docker buildx use "$BUILDER"; or exit 1
end

docker buildx inspect "$BUILDER" --bootstrap; or exit 1

echo "==> 构建并推送: $image"
echo "    platforms: $platforms"
echo "    CN_MIRROR: $mirror"
make docker-multiarch \
    IMAGE="$image" \
    PLATFORMS="$platforms" \
    CN_MIRROR="$mirror"; or exit 1

echo "==> 验证远端 manifest"
set -l manifest (docker buildx imagetools inspect "$image"); or exit 1
printf '%s\n' $manifest

if not string match -q -- '*linux/amd64*' $manifest
    echo "远端 manifest 缺少 linux/amd64" >&2
    exit 1
end

if not string match -q -- '*linux/arm64*' $manifest
    echo "远端 manifest 缺少 linux/arm64" >&2
    exit 1
end

echo "==> 完成: $image"
