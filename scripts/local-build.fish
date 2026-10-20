#!/usr/bin/env fish

# 本地用国内镜像（CN_MIRROR=true）构建 amd64 主应用镜像，可选直接部署到 xun + rua。
# 用于 CI 排队过久时的应急发布；CI 之后仍会按 tag 再部署一次同一 commit。
#
# 用法:
#   fish scripts/local-build.fish            只构建，产物 localhost/yggdrasil:latest
#   fish scripts/local-build.fish --deploy   构建后部署到 xun（docker）和 rua（podman）
#
# 前提: ~/.ssh/config 已配 xun / rua；两台的 /root/docker/yggdrasil 下有 compose 与 .env。
# 只替换 app 容器，postgres / pg-standby / pg-tunnel / runner 镜像都不动。

set -l DEPLOY false
for arg in $argv
    switch $arg
        case --deploy
            set DEPLOY true
        case -h --help
            sed -n '3,11p' (status filename)
            exit 0
        case '*'
            echo "未知参数: $arg" >&2
            exit 1
    end
end

set -l DIR /root/docker/yggdrasil
set -l TAR /tmp/yggdrasil-app.tar.gz

cd (dirname (status filename))/..; or exit 1

if test -n "$(git status --porcelain --untracked-files=no)"
    echo "工作区有未提交改动，镜像的 git hash 会对不上，先提交或 stash" >&2
    exit 1
end
set -l HASH (git rev-parse HEAD)
echo "==> 构建 commit: "(git describe --tags --always)" ($HASH)"

echo "==> [1] 构建 amd64 镜像（CN_MIRROR=true）"
set -l start (date +%s)
make docker-amd64 CN_MIRROR=true; or exit 1
docker tag yggdrasil:amd64 localhost/yggdrasil:latest; or exit 1
set -l arch (docker image inspect localhost/yggdrasil:latest --format '{{.Architecture}}')
if test "$arch" != amd64
    echo "架构错误: $arch，期望 amd64" >&2
    exit 1
end
echo "    构建耗时 "(math (date +%s) - $start)"s"

if test $DEPLOY != true
    echo "==> 完成（未部署）。部署请加 --deploy"
    exit 0
end

echo "==> [2] 导出镜像"
docker save localhost/yggdrasil:latest | gzip -1 > $TAR; or exit 1
ls -lh $TAR

# 远端默认 shell 是 fish，统一经 bash -c 执行。
# 用法: deploy_host <ssh 别名> <docker|podman>
function deploy_host -a host engine dir tar hash
    echo "==> [$host] 传输并导入镜像"
    ssh $host "bash -c 'gunzip | $engine load'" < $tar; or return 1

    echo "==> [$host] 重建 app 容器"
    ssh $host "bash -c 'cd $dir && $engine compose --env-file .env up -d --no-deps --force-recreate app'"; or return 1

    echo "==> [$host] 等待 /readyz 且版本为 $hash"
    for i in (seq 1 12)
        set -l headers (ssh $host "$engine exec nginx-proxy curl -fsS -D - -o /dev/null http://yggdrasil-app:3000/readyz" 2>/dev/null)
        if string match -qi "*x-yggdrasil-hash: $hash*" -- $headers
            echo "    [$host] readyz + 版本 OK"
            return 0
        end
        sleep 5
    end
    echo "    [$host] /readyz 未就绪或版本不对" >&2
    return 1
end

set -l failed
# xun 是 PostgreSQL Primary，先部署；失败则不继续动 rua。
deploy_host xun docker $DIR $TAR $HASH; or set -a failed xun
if not contains xun $failed
    deploy_host rua podman $DIR $TAR $HASH; or set -a failed rua
end
rm -f $TAR

echo "--- 外部 HTTPS ---"
curl -s https://rua.plus/healthz; echo

if test (count $failed) -gt 0
    echo "==> 部署失败: $failed" >&2
    exit 1
end
echo "==> 双机部署完成"
