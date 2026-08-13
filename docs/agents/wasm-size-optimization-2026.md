# Rust/Dioxus WebAssembly 发布产物体积优化事实（2026）

- 研究日期／访问日期：2026-08-13
- 截止日期：2026-08-13
- 目的：为 Yggdrasil 的 WASM 产物分析建立可复核的官方事实基线；本文只记录资料和测量方法，不判断本项目当前配置，也不提交构建改动。
- 来源范围：Rust/Cargo、wasm-bindgen、Binaryen、Dioxus 官方文档或源码，以及 WebAssembly 规范、MDN、Chrome/web.dev 官方资料。

## 先区分四种“体积”和两种成本

同一个模块至少要分别记录以下指标，不能把它们互换：

| 指标 | 含义 | 典型测量对象 | 不能直接推出的结论 |
| --- | --- | --- | --- |
| 原始 Wasm 体积 | `.wasm` 文件在文件系统中的字节数，包含仍保留的自定义段、名称段和 DWARF（若有） | `stat`/`wc -c`；发布目录中的最终 `.wasm` | 不等于网络传输字节数，也不等于编译或运行时间 |
| 传输体积 | HTTP `Content-Encoding` 应用后的响应字节数 | 浏览器 Network 面板压缩尺寸、CDN/服务器日志 | 不代表原始模块已变小；不同编码、级别和内容会产生不同结果 |
| 解压／编译／实例化成本 | 浏览器取得响应后解码、编译和实例化的时间及峰值内存 | Performance/Network 记录、目标设备冷缓存测量 | 不能从原始或压缩字节数单独预测；`-Oz` 也不保证运行最快 |
| 运行时性能 | 已实例化模块执行导出函数的吞吐、延迟、内存和电量 | 目标浏览器、设备、真实工作负载的基准 | 不能用下载体积或 `wasm-opt` 等级替代实测 |

HTTP `Content-Encoding` 的语义是：接收方解码后得到原始内容格式；存在该头时，`Content-Length` 等元数据指向编码后的形式（MDN [S1]）。因此“压缩后下载更少”与“原始 `.wasm` 更小”是两个独立结果。MDN 还说明 `WebAssembly.instantiateStreaming()` 可以从流中直接编译并实例化，要求服务器返回 `application/wasm` MIME 类型（[S2]）；这会改变加载路径和编译重叠方式，但不会把压缩传输尺寸当成运行时模块尺寸。

## 官方事实与适用条件

### 1. Cargo profile：先以 release 产物为基线，再逐项比较

Cargo 的 profile 设置位于 workspace 根 manifest；依赖 manifest 中同名设置会被忽略（Cargo [S3]）。官方 release 默认值是 `opt-level = 3`、`debug = false`、`lto = false`、`panic = "unwind"`、`incremental = false`、`codegen-units = 16`、`strip = "none"`（[S3]）。这些是 Cargo 默认值，不是对任何特定仓库的观测。

- **`opt-level`**：`"s"` 面向二进制体积，`"z"` 面向体积且关闭 loop vectorization；官方明确建议试验，因为 `3` 可能比 `2` 慢，`s`/`z` 也不保证更小，并且新编译器版本可能改变结果（[S3]）。因此对 WASM 应把 `s`、`z`、`2`、`3` 作为候选实验，而不是未经测量的永久结论。
- **LTO**：`true`/`"fat"` 做跨依赖图的 whole-program 优化；`"thin"` 以较少链接时间取得接近 fat 的优化机会；`false` 是本地 crate 的 thin-local LTO（在 `codegen-units = 1` 或 `opt-level = 0` 时不做 LTO）；`"off"` 关闭 LTO。代价是更长链接时间（[S3]）。WASM 发布包是否变小、编译是否变慢、运行是否变快，必须对同一源码和同一工具链分别记录。
- **`codegen-units`**：更多 codegen unit 可增加编译并行度但可能生成更慢的代码；设为 `1` 可能改善生成代码性能但编译更慢（Rust [S4]）。它影响编译器优化边界和编译时间，不能单独当作“越小越好”的体积开关。
- **增量编译**：Rust 文档说明增量编译会抑制某些优化（例如增加 codegen units），不推荐用于 release（[S4]）。不要为了节省发布 `.wasm` 而推断开发 profile 的增量策略；开发迭代和发布测量是不同工作流。
- **debug／symbols**：Cargo `debug` 控制 debuginfo；`strip` 可为 `"none"`、`"debuginfo"` 或 `"symbols"`，默认 `"none"`（[S3]）。去掉调试信息和符号可能减少原始文件，但会损失回溯、调试或分析能力；应按是否需要生产调试分别生成和保存符号，而非把 debug 包当 release 包比较。
- **panic 策略**：Cargo 支持 `"unwind"` 和 `"abort"`，后者发生 panic 时终止进程；测试、bench、build script 和 proc-macro 忽略该 profile 设置，Rust 测试 harness 目前要求 unwind（[S3]）。`panic = "abort"` 可能减少与展开有关的代码，但也改变错误处理和调试语义；体积收益、错误 UX 和依赖兼容性要在实际 WASM 入口及错误路径中验证，不能只看预期。

### 2. wasm-bindgen：绑定输出、名称/DWARF 和 split 不是同一件事

wasm-bindgen CLI 的官方用法是对 Rust 生成的 `.wasm` 运行 `wasm-bindgen [options] input.wasm`，输出 JS、类型定义和处理后的 `.wasm`（[S5]）。关键限制如下：

- `--debug` 会生成更多 JS 和 Wasm 以帮助捕获程序错误，官方明确说该输出不用于生产发布（[S5]）。
- `--keep-debug` 保留 DWARF 自定义段；默认处理会剥离这些调试信息。wasm-bindgen 的调试信息页还说目前没有已知的普遍 Wasm DWARF 环境，并指向 Chrome 的 DWARF 扩展／指南（[S6]）。所以 `keep-debug` 是“可调试性与发布体积”的明确取舍，不应默认开启后再用传输压缩掩盖原始包膨胀。
- `--no-demangle`、名称段处理及 `--keep-lld-exports` 会改变调试、回溯、工具分析或外部链接可见性；名称段和 DWARF 的用途不同。是否能移除，取决于生产错误报告、性能分析、调试和后续工具的需求。
- `--split-linked-modules` 把 linked modules 分成独立文件；官方推荐它以便懒加载和更严格的 CSP，但 `new URL(..., import.meta.url)` 会让多数 bundler 不知道要把文件纳入输出，Webpack 5 是文档列出的例外；其他 bundler 需要插件或手工复制 `snippets/`，而 no-modules 场景还有 document/worker URL 限制（[S5]）。split 不是无条件减小总原始字节数，而是改变初始下载、缓存和请求图。
- `--target web` 适合无 bundler 的浏览器 ES module，但官方列出它不能使用 NPM dependencies，并要求检查浏览器支持／无 polyfill；bundler、web、no-modules 等 target 的加载和产物约束不同（[S7]）。

### 3. Binaryen `wasm-opt`：Wasm 专项优化，但每个级别仍需实测

Binaryen 官方 README 将 `wasm-opt` 定义为加载 WebAssembly 并运行 Binaryen IR passes 的工具；其目标包括 Wasm-specific 的代码尺寸和速度优化（[S8]）。官方 Optimizer Cookbook 给出以下限制和实验方向：

- `-Oz`、`-Os`、`-O3` 等优化管线不是“固定收益百分比”。Cookbook 说明某些管线可在优化后再次运行，`--converge` 会重复直到文件继续缩小，但额外循环收益通常有限且依程序而异（[S9]）。
- 需要闭世界／调用假设的选项（例如 `--mark-js-called`、`--gufa`、`-tnh`）可能依赖模块确实不会被外部调用，或会影响 crash reporting／运行时断言；不要把 Cookbook 的特定 flag 直接复制到未知入口。
- 内联会增加代码尺寸，也可能解锁更多优化；官方明确建议谨慎处理（[S9]）。这再次说明更小的 `.wasm` 不自动意味着更快。
- `--low-memory-unused` 只有在链接器确保低地址未使用时才安全；`--strip-toolchain-annotations` 应在最终工具链优化后使用，不能丢弃优化过程仍需要的元数据（[S9]）。

### 4. Dioxus CLI 的官方构建链：不要重复运行或猜测阶段

Dioxus CLI 的官方源码 `packages/cli/src/build/web.rs` 把 Web bundle 流程写成：运行 wasm-bindgen → 按请求进行 bundle splitting → 运行 wasm-opt → 注册 `.wasm`/`.js` 资产；release 或启用 split 时优化主 `.wasm`（[S10]）。同一源码还显示：dev／保留 debug／split／fat 模式会影响 `keep_debug` 和 `keep_names`，release 默认走优化；这只是该源码版本的实现事实，不是对本项目已经运行过的配置断言。

Dioxus CLI 的官方 Web 配置源码声明：

- `[web] pre_compress` 是“release web build 中预压缩 assets 和 wasm”的开关，默认 `false`；
- `[web.wasm_opt] level` 默认 `z`（aggressively for size），可选 `s`、`0`、`1`–`4`；
- `debug` 保留 Wasm debug symbols，`keep_names` 保留 name section，`memory_packing` 和额外 feature flags 另有语义（[S11]）。

因此对 Dioxus 应先确认 CLI 版本、`Dioxus.toml` 和实际 bundle 日志／文件，再决定是否需要手动运行 wasm-bindgen 或 wasm-opt。重复后处理可能改变调试段、名称段、feature 或 split 产物，且无法据官方默认值断言本项目当前行为。

Dioxus 官方 Web 源码的 split 分支会把 chunks 和 modules 写成多个 `.wasm`，生成 `makeLoad` glue，并为每个 chunk/module 注册资产；这支持按需加载，但增加请求、缓存键和部署路径约束（[S10]）。它应以“首屏下载和导航路径”的指标评价，不应只比较所有 chunk 的总和。

### 5. 原始 `.wasm`、压缩传输和解压成本

- WebAssembly 规范把 module 编码为 sections；custom sections 可承载调试信息或第三方扩展，且 Wasm 语义会忽略它们（WebAssembly Core Specification [S12]）。这为“去掉不参与执行的 debug/name/custom section 可能缩小 raw 文件”提供了规范依据，但是否能去掉仍取决于调试／工具链需求。
- MDN `Content-Encoding` 说明服务器应用的编码顺序、接收方如何解码回原始媒体格式，以及 `Content-Length` 在存在编码时表示 encoded form；列出的 HTTP 编码包括 `gzip`、`br` 和 `zstd`（[S1]）。因此 gzip、Brotli、Zstandard 是传输表示选择，不是改变 Wasm 语义的编译优化。
- web.dev 的官方文章指出 gzip 和 Brotli 常用于文本类资源，并把“消除不必要下载”“内容特定预处理／minification”“传输压缩”作为不同步骤；它也强调压缩算法在压缩比、压缩速度和内存需求之间有取舍（[S13]）。对二进制 Wasm 不应照搬文本 minifier 的收益叙述；只应测量最终 `.wasm` 的实际编码结果。
- Chrome 官方 Lighthouse 文档检查 `br`、`gzip`、`deflate` 的 `Content-Encoding`，并在 Network 面板显示压缩尺寸与解压尺寸（[S14]）。用同一 URL、同一缓存状态和同一浏览器记录两者，避免把 DevTools 的“Size”单列误认为 raw 文件大小。
- 解压是从 encoded representation 返回原始内容的必要步骤（[S1]）。解压 CPU／内存、网络 RTT、流式编译、缓存命中及浏览器实现会共同影响加载；本文不提供未经测量的固定成本或收益数字。

### 6. split/code splitting 的适用条件

可把 split 分为三层：

1. **wasm-bindgen linked-module split**：由 `--split-linked-modules` 产生外部文件；官方给出 lazy-load、CSP 和 bundler URL 限制（[S5]）。
2. **Dioxus Web bundle split**：Dioxus CLI 在 `wasm_split` 请求下拆 chunks/modules，生成 loader glue，并分别优化／注册它们（[S10]）。
3. **应用路由或资源级拆分**：是否有真实的晚访问路径、稳定缓存边界和可接受的额外请求，属于应用部署决策，不是 Rust 编译器默认保证。

适合 split 的条件是：首屏只需模块子集、部署系统能正确复制所有 chunk、CSP／MIME／路径／缓存策略已验证，且额外请求不会抵消首屏收益。限制是：总 raw bytes 可能不变甚至因 glue／重复边界增加；冷启动会有更多请求和依赖调度；共享 chunk 的缓存收益依访问顺序而变。需要比较首屏关键路径、全功能路径、缓存命中和失败回退，而不是只比较单一文件。

## 建议的可复现实验矩阵（不替代项目验证）

每次只改变一个变量，并锁定 Rust、LLVM、wasm-bindgen、Binaryen、Dioxus CLI、目标 triple、源码和依赖锁文件：

1. **Cargo 阶段**：同一输入比较 `opt-level = 2/3/"s"/"z"`；再分别比较 `lto = false/thin/true`、`codegen-units = 1/16`、`panic = "unwind"/"abort"`。记录编译时长、链接时长、raw `.wasm`、运行基准和错误／回溯行为。
2. **符号阶段**：分别记录 debug/name/DWARF 保留与剥离后的 raw 字节数；确认生产诊断、回溯和 Chrome／外部调试工作流是否仍可用。不要把 `wasm-bindgen --debug` 产物当 production baseline。
3. **后处理阶段**：对同一 wasm-bindgen 输出比较 `wasm-opt -Oz`、`-Os`、`-O3`（以及 Dioxus `WasmOptConfig` 的实际 level），记录 raw、验证结果、冷编译／实例化和运行基准。只有能证明入口为 closed-world 时，才实验需要该假设的高级 flag。
4. **传输阶段**：对每个最终 `.wasm` 和 JS glue 预生成或服务器动态生成 gzip、br、zstd（若部署链和目标客户端协商支持），记录编码级别、编码耗时、压缩体积、HTTP `Content-Encoding`/`Content-Length`、浏览器解压后的体积和下载时间。不要用一个编码的百分比推断另一个编码。
5. **加载阶段**：分别测量冷缓存和热缓存、低带宽／高 RTT 与本地网络、首屏 split 与全量路径；使用正确 MIME，比较 `instantiateStreaming`／实际 loader 的编译和实例化时间。记录目标设备 CPU、内存和浏览器版本。
6. **报告格式**：至少包含 `raw bytes`、`encoded bytes`、下载时间、解压／编译／实例化时间、首个可交互时间、稳态运行基准、产物 hash、工具版本和是否保留调试信息。没有这些条件时，只能说“建议实测”，不能写收益百分比。

## 本研究没有验证的事项

- 没有对 Yggdrasil 当前 `Cargo.toml`、`Dioxus.toml`、CLI 参数、产物或服务器响应作断言。
- 没有假定本项目已启用或禁用 LTO、`panic = "abort"`、`wasm-opt`、预压缩或 split；这些需由主线程按上述矩阵检查。
- 没有将任何官方示例中的体积数字、浏览器兼容性日期或优化收益迁移到本项目；具体工具版本和浏览器能力需在发布环境重新确认。

## 来源索引（均于 2026-08-13 访问）

- **[S1] MDN — `Content-Encoding`**：编码表示、解码回原始格式、`Content-Length` 指向 encoded form、gzip/br/zstd token。<https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Content-Encoding>
- **[S2] MDN — `WebAssembly.instantiateStreaming()`**：流式编译／实例化、`application/wasm` MIME 要求。<https://developer.mozilla.org/en-US/docs/WebAssembly/Reference/JavaScript_interface/instantiateStreaming_static>
- **[S3] The Cargo Book — Profiles**：profile 根 manifest、opt-level、debug/strip、LTO、panic、incremental、release 默认值。<https://doc.rust-lang.org/cargo/reference/profiles.html>
- **[S4] The rustc Book — Codegen Options**：codegen units、增量编译抑制优化、debuginfo 和 embed-bitcode 约束。<https://doc.rust-lang.org/rustc/codegen-options/index.html>
- **[S5] wasm-bindgen Guide — CLI**：debug、keep-debug、名称、linked-module split 及 bundler 限制。<https://wasm-bindgen.github.io/wasm-bindgen/reference/cli.html>
- **[S6] wasm-bindgen Guide — Debug Information**：DWARF 默认剥离、`--keep-debug`、当前调试环境限制。<https://wasm-bindgen.github.io/wasm-bindgen/reference/debug-info.html>
- **[S7] wasm-bindgen Guide — Deployment**：`web`、`bundler`、`no-modules` target 的加载和依赖限制。<https://wasm-bindgen.github.io/wasm-bindgen/reference/deployment.html>
- **[S8] Binaryen 官方 README**：Binaryen／`wasm-opt` 的工具定位及 Wasm-specific size/speed passes。<https://github.com/WebAssembly/binaryen/blob/main/README.md>
- **[S9] Binaryen 官方 Optimizer Cookbook**：`-Oz`、converge、内联、closed-world／trap 假设、工具链注释和高级 flag 限制。<https://github.com/WebAssembly/binaryen/wiki/Optimizer-Cookbook>
- **[S10] Dioxus 官方 CLI Web build source（main）**：web bundle 的 wasm-bindgen → split → wasm-opt 顺序、split 文件和 loader glue。<https://github.com/DioxusLabs/dioxus/blob/main/packages/cli/src/build/web.rs>
- **[S11] Dioxus 官方 CLI Web config source（main）**：`pre_compress`、WasmOpt level、debug/name/memory-packing 配置语义。<https://github.com/DioxusLabs/dioxus/blob/main/packages/cli/src/config/web.rs>
- **[S12] WebAssembly Core Specification — Binary Modules**：sections、custom sections 和 streaming/parallel compilation 的编码基础。<https://webassembly.github.io/spec/core/binary/modules.html>
- **[S13] web.dev — Optimize encoding and transfer size**：消除不必要下载、内容特定优化、gzip/Brotli 和压缩速度／内存取舍。<https://web.dev/articles/optimizing-content-efficiency-optimize-encoding-and-transfer>
- **[S14] Chrome for Developers — Enable text compression**：Lighthouse 的编码检查条件、Brotli/gzip fallback guidance、Network 面板压缩／解压尺寸。<https://developer.chrome.com/docs/lighthouse/performance/uses-text-compression>
