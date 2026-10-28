# -----------------------------------------------------------------------------
# Builder stage: compile the static-linked musl server binary and frontend assets
# -----------------------------------------------------------------------------
FROM rust:1.99-trixie AS builder

# Master switch for Chinese mirrors.
#   CN_MIRROR=false (default): use all upstream/official sources (fast on CI
#     runners outside China — GitHub runners are in the US/EU).
#   CN_MIRROR=true: route all downloads through Chinese mirrors (TUNA for
#     Debian/Alpine apt, rsproxy for Rust/crates.io, npmmirror for Bun/npm,
#     gh-proxy for GitHub Releases). Pass via: --build-arg CN_MIRROR=true
# Individual mirror URLs are still ARG-overridable for custom mirrors.
ARG CN_MIRROR=false
# Exported as ENV so build recipes (make) can read $CN_MIRROR when needed.
ENV CN_MIRROR=${CN_MIRROR}

ARG DEBIAN_MIRROR=https://mirrors.tuna.tsinghua.edu.cn/debian
ARG DEBIAN_SECURITY_MIRROR=https://mirrors.tuna.tsinghua.edu.cn/debian-security
ARG NPM_REGISTRY=https://registry.npmmirror.com
ARG RS_PROXY=https://rsproxy.cn
ARG GH_PROXY=https://gh-proxy.com

# --- Debian apt: rewrite sources to the TUNA mirror (CN_MIRROR only). ---
RUN if [ "$CN_MIRROR" = "true" ]; then \
        sed -i \
          -e "s|http://deb.debian.org/debian|${DEBIAN_MIRROR}|g" \
          -e "s|http://deb.debian.org/debian-security|${DEBIAN_SECURITY_MIRROR}|g" \
          -e "s|http://security.debian.org/debian-security|${DEBIAN_SECURITY_MIRROR}|g" \
          /etc/apt/sources.list.d/debian.sources; \
    fi

# Install system build tooling. Native dependencies are needed for:
#   - build-essential + musl-tools: AWS-LC's non-FIPS C build (no OpenSSL,
#     CMake or Go requirement); musl-gcc supplies the native musl sysroot.
#   - musl-tools: linker for x86_64-unknown-linux-musl
#   - cmake/clang/nasm/libssl-dev: libwebp (zenwebp), ring, syntect
#   - curl/gnupg/ca-certificates: download tooling
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        cmake \
        clang \
        nasm \
        pkg-config \
        libssl-dev \
        musl-tools \
        brotli \
        ca-certificates \
        curl \
        gnupg \
        xz-utils \
    && rm -rf /var/lib/apt/lists/*

# --- Bun ---
# 官方 npm 包 @oven/bun-linux-<arch> 的 tgz 内只有单文件二进制，无需 Node.js 和 unzip。
# CN_MIRROR=true: 走 npmmirror；CN_MIRROR=false: 走 registry.npmjs.org。
ARG BUN_VERSION=1.4.4
RUN ARCH="$(dpkg --print-architecture)" \
    && case "$ARCH" in \
        amd64)  BUN_ARCH=x64     ;; \
        arm64)  BUN_ARCH=aarch64 ;; \
        *) echo "unsupported arch: $ARCH" >&2; exit 1 ;; \
    esac \
    && if [ "$CN_MIRROR" = "true" ]; then BUN_REGISTRY="${NPM_REGISTRY}"; else BUN_REGISTRY="https://registry.npmjs.org"; fi \
    && curl -fsSL "${BUN_REGISTRY}/@oven/bun-linux-${BUN_ARCH}/-/bun-linux-${BUN_ARCH}-${BUN_VERSION}.tgz" \
        | tar -xz -C /usr/local/bin --strip-components=2 package/bin/bun \
    && bun --version

# Configure the npm registry for `bun install` (CN_MIRROR only — default is registry.npmjs.org).
# Bun reads ~/.npmrc; bun.lock records default-registry packages without a URL,
# so this redirects every dependency download.
RUN if [ "$CN_MIRROR" = "true" ]; then \
        echo "registry=${NPM_REGISTRY}" > /root/.npmrc; \
    fi

# --- Rust: rsproxy mirror for rustup + cargo (CN_MIRROR only). ---
# RUSTUP_DIST_SERVER is not set as ENV because ENV cannot be conditional.
# It is exported inline in the sole rustup-download RUN below; all subsequent
# cargo commands read config.toml (or use the default crates.io).
RUN if [ "$CN_MIRROR" = "true" ]; then \
        mkdir -p /usr/local/cargo \
        && printf \
            '[source.crates-io]\nreplace-with = "rsproxy-sparse"\n\n[source.rsproxy-sparse]\nregistry = "sparse+%s/index/"\n' \
            "${RS_PROXY}" \
            > /usr/local/cargo/config.toml; \
    fi

# Release WASM is compiled with a pinned nightly (-Zbuild-std, panic=immediate-abort;
# see WASM_NIGHTLY in the Makefile). Keep this ARG in sync with that variable.
ARG WASM_NIGHTLY=nightly-2026-10-19

# Add the musl targets used by the server build. Both are
# installed; each buildx platform leg builds only its native one (see below).
# The wasm32 target lives on the pinned nightly that builds the client.
# RUSTUP_DIST_SERVER is exported inline (only rustup-download that needs it).
RUN if [ "$CN_MIRROR" = "true" ]; then \
        export RUSTUP_DIST_SERVER="${RS_PROXY}" \
        && export RUSTUP_UPDATE_ROOT="${RS_PROXY}/rustup"; \
    fi \
    && rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl \
    && rustup toolchain install "$WASM_NIGHTLY" --profile minimal \
        --component rust-src --target wasm32-unknown-unknown

# --- Tailwind CSS v4: the standalone binary is distributed via GitHub
# Releases (~106 MB). ---
ARG TAILWIND_VERSION=4.3.3
RUN ARCH="$(dpkg --print-architecture)" \
    && case "$ARCH" in \
        amd64)  TW_ARCH=x64   ;; \
        arm64)  TW_ARCH=arm64 ;; \
        *) echo "unsupported arch: $ARCH" >&2; exit 1 ;; \
    esac \
    && if [ "$CN_MIRROR" = "true" ]; then GH_PX="${GH_PROXY}"; else GH_PX=""; fi \
    && GH_URL="${GH_PX:+${GH_PX}/}https://github.com/tailwindlabs/tailwindcss/releases/download/v${TAILWIND_VERSION}/tailwindcss-linux-${TW_ARCH}" \
    && curl -fsSL -o /usr/local/bin/tailwindcss "${GH_URL}" \
    && chmod +x /usr/local/bin/tailwindcss

# --- cargo-leptos: 前端构建（wasm-bindgen、分包、CSS）。独立层，便于缓存。 ---
RUN cargo install cargo-leptos --version 0.3.11 --locked

# --- cargo-chef: 用于把 Rust 依赖编译与源码编译分离成独立的 Docker 层。
# 改一行 .rs 不再触发 576 个依赖的全量重编。CI 端用 GHA cache (type=gha)
# 跨 run 持久化 cooker 层,依赖不变时该层 cache-hit(秒级),只编 app。
# 用 GitHub Releases 预编译二进制(不 cargo install:后者编译 cargo-chef 自身
# 依赖要 2-3min,且在 QEMU 下 SIGSEGV)。与 tailwind 同模式:按架构选 tarball。
ARG CHEF_VERSION=0.1.77
RUN ARCH="$(dpkg --print-architecture)" \
    && case "$ARCH" in \
        amd64) CHEF_TRIPLET=x86_64-unknown-linux-musl   ;; \
        arm64) CHEF_TRIPLET=aarch64-unknown-linux-musl ;; \
        *) echo "unsupported arch: $ARCH" >&2; exit 1 ;; \
    esac \
    && if [ "$CN_MIRROR" = "true" ]; then GH_PX="${GH_PROXY}"; else GH_PX=""; fi \
    && CHEF_URL="${GH_PX:+${GH_PX}/}https://github.com/LukeMathWalker/cargo-chef/releases/download/v${CHEF_VERSION}/cargo-chef-${CHEF_TRIPLET}.tar.xz" \
    && curl -fsSL --retry 5 --retry-delay 5 --retry-all-errors "${CHEF_URL}" -o /tmp/chef.tar.xz \
    && tar -xJf /tmp/chef.tar.xz -C /usr/local/bin --strip-components=1 \
    && rm /tmp/chef.tar.xz \
    && cargo-chef --version


WORKDIR /build

# Cache the bun workspace node_modules by copying only package manifests first.
# package.json declares `workspaces: ["*"]`, so every sub-package manifest is
# copied before `bun install --frozen-lockfile`, keeping this layer reusable until
# a manifest or the lockfile changes (and matching the workspaces recorded in bun.lock).
# `patchedDependencies` in package.json points at
# `patches/@tiptap__markdown@3.31.4.patch`, so the patches/ tree must be present
# before the install or it fails on the missing patch. bunfig.toml carries the
# release-age policy and `[run] bun = true`.
COPY libs/package.json libs/bunfig.toml libs/bun.lock libs/
COPY libs/patches/                         libs/patches/
COPY libs/shared/package.json             libs/shared/
COPY libs/tiptap-editor/package.json      libs/tiptap-editor/
COPY libs/codemirror-editor/package.json  libs/codemirror-editor/
COPY libs/lightbox/package.json           libs/lightbox/
COPY libs/xterm-terminal/package.json     libs/xterm-terminal/
COPY libs/mermaid-renderer/package.json   libs/mermaid-renderer/
COPY libs/yggdrasil-core/package.json     libs/yggdrasil-core/
RUN cd libs && bun install --frozen-lockfile

# Build-time git info, injected by the caller via --build-arg. `.dockerignore`
# excludes `.git/`, so build.rs can't run `git` inside the container — these
# ARGs are the only channel for git metadata to reach build.rs (it reads them
# via std::env::var, its first-precedence source). Defaults are empty so a
# bare `docker build` without args degrades to "unknown" gracefully (build.rs
# falls back to the git command, then "unknown").
#
# Each ARG → ENV pair is needed: ARG is only visible in Dockerfile RUN
# commands (and build.rs is invoked by cargo, not a RUN), so we export it as
# ENV for the cargo build step to inherit. Both default to empty; Makefile's
# docker/docker-amd64/docker-multiarch targets override them on the host.
ARG YGG_BUILD_GIT_DESCRIBE=""
ARG YGG_BUILD_GIT_HASH=""
ARG YGG_BUILD_GIT_COMMIT_DATE=""
ENV YGG_BUILD_GIT_DESCRIBE=${YGG_BUILD_GIT_DESCRIBE}
ENV YGG_BUILD_GIT_HASH=${YGG_BUILD_GIT_HASH}
ENV YGG_BUILD_GIT_COMMIT_DATE=${YGG_BUILD_GIT_COMMIT_DATE}

# ──────────────────────────────────────────────────────────────────────────
# cargo-chef 依赖分层:把 576 个 Rust 依赖的编译与 app 源码分离。
# planner 只需 Cargo.toml + Cargo.lock 生成 recipe.json(无源码),所以只要
# 依赖清单不变,recipe.json 不变,cooker 层就 cache-hit。CI 用 GHA cache
# (type=gha, mode=max) 跨 run 持久化 cooker 层——改一行 .rs 时,依赖编译
# 从 ~10min 降到 ~0s,只编 app(~2min)。
#
# cook 必须用与最终 cargo build 完全相同的 target/features/rustflags,
# 否则产物目录不匹配、缓存形同虚设。这里镜像后端(server musl release),
# WASM 前端走 cargo-leptos 不经 cargo-chef(它另走 wasm32 target)。
COPY Cargo.toml Cargo.lock ./
# cargo-chef prepare 运行 `cargo metadata`，后者校验所有 autobins/[[bin]] 源文件
# 物理存在；此刻仅 COPY 了 manifest，需 dummy 入口让 metadata 解析通过。内容无关：
# cook 阶段由 cargo-chef 自带 skeleton 接管编译依赖，下方 `COPY . .` 再以真实源码覆盖。
RUN mkdir -p src/bin \
    && printf 'fn main() {}\n' > src/main.rs \
    && printf 'fn main() {}\n' > src/bin/generate_highlight_css.rs \
    && : > src/lib.rs
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    ARCH="$(dpkg --print-architecture)" \
    && case "$ARCH" in \
        amd64) MUSL_TARGET=x86_64-unknown-linux-musl  ;; \
        arm64) MUSL_TARGET=aarch64-unknown-linux-musl ;; \
        *) echo "unsupported arch: $ARCH" >&2; exit 1; \
    esac \
    && cargo chef prepare --recipe-path recipe.json \
    && export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=musl-gcc \
    && export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=musl-gcc \
    && export RUSTFLAGS="-C target-feature=+crt-static -C relocation-model=static --cfg erase_components" \
    && cargo chef cook --release --locked --target "$MUSL_TARGET" --recipe-path recipe.json
# 不再 cook host(dev) target: 下方 cargo doc 用 --no-deps(不编依赖),host cook 无服务对象;
# 且 cargo-chef 0.1.77 在 host cook 的 skeleton 模式下对 archmage-macros 等 proc-macro
# 报 "target does not support these crate types"(0.8.2 CI 实证,amd64/arm64 runner 均挂此)。
# server 依赖已由上方 musl release cook 完整预热;cargo doc 走正常 cargo 流程按需自编
# proc-macro(host),不触发 cargo-chef 该 bug。

# Copy the rest of the source tree and build everything. 依赖已由 cooker 编好,
# 此后的 cargo build/doc 只增量编译 app 代码。
COPY . .

# Build the JS libs, syntax-highlight CSS, KaTeX CSS + fonts and Tailwind
# stylesheet. These steps produce the contents of the public/ directory.
# Shared with make build / build-linux via the build-assets target, so the
# pipeline cannot drift (katex-css was once missing here, which left math
# rendering as bare spans without KaTeX fonts).
RUN make build-assets

# 用 cargo-leptos 构建分包前端（wasm/js/css → target/site，hash.txt → target/release）。
# make build-frontend 使用固定 nightly + build-std（见 Makefile 的 LEPTOS_WASM_ENV）。
# 服务端由下方的 plain cargo 以 musl 目标构建。
# --mount 缓存 wasm32 的 cargo registry/git/target：前端构建不经 cargo-chef，
# 改一行 .rs 也会全量重下重编 ~400 个 wasm 依赖；mount 让它们跨 build 持久化。
# cache mount 只在本 RUN 期间存在，因此 hash.txt 在同一 RUN 内先拷到 /build/hash.txt，
# 再由 build-release.ts 把 target/site 与 hash.txt 装配进 /build/dist。
# 与 cook/cargo build 的 server musl 产物互不干扰——后者在镜像层 /build/target，
# 本 RUN 的 cache 卷覆盖该路径但只装 wasm32 子目录与站点产物。
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/build/target,sharing=locked \
    make build-frontend && \
    cp target/release/hash.txt /build/hash.txt && \
    bun scripts/build-release.ts --out /build/dist --hash /build/hash.txt

# 生成带站点主题的 lib crate 文档，包含私有项，并提供 /doc 入口重定向。
# .dockerignore 排除了宿主 public/doc/；这里直接写入装配后的发布目录。
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    make doc DOC_OUT=dist/public/doc

# 与本地发布共用预压缩规则；查找或压缩失败时立即终止构建。
RUN make precompress RELEASE_OUT=/build/dist

# 校验发布产物：清单摘要、hash.txt 指向的产物、分包清单闭包与 .br 旁车。
RUN bun scripts/build-release.ts --verify --out /build/dist

# Build the server as a fully static musl binary, **natively for the buildx
# platform leg**. Each leg builds only its own arch, so musl-gcc (which Debian
# ships for the host arch only) and the target always match — no cross-compiler,
# no QEMU. Cross-compiling here (e.g. building the x86_64 musl target from an
# arm64 leg) breaks ring: cc-rs emits -m64 for the x86_64 target and hands it to
# the arm64 musl-gcc, whose cc1 has no -m64 → "unrecognized command-line option".
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    ARCH="$(dpkg --print-architecture)" \
    && case "$ARCH" in \
        amd64) MUSL_TARGET=x86_64-unknown-linux-musl  ;; \
        arm64) MUSL_TARGET=aarch64-unknown-linux-musl ;; \
        *) echo "unsupported arch: $ARCH" >&2; exit 1 ;; \
    esac \
    && export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=musl-gcc \
    && export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER=musl-gcc \
    && export RUSTFLAGS="-C target-feature=+crt-static -C relocation-model=static --cfg erase_components" \
    && cargo build --release --locked --target "$MUSL_TARGET" --bin yggdrasil

# Ensure the uploads/backups directories exist for runtime. backups/ holds DB
# dump files written by the backup feature; without a nobody-owned dir here the
# runtime (USER 65534) cannot create it under root-owned /app.
RUN mkdir -p uploads backups

# Stage the built binary + assets at arch-independent paths so the alpine
# runtime stage can COPY them without knowing which musl target was built.
RUN ARCH="$(dpkg --print-architecture)" \
    && case "$ARCH" in \
        amd64) MUSL_TARGET=x86_64-unknown-linux-musl  ;; \
        arm64) MUSL_TARGET=aarch64-unknown-linux-musl ;; \
        *) echo "unsupported arch: $ARCH" >&2; exit 1 ;; \
    esac \
    && cp "/build/target/${MUSL_TARGET}/release/yggdrasil" /build/server

# -----------------------------------------------------------------------------
# Runtime stage: alpine + PostgreSQL client tools
#
# pg_dump/psql 是应用内备份/恢复功能的运行时依赖：scratch 镜像里没有它们，
# 生产曾因此回退到「仅数据、不可恢复」的纯 SQL 导出。服务器 PG 为 16，
# 客户端包与其对齐（pg_dump 官方保证向下 dump 旧版 server）。
# 静态 musl server 二进制在 alpine 上原生运行；reqwest 的 Rustls/AWS-LC
# 使用 rustls-platform-verifier 读取系统 CA，运行时须安装 ca-certificates。
# -----------------------------------------------------------------------------
FROM alpine:3.22

RUN apk add --no-cache ca-certificates postgresql16-client

WORKDIR /app

# Copy the static musl server binary and the bundled public assets.
COPY --from=builder --chown=65534:65534 /build/server /app/server
COPY --from=builder --chown=65534:65534 /build/dist/public /app/public
# 服务端启动时从可执行文件同目录读取 hash.txt，校验前端产物与二进制配套。
COPY --from=builder --chown=65534:65534 /build/dist/hash.txt /app/hash.txt
COPY --from=builder --chown=65534:65534 /build/uploads /app/uploads
COPY --from=builder --chown=65534:65534 /build/backups /app/backups

# The app checks for DATABASE_URL on startup even though this image is intended
# to run without a real database. A placeholder is enough to let the server boot.
ENV DATABASE_URL=postgres://postgres:postgres@localhost:5432/yggdrasil
ENV LEPTOS_SITE_ROOT=/app/public
ENV IP=0.0.0.0
ENV PORT=3000
ENV RUST_LOG=info

USER 65534:65534

EXPOSE 3000

ENTRYPOINT ["/app/server"]
