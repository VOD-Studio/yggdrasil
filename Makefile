# Yggdrasil 构建入口。不带参数的 `make` 只列出常用 target（见 help）。
#
# 模块：
#   mk/assets.mk    生成资源：libs、高亮/KaTeX CSS、Tailwind；release 全量 + dev 增量
#   mk/dx-cache.mk  预置 dx 的 esbuild / wasm-bindgen 工具缓存
#   mk/docker.mk    镜像构建、Docker 开发环境、lint/test/fmt 工具容器
include mk/assets.mk mk/dx-cache.mk mk/docker.mk

.PHONY: help build build-linux build-client build-client-reader build-full-and-server snapshot-flavor wasm-toolchain restore-webp precompress
.PHONY: check-dev-tools check-build-tools check-brotli check-wasm-toolchain
.PHONY: dev test lint fmt fix doc doc-open clean

# ── 配置 ───────────────────────────────────────────────────────
# sccache × dx 兼容：
# dx build / dx serve 构建时把自己设为 RUSTC_WORKSPACE_WRAPPER 拦截 workspace
# crate 的 rustc 调用（资产捕获）。若宿主 ~/.cargo/config.toml 配了
# [build] rustc-wrapper（如 sccache），cargo 会组合出 `sccache dx rustc …`：
# sccache 把 dx 当编译器探测，探测必然失败（"Compiler not supported"）→ dx 报错。
# 空 RUSTC_WRAPPER env 覆盖 config（env 优先于 config，空值 = 无 wrapper），
# 只对 dx 构建关闭 sccache；直接 cargo 的构建（test / lint / Dockerfile / CI）不受影响。
# 所有本机 dx 调用（build / build-linux / dev）都经由 $(DX)。
DX := RUSTC_WRAPPER= dx

# release 构建用固定日期的 nightly（整次 dx 构建，含其附带的服务端构建）。
# 以下两组参数只作用于 wasm 客户端：-Zbuild-std 以 optimize_for_size 重编 std，
# -Cpanic=immediate-abort 去掉 panic 格式化与展开代码。2026-10-20 实测 brotli 后
# 1,098 KB → 1,010 KB（−8%）。代价：wasm 内 panic 直接 trap，不再打印信息。
# - build-std 经 @client --cargo-args 传入，服务端构建不受影响；
# - rustflags 经 wasm32 专属的 CARGO_TARGET_*_RUSTFLAGS 传入（服务端仍 panic=unwind）。
#   它会覆盖 .cargo/config.toml 的同名配置，因此需要重复 getrandom cfg。
# dx serve、test、lint 与 CI 仍用 stable。升级 nightly 时同步两个 Dockerfile 的
# ARG WASM_NIGHTLY。
WASM_NIGHTLY := nightly-2026-10-19
WASM_CARGO_ARGS := -Zbuild-std=std,panic_abort -Zbuild-std-features=optimize_for_size
DX_NIGHTLY := RUSTUP_TOOLCHAIN=$(WASM_NIGHTLY) \
	CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS='--cfg getrandom_backend="wasm_js" -Zunstable-options -Cpanic=immediate-abort' \
	$(DX)

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
		echo "error: rustup is required to check LLVM tools" >&2; \
		echo "  Install: https://rustup.rs" >&2; \
		missing=1; \
	elif ! rustup component list --installed 2>/dev/null | grep -Eq '^llvm-tools(-preview)?-'; then \
		echo "error: llvm-tools component is required for the active Rust toolchain" >&2; \
		toolchain=$$(rustup show active-toolchain 2>/dev/null | cut -d ' ' -f 1); \
		echo "  Install: rustup component add llvm-tools --toolchain $${toolchain:-stable}" >&2; \
		missing=1; \
	fi; \
	if ! command -v dx >/dev/null 2>&1; then \
		echo "error: dx CLI (Dioxus CLI 0.7.10) is required" >&2; \
		echo "  Install: cargo install dioxus-cli --version 0.7.10" >&2; \
		echo "  Prebuilt binary: https://github.com/DioxusLabs/dioxus/releases/tag/v0.7.10" >&2; \
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

# 本地 release 构建在 dx 结束后要用宿主的 brotli CLI。
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

wasm-toolchain: ## 安装 release 构建所需的固定 nightly 工具链
	@rustup toolchain install $(WASM_NIGHTLY) --profile minimal \
		--component rust-src --target wasm32-unknown-unknown

# ── release 构建 ───────────────────────────────────────────────
# P3 双客户端流水线（plan §6.4）：共享资源 → 服务端 → full 客户端 → 快照 →
# reader 客户端 → 快照 → 合并 + 发布清单 → Brotli → 校验。两次 dx 客户端构建
# 顺序执行（不并发写同一输出目录），各自 restore-webp 后立即快照，快照里的
# .webp 已还原；合并脚本从快照装配最终 public/，同路径不同内容直接失败。
RELEASE_ASSEMBLY := target/release-assembly

build: check-build-tools ## 完整 release 构建（资源 + 文档 + 双客户端 + 服务端 + 合并 + 压缩）
	@$(MAKE) build-assets
	@$(MAKE) doc
	@$(MAKE) build-full-and-server
	@$(MAKE) restore-webp
	@$(MAKE) snapshot-flavor FLAVOR=full
	@$(MAKE) build-client-reader
	@$(MAKE) restore-webp
	@$(MAKE) snapshot-flavor FLAVOR=reader
	@bun scripts/build-release.cjs --out target/dx/yggdrasil/release/web
	@$(MAKE) precompress
	@bun scripts/build-release.cjs --verify --out target/dx/yggdrasil/release/web

build-linux: check-build-tools ## 构建 Linux x86_64 musl 服务端 + 双客户端
	@$(MAKE) build-assets
	@$(MAKE) build-client
	@$(MAKE) restore-webp
	@$(MAKE) snapshot-flavor FLAVOR=full
	@$(MAKE) build-client-reader
	@$(MAKE) restore-webp
	@$(MAKE) snapshot-flavor FLAVOR=reader
	@$(DX) build @server --release --debug-symbols=false --target x86_64-unknown-linux-musl --wasm-js-cfg false --features server
	@bun scripts/build-release.cjs --out target/dx/yggdrasil/release/web
	@$(MAKE) precompress
	@bun scripts/build-release.cjs --verify --out target/dx/yggdrasil/release/web
	@echo ""
	@echo "Linux build complete! The server binary is at target/dx/yggdrasil/release/web/server"
	@echo "Remember to deploy it alongside the target/dx/yggdrasil/release/web/public directory"
	@echo "(contains full index.html, flavors/reader/index.html and release-manifest.json)."
	@echo "When running the server, ensure DIOXUS_ASSET_DIR is set or the public directory is in CWD."

# full 客户端 + 服务端（宿主架构）：沿用原 make build 的组合命令形态——
# dx 的 @server 选择器同样会刷新 wasm 半边，而服务端单独构建时拿不到
# -Zbuild-std（wasm std 的 panic 策略与 immediate-abort 不匹配会报错），
# 因此服务端由这段命令顺带产出，客户端特征与 build-client 一致。
build-full-and-server: check-wasm-toolchain ## 构建 full 客户端与宿主架构服务端
	@$(DX_NIGHTLY) build --release --debug-symbols=false \
		@client --release --debug-symbols=false --wasm-js-cfg false \
		--cargo-args='$(WASM_CARGO_ARGS)'

# reader 客户端：client-reader 剥离后台/登录/注册/图鉴（plan §6.1）。
# 潜在收益最大的产物，验收门槛见 plan §6.8（reader Brotli ≥35% 降幅）。
build-client-reader: check-wasm-toolchain ## 构建 reader 客户端（公开阅读裁剪版）
	@$(DX_NIGHTLY) build @client --release --debug-symbols=false --wasm-js-cfg false \
		--features client-reader --cargo-args='$(WASM_CARGO_ARGS)'

# 保存某个 flavor 的构建快照（public + dx 资产清单），供 build-release.cjs 合并。
# 必须在本 flavor 的 restore-webp 之后调用，快照内 .webp 已还原。
snapshot-flavor: ## 快照 dx 客户端产物（FLAVOR=full|reader）
	@test -n "$(FLAVOR)" || (echo "用法：make snapshot-flavor FLAVOR=full|reader" >&2; exit 1)
	@rm -rf $(RELEASE_ASSEMBLY)/$(FLAVOR)
	@mkdir -p $(RELEASE_ASSEMBLY)/$(FLAVOR)
	@cp -a target/dx/yggdrasil/release/web/public $(RELEASE_ASSEMBLY)/$(FLAVOR)/public
	@if [ -f target/dx/yggdrasil/release/web/.manifest.json ]; then \
		cp target/dx/yggdrasil/release/web/.manifest.json $(RELEASE_ASSEMBLY)/$(FLAVOR)/; \
	fi

# 兜底：dx build 0.7.10 会把 public/ 下的 .webp 重编码成 VP8L 无损静图
# （动画帧被丢弃，静图体积反增 7-8 倍），与文档承诺的"原样拷贝"不符。
# SVG/ICO 等其他格式不受影响，故只需覆盖 .webp。
# 遍历所有 dx 产物目录（release/debug），用源 public/ 的同名文件覆盖回去。
# 仅覆盖产物中已存在的 .webp，不引入源里新增但 dx 未生成的文件。
# 参考：https://dioxuslabs.com/learn/0.7/essentials/ui/assets/
# 复验记录：2026-10-09 在 dx 0.7.10 上重新验证，release 构建的 22 个 .webp 全部被
# 改写（体积 7-8 倍），其中 2 个 79 帧的动画 webp 被丢帧成静图；本 target 跑完后与源
# 文件逐字节一致。升级 dx 后复验方法：清空 target/dx/yggdrasil/release/web/public，
# 重新 dx build，再用 cmp 比对产物与 public/ 下同名 .webp，全部一致才可移除。
# 上游修复后可移除此 target 及 build/build-linux 里的调用（Dockerfile 也有调用）。
restore-webp: ## 把被 dx 重编码的 .webp 还原为源文件
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

# 用 brotli 预压缩静态文本资源（生成 .br 旁路文件）。dioxus-server 对 public
# 下每个叶子文件都注册了 ServeFile::precompressed_br()，请求带
# Accept-Encoding: br 时自动命中 .br。只压文本格式；字体/图片本身已压缩。
# dx 的 pre_compress 已覆盖 assets/。
precompress: check-brotli
	@find target/dx/yggdrasil/release/web/public -type f \
		\( -name '*.js' -o -name '*.css' -o -name '*.wasm' \
		   -o -name '*.svg' -o -name '*.html' -o -name '*.json' -o -name '*.xml' \) \
		-not -name '*.br' \
		-print0 | xargs -0 -r brotli -q 11 -kf

# ── 开发 ───────────────────────────────────────────────────────
# 开发态即完整应用：dx 的客户端构建取「default features 减 server」再加 CLI
# features（见 Cargo.toml [features] 注释），default 不含 client-reader，
# 后台/图鉴照常编译；reader 产物只在 release 流水线显式构建（plan §6.3）。
dev: dev-assets esbuild-cache wasm-bindgen-cache ## 启动开发服务器（增量构建资源 + dx serve）
	@echo "Starting dx serve..."
	@SSR_CACHE_SECS=0 $(DX) serve --features dev-logger --addr 0.0.0.0 --interactive false

.PHONY: regression-build test-databases test-browser
regression-build: dev-assets esbuild-cache wasm-bindgen-cache ## 构建隔离浏览器回归使用的应用
	@$(DX) build --platform web --features dev-logger
	@$(MAKE) restore-webp

test-databases: ## 在临时 PostgreSQL 中验证文章、笔记和备份，结束后清理
	@bun scripts/regression.cjs database

BROWSER_SUITE ?= smoke
test-browser: ## 隔离浏览器回归（先 regression-build；BROWSER_SUITE=smoke/admin/notes/backup/runner/slash/lightbox/showcase）
	@bun scripts/regression.cjs $(BROWSER_SUITE)

# ── 测试与代码质量 ─────────────────────────────────────────────
test: test-release-bootstrap ## 运行 Rust 与前端测试
	@cargo test
	@cd libs && bun run --filter '*' test

.PHONY: test-release-bootstrap test-wasm-preload
test-release-bootstrap: ## 验证双客户端模板、WASM preload 与发布清单
	@bun test scripts/release-bootstrap.test.cjs
	@bun scripts/test-wasm-preload.cjs --self-test

test-wasm-preload: ## 真实 release 的 WASM preload 冷/热缓存 A/B（默认自管隔离环境）
	@if [ -n "$(BASE_URL)" ]; then \
		bun scripts/test-wasm-preload.cjs; \
	else \
		WASM_PRELOAD_AB=1 bun scripts/test-dual-flavor.cjs; \
	fi

# JS + Rust 一次性检查（不改动文件）。
lint: ## Biome、TypeScript、Clippy、rustfmt 检查（只读）
	@echo "==> Biome check (libs)"
	@cd libs && bun run biome check . && bun run typecheck
	@echo "==> Cargo clippy (Rust)"
	@cargo clippy --all-targets --all-features -- -D warnings
	@echo "==> Cargo fmt check (Rust)"
	@cargo fmt -- --check

# JS + Rust 格式化（直接写入文件）。
# 故意不含 dx fmt：dioxus-autofmt 0.7.10（dx 0.7.10）会改写 rsx! 事件闭包内
# 的 Rust，插入重复行 / 悬空片段（如 `&web_file, ).ok();`）使代码无法编译
# （DioxusLabs/dioxus#5682、#3007）。此前的注释搬运/删除问题已让 docker-fmt
# 排除 dx fmt；现在它更进一步破坏编译，故从格式化/自动修复流水线移除。
# lint / CI 均不校验 dx fmt 输出，移除无回归。需要 RSX 格式化时手动 `dx fmt`
# 并务必 git diff 复核，必要时 checkout 被破坏的文件。
# fix、docker-fmt、docker-fix 都经由本 target，同样不含 dx fmt。
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
doc: ## 生成 rustdoc 到 public/doc/
	@RUSTDOCFLAGS="--default-theme=ayu" cargo doc --no-deps --document-private-items
	@rm -rf public/doc
	@cp -r target/doc public/doc
	@printf '<!DOCTYPE html><html><head><meta charset="utf-8"><meta http-equiv="refresh" content="0;url=yggdrasil/index.html"><title>Redirecting…</title></head><body><script>location.replace("yggdrasil/index.html")</script></body></html>' > public/doc/index.html

# 同 doc，生成完自动用浏览器打开。
doc-open: ## 生成 rustdoc 并用浏览器打开
	@RUSTDOCFLAGS="--default-theme=ayu" cargo doc --no-deps --document-private-items --open

# ── 清理 ───────────────────────────────────────────────────────
# public/{codemirror,lightbox,tiptap,xterm,yggdrasil-core} 里有被 git 跟踪的
# bundle，故不在此整目录清理；其余生成物（含 libs 的 node_modules）会被删除。
clean: ## 清理构建产物与前端依赖
	@cargo clean
	@rm -f public/style.css public/highlight.css
	@rm -rf public/katex
	@rm -rf public/mermaid
	@rm -rf public/doc
	@rm -rf uploads/.cache
	@rm -rf libs/node_modules libs/*/node_modules

.PHONY: test-public-performance
test-public-performance: regression-build ## 验证双机缓存、公开页精简数据、归档分页和评论延迟加载
	@bun scripts/test-public-performance.cjs
