#!/usr/bin/env fish

# Yggdrasil 双机部署到 xun + rua(共享 PostgreSQL + uploads 双向同步)
#
# 拓扑:
#   - xun(OCI): PostgreSQL Primary + app + runner 沙箱
#   - rua(腾讯云): app(经 SSH 隧道连 xun PG)+ PG 热备(流式复制,灾备)
#   - rua 的 app 不连本地 PG,而是经 pg-tunnel.service(SSH -L 15432)连 xun Primary
#   - uploads/ 双向 rsync systemd timer 每分钟同步(--ignore-existing,文件名 HHMMSS.uuid 天然唯一)
#
# 用法: fish scripts/both.fish
# 前提: ~/.ssh/config 已配 xun(HostName xunrua.top)和 rua(HostName api.rua.plus Port 29888)
#        xun 上已跑 postgres + app + nginx-proxy;rua 上已跑 app + pg-standby + pg-tunnel.service

# 5 个 runner 子镜像(base 由脚本自己先建,FROM 它)。runner 仅部署到 xun(rua 内存不够跑沙箱)
set -l RUNNERS python node go rust bun
# 两台目标主机(对应 ~/.ssh/config 的 Host 别名)
set -l PRIMARY xun      # PostgreSQL Primary + runner 沙箱
set -l REPLICA rua      # 无状态前端 + PG 热备

echo "==> [1/9] 构建 runner base 镜像"
docker buildx build --platform linux/amd64 --load \
  -t localhost/yggdrasil-runner-base:latest docker/runner-base; or exit 1
docker tag localhost/yggdrasil-runner-base:latest yggdrasil-runner-base:latest; or exit 1

echo "==> [2/9] 构建主应用镜像(容器内 zig 交叉编译 x86_64,无 QEMU)"
# make docker-amd64 在 arm64 机走 Dockerfile.cross(双 builder:Trixie 前端 + Alpine server)
# git 信息透传由 Makefile 内部 GIT_BUILD_ARGS 完成
make docker-amd64; or exit 1
# 脚本后续统一用 localhost/yggdrasil:latest,这里对齐 tag
docker tag yggdrasil:amd64 localhost/yggdrasil:latest; or exit 1

echo "==> [3/9] 构建 5 个 runner 子镜像"
for img in $RUNNERS
    echo "  -- $img"
    docker buildx build --platform linux/amd64 --load \
      -t localhost/yggdrasil-runner-$img:latest docker/runner-$img; or exit 1
    docker tag localhost/yggdrasil-runner-$img:latest yggdrasil-runner-$img:latest; or exit 1
end

echo "==> [4/9] 构建验证(期望全 amd64)"
set -l ALL_IMAGES yggdrasil yggdrasil-runner-base
for img in $RUNNERS
    set -a ALL_IMAGES yggdrasil-runner-$img
end
for img in $ALL_IMAGES
    set -l arch (docker image inspect localhost/$img:latest --format "{{.Architecture}}")
    echo "  $img: $arch"
    if test "$arch" != "amd64"
        echo "  架构错误!期望 amd64" >&2
        exit 1
    end
end

echo "==> [5/9] 导出镜像"
# 主应用单独一个 tar(两台都要)
docker save localhost/yggdrasil:latest -o /tmp/yggdrasil-app.tar; or exit 1
# 6 个 runner 镜像打包成一个 tar(仅 xun 需要)
docker save \
  localhost/yggdrasil-runner-base:latest \
  localhost/yggdrasil-runner-python:latest \
  localhost/yggdrasil-runner-node:latest \
  localhost/yggdrasil-runner-go:latest \
  localhost/yggdrasil-runner-rust:latest \
  localhost/yggdrasil-runner-bun:latest \
  -o /tmp/yggdrasil-runners.tar; or exit 1
gzip -f /tmp/yggdrasil-app.tar /tmp/yggdrasil-runners.tar; or exit 1

echo "==> [6/9] 传输:主应用到两台,runner 仅到 $PRIMARY"
# 主应用 + runner → xun
scp /tmp/yggdrasil-app.tar.gz /tmp/yggdrasil-runners.tar.gz $PRIMARY:/root/docker/yggdrasil/; or exit 1
# 主应用 → rua(经 xun 中转,云内网更快;ssh 已配 IPv4)
ssh $PRIMARY "rsync -4 -az -e 'ssh -4 -p 29888 -i /root/.ssh/id_ed25519 -o StrictHostKeyChecking=accept-new' /root/docker/yggdrasil/yggdrasil-app.tar.gz $REPLICA:/root/docker/yggdrasil/"; or exit 1
rm -f /tmp/yggdrasil-app.tar.gz /tmp/yggdrasil-runners.tar.gz

echo "==> [7/9] $PRIMARY:导入 + runner 去前缀 + 滚动重启 app"
ssh $PRIMARY 'cd /root/docker/yggdrasil && gunzip -kf yggdrasil-app.tar.gz && gunzip -kf yggdrasil-runners.tar.gz'; or exit 1
ssh $PRIMARY 'docker load -i /root/docker/yggdrasil/yggdrasil-app.tar'; or exit 1
ssh $PRIMARY 'docker load -i /root/docker/yggdrasil/yggdrasil-runners.tar'; or exit 1
# runner 去 localhost/ 前缀:LANGUAGES 注册表硬编码 yggdrasil-runner-*:latest
ssh $PRIMARY 'docker tag localhost/yggdrasil-runner-base:latest yggdrasil-runner-base:latest'
ssh $PRIMARY 'docker tag localhost/yggdrasil-runner-python:latest yggdrasil-runner-python:latest'
ssh $PRIMARY 'docker tag localhost/yggdrasil-runner-node:latest yggdrasil-runner-node:latest'
ssh $PRIMARY 'docker tag localhost/yggdrasil-runner-go:latest yggdrasil-runner-go:latest'
ssh $PRIMARY 'docker tag localhost/yggdrasil-runner-rust:latest yggdrasil-runner-rust:latest'
ssh $PRIMARY 'docker tag localhost/yggdrasil-runner-bun:latest yggdrasil-runner-bun:latest'
# 滚动重启:只重建 app 容器(postgres 和数据卷不动)
ssh $PRIMARY 'cd /root/docker/yggdrasil && docker compose --env-file .env up -d app'; or exit 1
ssh $PRIMARY 'rm -f /root/docker/yggdrasil/yggdrasil-app.tar* /root/docker/yggdrasil/yggdrasil-runners.tar*'

echo "==> [8/9] $REPLICA:导入主应用 + 滚动重启 app(PG 隧道不动)"
ssh $REPLICA 'cd /root/docker/yggdrasil && gunzip -kf yggdrasil-app.tar.gz'; or exit 1
ssh $REPLICA 'podman load -i /root/docker/yggdrasil/yggdrasil-app.tar'; or exit 1
# rua 的 app 经 SSH 隧道(10.89.0.1:15432)连 xun PG;pg-tunnel.service + pg-standby 不动
ssh $REPLICA 'cd /root/docker/yggdrasil && podman compose --env-file .env up -d app'; or exit 1
ssh $REPLICA 'rm -f /root/docker/yggdrasil/yggdrasil-app.tar*'

echo "==> [9/9] 双机验证"
echo "--- $PRIMARY 容器(postgres healthy + app up)---"
ssh $PRIMARY 'docker ps --filter name=yggdrasil --format "{{.Names}} {{.Status}}"'
echo "--- $REPLICA 容器(app up + pgstandby up)---"
ssh $REPLICA 'podman ps --filter name=yggdrasil --format "{{.Names}} {{.Status}}"'
echo "--- $PRIMARY 迁移日志(期望 applied,无 error/panic)---"
ssh $PRIMARY 'docker logs yggdrasil-app 2>&1 | grep -iE "migrat|error|panic" | tail -3'
echo "--- $REPLICA 迁移日志(期望 up to date,无 error/panic)---"
ssh $REPLICA 'podman logs yggdrasil-app 2>&1 | grep -iE "migrat|error|panic" | tail -3'
echo "--- 两台 app 健康检查 ---"
echo -n "  $PRIMARY: "; ssh $PRIMARY 'docker exec nginx-proxy curl -s http://yggdrasil-app:3000/healthz'; echo
echo -n "  $REPLICA: "; ssh $REPLICA 'podman exec nginx-proxy curl -s http://yggdrasil-app:3000/healthz'; echo
echo "--- 两台版本头(应一致)---"
echo -n "  $PRIMARY: "; ssh $PRIMARY 'docker exec nginx-proxy curl -sI http://yggdrasil-app:3000/ | grep -i x-yggdrasil-git'
echo -n "  $REPLICA: "; ssh $REPLICA 'podman exec nginx-proxy curl -sI http://yggdrasil-app:3000/ | grep -i x-yggdrasil-git'
echo "--- PG 复制延迟(期望 0)---"
ssh $PRIMARY 'docker exec yggdrasil-postgres psql -U yggdrasil -d yggdrasil -tAc \
  "select coalesce((sent_lsn - replay_lsn),0) from pg_stat_replication"'
echo "--- PG 隧道状态($REPLICA)---"
ssh $REPLICA 'systemctl is-active pg-tunnel.service'
echo "--- uploads 同步 timer(两台都应 active)---"
echo -n "  $PRIMARY: "; ssh $PRIMARY 'systemctl is-active ygg-uploads-sync.timer'
echo -n "  $REPLICA: "; ssh $REPLICA 'systemctl is-active ygg-uploads-sync.timer'
echo "--- 外部 HTTPS ---"
curl -s https://rua.plus/healthz; echo

echo "==> 双机部署完成"
echo "    $PRIMARY (Primary): PostgreSQL + app + runner"
echo "    $REPLICA (Replica): app + PG 热备(经隧道连 Primary)"
