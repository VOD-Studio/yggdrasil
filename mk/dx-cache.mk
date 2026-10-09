# 预置 dx 的工具缓存。dx 在 build / serve 时会自动下载 esbuild 和
# wasm-bindgen-cli，但下载源写死在 dx 源码里，国内直连很慢或被重置。
# 这里预先下载同版本产物到 dx 缓存目录，dx 命中缓存时会跳过联网。
# dev 和各 Dockerfile 都会调用；CN_MIRROR=true 时改走国内镜像。

.PHONY: esbuild-cache wasm-bindgen-cache

# 预置 dx 的 esbuild 工具缓存。
# dx CLI 硬编码 esbuild 下载源为 registry.npmjs.org（见 dx 源码 packages/cli/src/esbuild.rs），
# 不读 NPM_CONFIG_REGISTRY 也不读 .npmrc——npm config set registry 无效。
# 此处预下载与 dx 内置 ESBUILD_VERSION 完全一致的 tarball，解压到 dx 缓存目录；
# 默认取自 registry.npmjs.org，CN_MIRROR=true 时改取 npmmirror（阿里，SHA256 与
# npmjs.org 相同）。dx 在该路径已存在时跳过联网下载。
# 升级 dx 后须同步 ESBUILD_VERSION（查 dx 源码 esbuild.rs 的 ESBUILD_VERSION 常量）。
ESBUILD_VERSION := 0.27.3
esbuild-cache: ## 预置 dx 的 esbuild 缓存（CN_MIRROR=true 走国内镜像）
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

# 预置 dx 的 wasm-bindgen-cli 工具缓存（与 esbuild-cache 同构）。
# dx CLI 在 dx build 时会自动下载 wasm-bindgen-cli 二进制（packages/cli/src/
# wasm_bindgen.rs 的 verify_managed_install → install_github），下载源硬编码为
# github.com/rustwasm/wasm-bindgen/releases，既不读 GH_PROXY 也不读 NPM_REGISTRY——
# 国内直连必然慢/连接重置（"Taking a while..." 的主要来源之一）。此处预下载与
# Cargo.lock wasm-bindgen crate 版本完全一致的 tarball，解压到 dx 缓存目录；
# 默认直连 GitHub，CN_MIRROR=true 时经 gh-proxy。
# dx 的 wasm_bindgen.rs 在 install_dir.join(installed_bin_name).exists() 命中时
# 跳过联网下载。dx 按平台选 musl/darwin triplet（见 git_install_url）。
# WASM_BINDGEN_VERSION 直接取自 Cargo.lock，升级 wasm-bindgen 后无需手动同步。
# triplet 必须与 dx 源码 git_install_url 的平台映射一致。
WASM_BINDGEN_VERSION = $(shell awk '/^name = "wasm-bindgen"$$/ { getline; gsub(/[^0-9.]/, "", $$3); print $$3; exit }' Cargo.lock)
wasm-bindgen-cache: ## 预置 dx 的 wasm-bindgen 缓存（CN_MIRROR=true 走国内镜像）
	@set -e; \
	if [ -z "$(WASM_BINDGEN_VERSION)" ]; then \
		echo "error: 无法从 Cargo.lock 读取 wasm-bindgen 版本" >&2; \
		exit 1; \
	fi; \
	WB_DIR="$${DX_HOME:-$$HOME/.local/share/.dx}/tools/wasm-bindgen-$(WASM_BINDGEN_VERSION)"; \
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
