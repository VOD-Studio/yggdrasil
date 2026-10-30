#!/usr/bin/env fish

# 在本机构建 linux/amd64、linux/arm64 镜像，开启国内依赖镜像源，不推送或部署。
# 用法：fish scripts/build-docker.fish
# 产物：yggdrasil:amd64、yggdrasil:arm64，以及最近 Git tag 对应的 <版本>-<架构> 标签。
# Debian 用清华源，Rust/crates 用 rsproxy，Bun/npm 用 npmmirror，GitHub 下载走 gh-proxy。
# Docker Hub 基础镜像拉取由本机 Docker daemon 的 registry-mirrors 配置决定。
# 固定使用本机 default Docker context 和 builder，不使用远程构建节点。

if contains -- --help $argv; or contains -- -h $argv
    printf '%s\n' \
        '用法：fish scripts/build-docker.fish' \
        '使用国内依赖镜像源，在本机分别构建并加载 linux/amd64、linux/arm64 镜像。' \
        '产物：yggdrasil:amd64、yggdrasil:arm64；不推送、不部署。' \
        '异架构构建需要 Docker 的 QEMU/binfmt 支持。'
    exit 0
end
if test (count $argv) -ne 0
    echo '不接受构建参数（--help 查看用法）' >&2
    exit 1
end

set -l root (path resolve (status dirname)/..)
# 清空远端地址并固定 context，避免把构建发往环境中指定的远端 daemon。
set -lx DOCKER_CONTEXT default
set -lx DOCKER_HOST ''
cd $root; or exit 1

for cmd in docker make git
    if not type -q $cmd
        echo "缺少命令：$cmd" >&2
        exit 1
    end
end
if not docker info >/dev/null 2>&1
    echo 'Docker daemon 不可用' >&2
    exit 1
end
if not docker buildx version >/dev/null 2>&1
    echo 'Docker Buildx 不可用' >&2
    exit 1
end

# 覆盖环境中可能设置的远程 builder。
set -lx BUILDX_BUILDER default
set -l builder_info (docker buildx inspect default --bootstrap); or exit 1
for arch in amd64 arm64
    if not string match -rq -- 'linux/'$arch'([/,[:space:]]|$)' $builder_info
        echo "本机 builder 不支持 linux/$arch；请先配置 QEMU/binfmt。" >&2
        echo "例如：docker run --privileged --rm tonistiigi/binfmt --install $arch" >&2
        exit 1
    end
end

for arch in amd64 arm64
    echo "==> 构建 linux/$arch（CN_MIRROR=true）"
    make docker-$arch CN_MIRROR=true; or exit 1

    set -l platform (docker image inspect yggdrasil:$arch --format '{{.Os}}/{{.Architecture}}'); or exit 1
    if test "$platform" != "linux/$arch"
        echo "镜像架构不符：yggdrasil:$arch 实际为 $platform" >&2
        exit 1
    end
end

echo '==> 完成：yggdrasil:amd64、yggdrasil:arm64 已加载到本机，未推送'
