# 生成资源：libs 前端库、语法高亮 CSS、KaTeX CSS + 字体、CSS 打包。
# 两条流水线共用这里的 target：
#   - release：build-assets 全量构建（build / build-linux / Dockerfile 共用）
#   - dev：    dev-assets 基于 stamp 增量构建，只重建变化的部分
# dev 的 stamp 规则依赖本文件本身（DEV_COMMON_INPUTS 等），所以改动本文件会让
# 所有 stamp 失效、触发一次完整重建；改别的 Makefile / mk/*.mk 不会。

.PHONY: build-assets build-libs install-release-libs highlight-css katex-css css css-watch
.PHONY: dev-assets dev-libs dev-highlight-css dev-katex-css dev-assets-force

# 两个 CSS 入口：input.css → public/style.css（全站），input-admin.css → public/admin.css
# （后台与组件图鉴，按路由加载）。各自只编译显式导入的 styles/ 文件。
CSS_BUILD := bun build --target=browser
CSS_SITE := $(CSS_BUILD) input.css --outfile=public/style.css
CSS_ADMIN := $(CSS_BUILD) input-admin.css --outfile=public/admin.css

# release 构建共用的资源流水线：libs 前端库、语法高亮 CSS、KaTeX CSS + 字体、
# CSS 打包（压缩）。build / build-linux / 两个 Dockerfile 都调用它，不要在别处
# 再手写这几步——KaTeX 曾因 Dockerfile 里漏掉 katex-css 而退化成裸 span。
ifneq ($(RELEASE_LIBS_READY),1)
build-assets: install-release-libs
endif
build-assets: ## 构建 public/ 下的全部生成资源（libs、高亮/KaTeX CSS、CSS 打包）
	@cd libs && YGG_RELEASE_ASSETS=1 bun run build:release
	@$(MAKE) highlight-css
	@$(MAKE) katex-css
	@$(CSS_SITE) --minify
	@$(CSS_ADMIN) --minify

highlight-css: ## 生成语法高亮 CSS
	@bun libs/shared/scripts/generate-code-themes.ts --site-only

# 把 npm 包 katex 的 dist/ 拷贝到 public/katex/（服务端 katex-rs 不打包 CSS）。
# KaTeX 的 katex.min.css 用相对 URL 引 fonts/，故 fonts/ 必须与 CSS 同级。
# 只拷 woff2（现代浏览器全支持，省去 woff/ttf ~70% 字体体积）。
# katex 由 libs/ 工作区根声明，bun install 后在 libs/node_modules/katex/。
katex-css: ## 拷贝 KaTeX CSS 与 woff2 字体到 public/katex/
	@echo "Copying KaTeX CSS + woff2 fonts to public/katex/..."
	@mkdir -p public/katex/fonts
	@cp libs/node_modules/katex/dist/katex.min.css public/katex/katex.min.css
	@cp libs/node_modules/katex/dist/fonts/*.woff2 public/katex/fonts/
	@echo "KaTeX CSS ready at public/katex/"

# 并行构建全部 libs/ 子项目（bun --filter 按依赖拓扑排序，无相互依赖则并发）。
# 按锁文件安装依赖；清单不一致时失败，不在发布构建中更新锁文件。
install-release-libs:
	@cd libs && bun install --frozen-lockfile --omit=dev

build-libs: ## 全量构建 libs/ 前端库
	@cd libs && bun install --frozen-lockfile && bun run --filter '*' build

css: ## 生成站点 CSS（未压缩）
	@$(CSS_SITE)
	@$(CSS_ADMIN)

css-watch: ## 监听并持续生成站点 CSS
	@$(CSS_SITE) --watch --no-clear-screen & site_pid=$$!; \
	$(CSS_ADMIN) --watch --no-clear-screen & admin_pid=$$!; \
	trap 'kill $$site_pid $$admin_pid 2>/dev/null || true' EXIT; \
	trap 'exit 130' INT; trap 'exit 143' TERM; \
	wait $$site_pid $$admin_pid

# ── dev 增量构建 ───────────────────────────────────────────────
# 开发启动只重建变化的资源；发布和显式 build-libs 仍执行完整构建。
# stamp 写在成功构建之后。src 目录也参与依赖，文件新增/删除同样会使缓存失效。
DEV_ASSET_DIR := target/dev-assets
DEV_ASSET_JOBS ?= 4
DEV_LIBS := $(patsubst libs/%/package.json,%,$(wildcard libs/*/package.json))
DEV_COMMON_INPUTS := mk/assets.mk libs/package.json libs/bun.lock libs/bunfig.toml libs/tsconfig.base.json $(wildcard libs/patches/*) libs/patches $(wildcard libs/*.ts) $(wildcard libs/node_modules/.bun)
dev_lib_inputs = libs/$(1) $(shell find libs/$(1)/src -type f -o -type d) $(wildcard libs/$(1)/*config*.ts) libs/$(1)/tsconfig.json libs/$(1)/package.json

CODEMIRROR_MANIFEST := public/codemirror/.vite/manifest.json
CODEMIRROR_JS := $(addprefix public/codemirror/,$(shell if [ -f $(CODEMIRROR_MANIFEST) ]; then sed -n 's/.*"file": "\(.*\.js\)",\{0,1\}$$/\1/p' $(CODEMIRROR_MANIFEST); fi))
DEV_OUTPUTS_codemirror-editor := public/codemirror/.vite/manifest.json public/codemirror/editor-module.js public/codemirror/editor-module.js.map
DEV_OUTPUTS_codemirror-editor += $(CODEMIRROR_JS) $(addsuffix .map,$(CODEMIRROR_JS))
DEV_OUTPUTS_lightbox := public/lightbox/lightbox.js public/lightbox/lightbox.js.map public/lightbox/lightbox.css
DEV_OUTPUTS_tiptap-editor := public/tiptap/editor.js public/tiptap/editor.js.map public/tiptap/editor.css public/tiptap/comment.js public/tiptap/comment.js.map public/tiptap/comment.css public/tiptap/math-renderer.js public/tiptap/math-renderer.js.map public/tiptap/code-highlighter.js public/tiptap/code-highlighter.js.map
DEV_OUTPUTS_xterm-terminal := public/xterm/terminal.js public/xterm/terminal.js.map public/xterm/terminal.css
DEV_OUTPUTS_yggdrasil-core := public/yggdrasil-core/yggdrasil-core.js public/yggdrasil-core/yggdrasil-core.js.map public/yggdrasil-core/yggdrasil-core.css public/yggdrasil-core/yggdrasil-enhance.js public/yggdrasil-core/yggdrasil-enhance.js.map

define dev_lib_rule
$(DEV_ASSET_DIR)/$(1).stamp: $(call dev_lib_inputs,$(1)) $(DEV_COMMON_INPUTS)
	@mkdir -p $(DEV_ASSET_DIR)
	@cd libs/$(1) && bun run build
	@touch $$@
$(if $(filter-out $(wildcard $(DEV_OUTPUTS_$(1))),$(DEV_OUTPUTS_$(1))),$(DEV_ASSET_DIR)/$(1).stamp: dev-assets-force)
endef
$(foreach lib,$(DEV_LIBS),$(eval $(call dev_lib_rule,$(lib))))

# 引用 shared 的库在其源码变化后重建。
$(addprefix $(DEV_ASSET_DIR)/,$(addsuffix .stamp,$(filter-out shared,$(DEV_LIBS)))): $(DEV_ASSET_DIR)/shared.stamp
dev-libs: $(addprefix $(DEV_ASSET_DIR)/,$(addsuffix .stamp,$(DEV_LIBS)))
dev-assets-force:

$(DEV_ASSET_DIR)/highlight.stamp: mk/assets.mk libs/shared/scripts/generate-code-themes.ts libs/shared/src/code-palette.ts
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

# 先安装依赖，再由子 make 读取新依赖和输出清单。两份站点 CSS 直接由入口生成。
dev-assets: check-dev-tools ## 只刷新变化的前端/CSS 资源，不启动服务
	@cd libs && bun install
	@$(MAKE) --no-print-directory -j$(DEV_ASSET_JOBS) dev-libs dev-highlight-css dev-katex-css css
