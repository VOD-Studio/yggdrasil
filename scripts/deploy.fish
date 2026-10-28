#!/usr/bin/env fish

# 在本机构建镜像并部署到生产 xun / rua（与 CI 的 deploy-xun / deploy-rua 同一流程）。
#
# 构建走国内镜像源：`make docker-amd64 CN_MIRROR=true`，Debian 用清华源，rustup/crates 用
# rsproxy，npm 用 npmmirror，GitHub 下载（Tailwind、分包合并补丁版 wasm-split）走 gh-proxy；
# 代码运行器沙箱镜像用 docker/build-runners.sh --cn-mirror（Alpine 清华源）。
# Docker Hub 基础镜像由本机 Docker daemon 的 registry-mirrors 决定。
#
# 镜像经 `docker save | gzip | ssh <host> podman load` 直传服务器，不经过 GHCR。两台都是
# rootful Podman，部署目录 /root/docker/yggdrasil，按目录下 ROLE 文件区分角色：
#   primary  记录当前镜像 → 换 tag → 重建 app → 经 nginx-proxy 检查 /readyz 与
#            x-yggdrasil-hash；失败自动回滚到部署前镜像（不回退数据库迁移）
#   standby  只上传并打 tag，不启动 app；仅在 primary 部署成功后进行，
#            保证故障切换时启动的版本与主节点一致
# primary 先部署、standby 后部署（平时即先 xun 再 rua，同 CI）。primary 成功后用
# scripts/edgeone-prefetch.ts 预热 EdgeOne（读本机 tccli 凭据）。
#
# 用法：
#   fish scripts/deploy.fish                  构建并部署到 xun、rua
#   fish scripts/deploy.fish --skip-build     用本机已有的 yggdrasil:amd64（不是 HEAD 构建的会在健康检查失败并回滚）
#   fish scripts/deploy.fish --with-runners   同时构建并上传 6 个代码运行器镜像（rust 约 900 MB）
#   fish scripts/deploy.fish --only xun       只部署一台（xun 或 rua）
#   fish scripts/deploy.fish --allow-dirty    允许工作区有未提交改动
#
# 部署的是本机 HEAD；线上响应头 x-yggdrasil-hash 会是这个 commit，未推送的提交也会上线。

set -l root (path resolve (status dirname)/..)
cd $root; or exit 1

set -l hosts xun rua
set -l deploy_dir /root/docker/yggdrasil
set -l skip_build false
set -l with_runners false
set -l allow_dirty false
set -l runners base python node go rust bun

set -l i 1
while test $i -le (count $argv)
    switch $argv[$i]
        case --skip-build
            set skip_build true
        case --with-runners
            set with_runners true
        case --allow-dirty
            set allow_dirty true
        case --only
            set i (math $i + 1)
            if not contains -- "$argv[$i]" xun rua
                echo "--only 只接受 xun 或 rua" >&2
                exit 1
            end
            set hosts $argv[$i]
        case -h --help
            sed -n '3,26p' (status filename) | string replace -r '^# ?' ''
            exit 0
        case '*'
            echo "未知参数：$argv[$i]（--help 查看用法）" >&2
            exit 1
    end
    set i (math $i + 1)
end

for cmd in docker make git gzip ssh
    if not type -q $cmd
        echo "缺少命令：$cmd" >&2
        exit 1
    end
end
if not docker info >/dev/null 2>&1
    echo "Docker daemon 不可用" >&2
    exit 1
end

set -l commit (git rev-parse HEAD); or exit 1
set -l short (string sub -l 7 $commit)
if test $allow_dirty = false; and test -n "$(git status --porcelain)"
    echo "工作区有未提交改动；提交后再部署，或加 --allow-dirty" >&2
    exit 1
end
set -l ref localhost/yggdrasil:sha-$short-local

# ── 构建 ───────────────────────────────────────────────────────
if test $skip_build = false
    echo "==> 构建 amd64 应用镜像（CN_MIRROR=true，commit $short）"
    make docker-amd64 CN_MIRROR=true; or exit 1
end
if not docker image inspect yggdrasil:amd64 >/dev/null 2>&1
    echo "本机没有 yggdrasil:amd64，去掉 --skip-build 重新构建" >&2
    exit 1
end
docker tag yggdrasil:amd64 $ref; or exit 1

if test $with_runners = true
    echo "==> 构建代码运行器沙箱镜像（清华 Alpine 源）"
    sh docker/build-runners.sh --cn-mirror; or exit 1
end

# ── 远端脚本（bash）。参数：镜像 ref、部署目录、角色、期望 commit、需要检查的 runner 列表 ──
set -l remote_script '
set -euo pipefail
REF="$1"; DIR="$2"; ROLE="$3"; HASH="$4"; shift 4
cd "$DIR"

podman image inspect "$REF" >/dev/null || { echo "镜像 $REF 未加载"; exit 1; }
missing=()
for img in "$@"; do
  podman image inspect "localhost/yggdrasil-runner-$img:latest" >/dev/null 2>&1 \
    || missing+=("yggdrasil-runner-$img")
done
if ((${#missing[@]} > 0)); then
  echo "缺少运行器镜像：${missing[*]}（加 --with-runners 上传）"; exit 1
fi

if [[ "$ROLE" == primary ]]; then
  prev="$(podman inspect yggdrasil-app --format "{{.Image}}" 2>/dev/null || true)"
  printf "%s\n" "$prev" > .previous-app-image
  echo "  部署前镜像：${prev:-<无>}"
fi
podman tag "$REF" localhost/yggdrasil:latest
if [[ "$ROLE" != primary ]]; then
  echo "  standby：镜像已就位，不启动 app"; exit 0
fi

ready() {
  local i headers
  for i in $(seq 1 12); do
    if headers="$(podman exec nginx-proxy curl -fsS -D - -o /dev/null http://yggdrasil-app:3000/readyz </dev/null 2>/dev/null)" \
      && [[ -z "$1" || "$(printf "%s" "$headers" | tr -d "\r" | grep -i "^x-yggdrasil-hash:")" == *"$1"* ]]; then
      return 0
    fi
    sleep 5
  done
  return 1
}

podman compose --env-file .env up -d --no-deps --force-recreate app </dev/null || echo "  compose up 失败"
expected="$(podman image inspect "$REF" --format "{{.Id}}")"
running="$(podman inspect yggdrasil-app --format "{{.Image}}" 2>/dev/null || true)"
if [[ "${expected#sha256:}" == "${running#sha256:}" ]] && ready "$HASH"; then
  podman ps --filter name=yggdrasil --format "  {{.Names}}\t{{.Status}}"
  exit 0
fi

echo "  新版本未就绪或运行版本不是 $HASH，回滚"
if [[ -n "${prev:-}" ]] && podman image inspect "$prev" >/dev/null 2>&1; then
  podman tag "$prev" localhost/yggdrasil:latest
  podman compose --env-file .env up -d --no-deps --force-recreate app </dev/null || true
  if ready ""; then echo "  已回滚到 $prev"; else echo "  回滚后 /readyz 仍未就绪"; fi
else
  echo "  没有可用的部署前镜像，无法回滚"
fi
exit 1
'

function __ygg_upload --argument-names host image
    echo "  上传 $image → $host"
    docker save $image | gzip -1 | ssh $host podman load >/dev/null
    and return 0
    echo "上传 $image 到 $host 失败" >&2
    return 1
end

# ── 逐台部署：先读各台角色，primary 在前、standby 在后 ──────────
set -l primaries
set -l standbys
set -l failed
for host in $hosts
    set -l role (ssh $host cat $deploy_dir/ROLE 2>/dev/null </dev/null | string trim)
    test -n "$role"; or set role primary
    switch $role
        case primary
            set -a primaries $host
        case standby
            set -a standbys $host
        case '*'
            echo "$host 的 ROLE 无效：$role" >&2
            set -a failed $host
    end
end

set -l primary_ok false
for host in $primaries $standbys
    set -l role primary
    contains -- $host $standbys; and set role standby
    echo "==> $host（$role）"

    # standby 只跟随已成功的 primary；本次没有要部署的 primary（如 --only rua）时直接部署。
    if test $role = standby; and test (count $primaries) -gt 0; and test $primary_ok = false
        echo "  primary 未部署成功，跳过 standby（保留旧镜像）"
        set -a failed $host
        continue
    end

    set -l uploads $ref
    if test $with_runners = true
        for r in $runners
            docker tag yggdrasil-runner-$r:latest localhost/yggdrasil-runner-$r:latest; or exit 1
            set -a uploads localhost/yggdrasil-runner-$r:latest
        end
    end
    set -l uploaded true
    for image in $uploads
        if not __ygg_upload $host $image
            set uploaded false
            break
        end
    end
    if test $uploaded = false
        set -a failed $host
        continue
    end

    if printf '%s\n' $remote_script | ssh $host bash -s -- $ref $deploy_dir $role $commit $runners
        test $role = primary; and set primary_ok true
        echo "  $host 完成"
    else
        set -a failed $host
    end
end

# 新版本接流量后把分包与静态资源预热到 EdgeOne 中间层（本机 tccli 凭据），失败只警告。
if test $primary_ok = true
    set -q EDGEONE_ZONE_ID; or set -lx EDGEONE_ZONE_ID zone-3whynintait8
    bun scripts/edgeone-prefetch.ts --expect-hash $commit
    or echo "  警告：EdgeOne 预热失败，不影响部署" >&2
end

if test (count $failed) -gt 0
    echo "==> 部署未全部成功：$failed" >&2
    exit 1
end
echo "==> 部署完成：$ref（$commit）"
