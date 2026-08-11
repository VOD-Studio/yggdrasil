#[cfg(all(test, feature = "server"))]
mod seed_tests {
    use crate::api::markdown::render_markdown_enhanced;
    use crate::api::posts::helpers::{render_post_fields, sync_tags};
    use crate::api::slug::ensure_unique_slug;
    use crate::cache::{
        invalidate_all_comments, invalidate_all_post_caches, invalidate_friend_links,
    };
    use crate::db::pool::get_conn;
    use crate::ssr_cache::invalidate_ssr_all_public;

    #[tokio::test]
    async fn seed_mock_data() {
        let _ = dotenvy::dotenv();

        let mut client = match get_conn().await {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Failed to connect to database: {e}");
                return;
            }
        };

        // Ensure user 1 exists
        let user_row = client
            .query_opt("SELECT id FROM users WHERE id = 1", &[])
            .await
            .unwrap();
        if user_row.is_none() {
            eprintln!("User 1 does not exist, skipping seed");
            return;
        }

        // Clean up existing posts, tags, post_tags, comments
        client
            .execute(
                "TRUNCATE posts, tags, post_tags, comments RESTART IDENTITY CASCADE;",
                &[],
            )
            .await
            .ok();

        // 1. Post 1: Yggdrasil Features Demo (Published)
        let title1 = "Yggdrasil 核心特性全览：从 Docker 代码沙箱到 KaTeX 数学公式";
        let slug1 = "yggdrasil-features-demo";
        let tags1 = vec![
            "Yggdrasil".to_string(),
            "Rust".to_string(),
            "Docker".to_string(),
            "KaTeX".to_string(),
            "Mermaid".to_string(),
            "Tutorial".to_string(),
        ];
        let content1 = r#"Yggdrasil 是基于 **Dioxus 0.7** 与 **Axum** 构建的全栈 Rust 博客与内容管理系统（CMS）。本文集中展示本博客系统支持的所有核心 Markdown 增强渲染特性与交互组件。

[TOC]

## 1. 可执行 Docker 代码沙箱 (Runnable Code Blocks)

Yggdrasil 支持在文章中嵌入可交互执行的代码块。代码在底层的 Docker 隔离沙箱容器中运行，支持资源限制、超时控制及 SSE 实时输出流。

### 1.1 Python 沙箱演示

使用 ```python runnable``` 语法声明可执行 Python 代码块：

```python runnable
import sys
import math

print(f"Python 解释器版本: {sys.version}")

def calculate_pi_approximation(n):
    # Leibniz 级数近似求 Pi
    pi_fourth = sum((-1)**k / (2*k + 1) for k in range(n))
    return pi_fourth * 4

approx = calculate_pi_approximation(100000)
print(f"近似 π 值 (100k 项): {approx:.8f}")
print(f"math.pi 真实值:      {math.pi:.8f}")
print(f"相对误差:            {abs(approx - math.pi)/math.pi:.2e}")
```

### 1.2 Rust 沙箱演示（带资源配置参数）

你可以通过 JSON 元数据指定沙箱执行选项，例如限制内存为 256MB、超时时间为 10 秒：

```rust runnable {"timeout_secs":10,"memory_mb":256}
fn main() {
    println!("Hello from Yggdrasil Rust Execution Sandbox!");
    
    let numbers: Vec<u64> = (1..=10).map(|x| x * x).collect();
    println!("平方数序列: {:?}", numbers);
    
    let sum: u64 = numbers.iter().sum();
    println!("前 10 个平方数之和: {}", sum);
}
```

### 1.3 Node.js / Bun JavaScript 运行时

```node runnable
const fs = require('fs');

console.log("当前 Node.js 运行平台:", process.platform, process.arch);
console.log("内存使用率:", process.memoryUsage());

const data = { name: "Yggdrasil", type: "Fullstack Rust Blog", rating: 5.0 };
console.log("JSON 输出演示:", JSON.stringify(data, null, 2));
```

---

## 2. LaTeX 数学公式与化学方程式

基于 `katex-rs` 与 `mhchem` 引擎，Yggdrasil 支持高性能排版复杂数学公式与化学反应式。

### 2.1 行内与块级数学公式

- **行内公式**：质能方程 $E = mc^2$，欧拉恒等式 $e^{i\pi} + 1 = 0$。
- **高斯积分**：
  $$\int_{-\infty}^{\infty} e^{-x^2} dx = \sqrt{\pi}$$

- **二阶矩阵求逆公式**：
  $$\begin{pmatrix} a & b \\ c & d \end{pmatrix}^{-1} = \frac{1}{ad-bc} \begin{pmatrix} d & -b \\ -c & a \end{pmatrix}$$

- **极限与级数展开**：
  $$\lim_{n \to \infty} \left(1 + \frac{1}{n}\right)^n = e = \sum_{k=0}^{\infty} \frac{1}{k!}$$

### 2.2 mhchem 化学反应方程式

- **水的生成与分解**：
  $$\ce{2H2 + O2 -> 2H2O}$$

- **光合作用总反应式**：
  $$\ce{6CO2 + 6H2O ->[光照][叶绿素] C6H12O6 + 6O2}$$

- **沉淀与气体生成**：
  $\ce{Fe^3+ + 3OH- -> Fe(OH)3 v}$，$\ce{2HCl + CaCO3 -> CaCl2 + H2O + CO2 ^}$

---

## 3. Mermaid 架构图表与交互放大

使用 ```mermaid``` 语法即可在线渲染矢状图、序列图和状态机。点击图表即可开启全屏平移缩放（Pan / Zoom）沉浸式浮层。

### 3.1 Yggdrasil 请求处理架构图

```mermaid
graph TD
    Client[客户端 Browser / WASM] -->|HTTP POST /api/CreatePost| Axum[Axum Web Server]
    Axum --> CSRF{Origin / CSRF 校验}
    CSRF -->|通过| Auth[Session / Argon2 鉴权]
    Auth -->|管理员权限| Render[spawn_blocking Markdown 渲染引擎]
    Render --> Syntect[Syntect 语法高亮]
    Render --> KaTeX[KaTeX / mhchem 数学渲染]
    Render --> TOC[TOC 目录生成]
    Render --> Txn[PostgreSQL 数据库事务]
    Txn --> DB[(PostgreSQL 数据库)]
    Txn --> Moka[Moka 缓存失效]
    Txn --> SSRCache[SSR 文件缓存清理]
    SSRCache --> Resp[返回 200 OK 响应]
```

### 3.2 身份认证与 Session 校验序列表

```mermaid
sequenceDiagram
    autonumber
    actor User as 用户浏览器
    participant Middleware as Axum admin_guard
    participant Cache as Moka Session Cache
    participant DB as PostgreSQL DB
    
    User->>Middleware: 请求 /admin/posts (Cookie: session=xyz)
    Middleware->>Cache: 查询 Session 缓存 (key: xyz)
    alt Cache Hit
        Cache-->>Middleware: 返回 UserSession 结构体
    else Cache Miss
        Middleware->>DB: SELECT * FROM sessions WHERE token_hash = sha256(xyz)
        DB-->>Middleware: 返回 session 行记录
        Middleware->>Cache: 写入 Moka Cache (TTL 300s)
    end
    Middleware->>Middleware: 校验 user.role == 'admin' && session.generation == user.generation
    Middleware-->>User: 放行 / 允许访问管理后台
```

---

## 4. 多语言代码高亮 (Syntect)

针对静态展示的代码段，Yggdrasil 提供由 Catppuccin 主题加持的高清代码高亮：

### 4.1 Go 语言并发处理

```go
package main

import (
	"fmt"
	"sync"
)

func worker(id int, wg *sync.WaitGroup, ch chan<- string) {
	defer wg.Done()
	ch <- fmt.Sprintf("Worker %d 任务完成", id)
}

func main() {
	var wg sync.WaitGroup
	ch := make(chan string, 3)

	for i := 1; i <= 3; i++ {
		wg.Add(1)
		go worker(i, &wg, ch)
	}

	wg.Wait()
	close(ch)

	for msg := range ch {
		fmt.Println(msg)
	}
}
```

### 4.2 SQL 复杂查询与窗口函数

```sql
SELECT 
    p.id,
    p.title,
    p.slug,
    COUNT(c.id) AS comment_count,
    DENSE_RANK() OVER (ORDER BY COUNT(c.id) DESC) AS rank_by_comments
FROM posts p
LEFT JOIN comments c ON c.post_id = p.id AND c.status = 'approved'
WHERE p.status = 'published' AND p.deleted_at IS NULL
GROUP BY p.id, p.title, p.slug
ORDER BY rank_by_comments ASC
LIMIT 10;
```

---

## 5. 元素组件、表格与 GFM 扩展

### 5.1 响应式数据表格

| 功能特性 | 技术实现方案 | 状态 | 备注 |
| :--- | :--- | :---: | :--- |
| **前端框架** | Dioxus 0.7 (WASM) | $\text{Ready}$ | 单页响应式 UI |
| **后端引擎** | Axum + Tokio | $\text{Active}$ | 高并发与异步 IO |
| **数据库** | PostgreSQL + deadpool | $\text{Connected}$ | 带连接池与自动化迁移 |
| **代码执行** | Bollard + Docker API | $\text{Isolated}$ | 内存限制与 UID 隔离 |
| **公式渲染** | katex-rs + mhchem | $\text{Rendered}$ | 服务端预渲染 HTML |

### 5.2 GFM 任务列表与脚注引用

- [x] 搭建 Dioxus 0.7 响应式路由架构[^dioxus]
- [x] 实现 Docker Bollard 容器安全沙箱[^docker]
- [x] 集成 KaTeX 服务端公式渲染与 HTML 清洗
- [ ] 支持更多编程语言沙箱环境（Ruby, C++）

[^dioxus]: Dioxus 是基于 Rust 的跨平台 UI 框架，0.7 版本对 Signal 与 Component 渲染纯洁性做出了大幅优化。
[^docker]: Docker 容器在后台以后台 API 模式被调用，确保主进程内存与文件系统安全性。
"#;

        let fields1 = render_post_fields(
            content1,
            "published",
            Some("https://images.unsplash.com/photo-1518770660439-4636190af475?auto=format&fit=crop&w=1200&q=80"),
        )
        .await
        .unwrap();

        let tx = client.transaction().await.unwrap();
        let final_slug1 = ensure_unique_slug(&tx, slug1, None).await.unwrap();
        let pub_time1 = chrono::Utc::now() - chrono::Duration::hours(12);

        let row1 = tx
            .query_one(
                "INSERT INTO posts (author_id, title, slug, summary, content_md, content_html, toc_html, status, published_at, cover_image, word_count, reading_time)
                 VALUES (1, $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) RETURNING id",
                &[
                    &title1,
                    &final_slug1,
                    &fields1.auto_summary,
                    &content1,
                    &fields1.content_html,
                    &fields1.toc_html,
                    &fields1.status.as_str(),
                    &pub_time1,
                    &fields1.cover_image,
                    &fields1.word_count,
                    &fields1.reading_time,
                ],
            )
            .await
            .unwrap();
        let post_id1: i32 = row1.get("id");
        sync_tags(&tx, post_id1, &tags1).await.unwrap();
        tx.commit().await.unwrap();

        // 2. Post 2: Dioxus Fullstack Architecture (Published)
        let title2 = "基于 Dioxus 0.7 与 Axum 的全栈 Rust 架构设计实战";
        let slug2 = "dioxus-fullstack-architecture";
        let tags2 = vec![
            "Rust".to_string(),
            "Dioxus".to_string(),
            "Axum".to_string(),
            "Architecture".to_string(),
        ];
        let content2 = r#"在构建现代化 Web 应用时，Rust 语言凭其卓越的性能与内存安全保障，逐渐在全栈开发领域崭露头角。本文深度剖析 Yggdrasil 博客系统的底层架构。

[TOC]

## 1. 架构总览：单 Crate 双目标构建

Yggdrasil 采用了单 Cargo Crate 同时支持 WASM 前端与 Native 服务端编译的模式。

```rust
// 通过 Cargo Feature 控制编译分支
#[cfg(feature = "server")]
pub async fn serve_backend() {
    // Axum 路由组装与 PostgreSQL 连接池初始化
}

#[cfg(not(feature = "server"))]
pub fn main() {
    // WASM 前端单页应用启动
    dioxus::launch(App);
}
```

### 1.1 条件编译与 Feature 隔离

通过 `default = ["web", "server"]` 约束，在生产环境编译为服务器可执行文件；而在单纯打包客户端时，只需使用 `--no-default-features --features web --target wasm32-unknown-unknown`。

## 2. Server Function 数据流与缓存控制

Dioxus 的 Server Function 特性屏蔽了传统的 REST API 机械代码。

```rust runnable
// 演示服务端函数签名抽象逻辑
fn mock_server_fn_dispatch(api_name: &str, payload: &str) -> String {
    format!("RPC Dispatch -> API: {}, Payload Length: {} bytes", api_name, payload.len())
}

let res = mock_server_fn_dispatch("/api/CreatePost", "{\"title\": \"Test\"}");
println!("{}", res);
```

### 2.1 Moka 高性能二级内存缓存

系统引入了 moka 缓存库对文章详情、标签列表及 Session 进行多层级 LRU 缓存管理，缓存命中率可达 98% 以上。

$$\text{Cache Hit Ratio} = \frac{N_{\text{hits}}}{N_{\text{hits}} + N_{\text{misses}}} \times 100\%$$

## 3. 总结与展望

全栈 Rust 架构极大提升了前端与服务端的代码复用率，避免了 DTO 的二次声明与 JSON 序列化不一致的痛点。
"#;
        let fields2 = render_post_fields(
            content2,
            "published",
            Some("https://images.unsplash.com/photo-1555066931-4365d14bab8c?auto=format&fit=crop&w=1200&q=80"),
        )
        .await
        .unwrap();
        let tx = client.transaction().await.unwrap();
        let final_slug2 = ensure_unique_slug(&tx, slug2, None).await.unwrap();
        let pub_time2 = chrono::Utc::now() - chrono::Duration::hours(6);

        let row2 = tx
            .query_one(
                "INSERT INTO posts (author_id, title, slug, summary, content_md, content_html, toc_html, status, published_at, cover_image, word_count, reading_time)
                 VALUES (1, $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) RETURNING id",
                &[
                    &title2,
                    &final_slug2,
                    &fields2.auto_summary,
                    &content2,
                    &fields2.content_html,
                    &fields2.toc_html,
                    &fields2.status.as_str(),
                    &pub_time2,
                    &fields2.cover_image,
                    &fields2.word_count,
                    &fields2.reading_time,
                ],
            )
            .await
            .unwrap();
        let post_id2: i32 = row2.get("id");
        sync_tags(&tx, post_id2, &tags2).await.unwrap();
        tx.commit().await.unwrap();

        // 3. Post 3: Container Isolation (Published)
        let title3 = "现代前端与 Docker 容器隔离技术演进";
        let slug3 = "container-isolation-and-sandbox";
        let tags3 = vec![
            "Docker".to_string(),
            "Security".to_string(),
            "Linux".to_string(),
        ];
        let content3 = r#"在线代码沙箱是现代开发者平台不可或缺的功能。本文讨论如何利用 Linux Namespace、cgroups 以及 Docker API 构建轻量且安全的资源隔离沙箱。

[TOC]

## 1. 隔离维度对比

| 隔离技术 | 开销 (Overhead) | 启动延迟 | 隔离等级 | 适用场景 |
| :--- | :--- | :--- | :--- | :--- |
| **全虚拟化 (VM)** | 较高 (~G) | 秒级 (~10s) | 最高 | 绝对多租户安全隔离 |
| **容器隔离 (Docker)** | 极低 (~M) | 毫秒级 (~100ms) | 高 (Kernel unshare) | 短生存期代码运行沙箱 |
| **WASM 沙箱** | 微秒级 | 微秒级 | 中 | 边缘计算与轻量函数 |

## 2. 容器沙箱安全配置

在 Yggdrasil 中，所有提交的代码运行于独立的受限容器中：

```python runnable
import os, resource

# 获取当前进程软硬资源限制
mem_limit = resource.getrlimit(resource.RLIMIT_AS)
print(f"内存虚拟地址限制: {mem_limit[0] / (1024*1024):.1f} MB")
print(f"当前 PID / UID: {os.getpid()} / {os.getuid()}")
```

通过配置 `ReadonlyRootfs=true`, `AutoRemove=true` 以及 `NetworkDisabled=true`，能最大程度防范恶意逃逸行为。
"#;
        let fields3 = render_post_fields(
            content3,
            "published",
            Some("https://images.unsplash.com/photo-1605745341112-85968b19335b?auto=format&fit=crop&w=1200&q=80"),
        )
        .await
        .unwrap();
        let tx = client.transaction().await.unwrap();
        let final_slug3 = ensure_unique_slug(&tx, slug3, None).await.unwrap();
        let pub_time3 = chrono::Utc::now() - chrono::Duration::hours(2);

        let row3 = tx
            .query_one(
                "INSERT INTO posts (author_id, title, slug, summary, content_md, content_html, toc_html, status, published_at, cover_image, word_count, reading_time)
                 VALUES (1, $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) RETURNING id",
                &[
                    &title3,
                    &final_slug3,
                    &fields3.auto_summary,
                    &content3,
                    &fields3.content_html,
                    &fields3.toc_html,
                    &fields3.status.as_str(),
                    &pub_time3,
                    &fields3.cover_image,
                    &fields3.word_count,
                    &fields3.reading_time,
                ],
            )
            .await
            .unwrap();
        let post_id3: i32 = row3.get("id");
        sync_tags(&tx, post_id3, &tags3).await.unwrap();
        tx.commit().await.unwrap();

        // 4. Post 4: Draft Roadmap
        let title4 = "[草稿] Yggdrasil 2.0 路线图与 MCP 协议集成计划";
        let slug4 = "yggdrasil-roadmap-draft";
        let tags4 = vec![
            "Roadmap".to_string(),
            "MCP".to_string(),
            "Draft".to_string(),
        ];
        let content4 = r#"# Yggdrasil 2.0 Draft Roadmap

本文为草稿文档，规划了 Yggdrasil 下一代版本的主要研发目标：

- [ ] 完全集成 MCP (Model Context Protocol) 3.0 工具链
- [ ] 增加更多 Docker Code Runner 的预置语言镜像（如 C++, Ruby, PHP）
- [ ] 支持基于 Vector Store 的智能全文搜索与 AI 问答
- [ ] 增强多用户协作与更精细的 RBAC 权限管理

> 注：本草稿尚未公开发布，仅在管理员面板可见。
"#;
        let fields4 = render_post_fields(content4, "draft", None).await.unwrap();
        let tx = client.transaction().await.unwrap();
        let final_slug4 = ensure_unique_slug(&tx, slug4, None).await.unwrap();

        let row4 = tx
            .query_one(
                "INSERT INTO posts (author_id, title, slug, summary, content_md, content_html, toc_html, status, published_at, cover_image, word_count, reading_time)
                 VALUES (1, $1, $2, $3, $4, $5, $6, $7, NULL, $8, $9, $10) RETURNING id",
                &[
                    &title4,
                    &final_slug4,
                    &fields4.auto_summary,
                    &content4,
                    &fields4.content_html,
                    &fields4.toc_html,
                    &fields4.status.as_str(),
                    &fields4.cover_image,
                    &fields4.word_count,
                    &fields4.reading_time,
                ],
            )
            .await
            .unwrap();
        let post_id4: i32 = row4.get("id");
        sync_tags(&tx, post_id4, &tags4).await.unwrap();
        tx.commit().await.unwrap();

        // 5. Seed Approved Comments for Post 1
        let cmd1 = "这个在线运行 Rust 和 Python 代码的功能太酷了！试了一下数学公式渲染速度极快，Markdown 的表格与目录体验也非常棒！";
        let chtml1 = render_markdown_enhanced(cmd1).html;
        let row_c1 = client
            .query_one(
                "INSERT INTO comments (post_id, parent_id, depth, author_name, author_email, author_url, content_md, content_html, status, approved_at)
                 VALUES ($1, NULL, 0, '极客小明', 'xiaoming@example.com', 'https://github.com/xiaoming', $2, $3, 'approved', NOW()) RETURNING id",
                &[&post_id1, &cmd1, &chtml1],
            )
            .await
            .unwrap();
        let parent_comment_id: i64 = row_c1.get("id");

        let cmd2 = "感谢支持！代码沙箱底层基于 Docker 容器，公式渲染是服务端预先生成的，因此前端加载非常顺畅。";
        let chtml2 = render_markdown_enhanced(cmd2).html;
        client
            .execute(
                "INSERT INTO comments (post_id, parent_id, depth, author_name, author_email, author_url, content_md, content_html, status, approved_at)
                 VALUES ($1, $2, 1, 'xfy', 'i@rua.plus', 'https://rua.plus', $3, $4, 'approved', NOW())",
                &[&post_id1, &parent_comment_id, &cmd2, &chtml2],
            )
            .await
            .unwrap();

        // 6. Seed Friend Links
        client
            .execute(
                "DELETE FROM friend_links WHERE name IN ('Dioxus 官方网站', 'Rust 语言中文社区', 'Axum Web 框架');",
                &[],
            )
            .await
            .ok();
        client
            .execute(
                "INSERT INTO friend_links (name, url, avatar_url, description, sort_order, is_active) VALUES
                 ('Dioxus 官方网站', 'https://dioxuslabs.com', 'https://dioxuslabs.com/favicon.ico', 'Rust 跨平台 UI 框架官方主页', 1, true),
                 ('Rust 语言中文社区', 'https://rustcc.cn', 'https://rustcc.cn/favicon.ico', 'Rust 语言中文开发者交流平台', 2, true),
                 ('Axum Web 框架', 'https://github.com/tokio-rs/axum', 'https://github.com/tokio-rs.png', 'Tokio 团队打造的高性能 Axum 框架', 3, true)",
                &[],
            )
            .await
            .unwrap();

        // 7. Clear Caches
        invalidate_all_post_caches();
        invalidate_all_comments();
        invalidate_friend_links();
        invalidate_ssr_all_public();

        println!("Successfully seeded 4 mock posts, approved comments, and friend links!");
    }
}
