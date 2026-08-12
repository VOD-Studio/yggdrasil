# 开发构建产物磁盘占用（2026）

- 研究日期：2026-08-12
- 适用项目：Yggdrasil（Rust 1.97.1 / Cargo 1.97.1、Dioxus 0.7.10、pnpm 11.x）
- 目标：区分 Cargo 目标产物、Dioxus 打包产物、全局下载缓存与 Docker volume，给出低风险的释放与限额策略。

## 结论

1. Cargo 1.97 的自动清理针对 Cargo 全局缓存（`CARGO_HOME`）而不是项目 `target/`；官方文档明确说明 build artifacts 尚未纳入自动跟踪/GC。因此 `target/` 需要按构建模式手动维护。
2. 本仓库的主要占用不是 pnpm，而是 Rust/Dioxus 的多目标、多 profile、增量编译和 fullstack client+server 输出。当前测得 `target/` 约 20G；`cargo clean --dry-run` 报告约 23.0 GiB 可清理。
3. 最低风险顺序：先停止 `dx serve` 并清理确认不用的 Dioxus 生成物；再把 dev 调试信息降到 `line-tables-only`（本机 Cargo 配置，不先提交）；仅对一次性 Cargo 检查考虑 `CARGO_INCREMENTAL=0`；不要为 `dx serve` 全局关闭增量编译。
4. `sccache` 应当设置磁盘上限，但它不能直接缩小已有 `target/`；它把可复用编译结果放到另一个缓存目录。Dioxus dev 路径本仓库已经显式清空 `RUSTC_WRAPPER`，不要绕过该兼容性约束。
5. pnpm 的 `store prune` 适合偶尔运行，不适合每次开发运行；它不是当前 20G 问题的主因。

## 本仓库实测基线

命令：`du -sh target/* ~/.cargo/registry ~/.cache/sccache libs/node_modules`（2026-08-12）。

| 路径 | 大小 | 判断 |
| --- | ---: | --- |
| `target/` | 约 20G | 主问题 |
| `target/debug/` | 7.2G | `deps` 4.4G、`incremental` 2.3G、`build` 506M |
| `target/dx/` | 4.5G | Dioxus fullstack 输出 |
| `target/aarch64-unknown-linux-gnu/` | 4.0G | 独立 target/profile 产物，需确认是否仍使用 |
| `target/wasm32-unknown-unknown/` | 1.6G | wasm target 产物 |
| `target/server-dev/` | 1.3G | 另一套 server dev profile，需确认是否为历史产物 |
| `target/wasm-dev/` | 799M | 另一套 wasm dev profile，需确认是否为历史产物 |
| `libs/node_modules/` | 324M | 次要 |
| pnpm store (`pnpm store path`) | 340M | 次要 |
| `~/.cargo/registry/` | 1.3G | 全局下载/源码缓存 |
| `~/.cache/sccache/` | 572M（上限 10G） | 有增长上限风险 |

`target/dx/yggdrasil/debug/web/` 目前包含 8 个约 558M 的 `server*` 文件。当前运行中的进程是 `server-ee18f2a6`；因此不能在 `dx serve` 运行时按名称盲删。其余文件是否可删应在停止开发服务器后确认。

## 官方事实与适用含义

### Cargo 产物目录与 GC 边界

Cargo 将最终产物放在 target-dir、将中间产物放在 build-dir；可用 `CARGO_TARGET_DIR`、`build.target-dir`、`CARGO_BUILD_BUILD_DIR` 或 `build.build-dir` 改变位置。`target/<profile>` 与 `target/<triple>/<profile>` 会按 profile/target 分开保存，所以同一仓库的 host、wasm、musl、不同 profile 会自然产生多份依赖产物。

Cargo 当前的自动缓存清理只跟踪 `CARGO_HOME` 中的 registry index/source 与 git 依赖；官方明确写出 build artifacts 尚未被跟踪，相关工作见 cargo#13136。`cache.auto-clean-frequency` 默认每天检查一次，但它不是 `target/` 的大小上限。

来源：

- https://doc.rust-lang.org/cargo/reference/build-cache.html
- https://doc.rust-lang.org/cargo/reference/config.html#cache
- https://github.com/rust-lang/cargo/issues/13136

### dev profile 与增量编译

Cargo 默认 `dev` profile 是完整 debug info、增量编译开启。增量编译会把额外状态写入 `target`，换取后续重编速度；`debug = "line-tables-only"` 只保留文件/行号级信息，`debug = 0` 关闭调试信息。`CARGO_INCREMENTAL=0` 或 profile 配置可关闭增量编译。Cargo 还支持对所有非 workspace 依赖使用 `[profile.dev.package."*"]` 覆盖设置。

来源：

- https://doc.rust-lang.org/cargo/reference/profiles.html
- https://doc.rust-lang.org/cargo/reference/environment-variables.html

### sccache 的取舍

sccache 是 rustc wrapper，可复用不同 workspace 的编译结果。官方本地后端默认上限为 10GB，可用 `SCCACHE_CACHE_SIZE` 限制，可用 `SCCACHE_DIR` 搬到单独磁盘。Rust 支持文档要求关闭 rustc incremental 才能缓存 Rust 编译；这意味着“sccache + `CARGO_INCREMENTAL=0`”适合一次性 `cargo check/test/clippy` 或跨项目复用，不应无条件套在 Dioxus 热开发路径上。

来源：

- https://github.com/mozilla/sccache/blob/main/docs/Local.md
- https://github.com/mozilla/sccache/blob/main/docs/Rust.md
- https://doc.rust-lang.org/cargo/reference/build-cache.html#shared-cache

### pnpm

`pnpm store prune` 删除全局 store 中不再被任何项目引用的包；pnpm 官方建议偶尔运行，不要过于频繁，否则切换旧分支时会重新下载。pnpm 11 的 `modulesCacheMaxAge` 默认 7 天，`enableGlobalVirtualStore` 默认关闭。

来源：

- https://pnpm.io/cli/store
- https://pnpm.io/settings/node-modules

### Dioxus fullstack

Dioxus CLI 0.7.10 会从 fullstack 配置拆出 client 与 server 两个 build request；server 产物放到 web 输出目录。CLI 为各平台/模式生成独立 profile（如 `web-dev`、`server-dev`），并把工作目录放在 Cargo target-dir 下的 `target/dx/<app>/<debug|release>/...`。这解释了该项目同时存在普通 Cargo 产物、target triple 产物、Dioxus bundle 产物以及多份 server 可执行文件。

来源：

- https://github.com/DioxusLabs/dioxus/blob/v0.7.10/packages/cli/src/cli/build.rs
- https://github.com/DioxusLabs/dioxus/blob/v0.7.10/packages/cli/src/build/request.rs
- https://github.com/DioxusLabs/dioxus/blob/v0.7.10/packages/cli/README.md

## 推荐落地顺序

### 1. 立即释放已确认的生成物

先停止 `make dev`/`dx serve`。先查看分项大小：

```bash
du -sh target/* 2>/dev/null | sort -h
cargo clean --dry-run
```

如果只想重置 Dioxus dev bundle，停服后可删除对应的 `target/dx/yggdrasil/debug/`；如果只想释放历史 server 副本，可在确认没有进程使用它们后删除 `target/dx/yggdrasil/debug/web/server*`。下一次 `make dev` 会重新生成这些文件。

不要把 `make clean` 当日常命令：本仓库的 `Makefile` 会执行 `cargo clean`，同时删除 `libs/node_modules`、生成 CSS、文档和静态缓存，下一次开发会重新编译/安装。

### 2. 先只降低 debug info，不关增量

在用户级 `~/.cargo/config.toml`（不要先提交到仓库）试验：

```toml
[profile.dev]
debug = "line-tables-only"
```

若仍太大且不需要依赖内部调试信息，再评估：

```toml
[profile.dev.package."*"]
debug = 0
```

每次只改一项，重建一次后比较 `du -sh target` 与热重载体验。`line-tables-only` 保留行号级回溯；`debug = 0` 会牺牲依赖调试能力。Dioxus 的 adhoc dev profile 会继承 Cargo dev profile，仍需用实际 `dx serve` 验证。

### 3. 只对一次性 Cargo 工作流关闭增量

例如在不需要连续热重载的命令上单次使用：

```bash
CARGO_INCREMENTAL=0 cargo check --all-features
```

不要把它放进 `make dev`：Dioxus dev 依赖增量/热 patch 的开发体验；且 sccache 的 Rust 缓存要求增量关闭，二者是取舍而不是同时免费获得。

### 4. 给 sccache 设置硬上限

当前用户 Cargo 配置已启用 `rustc-wrapper = "sccache"`，但本仓库 Makefile 对 `dx build/serve` 明确设置空的 `RUSTC_WRAPPER`，以避免 `sccache dx rustc` 探测失败。保留该约束；只为直接 Cargo 工作流设置：

```bash
export SCCACHE_CACHE_SIZE=2G
export SCCACHE_DIR="$HOME/.cache/sccache"
sccache --show-stats
```

这限制的是 sccache，不是 `target/`。要立即清空 sccache，应先停止 sccache server，再处理缓存目录；不应在编译进行时删除它。

### 5. 需要时搬移 target，而不是复制多套 target

若磁盘分区是瓶颈，可只对开发命令使用更大的独立路径：

```bash
mkdir -p /path/to/large-disk/yggdrasil-target
CARGO_TARGET_DIR=/path/to/large-disk/yggdrasil-target make dev
```

这不会减少总字节数，只会把可重建产物移到更合适的盘；务必让 `dx serve`、普通 Cargo 命令长期复用同一个路径，避免每个命令各自生成完整依赖树。注意本仓库 `Makefile` 的 `restore-webp` 和部分提示写死了 `target/dx`，因此不要未经验证就对 `make build`/`make build-linux` 全局设置外部 target-dir；先限定在 `make dev` 并验证完整路径。

### 6. 低优先级的 pnpm/Docker 清理

```bash
cd libs
pnpm store path
pnpm store prune
```

偶尔执行即可。若使用 `make docker-dev` 或 `make docker-test`，Cargo target、`node_modules`、registry 和 pnpm store 在 Docker named volume 中，不会显示在仓库目录；应单独查看/清理对应 volume，不要误以为删除宿主 `target/` 会释放它们。

## 不推荐

- 每次改文件都 `cargo clean`：牺牲增量编译，换来下一次全量重编。
- 对 `dx serve` 全局设置 `RUSTC_WRAPPER=sccache`：本仓库现有 Makefile 已记录该组合会变成 `sccache dx rustc` 并失败。
- 对 `dx serve` 全局 `CARGO_INCREMENTAL=0`：磁盘下降但热重载/重编体验变差，且会改变 Dioxus 的开发构建策略。
- 每次 `pnpm install` 后都 `pnpm store prune`：会导致分支切换反复下载。
- 把绝对路径 `CARGO_TARGET_DIR` 写进仓库配置：破坏其他开发者和 CI 的路径可移植性。

## 验证清单

1. 停止 `dx serve` 后清理，再运行 `make dev`，确认 fullstack 页面、SSR 与 server function 请求正常。
2. 对 `debug = "line-tables-only"` 和依赖 `debug = 0` 分开测量 `target/debug`、`target/dx`、首次启动时间与二次热重载时间。
3. 对直接 Cargo 命令分别比较默认增量与 `CARGO_INCREMENTAL=0` 的 `sccache --show-stats` 命中率；不要用单次冷构建时间推断长期收益。
4. 清理后再次运行：

```bash
du -sh target libs/node_modules ~/.cargo/registry ~/.cache/sccache 2>/dev/null
```

研究记录不替代实际删除操作；所有 `rm -rf` 仅针对可重建生成物，并须在相关开发进程停止后执行。
