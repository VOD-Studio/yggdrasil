# Yggdrasil 构建入口。不带参数的 `make` 只列出常用 target（见 help）。
#
# 模块：
#   mk/assets.mk    生成资源：libs、高亮/KaTeX CSS、Tailwind；release 全量 + dev 增量
#   mk/docker.mk    镜像构建、Docker 开发环境、lint/test/fmt 工具容器
include mk/assets.mk mk/docker.mk

.PHONY: help build build-linux build-frontend wasm-toolchain install-cargo-leptos precompress
.PHONY: check-dev-tools check-build-tools check-brotli check-wasm-toolchain check-chunk-merge
.PHONY: dev test lint fmt fix doc doc-open clean

# ── 配置 ───────────────────────────────────────────────────────
# release 构建用固定日期的 nightly（前端 WASM 构建）。
# 以下两组参数只作用于 wasm 前端：-Zbuild-std 以 optimize_for_size 重编 std，
# -Cpanic=immediate-abort 减少 panic 处理代码；代价是 wasm 内 panic 直接 trap，
# 不再打印诊断信息。
# - -Zlocation-detail=none 去掉 panic 与 #[track_caller] 的文件、行号；-Zfmt-debug=none
#   让 {:?} 与 derive(Debug) 输出空串，所以 wasm 端代码不能拿 Debug 输出当数据（key、
#   文案、序列化），只能用于日志。
# - rustflags 经 wasm32 专属的 CARGO_TARGET_*_RUSTFLAGS 传入（服务端仍 panic=unwind）。
#   它会覆盖 .cargo/config.toml 的同名配置，因此需要重复 getrandom cfg。
# - RUSTUP_TOOLCHAIN 让 cargo-leptos 及其调用的 cargo 都走 nightly；
#   --lib-cargo-args 只把 build-std 传给 wasm 的 lib 构建。
# dev、test、lint 与 CI 仍用 stable。升级 nightly 时同步两个 Dockerfile 的
# ARG WASM_NIGHTLY。
WASM_NIGHTLY := nightly-2026-10-19
RELEASE_OUT ?= target/release-dist
LEPTOS_WASM_ENV := RUSTUP_TOOLCHAIN=$(WASM_NIGHTLY) CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS='--cfg getrandom_backend="wasm_js" -Zunstable-options -Cpanic=immediate-abort -Zlocation-detail=none -Zfmt-debug=none'
LEPTOS_WASM_ARGS := --lib-cargo-args=-Zbuild-std=std,panic_abort --lib-cargo-args=-Zbuild-std-features=optimize_for_size

# cargo-leptos 0.3.11，其中的 wasm_split_cli_support 换成 VOD-Studio/wasm-split-prototype 的
# ygg/chunk-merge 分支：原版按「需要它的分包集合」逐一切出共享分包，59 个懒加载点切出上千个
# 几百字节的分包，一页要请求几百个；补丁把收益为正的小分包合并，单页 WASM 请求减少 3–5 倍，
# 移动端水合快 0.3–2 s。二进制里带 WASM_SPLIT_CHUNK_LOAD_COST 字样即为补丁版。
# 两个 Dockerfile 与 CI 用同一 rev，升级时同步修改。
WASM_SPLIT_REV := 8ca741dd3c1ca52231b6b8ec4bed23955e376cbd
CARGO_LEPTOS_INSTALL := cargo install cargo-leptos --version 0.3.11 --locked \
	--config 'patch.crates-io.wasm_split_cli_support.git="https://github.com/VOD-Studio/wasm-split-prototype"' \
	--config 'patch.crates-io.wasm_split_cli_support.rev="$(WASM_SPLIT_REV)"'

# 合并补丁的「一次分包加载折合多少字节」（未压缩），补丁默认 1024。线上 EdgeOne 对同一客户端的
# 并发请求大致逐个处理，每个约 2–3 ms，按常见带宽折合几 KB；首页 60 个分包整批要排 120–200 ms。
# 实测 1024/4096/8192/16384 下首页 60/29/21/14 个、文章页 70/34/26/18 个分包，Brotli 总量
# 变化在 ±5% 内，8192 在快慢网络之间折中（docs/首屏性能实测-2026-10-28.md「分包合并代价」）。
WASM_SPLIT_CHUNK_LOAD_COST ?= 8192

# 裸 `make` 只列出 target，不触发任何构建。
.DEFAULT_GOAL := help

# 在 target 行尾加 `## 说明` 即可出现在 help 里；无说明的是内部构建步骤。
help: ## 列出常用 target
	@awk 'BEGIN { FS = ":.*## " } /^[a-zA-Z0-9_-]+:.*## / { printf "  %-20s %s\n", $$1, $$2 }' $(MAKEFILE_LIST)

# ── 宿主工具检查 ───────────────────────────────────────────────
# 检查开发所需的宿主 CLI 是否已安装；release 构建（check-build-tools）另需 brotli。
# 在耗时 target 开跑之前尽早失败，并给出可执行的安装提示。
check-dev-tools: ## 检查开发所需的宿主工具
	@missing=0; \
	if ! command -v cargo >/dev/null 2>&1; then \
		echo "error: cargo (Rust toolchain) is required" >&2; \
		echo "  Install: https://rustup.rs" >&2; \
		missing=1; \
	fi; \
	if ! command -v rustup >/dev/null 2>&1; then \
		echo "error: rustup is required" >&2; \
		echo "  Install: https://rustup.rs" >&2; \
		missing=1; \
	fi; \
	if ! command -v cargo-leptos >/dev/null 2>&1 || ! cargo leptos --version 2>/dev/null | grep -q '0\.3\.11'; then \
		echo "error: cargo-leptos 0.3.11 is required" >&2; \
		echo "  Install: make install-cargo-leptos" >&2; \
		missing=1; \
	fi; \
	if ! command -v tailwindcss >/dev/null 2>&1; then \
		echo "error: tailwindcss CLI (Tailwind CSS v4) is required" >&2; \
		echo "  Install: brew install tailwindcss" >&2; \
		echo "  Standalone binary: https://github.com/tailwindlabs/tailwindcss/releases" >&2; \
		missing=1; \
	fi; \
	if ! command -v bun >/dev/null 2>&1; then \
		bun_want=$$(sed -n 's/.*"packageManager": *"bun@\([^"]*\)".*/\1/p' libs/package.json); \
		echo "error: bun $${bun_want:+(>= $$bun_want) }is required for frontend libraries in libs/" >&2; \
		echo "  Install: brew install oven-sh/bun/bun | curl -fsSL https://bun.sh/install | bash" >&2; \
		missing=1; \
	fi; \
	if [ "$$missing" -ne 0 ]; then \
		exit 1; \
	fi

check-build-tools: check-dev-tools check-brotli check-wasm-toolchain ## 检查 release 构建所需的宿主工具

# 本地 release 构建在前端构建后要用宿主的 brotli CLI。
# 在耗时构建之前先检查，缺包时直接给出安装提示。
check-brotli:
	@if command -v brotli >/dev/null 2>&1; then \
		:; \
	else \
		echo "error: brotli CLI is required for release builds" >&2; \
		echo "  Install: sudo dnf install brotli | sudo apt-get install brotli | brew install brotli" >&2; \
		exit 1; \
	fi

# release 构建需要固定 nightly 及其 rust-src（build-std）、wasm32 target。
check-wasm-toolchain:
	@if ! rustup run $(WASM_NIGHTLY) rustc --print sysroot >/dev/null 2>&1 \
		|| ! rustup component list --installed --toolchain $(WASM_NIGHTLY) 2>/dev/null | grep -q '^rust-src' \
		|| ! rustup target list --installed --toolchain $(WASM_NIGHTLY) 2>/dev/null | grep -q '^wasm32-unknown-unknown$$'; then \
		echo "error: $(WASM_NIGHTLY) with rust-src and wasm32-unknown-unknown is required for release WASM builds" >&2; \
		echo "  Install: make wasm-toolchain" >&2; \
		exit 1; \
	fi

# release 前端必须用分包合并补丁版 cargo-leptos（见 CARGO_LEPTOS_INSTALL）。dev 与回归构建
# 用原版也能跑，只是分包更碎，因此只在 release 前端构建前检查。
check-chunk-merge:
	@if ! grep -aq WASM_SPLIT_CHUNK_LOAD_COST "$$(command -v cargo-leptos)" 2>/dev/null; then \
		echo "error: release builds need cargo-leptos with the wasm-split chunk-merge patch" >&2; \
		echo "  Install: make install-cargo-leptos" >&2; \
		exit 1; \
	fi

wasm-toolchain: ## 安装 release 构建所需的固定 nightly 工具链
	@rustup toolchain install $(WASM_NIGHTLY) --profile minimal \
		--component rust-src --target wasm32-unknown-unknown

install-cargo-leptos: ## 安装带分包合并补丁的 cargo-leptos 0.3.11
	$(CARGO_LEPTOS_INSTALL)

# ── release 构建 ───────────────────────────────────────────────
# 流水线：工具检查 → 共享资源与文档 → 分包前端 → 服务端 → 装配 → Brotli → 校验。
# 资源与文档可并行；配方中的递归 make 等它们全部成功后才开始前端，兼容 make -j。
# 前端用固定 nightly + build-std + immediate-abort；服务端用 stable。

build-frontend: check-wasm-toolchain check-chunk-merge ## 构建分包前端（wasm/js/css → target/site）
	@$(LEPTOS_WASM_ENV) WASM_SPLIT_CHUNK_LOAD_COST=$(WASM_SPLIT_CHUNK_LOAD_COST) cargo leptos build --release --split --frontend-only $(LEPTOS_WASM_ARGS)

build: RELEASE_SERVER := target/release/yggdrasil
build-linux: RELEASE_SERVER := target/x86_64-unknown-linux-musl/release/yggdrasil
build-linux: RELEASE_SERVER_ARGS := --target x86_64-unknown-linux-musl
build: ## 完整 release 构建（资源 + 文档 + 分包前端 + 服务端 + 装配 + 压缩）
build-linux: ## 构建 Linux x86_64 musl 服务端 + 分包前端
build build-linux: check-build-tools
	@$(MAKE) build-assets doc
	@$(MAKE) build-frontend
	@cargo build --release --locked --bin yggdrasil $(RELEASE_SERVER_ARGS)
	@bun scripts/build-release.ts --out "$(RELEASE_OUT)" --server "$(RELEASE_SERVER)"
	@$(MAKE) precompress
	@bun scripts/build-release.ts --verify --out "$(RELEASE_OUT)"
	@echo "发布目录：$(RELEASE_OUT)；部署时请保留 server、hash.txt 与 public/ 的配套关系。"

# 用 brotli 预压缩静态文本资源（生成 .br 旁路文件）。site_fallback 的 ServeDir
# 开启了 precompressed_br，请求带 Accept-Encoding: br 时自动命中 .br。只压文本
# 格式；字体/图片本身已压缩，pkg 下的 .d.ts 不被浏览器请求也不压。
precompress: check-brotli
	@find "$(RELEASE_OUT)/public" -type f \
		\( -name '*.js' -o -name '*.css' -o -name '*.wasm' \
		   -o -name '*.svg' -o -name '*.html' -o -name '*.json' -o -name '*.xml' \) \
		-exec brotli -q 11 -kf {} +

# ── 开发 ───────────────────────────────────────────────────────
# cargo-leptos watch：服务端（server feature）与浏览器 WASM（hydrate feature）
# 各自增量构建并热重载，配置见 Cargo.toml 的 [package.metadata.leptos]。
# 必须带 --split：`Lazy`/`#[lazy]` 路由会让 wasm-bindgen 产物导入占位模块
# `__wasm_split_placeholder__`，只有分包后处理才会解析它，不分包的 WASM 无法加载。
dev: dev-assets ## 启动开发服务器（增量构建资源 + cargo leptos watch --split）
	@echo "Starting cargo leptos watch..."
	@SSR_CACHE_SECS=0 IP=0.0.0.0 cargo leptos watch --split

.PHONY: regression-build test-databases test-browser
# debug 构建、分包（原因见 dev 目标）：产物在 target/debug/yggdrasil（服务端 + hash.txt）与 target/site（站点资源 + pkg）。
regression-build: dev-assets ## 构建隔离浏览器回归使用的应用
	@cargo leptos build --split

test-databases: ## 在临时 PostgreSQL 中验证文章、笔记和备份，结束后清理
	@bun scripts/regression.ts database

BROWSER_SUITE ?= smoke
test-browser: ## 隔离浏览器回归（先 regression-build；BROWSER_SUITE=smoke/admin/notes/backup/runner/slash/lightbox/showcase）
	@bun scripts/regression.ts $(BROWSER_SUITE)

# ── 测试与代码质量 ─────────────────────────────────────────────
test: ## 运行 Rust 与前端测试
	@cargo test
	@cd libs && bun run --filter '*' test

.PHONY: test-release-split
test-release-split: build ## 对真实 release 产物做分包、水合、缓存与体积门槛验收
	@bun scripts/test-release-split.ts

# JS + Rust 一次性检查（不改动文件）。
lint: ## Biome、TypeScript、Clippy、rustfmt 检查（只读）
	@echo "==> Biome check (libs)"
	@cd libs && bun run biome check . && bun run typecheck
	@echo "==> Cargo clippy (Rust, server)"
	@cargo clippy --all-targets -- -D warnings
	@echo "==> Cargo clippy (Rust, wasm32 hydrate)"
	@cargo clippy --target wasm32-unknown-unknown --no-default-features --features hydrate -- -D warnings
	@echo "==> Cargo fmt check (Rust)"
	@cargo fmt -- --check

# JS + Rust 格式化（直接写入文件）。
# fix、docker-fmt、docker-fix 都经由本 target。
fmt: ## Biome format + cargo fmt（写入文件）
	@echo "==> Biome format (libs, 写入文件)"
	@cd libs && bun run format
	@echo "==> Cargo fmt (Rust, 格式化)"
	@cargo fmt

# JS + Rust 自动修复（直接写入文件）。
# 顺序：cargo fix（应用编译器建议，重写代码）→ fmt（Biome + cargo fmt）。
fix: ## cargo fix + Biome format + cargo fmt（写入文件）
	@echo "==> Cargo fix (Rust, 应用编译器建议)"
	@cargo fix --allow-dirty
	@$(MAKE) --no-print-directory fmt

# ── 文档 ───────────────────────────────────────────────────────
# 只生成当前 lib crate 的文档（--no-deps），并包含内部模块与私有项。
#
# 站点化主题：源文件在 rustdoc-theme/。
#   - theme.css（只放 import，模块在 css/）经 Tailwind 编译后用 rustdoc -e 注入
#     （落到文档根 theme.css）。
#   - 三个 HTML 片段：in-header.html（<head> 末尾，加载 doc-boot.js 同步主题）、
#     before-content.html（站点顶栏）、after-content.html（页脚、手机抽屉、
#     doc-shell.js）。rustdoc 的 --html-* 不改 rustdoc 自身的 HTML。
#   - 两个 JS：doc-boot.js 不打包直接拷贝；js/index.js 由 bun 打成 IIFE
#     （页面里是 /doc/doc-shell.js）。
# 片段按内容 cksum 命名目录（DOC_FRAGMENT_DIR）：片段一变，RUSTDOCFLAGS 就变，
# cargo 会重新生成 HTML。而 cargo 不跟踪 -e 指向文件的内容，所以生成后再把
# theme.css 和两个 JS 复制一次到 target/doc。不设默认主题：主题只由站点顶栏
# 三档（浅色/深色/跟随系统）切换，由 doc-boot.js 写入 rustdoc 的存储键，不再提供 ayu。
# DOC_OUT 供 Dockerfile 指定输出目录。用绝对路径，避免依赖 rustdoc 运行时的 cwd。
#
# `/doc` 由 `site_fallback` 的 ServeDir 从站点根（`public/` 经 cargo-leptos 拷入
# target/site）提供；生成后拷贝到 public/doc/，先清空旧目录再整体拷贝，避免删除
# 模块后残留旧文件。rustdoc 内部用相对路径引用资源（如 ../../static.files/），
# 原样挂载不会断链。
#
# 额外生成 public/doc/index.html 重定向页：ServeDir 访问目录根时默认返回
# index.html。用 meta refresh + JS 跳转到真正的文档入口 yggdrasil/index.html，
# 这样裸路径 /doc 也能直达文档。
DOC_OUT ?= public/doc
DOC_THEME_CSS := target/rustdoc-theme.css
DOC_SHELL_JS := target/rustdoc-doc-shell.js
DOC_FRAGMENTS := rustdoc-theme/in-header.html rustdoc-theme/before-content.html rustdoc-theme/after-content.html
DOC_FRAGMENT_DIR = target/rustdoc-fragments/$(shell cat $(DOC_FRAGMENTS) | cksum | cut -d ' ' -f 1)
# 文档始终带 --document-private-items（私有项与公共项一起渲染），公共项文档链接
# 私有实现项（server fn 内部常量、私有工具函数等）是预期行为，放行该 lint；
# 断链与歧义链接仍然告警。
DOC_RUSTDOCFLAGS = -e $(CURDIR)/$(DOC_THEME_CSS) \
	--html-in-header $(CURDIR)/$(DOC_FRAGMENT_DIR)/in-header.html \
	--html-before-content $(CURDIR)/$(DOC_FRAGMENT_DIR)/before-content.html \
	--html-after-content $(CURDIR)/$(DOC_FRAGMENT_DIR)/after-content.html \
	-A rustdoc::private_intra_doc_links

doc: ## 生成 rustdoc（站点同款主题）到 public/doc/
	@mkdir -p target $(DOC_FRAGMENT_DIR)
	@cp $(DOC_FRAGMENTS) $(DOC_FRAGMENT_DIR)/
	@tailwindcss -i rustdoc-theme/theme.css -o $(DOC_THEME_CSS) --minify
	@bun build rustdoc-theme/js/index.js --outfile $(DOC_SHELL_JS) --format=iife --target=browser --minify
	@RUSTDOCFLAGS="$(DOC_RUSTDOCFLAGS)" cargo doc --lib --no-deps --document-private-items
	@cp $(DOC_THEME_CSS) target/doc/theme.css
	@cp rustdoc-theme/doc-boot.js target/doc/doc-boot.js
	@cp $(DOC_SHELL_JS) target/doc/doc-shell.js
	@rm -rf $(DOC_OUT)
	@cp -r target/doc $(DOC_OUT)
	@printf '<!DOCTYPE html><html><head><meta charset="utf-8"><meta http-equiv="refresh" content="0;url=yggdrasil/index.html"><title>Redirecting…</title></head><body><script>location.replace("yggdrasil/index.html")</script></body></html>' > $(DOC_OUT)/index.html

# 同 doc，生成完自动用浏览器打开。flags 与 doc 相同，缓存是新的，只会打开
# 页面，不会重新生成，也不会覆盖已复制的 theme.css。
doc-open: doc ## 生成 rustdoc 并用浏览器打开
	@RUSTDOCFLAGS="$(DOC_RUSTDOCFLAGS)" cargo doc --lib --no-deps --document-private-items --open

# ── 清理 ───────────────────────────────────────────────────────
# 只删除已知生成目录；public/ 下的图标、图片等受版本控制的资源必须保留。
clean: ## 清理构建产物与前端依赖
	@cargo clean
	@rm -f public/style.css public/admin.css public/highlight.css
	@rm -rf public/katex public/doc public/codemirror public/lightbox public/mermaid \
		public/tiptap public/xterm public/yggdrasil-core
	@rm -rf uploads/.cache
	@rm -rf libs/node_modules libs/*/node_modules

.PHONY: test-public-performance
test-public-performance: regression-build ## 验证双机缓存、公开页精简数据、归档分页和评论延迟加载
	@bun scripts/test-public-performance.ts
