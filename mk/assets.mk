# 生成资源：libs 前端库、语法高亮 CSS、KaTeX CSS + 字体、Tailwind。
# 两条流水线共用这里的 target：
#   - release：build-assets 全量构建（build / build-linux / Dockerfile 共用）
#   - dev：    dev-assets 基于 stamp 增量构建，只重建变化的部分
# dev 的 stamp 规则依赖本文件本身（DEV_COMMON_INPUTS 等），所以改动本文件会让
# 所有 stamp 失效、触发一次完整重建；改别的 Makefile / mk/*.mk 不会。

.PHONY: build-assets build-libs highlight-css katex-css css css-watch
.PHONY: dev-assets dev-libs dev-highlight-css dev-katex-css dev-assets-force

TAILWIND := tailwindcss -i input.css -o public/style.css

# release 构建共用的资源流水线：libs 前端库、语法高亮 CSS、KaTeX CSS + 字体、
# Tailwind（压缩）。build / build-linux / 两个 Dockerfile 都调用它，不要在别处
# 再手写这几步——KaTeX 曾因 Dockerfile 里漏掉 katex-css 而退化成裸 span。
build-assets: ## 构建 public/ 下的全部生成资源（libs、高亮/KaTeX CSS、Tailwind）
	@$(MAKE) build-libs
	@$(MAKE) highlight-css
	@$(MAKE) katex-css
	@$(TAILWIND) --minify

highlight-css: ## 生成语法高亮 CSS
	@cargo run --bin generate_highlight_css

# 把 npm 包 katex 的 dist/ 拷贝到 public/katex/（服务端 katex-rs 不打包 CSS）。
# KaTeX 的 katex.min.css 用相对 URL 引 fonts/，故 fonts/ 必须与 CSS 同级。
# 只拷 woff2（现代浏览器全支持，省去 woff/ttf ~70% 字体体积）。
# katex 作为 libs/ workspace 根 devDependency，bun install 后在 libs/node_modules/katex/。
katex-css: ## 拷贝 KaTeX CSS 与 woff2 字体到 public/katex/
	@echo "Copying KaTeX CSS + woff2 fonts to public/katex/..."
	@mkdir -p public/katex/fonts
	@cp libs/node_modules/katex/dist/katex.min.css public/katex/katex.min.css
	@cp libs/node_modules/katex/dist/fonts/*.woff2 public/katex/fonts/
	@echo "KaTeX CSS ready at public/katex/"

# 并行构建全部 libs/ 子项目（bun --filter 按依赖拓扑排序，无相互依赖则并发）。
# build-libs 会先安装依赖（bun install），无需调用方自行安装。
build-libs: ## 全量构建 libs/ 前端库
	@cd libs && bun install && bun run --filter '*' build

css: ## 生成 Tailwind CSS（未压缩）
	@$(TAILWIND)

css-watch: ## 监听并持续生成 Tailwind CSS
	@$(TAILWIND) --watch

# ── dev 增量构建 ───────────────────────────────────────────────
# 开发启动只重建变化的资源；发布和显式 build-libs 仍执行完整构建。
# stamp 写在成功构建之后。src 目录也参与依赖，文件新增/删除同样会使缓存失效。
DEV_ASSET_DIR := target/dev-assets
DEV_ASSET_JOBS ?= 4
DEV_LIBS := shared codemirror-editor lightbox mermaid-renderer tiptap-editor xterm-terminal yggdrasil-core
DEV_COMMON_INPUTS := mk/assets.mk libs/package.json libs/bun.lock libs/bunfig.toml libs/tsconfig.base.json $(wildcard libs/patches/*) libs/patches $(wildcard libs/node_modules/.bun)
dev_lib_inputs = libs/$(1) $(shell find libs/$(1)/src -type f -o -type d) $(wildcard libs/$(1)/*config*.ts) libs/$(1)/tsconfig.json libs/$(1)/package.json

DEV_OUTPUTS_codemirror-editor := public/codemirror/editor.js public/codemirror/editor.js.map
DEV_OUTPUTS_lightbox := public/lightbox/lightbox.js public/lightbox/lightbox.js.map public/lightbox/lightbox.css
DEV_OUTPUTS_mermaid-renderer := public/mermaid/mermaid.js public/mermaid/mermaid.js.map
DEV_OUTPUTS_tiptap-editor := public/tiptap/editor.js public/tiptap/editor.js.map public/tiptap/editor.css
DEV_OUTPUTS_xterm-terminal := public/xterm/terminal.js public/xterm/terminal.js.map public/xterm/terminal.css
DEV_OUTPUTS_yggdrasil-core := public/yggdrasil-core/yggdrasil-core.js public/yggdrasil-core/yggdrasil-core.js.map public/yggdrasil-core/yggdrasil-core.css

define dev_lib_rule
$(DEV_ASSET_DIR)/$(1).stamp: $(call dev_lib_inputs,$(1)) $(DEV_COMMON_INPUTS)
	@mkdir -p $(DEV_ASSET_DIR)
	@cd libs/$(1) && bun run build
	@touch $$@
$(if $(filter-out $(wildcard $(DEV_OUTPUTS_$(1))),$(DEV_OUTPUTS_$(1))),$(DEV_ASSET_DIR)/$(1).stamp: dev-assets-force)
endef
$(foreach lib,$(DEV_LIBS),$(eval $(call dev_lib_rule,$(lib))))

# shared 内联进这些库；Mermaid 自身不依赖 shared。
$(addprefix $(DEV_ASSET_DIR)/,$(addsuffix .stamp,$(filter-out shared mermaid-renderer,$(DEV_LIBS)))): $(DEV_ASSET_DIR)/shared.stamp
dev-libs: $(addprefix $(DEV_ASSET_DIR)/,$(addsuffix .stamp,$(DEV_LIBS)))
dev-assets-force:

$(DEV_ASSET_DIR)/highlight.stamp: mk/assets.mk Cargo.toml Cargo.lock build.rs src/bin/generate_highlight_css.rs themes/Catppuccin\ Latte.tmTheme themes/Catppuccin\ Mocha.tmTheme
	@$(MAKE) --no-print-directory highlight-css
	@mkdir -p $(DEV_ASSET_DIR)
	@touch $@
ifeq ($(wildcard public/highlight.css),)
$(DEV_ASSET_DIR)/highlight.stamp: dev-assets-force
endif
dev-highlight-css: $(DEV_ASSET_DIR)/highlight.stamp

DEV_KATEX_FONTS := $(wildcard libs/node_modules/katex/dist/fonts/*.woff2)
DEV_KATEX_OUTPUTS := public/katex/katex.min.css $(patsubst libs/node_modules/katex/dist/%,public/katex/%,$(DEV_KATEX_FONTS))
$(DEV_ASSET_DIR)/katex.stamp: mk/assets.mk libs/package.json libs/bun.lock libs/node_modules/katex/dist/katex.min.css $(DEV_KATEX_FONTS)
	@$(MAKE) --no-print-directory katex-css
	@mkdir -p $(DEV_ASSET_DIR)
	@touch $@
ifneq ($(filter-out $(wildcard $(DEV_KATEX_OUTPUTS)),$(DEV_KATEX_OUTPUTS)),)
$(DEV_ASSET_DIR)/katex.stamp: dev-assets-force
endif
dev-katex-css: $(DEV_ASSET_DIR)/katex.stamp

# 先安装依赖，再由子 make 读取新依赖和输出清单。Tailwind 扫描所有模板，继续实时生成。
dev-assets: check-dev-tools ## 只刷新变化的前端/CSS 资源，不启动服务
	@cd libs && bun install
	@$(MAKE) --no-print-directory -j$(DEV_ASSET_JOBS) dev-libs dev-highlight-css dev-katex-css css
