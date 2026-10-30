#!/usr/bin/env fish

# 构建并推送 Yggdrasil 多架构镜像。
#
# 用法:
#   docker login hub.rua.plus
#   fish scripts/push-docker.fish          # 默认 dev：<版本>-dev
#   fish scripts/push-docker.fish dev      # 开发镜像：<版本>-dev
#   fish scripts/push-docker.fish release  # 正式镜像：<版本>，不更新 latest
#
# 可选环境变量:
#   YGG_VERSION=v1.2.5  覆盖自动读取的最近 Git tag
#   PLATFORMS=linux/amd64,linux/arm64  覆盖构建平台
#   CN_MIRROR=false       关闭国内依赖镜像

argparse h/help -- $argv; or exit 1
if set -q _flag_help
    printf '%s\n' \
        '用法：fish scripts/push-docker.fish [dev|release]' \
        '默认 dev：推送 hub.rua.plus/yggdrasil:<版本>-dev。' \
        'release：推送 hub.rua.plus/yggdrasil:<版本>，不更新 latest。' \
        '构建当前工作目录源码，不会自动检出版本 tag；请先 docker login hub.rua.plus。' \
        'YGG_VERSION：覆盖最近 Git tag；PLATFORMS：默认 linux/amd64,linux/arm64。' \
        'CN_MIRROR：默认 true，设为 false 关闭国内依赖镜像。'
    exit 0
end
if test (count $argv) -gt 1
    echo '只接受一个模式参数：dev 或 release（--help 查看用法）' >&2
    exit 1
end
set -l mode dev
if test (count $argv) -eq 1
    set mode "$argv[1]"
end
if not contains -- "$mode" dev release
    echo "未知发布模式：$mode（可选 dev、release）" >&2
    exit 1
end

set -l root (path resolve (status dirname)/..)
cd "$root"; or exit 1

set -l REGISTRY hub.rua.plus
set -l REPOSITORY yggdrasil
set -l BUILDER yggdrasil-multiarch

set -l image_version
if set -q YGG_VERSION[1]
    set image_version "$YGG_VERSION"
else
    set image_version (git describe --tags --abbrev=0 2>/dev/null)
end

if test (count $image_version) -eq 0
    echo "无法确定版本号，请设置 YGG_VERSION，例如: set -x YGG_VERSION v1.2.5" >&2
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

set -l image "$REGISTRY/$REPOSITORY:$image_version"
if test "$mode" = dev
    set image "$image-dev"
end

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

if contains -- linux/arm64 (string split , -- "$platforms")
    echo "==> 启用 arm64 构建支持"
    docker run --privileged --rm tonistiigi/binfmt --install arm64; or exit 1
end

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

for platform in (string split , -- "$platforms")
    if not string match -rq -- (string escape --style=regex -- "$platform")'([/[:space:]]|$)' $manifest
        echo "远端 manifest 缺少 $platform" >&2
        exit 1
    end
end

echo "==> 完成: $image"
