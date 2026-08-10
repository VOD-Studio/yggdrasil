# 代码运行器的共享基础镜像。
# 仅创建非 root 用户 1000:1000 与 /code 工作目录；具体语言镜像在此基础上安装运行时。
# 与 src/infra/docker.rs 的 user: "1000:1000"、working_dir: "/code" 对齐。
FROM alpine:3.18

ARG CN_MIRROR=false

# 替换为国内镜像源（仅 CN_MIRROR=true 时启用），避免 apk 拉取 dl-cdn.alpinelinux.org 失败。
RUN if [ "$CN_MIRROR" = "true" ]; then \
        sed -i 's|dl-cdn.alpinelinux.org|mirrors.tuna.tsinghua.edu.cn|g' /etc/apk/repositories; \
    fi
RUN addgroup -S runner && adduser -S runner -G runner -u 1000 \
    && mkdir -p /code \
    && chown -R runner:runner /code

WORKDIR /code
