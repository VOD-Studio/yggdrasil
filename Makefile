# Yggdrasil 构建入口。不带参数的 `make` 只列出常用 target（见 help）。
#
# 模块：
#   mk/assets.mk    生成资源：libs、高亮/KaTeX CSS、Tailwind；release 全量 + dev 增量
#   mk/dx-cache.mk  预置 dx 的 esbuild / wasm-bindgen 工具缓存
#   mk/docker.mk    镜像构建、Docker 开发环境、lint/test/fmt 工具容器
include mk/assets.mk mk/dx-cache.mk mk/docker.mk

.PHONY: help build build-linux restore-webp precompress
.PHONY: check-dev-tools check-build-tools check-brotli
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

check-build-tools: check-dev-tools check-brotli ## 检查 release 构建所需的宿主工具

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

# ── release 构建 ───────────────────────────────────────────────
build: check-build-tools ## 完整 release 构建（资源 + 文档 + dx build + 压缩）
	@rm -rf static/
	@$(MAKE) build-assets
	@$(MAKE) doc
	@$(DX) build --release --debug-symbols=false
	@$(MAKE) restore-webp
	@$(MAKE) precompress

build-linux: check-build-tools ## 构建 Linux x86_64 musl 服务端 + 客户端
	@$(MAKE) build-assets
	@$(DX) build @client --release --debug-symbols=false --wasm-js-cfg false
	@$(DX) build @server --release --debug-symbols=false --target x86_64-unknown-linux-musl --wasm-js-cfg false --features server
	@$(MAKE) restore-webp
	@$(MAKE) precompress
	@echo ""
	@echo "Linux build complete! The server binary is at target/dx/yggdrasil/release/web/server"
	@echo "Remember to deploy it alongside the target/dx/yggdrasil/release/web/public directory."
	@echo "When running the server, ensure DIOXUS_ASSET_DIR is set or the public directory is in CWD."

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
dev: dev-assets esbuild-cache wasm-bindgen-cache ## 启动开发服务器（增量构建资源 + dx serve）
	@echo "Cleaning static/..."
	@rm -rf static/
	@echo "Starting dx serve..."
	@SSR_CACHE_SECS=0 $(DX) serve --addr 0.0.0.0 --interactive false

# ── 测试与代码质量 ─────────────────────────────────────────────
test: ## 运行 Rust 与前端测试
	@cargo test
	@cd libs && bun run --filter '*' test

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
	@rm -rf static/
	@rm -rf uploads/.cache
	@rm -rf libs/node_modules libs/*/node_modules
