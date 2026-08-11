//! 组件模块 —— 按「可复用层级」组织，从通用到特定。
//!
//! 【导航】写 `rsx!` 前先查这里，避免手搓重复的 Tailwind class：
//! - 要 class 常量或通用原子（按钮 / 输入 / 徽章 / 空状态）？ → **原子层**
//! - 要页面外壳或全局导航？                              → **布局层**
//! - 要特定页面域（文章 / 评论 / 代码运行）的组件？       → **feature 层**
//! - 数据加载占位？                                      → **骨架屏层**
//!
//! 层级越靠上越通用、对 app 其余部分依赖越少；feature 层组件绑定各自的
//! `models` / `Route` / `api`，不应跨页面域复用。同一类样式或结构只允许
//! 存在一个实现 —— 新增前先在本模块对应层查找，缺功能就扩展现有组件，
//! 不要在旁边另起一套（散落的内联 class 是被禁止的反模式）。

// ========= 原子层 (atoms) — 跨页面复用，无 app 领域依赖 =========
// class 常量与最基础的展示 / 输入原子。写新页面前先扫这一层。

/// 通用 UI 原子与全部 class 常量（卡片 / 按钮 / 徽章 / 分页 / spinner / checkbox）。
/// 任何 inline Tailwind class 字符串上线前，先在此查是否已有常量。
pub mod ui;
/// 表单控件（FormInput / FormSelect / AlertBox / …）与 INPUT class 常量。
pub mod forms;
/// 空状态组件（无数据时的插画配图 + 文案）。
pub mod empty_state;

// ========= 骨架屏层 (skeletons) — 纯展示占位 =========
// 内部仅依赖原子层的 SkeletonBox + ADMIN class；不含业务逻辑。

/// 各页面数据加载期间的占位组件集合，以及通用骨架原子。
pub mod skeletons;

// ========= 布局层 (layouts) — 页面外壳与全局导航 =========

/// 前台布局组件（header + footer + 路由骨架屏切换）。
pub mod frontend_layout;
/// 后台布局组件（侧栏 + 内容区 + admin 路由骨架屏切换）。
pub mod admin_layout;
/// 顶部导航栏组件。
pub mod header;
/// 导航项生成组件。
pub mod nav;
/// 页脚与回到顶部按钮组件。
pub mod footer;

// ========= feature 层 — 绑定特定领域 (models / Route / api) =========
// 按页面域划分；跨域复用前先考虑原子层。

/// 文章详情页组件（标题 / 正文 / 目录 / 面包屑 / 相邻导航）。
pub mod post;
/// 文章列表卡片组件。
pub mod post_card;
/// 评论组件（列表 / 表单 / 待审核项）。
pub mod comments;
/// 代码运行器组件（可运行代码块 UI）。
pub mod code_runner;
/// SQL 查询结果表格组件（SQL 控制台专用）。
pub mod sql_result_table;

// ========= 页面级骨架 =========

/// 后台仪表盘内容区骨架屏组件。
pub mod admin_skeleton;
/// 编辑器页面骨架屏组件。
pub mod write_skeleton;
