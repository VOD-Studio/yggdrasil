# Docker：镜像构建和 lint/test/fmt 工具容器。

.PHONY: docker docker-amd64 docker-arm64 docker-multiarch
.PHONY: docker-run docker-lint docker-clippy docker-check docker-fmt docker-fix docker-test
.PHONY: docker-tools-build docker-tools-clean

# Docker 镜像构建。两个 Dockerfile 对应两种场景（构建细节见各自文件头部）：
#
#   Dockerfile        目标架构 == 宿主架构时用，容器内原生构建（如 x86 Linux 构建 amd64）。
#   Dockerfile.cross  Apple Silicon 等非 x86_64 宿主构建 amd64 时用，
#                     容器内通过 cargo-zigbuild 交叉编译服务端，无需 QEMU 或宿主工具链。
#
#   make docker              只构建本机架构，加载到本地 daemon（用于测试）
#   make docker-amd64        x86_64 镜像；按宿主架构自动选 Dockerfile
#   make docker-arm64        arm64 镜像；异架构宿主需要 QEMU/binfmt 支持
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
VERSION_TAG_ARM64 = $(if $(VERSION),-t yggdrasil:$(VERSION)-arm64)
# build-arg 复用块:每个 docker target 展开一次。空值也传(让 Dockerfile 默认接管)。
GIT_BUILD_ARGS = --build-arg YGG_BUILD_GIT_DESCRIBE="$(GIT_DESCRIBE)" \
                 --build-arg YGG_BUILD_GIT_HASH="$(GIT_HASH)" \
                 --build-arg YGG_BUILD_GIT_COMMIT_DATE="$(GIT_DATE)"
# CN_MIRROR build-arg：传 CN_MIRROR=true 时透传 --build-arg CN_MIRROR=true，
# 否则不传（Dockerfile 内 ARG CN_MIRROR=false 默认关闭国内镜像）。
CN_BUILD_ARGS = $(if $(filter true,$(CN_MIRROR)),--build-arg CN_MIRROR=true)
DOCKER_BUILD_ARGS = $(GIT_BUILD_ARGS) $(CN_BUILD_ARGS) --build-arg WASM_FRONTEND_JOBS="$(WASM_FRONTEND_JOBS)"
docker: ## 构建本机架构镜像并加载到本地 daemon
	@docker buildx build --load $(DOCKER_BUILD_ARGS) \
		-t yggdrasil:latest $(VERSION_TAG) .

# x86_64 宿主用普通 Dockerfile，其他宿主（如 Apple Silicon）用 Dockerfile.cross。
# 产物可直接 docker run / docker save 导出。
DOCKERFILE_AMD64 = $(if $(filter x86_64,$(shell uname -m)),,-f Dockerfile.cross)
docker-amd64: ## 构建 amd64 镜像并加载到本地 daemon
	@docker buildx build --platform linux/amd64 --load $(DOCKERFILE_AMD64) $(DOCKER_BUILD_ARGS) \
		-t yggdrasil:amd64 $(VERSION_TAG_AMD64) .

docker-arm64: ## 构建 arm64 镜像并加载到本地 daemon（异架构需要 QEMU/binfmt）
	@docker buildx build --platform linux/arm64 --load $(DOCKER_BUILD_ARGS) \
		-t yggdrasil:arm64 $(VERSION_TAG_ARM64) .

docker-multiarch: ## 构建多架构镜像并推送（IMAGE=... PLATFORMS=...）
	@docker buildx build --platform $(PLATFORMS) $(DOCKER_BUILD_ARGS) -t $(IMAGE) --push .

# ── Docker 工具容器（lint / test / fix / check）──────────────────
# 工具容器提供隔离的 lint/fmt/fix/test/check 环境；bind mount 将修改写回宿主。
# 依赖与编译缓存使用独立命名卷，避免混用宿主产物，见 docker-compose.tools.yml。
TOOLS_COMPOSE := docker compose -f docker-compose.tools.yml
# 在工具容器内先冻结 lockfile 安装前端依赖，再于 /build 执行 `make <target>`。
tools_make = $(TOOLS_COMPOSE) run --rm tools bash -c 'cd libs && bun install --frozen-lockfile >/dev/null && cd /build && make $(1)'

# 一次性运行任意命令（例：make docker-run CMD='cargo build'）。
docker-run: ## 在工具容器内运行任意命令（CMD='...'）
	@$(TOOLS_COMPOSE) run --rm tools bash -c '$(CMD)'

# lint（只读）：clippy + cargo fmt --check + biome check + typecheck。
docker-lint: ## 容器内运行 make lint
	@$(call tools_make,lint)

# 仅 clippy（最常用的编译期检查，不需要 bun）。
docker-clippy: ## 容器内只运行 clippy
	@$(TOOLS_COMPOSE) run --rm tools cargo clippy --all-targets -- -D warnings

# 最快编译校验：cargo check（不跑 clippy lint、不跑测试）。
docker-check: ## 容器内运行 cargo check
	@$(TOOLS_COMPOSE) run --rm tools cargo check

# 格式化（写入文件，回流宿主）：委托 make fmt。
docker-fmt: ## 容器内运行 make fmt（写回宿主）
	@$(call tools_make,fmt)

# fix（写入文件，回流宿主）：委托 make fix。
docker-fix: ## 容器内运行 make fix（写回宿主）
	@$(call tools_make,fix)

# test：cargo test + libs 前端测试（vitest）。需要 Docker daemon 的 code-runner 测试自动 skip。
docker-test: ## 容器内运行 make test
	@$(call tools_make,test)

# 重建工具镜像（Dockerfile.tools 变更后可用）。
docker-tools-build: ## 重建工具镜像
	@$(TOOLS_COMPOSE) build

# 清理工具容器命名卷（释放磁盘；下次运行重新编译依赖）。
docker-tools-clean: ## 清理工具容器的命名卷
	@$(TOOLS_COMPOSE) down -v
