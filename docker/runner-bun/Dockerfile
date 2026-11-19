# Bun 运行沙箱镜像。
# 与 src/api/code_runner/languages.rs 的 bun 注册项对齐：
#   image: yggdrasil-runner-bun:latest
#   run_cmd: bun run /code/main.ts
#
# 通过 bun.sh 官方安装脚本安装 Bun（多架构，自动检测 /etc/alpine-release 选 musl 变体）。
# Bun 的 musl 二进制是 C++ 写的，
# 运行时依赖 libstdc++ + libgcc（musl 只带 libc，不带 C++ 运行时），必须保留。
# 安装期另需 unzip + bash + curl；安装后卸载 unzip/curl 减小镜像。
FROM yggdrasil-runner-base:latest

# libstdc++ / libgcc 是 bun 运行时依赖，不能删；curl/unzip/bash 仅供安装期。
# BUN_INSTALL 用 export 对管道两侧生效——「VAR=x cmd | bash」只给左侧设变量，
# bash 侧读不到会 fallback 到 $HOME/.bun。指定到 /opt/bun 避免依赖 $HOME
# （runner 用户 $HOME 可能不可写或不存在）。
RUN apk add --no-cache curl unzip bash libstdc++ libgcc \
    && export BUN_INSTALL=/opt/bun \
    && curl -fsSL https://bun.sh/install | bash \
    && ln -s /opt/bun/bin/bun /usr/local/bin/bun \
    && apk del curl unzip \
    && bun --version

COPY runner.toml /runner.toml

USER runner
