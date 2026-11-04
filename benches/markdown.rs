//! `render_markdown_enhanced` 微基准（criterion）。
//!
//! 语料 `benches/fixtures/*.md` 取自线上真实文章，同时供 `scripts/bench-http.ts` 作种子：
//!
//! | 文件 | 线上 slug | 特征 |
//! |---|---|---|
//! | `plain-prose.md` | `new-era-programming` | 纯散文基线 |
//! | `tables-prose.md` | `yggdrasil-leptos-performance-engineering` | 长文、18 张表、1 张 Mermaid、脚注、3 张图 |
//! | `math-heavy.md` | `pixel-bending-spacetime-singularity-rs` | 28 个块公式、约 346 个行内公式、28 个代码块 |
//! | `all-features.md` | `markdown-quan-te-xing-ce-shi-wen-zhang` | 24 张 Mermaid、KaTeX/mhchem、可运行代码、脚注、表格、3 张图 |
//!
//! 语料里的 `/uploads/` 全部改写成 `/uploads/bench-fixture/`，其余内容未动：
//! `post_process::wrap_images_with_blur` 会相对 cwd 读 `uploads/<rel>` 取图片尺寸，
//! 仓库根的 `uploads/` 含本地开发数据，不改写则不同机器走不同分支；
//! `bench-fixture/` 在任何机器上都不存在，每次都走确定的“打开失败”路径。
//!
//! A/B 对比：
//!
//! ```sh
//! make bench-markdown BENCH_ARGS='--save-baseline base'   # 在基准版本上
//! make bench-markdown BENCH_ARGS='--baseline base'        # 在修改版本上
//! ```

use std::time::Duration;

use criterion::{
    criterion_group, criterion_main, BenchmarkId, Criterion, SamplingMode, Throughput,
};
use yggdrasil::api::markdown::render_markdown_enhanced;

// 与生产保持一致（src/main.rs）：分配器会显著影响渲染中的小对象分配。
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

const FIXTURES: [(&str, &str); 4] = [
    ("plain-prose", include_str!("fixtures/plain-prose.md")),
    ("tables-prose", include_str!("fixtures/tables-prose.md")),
    ("math-heavy", include_str!("fixtures/math-heavy.md")),
    ("all-features", include_str!("fixtures/all-features.md")),
];

fn render(c: &mut Criterion) {
    // 先各渲染一次：syntect 语法集、KaTeX 线程局部上下文、merman 字体表、
    // 正则 LazyLock 等惰性初始化因此不计入测量。
    for (_, md) in FIXTURES {
        std::hint::black_box(render_markdown_enhanced(md));
    }

    let mut group = c.benchmark_group("render_markdown_enhanced");
    // all-features 单次约 50 ms，Linear 采样（1..n 次递增迭代）总耗时远超 10 s 预算，改用 Flat。
    group
        .sampling_mode(SamplingMode::Flat)
        .sample_size(30)
        .warm_up_time(Duration::from_secs(3))
        .measurement_time(Duration::from_secs(10))
        // 收紧显著性与噪声阈值，减少 A/B 假阳性。
        .significance_level(0.01)
        .noise_threshold(0.03);

    for (name, md) in FIXTURES {
        group.throughput(Throughput::Bytes(md.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(name), md, |b, md| {
            // 返回值交给 criterion 丢弃，析构成本计入测量，与生产路径一致。
            b.iter(|| render_markdown_enhanced(std::hint::black_box(md)))
        });
    }
    group.finish();
}

criterion_group!(benches, render);
criterion_main!(benches);
