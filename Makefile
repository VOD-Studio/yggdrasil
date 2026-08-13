.PHONY: dev build build-linux build-freebsd freebsd-sysroot docker docker-amd64 docker-apple docker-multiarch docker-dev docker-dev-down docker-dev-shell docker-run docker-lint docker-clippy docker-check docker-fmt docker-fix docker-test docker-tools-build docker-tools-clean css css-watch clean build-libs build-editor build-codemirror build-lightbox build-core build-xterm highlight-css katex-css test doc doc-open start lint fix restore-webp esbuild-cache wasm-bindgen-cache
.PHONY: precompress

# ── sccache × dx 兼容 ──────────────────────────────────────────
# dx build / dx serve 构建时把自己设为 RUSTC_WORKSPACE_WRAPPER 拦截 workspace
# crate 的 rustc 调用（资产捕获）。若宿主 ~/.cargo/config.toml 配了
# [build] rustc-wrapper（如 sccache），cargo 会组合出 `sccache dx rustc …`：
# sccache 把 dx 当编译器探测，探测必然失败（"Compiler not supported"）→ dx 报错。
# 空 RUSTC_WRAPPER env 覆盖 config（env 优先于 config，空值 = 无 wrapper），
# 只对 dx 构建关闭 sccache；直接 cargo 的构建（test / lint / Dockerfile / CI）不受影响。
# 下面的 build / build-linux / dev 三个 target 的 dx 调用都带此前缀。
build:
	@rm -rf static/
	@$(MAKE) build-libs
	@$(MAKE) highlight-css
	@$(MAKE) katex-css
	@tailwindcss -i input.css -o public/style.css --minify
	@$(MAKE) doc
	@RUSTC_WRAPPER= dx build --release --debug-symbols=false
	@$(MAKE) restore-webp
	@$(MAKE) precompress

build-linux:
	@$(MAKE) build-libs
	@$(MAKE) highlight-css
	@$(MAKE) katex-css
	@tailwindcss -i input.css -o public/style.css --minify
	@RUSTC_WRAPPER= dx build @client --release --debug-symbols=false --wasm-js-cfg false
	@RUSTC_WRAPPER= dx build @server --release --debug-symbols=false --target x86_64-unknown-linux-musl --wasm-js-cfg false --features server
	@$(MAKE) restore-webp
	@$(MAKE) precompress
	@echo ""
	@echo "Linux build complete! The server binary is at target/dx/yggdrasil/release/web/server"
	@echo "Remember to deploy it alongside the target/dx/yggdrasil/release/web/public directory."
	@echo "When running the server, ensure DIOXUS_ASSET_DIR is set or the public directory is in CWD."

# FreeBSD 15.1 base.txz 版本与下载源。sysroot 仅需 ./lib 与 ./usr/lib（crt 对象 + 系统库）。
FREEBSD_VERSION ?= 15.1-RELEASE
FREEBSD_BASE_URL ?= https://download.freebsd.org/ftp/releases/amd64/amd64/$(FREEBSD_VERSION)/base.txz
FREEBSD_SYSROOT := $(CURDIR)/.freebsd-sysroot

# 下载并解压 FreeBSD base.txz 到 .freebsd-sysroot/，供交叉链接（crt 对象 + 系统库）。
# 幂等：若 sysroot 已存在则跳过下载。
freebsd-sysroot:
	@if [ -d "$(FREEBSD_SYSROOT)/usr/lib" ] && [ -d "$(FREEBSD_SYSROOT)/lib" ]; then \
		echo "FreeBSD sysroot already present at $(FREEBSD_SYSROOT)"; \
	else \
		echo "Downloading FreeBSD $(FREEBSD_VERSION) base.txz..."; \
		curl -fL --retry 3 -o /tmp/freebsd-base.txz "$(FREEBSD_BASE_URL)"; \
		mkdir -p "$(FREEBSD_SYSROOT)"; \
		echo "Extracting crt objects and system libs..."; \
		tar -xf /tmp/freebsd-base.txz -C "$(FREEBSD_SYSROOT)" ./lib ./usr/lib; \
		rm -f /tmp/freebsd-base.txz; \
		echo "FreeBSD sysroot ready at $(FREEBSD_SYSROOT)"; \
	fi

# 交叉编译 FreeBSD x86_64 release server 二进制。
# 前置：clang + lld 已装（pacman -S clang lld）、rustup target add x86_64-unknown-freebsd、
# `make freebsd-sysroot`。sysroot 路径经 CARGO_TARGET_*_RUSTFLAGS 注入，避免在
# .cargo/config.toml 里硬编码机器相关路径。server 二进制用 cargo 直出（dx CLI 对该
# target 未经验证）；前端 wasm 与静态资源与 build-linux 相同，不在此重复构建。
build-freebsd:
	@$(MAKE) freebsd-sysroot
	@SYSROOT="$(FREEBSD_SYSROOT)"; \
	RUSTFLAGS_FREEBSD="-C linker=clang -C link-arg=--target=x86_64-unknown-freebsd -C link-arg=-fuse-ld=lld -C link-arg=--sysroot=$$SYSROOT -C link-arg=-L$$SYSROOT/usr/lib -C link-arg=-L$$SYSROOT/lib"; \
	echo "Cross-compiling yggdrasil server for FreeBSD x86_64..."; \
	CARGO_TARGET_X86_64_UNKNOWN_FREEBSD_RUSTFLAGS="$$RUSTFLAGS_FREEBSD" \
		cargo build --release --target x86_64-unknown-freebsd --features server --bin yggdrasil
	@echo ""
	@echo "FreeBSD build complete! Server binary: target/x86_64-unknown-freebsd/release/yggdrasil"
	@echo "Deploy it to FreeBSD 15+ alongside the static public/ directory."
	@echo "Runtime needs (bundled in FreeBSD base): libc.so.7 libthr.so.3 libkvm.so.7 etc."

# 兜底：dx build 0.7.10 会把 public/ 下的 .webp 重编码成 VP8L 无损静图
# （动画帧被丢弃，静图体积反增 7-8 倍），与文档承诺的"原样拷贝"不符。
# SVG/ICO 等其他格式不受影响，故只需覆盖 .webp。
# 遍历所有 dx 产物目录（release/debug），用源 public/ 的同名文件覆盖回去。
# 仅覆盖产物中已存在的 .webp，不引入源里新增但 dx 未生成的文件。
# 参考：https://dioxuslabs.com/learn/0.7/essentials/ui/assets/
# 上游修复后可移除此 target 及 build/build-linux 里的调用。
restore-webp:
	@find target/dx -type d -path "*/web/public" 2>/dev/null | while read prod; do \
		find "$$prod" -type f -name "*.webp" 2>/dev/null | while read p; do \
			rel=$${p#$$prod/}; \
			src="public/$$rel"; \
			if [ -f "$$src" ]; then \
				cp "$$src" "$$p"; \
			else \
				echo "restore-webp: 源缺失，跳过 $$rel"; \
			fi; \
		done; \
	done

# Pre-compress static text assets with brotli (.br sidecars). dioxus-server
# registers ServeFile::precompressed_br() for every public leaf, so .br
# sidecars are auto-served on Accept-Encoding: br. Text formats only; fonts/
# images are already compressed. dx's pre_compress already covers assets/.
precompress:
	@find target/dx/yggdrasil/release/web/public -type f \
		\( -name '*.js' -o -name '*.css' -o -name '*.wasm' \
		   -o -name '*.svg' -o -name '*.html' -o -name '*.json' -o -name '*.xml' \) \
		-not -name '*.br' \
		-print0 | xargs -0 -r brotli -q 11 -kf

# Pre-populate dx 的 esbuild 工具缓存（国内镜像加速）。
# dx CLI 硬编码 esbuild 下载源为 registry.npmjs.org（packages/cli/src/esbuild.rs:62），
# 不读 NPM_CONFIG_REGISTRY 也不读 .npmrc——npm config set registry 无效。
# 此处从 npmmirror（阿里）预下载与 dx 内置 ESBUILD_VERSION 完全一致的 tarball
# （SHA256 与 npmjs.org 相同），解压到 dx 缓存目录。dx 的 esbuild.rs:24-28 在
# path.exists() 命中时跳过联网下载。
# 升级 dx 后须同步 ESBUILD_VERSION（查 dx 源码 esbuild.rs 的 ESBUILD_VERSION 常量）。
ESBUILD_VERSION := 0.27.3
esbuild-cache:
	@ESBUILD_DIR="$${DX_HOME:-$$HOME/.local/share/.dx}/tools/esbuild-$(ESBUILD_VERSION)"; \
	if [ -x "$$ESBUILD_DIR/esbuild" ]; then \
		echo "esbuild $(ESBUILD_VERSION) already cached at $$ESBUILD_DIR/esbuild"; \
	else \
		mkdir -p "$$ESBUILD_DIR"; \
		case "$$(uname -s)-$$(uname -m)" in \
			Linux-x86_64)   ESBUILD_PLATFORM=linux-x64   ;; \
			Linux-aarch64)  ESBUILD_PLATFORM=linux-arm64 ;; \
			Darwin-x86_64)  ESBUILD_PLATFORM=darwin-x64  ;; \
			Darwin-arm64)   ESBUILD_PLATFORM=darwin-arm64 ;; \
			*) echo "unsupported platform: $$(uname -s)-$$(uname -m)" >&2; exit 1 ;; \
		esac; \
		echo "Downloading esbuild $(ESBUILD_VERSION) ($$ESBUILD_PLATFORM) from npm registry..."; \
		TMP="$$(mktemp -d)"; \
		if [ "$$CN_MIRROR" = "true" ]; then ESBUILD_REGISTRY="https://registry.npmmirror.com"; else ESBUILD_REGISTRY="https://registry.npmjs.org"; fi; \
		curl -fsSL "$$ESBUILD_REGISTRY/@esbuild/$$ESBUILD_PLATFORM/-/$$ESBUILD_PLATFORM-$(ESBUILD_VERSION).tgz" \
			| tar -xz -C "$$TMP"; \
		mv "$$TMP/package/bin/esbuild" "$$ESBUILD_DIR/esbuild"; \
		rm -rf "$$TMP"; \
		chmod +x "$$ESBUILD_DIR/esbuild"; \
		echo "esbuild $(ESBUILD_VERSION) cached at $$ESBUILD_DIR/esbuild"; \
	fi

# Pre-populate dx 的 wasm-bindgen-cli 工具缓存（与 esbuild-cache 同构）。
# dx CLI 在 dx build 时会自动下载 wasm-bindgen-cli 二进制（packages/cli/src/
# wasm_bindgen.rs 的 verify_managed_install → install_github），下载源硬编码为
# github.com/rustwasm/wasm-bindgen/releases，既不读 GH_PROXY 也不读 NPM_REGISTRY——
# 国内直连必然慢/连接重置（"Taking a while..." 的主要来源之一）。此处经 gh-proxy
# 预下载与 Cargo.lock wasm-bindgen crate 版本完全一致的 tarball，解压到 dx 缓存目录。
# dx 的 wasm_bindgen.rs 在 install_dir.join(installed_bin_name).exists() 命中时
# 跳过联网下载。dx 按平台选 musl/darwin triplet（见 git_install_url）。
# 升级 wasm-bindgen 后须同步 WASM_BINDGEN_VERSION（查 Cargo.lock 的 [[package]]
# wasm-bindgen 版本）。triplet 必须与 dx 源码 git_install_url 的平台映射一致。
WASM_BINDGEN_VERSION := 0.2.126
wasm-bindgen-cache:
	@WB_DIR="$${DX_HOME:-$$HOME/.local/share/.dx}/tools/wasm-bindgen-$(WASM_BINDGEN_VERSION)"; \
	if [ -x "$$WB_DIR/wasm-bindgen" ]; then \
		echo "wasm-bindgen $(WASM_BINDGEN_VERSION) already cached at $$WB_DIR/wasm-bindgen"; \
	else \
		mkdir -p "$$WB_DIR"; \
		case "$$(uname -s)-$$(uname -m)" in \
			Linux-x86_64)   WB_TRIPLET=x86_64-unknown-linux-musl   ;; \
			Linux-aarch64)  WB_TRIPLET=aarch64-unknown-linux-musl ;; \
			Darwin-x86_64)  WB_TRIPLET=x86_64-apple-darwin  ;; \
			Darwin-arm64)   WB_TRIPLET=aarch64-apple-darwin ;; \
			*) echo "unsupported platform: $$(uname -s)-$$(uname -m)" >&2; exit 1; \
		esac; \
		echo "Downloading wasm-bindgen $(WASM_BINDGEN_VERSION) ($$WB_TRIPLET) from GitHub..."; \
		TMP="$$(mktemp -d)"; \
		if [ "$$CN_MIRROR" = "true" ]; then WB_GH_PROXY="https://gh-proxy.com"; else WB_GH_PROXY=""; fi; \
		curl -fsSL "$${WB_GH_PROXY:+$$WB_GH_PROXY/}https://github.com/rustwasm/wasm-bindgen/releases/download/$(WASM_BINDGEN_VERSION)/wasm-bindgen-$(WASM_BINDGEN_VERSION)-$$WB_TRIPLET.tar.gz" \
			| tar -xz -C "$$TMP"; \
		mv "$$TMP/wasm-bindgen-$(WASM_BINDGEN_VERSION)-$$WB_TRIPLET/wasm-bindgen" "$$WB_DIR/wasm-bindgen"; \
		rm -rf "$$TMP"; \
		chmod +x "$$WB_DIR/wasm-bindgen"; \
		echo "wasm-bindgen $(WASM_BINDGEN_VERSION) cached at $$WB_DIR/wasm-bindgen"; \
	fi

highlight-css:
	@cargo run --bin generate_highlight_css

# 把 npm 包 katex 的 dist/ 拷贝到 public/katex/（服务端 katex-rs 不打包 CSS）。
# KaTeX 的 katex.min.css 用相对 URL 引 fonts/，故 fonts/ 必须与 CSS 同级。
# 只拷 woff2（现代浏览器全支持，省去 woff/ttf ~70% 字体体积）。
# katex 作为 libs/ workspace 根 devDependency，pnpm install 后在 libs/node_modules/katex/。
katex-css:
	@echo "Copying KaTeX CSS + woff2 fonts to public/katex/..."
	@mkdir -p public/katex/fonts
	@cp libs/node_modules/katex/dist/katex.min.css public/katex/katex.min.css
	@cp libs/node_modules/katex/dist/fonts/*.woff2 public/katex/fonts/
	@echo "KaTeX CSS ready at public/katex/"

# 并行构建全部 libs/ 子项目（pnpm -r 拓扑顺序，无相互依赖则并发）。
# build-libs 会先安装依赖（pnpm install），无需调用方自行安装。
build-libs:
	@cd libs && pnpm install && pnpm -r run build

# 单库便利 target（替代旧的 build-<name>，用 pnpm --filter 精确定位）。
build-editor:     ; @cd libs && pnpm --filter @yggdrasil/tiptap-editor run build
build-codemirror: ; @cd libs && pnpm --filter @yggdrasil/codemirror-editor run build
build-lightbox:   ; @cd libs && pnpm --filter @yggdrasil/lightbox run build
build-core:       ; @cd libs && pnpm --filter @yggdrasil/core run build
build-xterm:      ; @cd libs && pnpm --filter @yggdrasil/xterm-terminal run build
build-mermaid:    ; @cd libs && pnpm --filter @yggdrasil/mermaid-renderer run build

dev: build-libs highlight-css katex-css esbuild-cache wasm-bindgen-cache
	@echo "Cleaning static/..."
	@rm -rf static/
	@echo "Building CSS..."
	@$(MAKE) css
	@echo "Starting dx serve..."
	@SSR_CACHE_SECS=0 RUSTC_WRAPPER= dx serve --addr 0.0.0.0 --interactive false

css:
	@tailwindcss -i input.css -o public/style.css

css-watch:
	@tailwindcss -i input.css -o public/style.css --watch

test:
	@cargo test
	@cd libs && pnpm -r run test

# JS + Rust 一次性检查（不改动文件）。
lint:
	@echo "==> Biome check (libs)"
	@cd libs && pnpm exec biome check . && pnpm typecheck
	@echo "==> Cargo clippy (Rust)"
	@cargo clippy --all-targets --all-features -- -D warnings
	@echo "==> Cargo fmt check (Rust)"
	@cargo fmt -- --check
# JS + Rust 自动修复（直接写入文件）。
# 顺序：Biome → cargo fix（应用编译器建议，重写代码）→ cargo fmt（格式化 Rust）
# → dx fmt（格式化 RSX 宏）。两道格式化收尾，保证最终文件状态整洁。
fix:
	@echo "==> Biome format (libs, 写入文件)"
	@cd libs && pnpm exec biome format --write .
	@echo "==> Cargo fix (Rust, 应用编译器建议)"
	@cargo fix --allow-dirty
	@echo "==> Cargo fmt (Rust, 格式化)"
	@cargo fmt
	@echo "==> Dioxus fmt (RSX 宏, 格式化)"
	@dx fmt

# 只编译当前 crate 的文档（--no-deps 跳过依赖，--document-private-items
# 让纯 binary crate 的内部模块/私有项也进文档，否则页面基本是空的）。
# RUSTDOCFLAGS 把 rustdoc 的 --default-theme=ayu 透传过去——cargo doc 本身
# 无主题参数，但会把该环境变量转交给底层 rustdoc。注意它是默认值，浏览器
# 若已记住上次的主题选择（localStorage）则不会被覆盖。
#
# 生成后拷贝到 public/doc/，让文档随 Dioxus 静态目录发布。先清空旧目录再
# 整体拷贝，避免删除模块后残留旧文件。rustdoc 内部用相对路径引用资源
# （如 ../../static.files/），原样挂载不会断链。
#
# 额外生成 public/doc/index.html 重定向页：Dioxus 在 dev 用
# nest_service("/doc", ServeDir) 托管该目录，ServeDir 访问目录根时默认
# 返回 index.html。用 meta refresh + JS 跳转到真正的文档入口
# yggdrasil/index.html，这样裸路径 /doc 也能直达文档，且不与 Dioxus 的
# /doc/* 路由冲突（手动注册 /doc 会在 merge 时 panic）。
doc:
	@RUSTDOCFLAGS="--default-theme=ayu" cargo doc --no-deps --document-private-items
	@rm -rf public/doc
	@cp -r target/doc public/doc
	@printf '<!DOCTYPE html><html><head><meta charset="utf-8"><meta http-equiv="refresh" content="0;url=yggdrasil/index.html"><title>Redirecting…</title></head><body><script>location.replace("yggdrasil/index.html")</script></body></html>' > public/doc/index.html

# 同 doc，生成完自动用浏览器打开。
doc-open:
	@RUSTDOCFLAGS="--default-theme=ayu" cargo doc --no-deps --document-private-items --open

# Docker image build. Two Dockerfiles cover two scenarios:
#
#   Dockerfile        in-container server build (native arch). Works on any host
#                     when the target arch == host arch — e.g. an x86 Linux box
#                     building an amd64 image. Zero host-side toolchain deps.
#   Dockerfile.cross  fully in-container build pinned to $BUILDPLATFORM (native
#                     arm64, zero QEMU). Two builder stages: a glibc Trixie stage
#                     for the WASM frontend (the prebuilt dx CLI needs GLIBC_2.39)
#                     and an Alpine-musl stage where zig (apk) cross-compiles the
#                     x86_64 server. Used when host arch != target arch — e.g.
#                     Apple Silicon building an amd64 image. Needs only Docker.
#
#   make docker              native arch only, load into local daemon (for testing)
#   make docker-amd64        x86_64 image; picks the right Dockerfile for your host
#   make docker-apple        x86_64 only, via Apple Container CLI (macOS 26+, Apple Silicon native; no Docker needed)
#   make docker-multiarch    build amd64+arm64 and push to a registry
#                            (multi-arch manifests can't be --load-ed locally)
#
# Push examples:
#   make docker-multiarch IMAGE=ghcr.io/owner/yggdrasil:latest
#   make docker-multiarch IMAGE=user/yggdrasil:v1 PLATFORMS=linux/amd64
#
# git 信息透传:.dockerignore 排除 .git/,容器内 build.rs 跑不了 git 命令,
# 所以在宿主(本 Makefile 里)采集后用 --build-arg 注入。Dockerfile 把 ARG
# 再 export 成 ENV,build.rs 的 std::env::var 优先读它。三个值在 Make 变量
# 里采集一次,所有 docker target 复用;git 不可用时退化为空串,Dockerfile
# 默认值也是空,build.rs 最终降级为 "unknown",不阻断构建。
HOST_ARCH := $(shell uname -m)
IMAGE ?= yggdrasil
PLATFORMS ?= linux/amd64,linux/arm64
GIT_DESCRIBE := $(shell git describe --tags --always --dirty 2>/dev/null)
GIT_HASH := $(shell git rev-parse HEAD 2>/dev/null)
GIT_DATE := $(shell git log -1 --format=%cd --date=iso-strict 2>/dev/null)
# 镜像版本号:取最近 git tag 原值(v0.10.0,带 v,与 CI publish-ghcr 的 GITHUB_REF_NAME 一致);
# 可用 VERSION=v0.10.1 覆盖。生产镜像据此打版本 tag(yggdrasil:v0.10.0、yggdrasil:v0.10.0-amd64)。
VERSION ?= $(shell git describe --tags --abbrev=0 2>/dev/null)
# build-arg 复用块:每个 docker target 展开一次。空值也传(让 Dockerfile 默认接管)。
GIT_BUILD_ARGS = --build-arg YGG_BUILD_GIT_DESCRIBE="$(GIT_DESCRIBE)" \
                 --build-arg YGG_BUILD_GIT_HASH="$(GIT_HASH)" \
                 --build-arg YGG_BUILD_GIT_COMMIT_DATE="$(GIT_DATE)"
# CN_MIRROR build-arg：传 CN_MIRROR=true 时透传 --build-arg CN_MIRROR=true，
# 否则不传（Dockerfile 内 ARG CN_MIRROR=false 默认关闭国内镜像）。
CN_BUILD_ARGS = $(if $(filter true,$(CN_MIRROR)),--build-arg CN_MIRROR=true)
docker:
	@docker buildx build --load $(GIT_BUILD_ARGS) $(CN_BUILD_ARGS) \
		-t yggdrasil:latest -t yggdrasil:$(VERSION) .

# Build an amd64 image. On an x86_64 host the server compiles in-container via
# the plain Dockerfile (native, no host toolchain). On any other host (e.g.
# Apple Silicon arm64) Dockerfile.cross does the whole build in-container too:
# a native-arm64 frontend stage (glibc Trixie, for the prebuilt dx CLI) plus a
# native-arm64 server stage (Alpine musl, where zig — installed via apk, the
# only China-reachable zig source — cross-compiles a static x86_64-musl binary).
# No QEMU, no Rosetta, no `cross`, no host zig: the only host-side dep is Docker
# itself. Product is directly docker run / docker save exportable.
docker-amd64:
ifeq ($(HOST_ARCH),x86_64)
	@docker buildx build --platform linux/amd64 --load $(GIT_BUILD_ARGS) $(CN_BUILD_ARGS) \
		-t yggdrasil:amd64 -t yggdrasil:$(VERSION)-amd64 .
else
	@docker buildx build --platform linux/amd64 --load -f Dockerfile.cross $(GIT_BUILD_ARGS) $(CN_BUILD_ARGS) \
		-t yggdrasil:amd64 -t yggdrasil:$(VERSION)-amd64 .
endif

docker-multiarch:
	@docker buildx build --platform $(PLATFORMS) $(GIT_BUILD_ARGS) $(CN_BUILD_ARGS) -t $(IMAGE) --push .

# ── Docker 开发环境 ────────────────────────────────────────────
# 使用 Dockerfile.dev + docker-compose.dev.yml 在容器内运行 dx serve。
# 源码以镜像内 COPY 快照 + compose watch 增量 sync 进容器（宿主 IDE 编辑即时
# 生效, 容器内 inotify 原生热重载）。PostgreSQL 用宿主原生实例
# （host.docker.internal:5432, 不由 compose 管理）。
# 首次启动需编译 Rust 依赖（~10 分钟），后续启动约 10 秒（cargo target 缓存）。
# up --build 重建镜像（COPY 当前源码快照）后前台跑 compose watch：
# 源码变更增量 sync 进容器（dx serve inotify 热重载），Cargo.lock/pnpm-lock
# 等依赖清单变更自动 rebuild 镜像。Ctrl+C 只停 watch, 容器继续后台跑。
docker-dev:
	@docker compose -f docker-compose.dev.yml up --build -d
	@docker compose -f docker-compose.dev.yml watch

# 停止并移除 dev 容器（volume 数据保留）。
docker-dev-down:
	@docker compose -f docker-compose.dev.yml down

# 进入 dev 容器的交互式 shell（容器需已在运行）。
docker-dev-shell:
	@docker compose -f docker-compose.dev.yml exec dev bash

# ── Docker 工具容器（lint / test / fix / check）──────────────────
# 本地（xfy 的 Mac）AliEDR 会 SIGKILL 本地构建链二进制（wasm-bindgen exit 137），
# 故 lint/fix/test/check 一律在容器内跑，避开 EDR。见 docker-compose.tools.yml。
#
# 用 bind mount（双向）：fmt/fix 写文件直接回流宿主工作区。
# 首次运行 cargo 依赖全量编译约 10 分钟；之后命名卷缓存命中，秒级启动。
TOOLS_COMPOSE := docker compose -f docker-compose.tools.yml

# 一次性运行任意命令（例：make docker-run CMD='cargo build --features server'）。
docker-run:
	@$(TOOLS_COMPOSE) run --rm tools bash -c '$(CMD)'

# lint（只读）：clippy + cargo fmt --check + biome check + typecheck。
docker-lint:
	@$(TOOLS_COMPOSE) run --rm tools bash -c 'cd libs && pnpm install --frozen-lockfile >/dev/null && cd /build && make lint'

# 仅 clippy（最常用的编译期检查，不需要 pnpm）。
docker-clippy:
	@$(TOOLS_COMPOSE) run --rm tools cargo clippy --all-targets --all-features -- -D warnings

# 最快编译校验：cargo check --all-features（不跑 clippy lint、不跑测试）。
docker-check:
	@$(TOOLS_COMPOSE) run --rm tools cargo check --all-features

# 格式化（写入文件，回流宿主）：cargo fmt + biome format。
# 注意：不含 dx fmt——dx fmt 0.7.10 会搬运/删除 rsx 注释，仅按需手动跑 docker-fix。
docker-fmt:
	@$(TOOLS_COMPOSE) run --rm tools bash -c 'cd libs && pnpm install --frozen-lockfile >/dev/null && pnpm exec biome format --write . && cd /build && cargo fmt'

# fix（写入文件，回流宿主）：biome format + cargo fix + cargo fmt + dx fmt。
# 警告：dx fmt 会重排 rsx 宏内注释——改完务必 git diff 复核，必要时 checkout 无关文件。
docker-fix:
	@$(TOOLS_COMPOSE) run --rm tools bash -c 'cd libs && pnpm install --frozen-lockfile >/dev/null && cd /build && make fix'

# test：cargo test + libs pnpm test。需要 Docker daemon 的 code-runner 测试自动 skip。
docker-test:
	@$(TOOLS_COMPOSE) run --rm tools bash -c 'cd libs && pnpm install --frozen-lockfile >/dev/null && cd /build && make test'

# 重建工具镜像（Dockerfile.dev 变更后用；正常情况下 run 会按需自动构建）。
docker-tools-build:
	@$(TOOLS_COMPOSE) build

# 清理工具容器命名卷（释放磁盘；下次运行重新编译依赖）。
docker-tools-clean:
	@$(TOOLS_COMPOSE) down -v

clean:
	@cargo clean
	@rm -f public/style.css public/highlight.css
	@rm -rf public/katex
	@rm -rf public/mermaid
	@rm -rf public/doc
	@rm -rf static/
	@rm -rf uploads/.cache
	@rm -rf libs/node_modules libs/*/node_modules
