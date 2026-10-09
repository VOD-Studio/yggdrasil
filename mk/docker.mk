# Docker：镜像构建、Docker 开发环境、lint/test/fmt 工具容器。

.PHONY: docker docker-amd64 docker-multiarch docker-dev docker-dev-down docker-dev-shell
.PHONY: docker-run docker-lint docker-clippy docker-check docker-fmt docker-fix docker-test
.PHONY: docker-tools-build docker-tools-clean

# Docker 镜像构建。两个 Dockerfile 对应两种场景（构建细节见各自文件头部）：
#
#   Dockerfile        目标架构 == 宿主架构时用，容器内原生构建（如 x86 Linux 构建 amd64）。
#   Dockerfile.cross  目标架构 != 宿主架构时用（如 Apple Silicon 构建 amd64），
#                     全程在容器内原生编译，无 QEMU / Rosetta / 宿主工具链，只需 Docker。
#
#   make docker              只构建本机架构，加载到本地 daemon（用于测试）
#   make docker-amd64        x86_64 镜像；按宿主架构自动选 Dockerfile
#   make docker-multiarch    构建 amd64+arm64 并推送到 registry
#                            （多架构 manifest 无法 --load 到本地）
#
# 推送示例：
#   make docker-multiarch IMAGE=ghcr.io/owner/yggdrasil:latest
#   make docker-multiarch IMAGE=user/yggdrasil:v1 PLATFORMS=linux/amd64
#
# git 信息透传:.dockerignore 排除 .git/,容器内取不到 git 信息,所以在宿主采集后
# 用 --build-arg 注入(取值顺序见 build.rs)。所有 docker target 复用;git 不可用时
# 为空串,build.rs 最终降级为 "unknown",不阻断构建。
# 下面的 $(shell ...) 一律用 `=` 延迟求值：只有 docker target 的配方展开到它们时才会
# 真正启动 git/uname；用 `:=` 的话每次 make（包括 make lint）都要白白付这几个进程的成本。
IMAGE ?= yggdrasil
PLATFORMS ?= linux/amd64,linux/arm64
GIT_DESCRIBE = $(shell git describe --tags --always --dirty 2>/dev/null)
GIT_HASH = $(shell git rev-parse HEAD 2>/dev/null)
GIT_DATE = $(shell git log -1 --format=%cd --date=iso-strict 2>/dev/null)
# 镜像版本号:取最近 git tag 原值(v0.10.0,带 v,与 CI publish-ghcr 的 GITHUB_REF_NAME 一致);
# 可用 VERSION=v0.10.1 覆盖。生产镜像据此打版本 tag(yggdrasil:v0.10.0、yggdrasil:v0.10.0-amd64)。
# 没有 tag 时 VERSION 为空，此时只打 latest/amd64，不生成非法的 `yggdrasil:` tag。
VERSION ?= $(shell git describe --tags --abbrev=0 2>/dev/null)
VERSION_TAG = $(if $(VERSION),-t yggdrasil:$(VERSION))
VERSION_TAG_AMD64 = $(if $(VERSION),-t yggdrasil:$(VERSION)-amd64)
# build-arg 复用块:每个 docker target 展开一次。空值也传(让 Dockerfile 默认接管)。
GIT_BUILD_ARGS = --build-arg YGG_BUILD_GIT_DESCRIBE="$(GIT_DESCRIBE)" \
                 --build-arg YGG_BUILD_GIT_HASH="$(GIT_HASH)" \
                 --build-arg YGG_BUILD_GIT_COMMIT_DATE="$(GIT_DATE)"
# CN_MIRROR build-arg：传 CN_MIRROR=true 时透传 --build-arg CN_MIRROR=true，
# 否则不传（Dockerfile 内 ARG CN_MIRROR=false 默认关闭国内镜像）。
CN_BUILD_ARGS = $(if $(filter true,$(CN_MIRROR)),--build-arg CN_MIRROR=true)
docker: ## 构建本机架构镜像并加载到本地 daemon
	@docker buildx build --load $(GIT_BUILD_ARGS) $(CN_BUILD_ARGS) \
		-t yggdrasil:latest $(VERSION_TAG) .

# x86_64 宿主用普通 Dockerfile，其他宿主（如 Apple Silicon）用 Dockerfile.cross。
# 产物可直接 docker run / docker save 导出。
DOCKERFILE_AMD64 = $(if $(filter x86_64,$(shell uname -m)),,-f Dockerfile.cross)
docker-amd64: ## 构建 amd64 镜像并加载到本地 daemon
	@docker buildx build --platform linux/amd64 --load $(DOCKERFILE_AMD64) $(GIT_BUILD_ARGS) $(CN_BUILD_ARGS) \
		-t yggdrasil:amd64 $(VERSION_TAG_AMD64) .

docker-multiarch: ## 构建多架构镜像并推送（IMAGE=... PLATFORMS=...）
	@docker buildx build --platform $(PLATFORMS) $(GIT_BUILD_ARGS) $(CN_BUILD_ARGS) -t $(IMAGE) --push .

# ── Docker 开发环境 ────────────────────────────────────────────
# 使用 Dockerfile.dev + docker-compose.dev.yml 在容器内运行 dx serve；源码同步、
# 网络和数据库见 docker-compose.dev.yml 头部。
# 首次启动需编译 Rust 依赖（~10 分钟），后续启动约 10 秒（cargo target 缓存）。
# up --build 后前台跑 compose watch；Ctrl+C 只停 watch，容器继续后台跑。
docker-dev: ## 启动 Docker 开发环境并 watch 同步源码
	@docker compose -f docker-compose.dev.yml up --build -d
	@docker compose -f docker-compose.dev.yml watch

# 停止并移除 dev 容器（volume 数据保留）。
docker-dev-down: ## 停止并移除开发容器
	@docker compose -f docker-compose.dev.yml down

# 进入 dev 容器的交互式 shell（容器需已在运行）。
docker-dev-shell: ## 进入开发容器的 shell
	@docker compose -f docker-compose.dev.yml exec dev bash

# ── Docker 工具容器（lint / test / fix / check）──────────────────
# lint/fmt/fix/test/check 一律在容器内跑，避开本机 EDR 对构建链二进制的拦截；
# bind mount 让 fmt/fix 的改动回流宿主。原因与卷策略见 docker-compose.tools.yml。
TOOLS_COMPOSE := docker compose -f docker-compose.tools.yml
# 在工具容器内先冻结 lockfile 安装前端依赖，再于 /build 执行 `make <target>`。
tools_make = $(TOOLS_COMPOSE) run --rm tools bash -c 'cd libs && bun install --frozen-lockfile >/dev/null && cd /build && make $(1)'

# 一次性运行任意命令（例：make docker-run CMD='cargo build --features server'）。
docker-run: ## 在工具容器内运行任意命令（CMD='...'）
	@$(TOOLS_COMPOSE) run --rm tools bash -c '$(CMD)'

# lint（只读）：clippy + cargo fmt --check + biome check + typecheck。
docker-lint: ## 容器内运行 make lint
	@$(call tools_make,lint)

# 仅 clippy（最常用的编译期检查，不需要 bun）。
docker-clippy: ## 容器内只运行 clippy
	@$(TOOLS_COMPOSE) run --rm tools cargo clippy --all-targets --all-features -- -D warnings

# 最快编译校验：cargo check --all-features（不跑 clippy lint、不跑测试）。
docker-check: ## 容器内运行 cargo check --all-features
	@$(TOOLS_COMPOSE) run --rm tools cargo check --all-features

# 格式化（写入文件，回流宿主）：委托 make fmt，不含 dx fmt（原因见 fmt）。
docker-fmt: ## 容器内运行 make fmt（写回宿主）
	@$(call tools_make,fmt)

# fix（写入文件，回流宿主）：委托 make fix，同样不含 dx fmt。
docker-fix: ## 容器内运行 make fix（写回宿主）
	@$(call tools_make,fix)

# test：cargo test + libs 前端测试（vitest）。需要 Docker daemon 的 code-runner 测试自动 skip。
docker-test: ## 容器内运行 make test
	@$(call tools_make,test)

# 重建工具镜像（Dockerfile.dev 变更后用；正常情况下 run 会按需自动构建）。
docker-tools-build: ## 重建工具镜像
	@$(TOOLS_COMPOSE) build

# 清理工具容器命名卷（释放磁盘；下次运行重新编译依赖）。
docker-tools-clean: ## 清理工具容器的命名卷
	@$(TOOLS_COMPOSE) down -v
