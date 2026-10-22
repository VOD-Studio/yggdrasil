# 抽出 AdminPageHead、补全 FormInput 属性

## Context

审计 `docs/code-audit-2026-10-22.md:100-101` 记了两处重复：后台 14 个页面（审计列了 13 个，`logs.rs` 漏计）手写同一段 `div.admin-page-head` 结构，12 个骨架屏文件里另有 13 份同样的写法；`FormInput` 缺 `maxlength`、`required`、`readonly` 等属性，多行输入也没有对应组件，所以有 29 个 `input`/`textarea` 直接写 `class: "field…"`，没有走组件。

要达到的状态：
- 页面标题行只在 `AdminPageHead` 里实现一次，骨架屏标题行只在 `AdminPageHeadSkeleton` 里实现一次。
- `FormInput` 补齐属性，新增 `FormTextarea`。
- 上面 29 个控件全部改用组件。

以下 10 处保持原样：`field` 类写在外层 `label`/`div` 上的包裹式控件，包括搜索框、登录框、写作页的 slug/标签输入。

用户当前工作区有未提交的改动：`.env.example`、`CHANGELOG.md`、`src/api/rate_limit.rs`、`src/api/settings/rate_limit.rs`、`src/mcp/auth.rs`、`src/models/settings.rs`、`src/pages/admin/settings/ratelimit_section.rs`。这些文件不要碰，提交时也不要暂存。

## Approach

改动分为下面两个功能单元（A、B），各自独立提交，没有先后依赖。每个单元完成后用 `git add <显式路径>` 暂存，只提交本单元涉及的文件。

### A. 页面标题行组件（提交 `refactor(admin): 抽出 AdminPageHead 统一后台页面标题行`）

**A1. 新增 `AdminPageHead`**

位置：`src/components/admin_layout.rs`，紧挨在 `pub(crate) fn AdminShell` 的 `#[component]` 上方。该文件是布局层，只在 full 客户端编译。

可见性必须是 `pub(crate)`，与 `AdminShell` 一致。原因：图鉴测试 `catalog_covers_public_components_and_has_unique_routes`（`src/pages/components_showcase.rs:1760`）只扫描以 `pub fn ` 开头的行；`docs/components-showcase.md:18` 规定页面布局组件不进图鉴。

```rust
/// 后台页面标题行：左侧 h1 与副标题，右侧可选操作区。样式见 admin.css `.admin-page-head`。
///
/// - `title`：页面标题（手机宽度隐藏，顶栏已显示）
/// - `class`：附加类，如日志页的 `logs-head`
/// - `actions`：右侧内容，统一包进 `.admin-page-actions`
/// - `children`：副标题内容；为空时 `p` 由 CSS `min-height: 1lh` 保留一行高度
#[component]
pub(crate) fn AdminPageHead(
    title: String,
    #[props(default)] class: Option<&'static str>,
    #[props(default)] actions: Option<Element>,
    children: Element,
) -> Element {
    let extra = class.unwrap_or_default();
    rsx! {
        div { class: "admin-page-head {extra}",
            div {
                h1 { "{title}" }
                p { {children} }
            }
            if let Some(actions) = actions {
                div { class: "admin-page-actions", {actions} }
            }
        }
    }
}
```

**A2. 用 CSS 保留空副标题的行高**

修改 `styles/components/admin.css` 的 `.admin-page-head p` 规则（约 617-622 行），追加 `min-height: 1lh;`。

这样页面不再需要用 `"\u{00a0}"` 撑行高，审计里提到的"占位写法分叉"随之消失。

**A3. 替换 14 个页面的标题行**

每个页面都在文件顶部加：

```rust
use crate::components::admin_layout::AdminPageHead;
```

然后把整个 `div { class: "admin-page-head" … }` 块换成下面的写法。

通用规则：
- 原 `h1` 的文字放进 `title:`。
- 原 `p { … }` 里面的内容原样作为 `AdminPageHead` 的子节点。
- 原 `div { class: "admin-page-actions", … }` 里面的内容放进 `actions: rsx! { … }`，不再写这个外层 div。

| 文件（锚点） | 改法 |
|---|---|
| `dashboard.rs:209` | `title: "{greeting(chrono::Timelike::hour(&now))}，{name}"`；子节点 `"{summary}"`；actions 为原来的 3 个 `Link` |
| `posts.rs:110` | `title: "文章"`；子节点改为 `if let Some(summary) = summary { "{summary}" }`（删掉 `else { "\u{00a0}" }`）；actions 为 `RebuildMenu {}` 加 `Link` |
| `notes.rs:158` | 静态副标题；actions 为 2 个 `Link` |
| `comments.rs:184` | 子节点改为 `if resp.is_some() { "{all_count} 条 · 待审 {counts.pending}" }`（删掉 else 分支）；不传 actions |
| `assets.rs:250` | 和 posts 一样删掉 `"\u{00a0}"` 分支；actions 为 `Menu` 加上传按钮 |
| `friends.rs:139` | 静态副标题；actions 为 `a` |
| `posts_trash.rs:243` | 子节点为原来的三段文字加 `Link`，原样搬过来；`actions: (total() > 0).then(\|\| rsx! { button { …清空回收站… } })` |
| `profile.rs:77`、`system/mod.rs:165` | 静态副标题；不传 actions |
| `runner.rs:69` | 静态副标题；`actions: rsx! { StatusDot { label: "Docker 沙箱", tone: "success" } }`。原来 `StatusDot` 直接挂在标题行下面，现在统一包进 `.admin-page-actions`，这是有意的归一化 |
| `mcp.rs:126`、`notes/notebooks.rs:87` | 静态副标题；actions 各为 1 个按钮 |
| `settings/mod.rs:322` | 子节点为文字、`span.settings-restart-dot`、文字三段，原样搬过来；不传 actions |
| `logs.rs:77` | `class: "logs-head"`；子节点为原来两段文字，第二段带条件；actions 为 `span.logs-live` 加 2 个按钮 |

posts_trash 的回退方案：如果 `.then(...)` 传给 `Option<Element>` 编译不过，改成 `actions: rsx! { if total() > 0 { button {…} } }`。这样 `total() == 0` 时会多渲染一个空的 `.admin-page-actions`，视觉上没有差别，可以接受。

**A4. 让 `SkeletonBox` 接受拼接出来的类名**

修改 `src/components/skeletons/atoms.rs:17-21`，把参数从 `class: &'static str` 改成：

```rust
#[props(into)] class: Cow<'static, str>
```

同时加 `use std::borrow::Cow;`，函数体不变。

为什么用 `Cow` 而不是 `String`：现有约 270 处调用全部传字面量，`Cow` 不会给它们增加分配。`#[props(into)]` 必须加，否则字面量传不进去。如果仍然编译不过，退回 `class: String`。

**A5. 新增 `AdminPageHeadSkeleton`**

位置：`skeletons/atoms.rs`，`SkeletonBox` 之后，可见性为 `pub fn`。图鉴测试会跳过 `skeletons/` 目录，所以不用登记到图鉴。

```rust
/// 后台页面标题行骨架，结构与 `admin_layout::AdminPageHead` 一致。
/// `title_width` / `subtitle_width` 只传宽度类（如 `"w-20"`、`"w-96 max-w-full"`）。
#[component]
pub fn AdminPageHeadSkeleton(
    title_width: &'static str,
    subtitle_width: &'static str,
    #[props(default)] class: Option<&'static str>,
    #[props(default)] actions: Option<Element>,
) -> Element {
    let extra = class.unwrap_or_default();
    rsx! {
        div { class: "admin-page-head {extra}",
            div {
                SkeletonBox { class: format!("h-7 {title_width} rounded-md") }
                SkeletonBox { class: format!("h-3.5 {subtitle_width} rounded mt-2") }
            }
            if let Some(actions) = actions {
                div { class: "admin-page-actions", {actions} }
            }
        }
    }
}
```

**A6. 替换 13 份骨架屏标题行**

每个文件的 import 改成 `use crate::components::skeletons::atoms::{AdminPageHeadSkeleton, SkeletonBox};`。如果编译器报 `SkeletonBox` 未使用，就去掉它。

下表中"操作区"一列的每一项，都写成 `SkeletonBox { class: "h-[34px] <宽> rounded-lg" }`，放进 `actions: rsx! { … }`。原来没有用 `.admin-page-actions` 包、直接挂在标题行下的单个方块，也一律放进 `actions`。

| 文件（锚点） | 标题宽 | 副标题宽 | 操作区 |
|---|---|---|---|
| `admin_comments_skeleton.rs:34` | `w-20` | `w-28` | — |
| `assets_skeleton.rs:35` | `w-20` | `w-64` | `w-[34px]`、`w-20` |
| `dashboard_skeleton.rs:15` | `w-44` | `w-56` | — |
| `friends_admin_skeleton.rs:34` | `w-16` | `w-80` | `w-28` |
| `logs_skeleton.rs:15`（另加 `class: "logs-head"`） | `w-28` | `w-64` | `w-24`、`w-24` |
| `mcp_skeleton.rs:14` | `w-28` | `w-96 max-w-full` | `w-28` |
| `notes_admin_skeleton.rs:36`（`AdminNotesSkeleton`） | `w-20` | `w-64` | `w-24`、`w-24` |
| `notes_admin_skeleton.rs:89`（`AdminNotebooksSkeleton`） | `w-24` | `w-64` | `w-28` |
| `posts_skeleton.rs:52` | `w-20` | `w-44` | `w-[34px]`、`w-28` |
| `posts_trash_skeleton.rs:50` | `w-20` | `w-80 max-w-full` | `w-24` |
| `runner_skeleton.rs:14` | `w-28` | `w-72` | — |
| `settings_admin_skeleton.rs:22` | `w-16` | `w-80` | — |
| `system_skeleton.rs:14` | `w-16` | `w-80` | — |

**A 单元的完成标准：** 在 `src/` 下用 grep 搜索 `admin-page-head`，只剩 `admin_layout.rs` 和 `skeletons/atoms.rs` 两处命中。`docs/` 下的审计记录不算在内，也不要改它。

### B. 表单控件（提交 `feat(forms): FormInput 补全属性并新增 FormTextarea，迁移裸 field 输入`）

**B1. 改造 `FormInput`**

位置：`src/components/forms.rs:613-682`。新的参数表如下，未提到的参数保持原样：

```rust
pub fn FormInput(
    id: Option<String>,
    #[props(default = "text")] r#type: &'static str,      // 原为必填
    #[props(default)] placeholder: Option<&'static str>,   // 原为必填 &'static str
    value: String,
    #[props(default)] disabled: bool,
    #[props(default)] invalid: bool,
    #[props(default)] required: bool,
    #[props(default)] readonly: bool,
    #[props(default)] autofocus: bool,
    #[props(default)] maxlength: Option<i32>,
    #[props(default)] autocomplete: Option<&'static str>,
    #[props(default)] min: Option<&'static str>,
    #[props(default)] step: Option<&'static str>,
    #[props(default)] aria_describedby: Option<&'static str>,
    #[props(default)] oninput: Option<EventHandler<String>>, // 原为必填；只读框不需要
    // onkeydown / onfocus / onblur / class / mono / inputmode / title / aria_label 不变
)
```

两个类型选择的原因：
- `maxlength` 用 `i32`，是为了让 `maxlength: 100` 这样的整数字面量经 SuperInto 推断时落到 i32；用 `u32` 会推断失败。
- `placeholder` 改成 `Option`，是为了不填时不输出 `placeholder` 属性。

`rsx!` 里 `input` 元素的改动：
- 加上 `required, readonly, autofocus, maxlength, autocomplete, min, step, aria_describedby,`。Option 为 None、bool 为 false 时，Dioxus 不输出该属性。
- `placeholder: "{placeholder}"` 改成 `placeholder,`。
- `oninput` 改成 `oninput: move |e| { if let Some(handler) = &oninput { handler.call(e.value()); } },`。

同时重写函数上方的 Props 文档，列全所有参数，并删掉对不存在的 `INPUT_INLINE_CLASS` 的引用（第 625 行）。

**B2. 新增 `TEXTAREA_CLASS` 和 `FormTextarea`**

常量放在 `INPUT_CLASS`（第 10 行）之后：

```rust
/// 多行输入框基础类：随内容长高（`.field-textarea`）。
pub const TEXTAREA_CLASS: &str = "field field-textarea";
```

组件放在 `FormInput` 之后：

```rust
/// 多行输入框：外观同 FormInput，`.field-textarea` 随内容长高，
/// `rows` 是不支持 field-sizing 时的初始行数。
#[component]
pub fn FormTextarea(
    id: Option<String>,
    value: String,
    #[props(default)] placeholder: Option<&'static str>,
    #[props(default = 3)] rows: u32,
    #[props(default)] maxlength: Option<i32>,
    #[props(default)] invalid: bool,
    oninput: EventHandler<String>,
    #[props(default)] class: Option<&'static str>,
) -> Element {
    rsx! {
        textarea {
            id: id.unwrap_or_default(),
            class: class.unwrap_or(TEXTAREA_CLASS),
            rows,
            maxlength,
            placeholder,
            value: "{value}",
            aria_invalid: invalid.then_some("true"),
            oninput: move |e| oninput.call(e.value()),
        }
    }
}
```

**B3. 清理现有调用点的冗余参数**

- `src/components/ui/navigation.rs:106-107`：删掉 `r#type: "text",` 和 `placeholder: "",`。前者现在是默认值；后者如果保留，会输出一个空的 `placeholder=""`。
- 其余调用点不用改，`placeholder: "x"` 会自动包成 `Some`：`comments/form.rs:535-563`、`components_showcase.rs:470/860/1219/1229`。

**B4. 迁移 29 个控件**

在相应文件的 forms import 里补上 `FormInput` 和/或 `FormTextarea`（`friends.rs`、`notebooks.rs`、`runner.rs` 已经导入了 `FormLabel` 等，在原来那行扩展即可），然后按下面的规则逐个改写。

通用映射规则：
- `id: "x"` 原样保留。如果 `Option<String>` 不接受字面量，改写为 `id: Some("x".to_string())`。
- `class` 一律原样传 `class: "<原串>"`，因为 `FormInput` 默认的 `"field w-full"` 和这些串都不一样。textarea 的类恰好是 `"field field-textarea"` 时，省略 `class`。
- `r#type: "text"` 删掉。
- `"aria-invalid": X.then_some("true")` 改成 `invalid: X`。
- `rows: "N"` 改成 `rows: N`。
- `oninput: move |e| s.set(e.value())` 改成 `oninput: move |v: String| s.set(v)`。函数体有多条语句的，把里面的 `e.value()` 换成 `v`。
- `value`、`placeholder`、`maxlength`、`disabled`、`onkeydown`、`onblur`、`aria_label` 原样保留。

`FormInput` 的 25 处：
- `pages/admin/friends.rs`：437 friend-name（maxlength 100）、448 friend-url、467 friend-avatar（两条语句）、500 friend-order（inputmode、invalid）
- `profile.rs`：275 display（maxlength 50）、286 email、368 pw0（autocomplete）、380 pw1（autocomplete、aria_describedby）、400 pw2（autocomplete、invalid）
- `assets/references.rs:185`：asset-alt（maxlength、disabled、onblur、onkeydown）
- `mcp/configs.rs:166`：mcp-plaintext（`readonly: true`，不传 oninput）
- `mcp/token_editor.rs:252`：mcp-token-name（autofocus、invalid）
- `note_editor/metadata.rs:185`：note-tags（disabled、两条语句）
- `notes/notebooks.rs:341`：notebook-title（maxlength 100、`required: true`）
- `settings/runner_section.rs`：65 runner-cpu（`r#type: "number"`、`min: "0.1"`、`step: "0.1"`、`value: "{d.max_cpu_cores:.1}"`）、160 runner-langs
- `settings/security_section.rs:41`：sec-base-url
- `settings/site_section.rs`：35、46、59、72、85，共 5 个
- `system/export.rs:75`：export-table
- `write/cover.rs:198`：封面 URL 输入，没有 id，带 aria_label 和 onkeydown

`FormTextarea` 的 5 处：
- `friends.rs:487`：friend-desc，rows 3，maxlength 300
- `runner.rs:121`：runner-overrides，rows 4，`class: "field field-textarea settings-mono"`，invalid
- `notes/notebooks.rs:352`：notebook-description，rows 2，maxlength 1000
- `system/export.rs:89`：export-query，rows 4，带 settings-mono 类
- `write/metadata.rs:121`：write-summary，rows 5，`class: "field field-textarea write-summary"`

`runner.rs` 的占位文本需要特殊处理。在模块顶层新增：

```rust
const OVERRIDES_PLACEHOLDER: &str = "{\"timeout_secs\": 3}";
```

然后传 `placeholder: OVERRIDES_PLACEHOLDER`。原来的 `"{{…}}"` 是 rsx 的格式串转义，直接传给 `&'static str` 参数时行为不明确，用常量可以避开。

已知的轻微视觉差：`asset-alt` 和 `note-tags` 禁用时会多出 FormInput 自带的 `opacity-60 cursor-not-allowed`，不透明度从 0.55 变成 0.6，不需要处理。

`forms.rs:349` 是 `FormSelect` 内部的筛选框，带动态的 `aria_activedescendant`，不在本次迁移范围内。

**B5. 更新图鉴**（`catalog_covers_public_components_and_has_unique_routes` 会强制检查新组件是否登记）

`src/pages/components_showcase_data.json` 的 `FormInput` 条目（约 263-329 行）：
- `props` 里 `r#type` 的默认值改为 `"text"`；`placeholder` 的类型改为 `Option<&'static str>`、默认值改为 `"无"`；`oninput` 的类型改为 `Option<EventHandler<String>>`、默认值改为 `"无"`。
- 新增 4 行：`["maxlength","Option<i32>","无","最大字符数。"]`、`["required","bool","false","必填。"]`、`["readonly","bool","false","只读，可选中复制。"]`、`["autocomplete","Option<&'static str>","无","浏览器自动填充提示，如 new-password。"]`。

在 `FormInput` 条目后插入新条目：

```json
{"name":"FormTextarea","slug":"form-textarea","label":"多行输入框","group":"form","summary":"容得下一段话。","states":"空 · 有内容 · 校验失败","description":"外观同输入框的多行版本，随内容长高；rows 是不支持自动长高时的初始行数。","props":[["value","String","必填","当前内容。"],["oninput","EventHandler<String>","必填","输入时返回新内容。"],["rows","u32","3","初始行数。"],["placeholder","Option<&'static str>","无","空内容时的提示。"],["maxlength","Option<i32>","无","最大字符数。"],["invalid","bool","false","校验未通过：红环并输出 aria-invalid。"],["class","Option<&'static str>","field field-textarea","替换默认类。"]],"keys":[["Tab","移动焦点"],["Enter","换行"]],"source":"src/components/forms.rs","code":"let mut summary = use_signal(String::new);\nrsx! {\n    FormTextarea {\n        rows: 4,\n        placeholder: \"写下这篇文章想说的事…\",\n        value: summary(),\n        oninput: move |value| summary.set(value),\n    }\n}","preview_mode":"interactive","usage_kind":"rust"}
```

格式按文件现有风格（2 空格缩进，数组逐项换行）展开。

`src/pages/components_showcase.rs` 的改动：
- 第 1064 行：在 `"form-input"` 后加 `| "form-textarea"`。
- `LiveComponentPreview` 的 `input_value` 之后（约 1129 行）加 `let mut textarea_value = use_signal(String::new);`。
- `"FormInput"` 分支之后加：

```rust
"FormTextarea" => rsx! {
    crate::components::forms::FormTextarea {
        rows: 4,
        placeholder: "写下这篇文章想说的事…",
        value: textarea_value(),
        oninput: move |value| textarea_value.set(value),
    }
},
```

**B 单元的完成标准：** 在 `src` 下用 grep 工具跨行搜索下面这个正则，只命中 `src/components/forms.rs:349` 一处：

```
\b(input|textarea) \{\n(\s*(id|r#type): [^\n]*\n)*\s*class: "field
```

迁移前它命中 30 处，即 29 个待迁移控件加 forms.rs:349。

## Critical files & anchors

- `src/components/admin_layout.rs`：`AdminShell`（约 489 行），新组件放在它上方；用 `pub(crate)` 才能避开图鉴扫描。
- `src/components/skeletons/atoms.rs`：`SkeletonBox` 的 `class` 改为 `Cow`，新增 `AdminPageHeadSkeleton`。
- `src/components/forms.rs`：常量在第 10 行，`FormInput` 在 613-682 行。
- `src/pages/components_showcase.rs`：`preview_route` 在 1062 行，`LiveComponentPreview` 在 1108 行，目录测试在 1725-1857 行。
- `styles/components/admin.css`：`.admin-page-head p` 在约 617 行。

## Verification

工作目录：`/Users/issuser/Developer/xfy/yggdrasil`。

1. **编译和 lint。** 每个单元完成后都跑一遍：
   - `cargo clippy --locked --all-features -- -D warnings`
   - `cargo clippy --locked --target wasm32-unknown-unknown --no-default-features --features web -- -D warnings`
   - 上一条把 features 换成 `web,client-reader` 再跑一次，确认 reader 构建没有出现未使用警告。
   - `cargo fmt -- --check`：只要求本次改动的文件没有 diff。不要跑全量 `cargo fmt`，也不要跑 `dx fmt`。

2. **图鉴契约。** 运行 `cargo test --locked catalog_`，3 个目录测试必须通过。这一步证明 `FormTextarea` 已登记，而且 `AdminPageHead`（pub(crate)）和 `AdminPageHeadSkeleton`（位于 skeletons/）没有被误报为缺失。

3. **临时 SSR 冒烟测试，跑完删除。**

   在 `forms.rs` 末尾临时加 `#[cfg(test)] mod smoke_tmp`，参照 `src/components/ui/display.rs:163-180` 的写法，用 `dioxus::ssr::render_element` 渲染：

   ```rust
   FormInput { id: "a", value: String::new(), maxlength: 40, required: true, readonly: true, autocomplete: "off", min: "0.1", step: "0.1", aria_describedby: "h" }
   ```

   断言 HTML 包含 `maxlength="40"`、`required`、`readonly`、`autocomplete="off"`、`min="0.1"`、`aria-describedby="h"`，并且不包含 `placeholder=`。

   再渲染 `FormInput { value: String::new() }`，断言不包含 `required`、`readonly`、`maxlength`。

   再渲染 `FormTextarea { rows: 4, value: "x".to_string(), oninput: |_| {} }`，断言包含 `rows="4"` 和 `class="field field-textarea"`。

   在 `admin_layout.rs` 里同样临时加测试：
   - `AdminPageHead { title: "T", "S" }` 的输出包含 `<h1>T</h1>`，不包含 `admin-page-actions`。
   - 加上 `class: "logs-head"` 和 `actions: rsx!{ span { "A" } }` 后，输出包含 `admin-page-head logs-head` 和 `admin-page-actions`。

   用 `cargo test --locked smoke_tmp` 跑这些测试，通过后删掉临时模块。

4. **浏览器冒烟。**

   先运行 `make css`。然后用 `curl --noproxy '*' -s -o /dev/null -w '%{http_code}' http://localhost:8080/` 确认用户已经启动开发服务器；不要自己运行 `make dev`。拿到 200 后，用 `.env` 里的管理员账号登录 `http://localhost:8080`，域名必须用 localhost，不能换成 127.0.0.1。

   - **标题行：** 依次打开 `/admin`、`/admin/posts`、`/admin/notes`、`/admin/notebooks`、`/admin/posts/trash`、`/admin/assets`、`/admin/friends`、`/admin/comments`、`/admin/system`、`/admin/runner`、`/admin/mcp`、`/admin/logs`、`/admin/settings`、`/admin/profile`。每页检查：
     - `document.querySelectorAll('.admin-page-head').length === 1`，且 `h1` 文字正确。
     - `/admin/runner` 页的 `.admin-page-actions` 里有 StatusDot。
     - `/admin/logs` 页的标题行带 `logs-head` 类。
     - 在 `/admin/posts` 截图，确认标题行和操作区的位置与改动前一致。
   - **空副标题：** 在 DevTools 里把 `/admin/posts` 标题行的 `p` 清空，读 `getBoundingClientRect().height`，结果应大于 0，并且等于有文字时的高度。
   - **表单属性：**
     - `/admin/friends` 打开新建表单：`#friend-name` 的 `maxLength` 为 100，`#friend-desc` 是 TEXTAREA 且 `rows` 为 3。
     - `/admin/notebooks` 新建：`#notebook-title` 的 `required` 为 true。
     - `/admin/settings`：在 `#site-home-title` 里输入字符后，保存栏应变成未保存状态，证明 oninput 已接通；它的 `maxLength` 为 40。`#runner-cpu` 的 `type`、`min`、`step` 分别为 number、0.1、0.1。
     - `/admin/profile`：`#profile-pw1` 的 `autocomplete` 为 new-password，`aria-describedby` 为 profile-pw1-hint。
     - 在 `/admin/mcp` 显示一个令牌：`#mcp-plaintext` 的 `readOnly` 为 true。
   - **图鉴：** 打开 `/about/components/form-textarea`，输入多行文字，确认能随内容长高。
   - 验证完关闭浏览器标签页。

   如果 8080 上没有服务，在汇报里写明缺少浏览器验证，只以第 1-3 步为证据。

## Assumptions & contingencies

- `"\u{00a0}"` 占位改用 CSS `min-height: 1lh`，依赖浏览器支持 `lh` 单位（Chrome 109、Safari 16.4、Firefox 120 起）。后台只面向作者本人，这个前提可以接受。
- 如果 `Option<String>`、`Option<i32>` 这类参数在调用点编译不过，按 B4 里写的写法改为显式的 `Some(...)` 或带类型后缀的字面量，不要改组件签名。
- 如果省略类型标注的 `oninput` 闭包推断失败，在调用点补上 `: String`。
