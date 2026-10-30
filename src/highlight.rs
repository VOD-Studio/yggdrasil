//! 语法高亮模块。
//!
//! 仅在 `server` feature 启用时可用，使用 `syntect` 将代码块转换为带 CSS class 的 HTML，
//! 配合 `public/highlight.css` 中生成的主题规则实现亮/暗主题高亮。

#[cfg(feature = "server")]
pub mod server {
    use std::sync::LazyLock;

    use syntect::html::{ClassStyle, ClassedHTMLGenerator};
    use syntect::parsing::{SyntaxDefinition, SyntaxSet};
    use syntect::util::LinesWithEndings;

    /// 编译期内嵌的自定义语法定义（文件名 stem → .sublime-syntax 内容）。
    ///
    /// 生产镜像是 `FROM scratch` 的静态 musl 二进制，容器内不存在 `syntaxes/`
    /// 目录；而 `CARGO_MANIFEST_DIR` 烘焙的是构建机路径（Docker 里是 /build），
    /// 运行时 `add_from_folder` 注定失败、这些语言静默回退为纯文本。
    /// 因此改为 `include_str!` 编译期嵌入，彻底消除运行时文件依赖。
    /// 列表与 `syntaxes/` 目录的一致性由测试 `custom_syntax_list_matches_directory` 保证。
    pub(crate) const CUSTOM_SYNTAXES: &[(&str, &str)] = &[
        ("JSX", include_str!("../syntaxes/JSX.sublime-syntax")),
        ("Kotlin", include_str!("../syntaxes/Kotlin.sublime-syntax")),
        ("Swift", include_str!("../syntaxes/Swift.sublime-syntax")),
        ("TOML", include_str!("../syntaxes/TOML.sublime-syntax")),
        ("TSX", include_str!("../syntaxes/TSX.sublime-syntax")),
        (
            "TypeScript",
            include_str!("../syntaxes/TypeScript.sublime-syntax"),
        ),
        ("Vue", include_str!("../syntaxes/Vue.sublime-syntax")),
        ("WGSL", include_str!("../syntaxes/WGSL.sublime-syntax")),
        ("Zig", include_str!("../syntaxes/Zig.sublime-syntax")),
    ];

    /// 全局语法集合，懒加载时合并内置语法与内嵌的自定义语法。
    pub(crate) static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(|| {
        let mut builder = SyntaxSet::load_defaults_newlines().into_builder();
        for (name, src) in CUSTOM_SYNTAXES {
            match SyntaxDefinition::load_from_str(src, true, Some(name)) {
                Ok(def) => {
                    builder.add(def);
                }
                Err(e) => tracing::warn!("Failed to load embedded syntax {}: {:?}", name, e),
            }
        }
        let built = builder.build();
        tracing::info!(
            "SyntaxSet built: {} syntaxes, swift={:?}",
            built.syntaxes().len(),
            built.find_syntax_by_extension("swift").map(|s| &s.name)
        );
        built
    });

    /// 根据语言标识查找对应的语法定义。
    ///
    /// 依次尝试：扩展名、语法名称、小写扩展名/名称、常用别名映射。
    /// 如果全部失败，则回退到纯文本语法。
    fn find_syntax(lang: Option<&str>) -> &'static syntect::parsing::SyntaxReference {
        let ss = &*SYNTAX_SET;
        if let Some(lang) = lang {
            if !lang.is_empty() {
                // 尝试按扩展名匹配
                if let Some(s) = ss.find_syntax_by_extension(lang) {
                    return s;
                }
                // 尝试按语法名称匹配
                if let Some(s) = ss.find_syntax_by_name(lang) {
                    return s;
                }
                // 小写扩展名再匹配一次（部分语言的扩展名习惯小写）
                let lower = lang.to_lowercase();
                if lower != lang {
                    if let Some(s) = ss.find_syntax_by_extension(&lower) {
                        return s;
                    }
                }
                // 大小写不敏感的语法名称匹配（syntect 的语法名通常首字母大写，如 Haskell）
                if let Some(s) = ss
                    .syntaxes()
                    .iter()
                    .find(|s| s.name.eq_ignore_ascii_case(lang))
                {
                    return s;
                }
                // 常用语言别名映射表
                let aliases: &[(&str, &str)] = &[
                    ("rust", "rs"),
                    ("js", "js"),
                    ("javascript", "js"),
                    ("typescript", "ts"),
                    // bun 运行器跑的是 TypeScript，归一化后用 ts 语法高亮。
                    ("bun", "ts"),
                    ("py", "py"),
                    ("python", "py"),
                    ("rb", "rb"),
                    ("ruby", "rb"),
                    ("sh", "sh"),
                    ("bash", "sh"),
                    ("yaml", "yaml"),
                    ("yml", "yaml"),
                    ("md", "md"),
                    ("markdown", "md"),
                    ("kotlin", "kt"),
                    ("swift", "swift"),
                    ("golang", "go"),
                    // Vue SFC:file_extensions 已含 vue,此条兜底大写 ```Vue 等边界。
                    ("vue", "vue"),
                ];
                for &(from, to) in aliases {
                    // 别名比较同样不区分大小写，保证 "RUST" 与 "rust" 等价。
                    if lang.eq_ignore_ascii_case(from) {
                        if let Some(s) = ss.find_syntax_by_extension(to) {
                            return s;
                        }
                    }
                }
            }
        }
        ss.find_syntax_by_extension("txt")
            .or_else(|| ss.find_syntax_by_name("Plain Text"))
            .expect("no plain text syntax")
    }

    /// 对给定代码字符串按指定语言进行高亮，返回 HTML 字符串。
    ///
    /// 输出使用 spaced CSS class 风格，便于与 `highlight.css` 中的选择器匹配。
    pub fn highlight_code(code: &str, lang: Option<&str>) -> String {
        let trimmed = code.trim();
        let syntax = find_syntax(lang);
        let ss = &*SYNTAX_SET;
        let mut generator =
            ClassedHTMLGenerator::new_with_class_style(syntax, ss, ClassStyle::Spaced);

        // 逐行解析，出错时记录警告并继续
        for line in LinesWithEndings::from(trimmed) {
            if let Err(e) = generator.parse_html_for_line_which_includes_newline(line) {
                tracing::warn!("syntect parse error: {:?}", e);
            }
        }

        generator.finalize()
    }
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::server::*;

    #[test]
    fn custom_syntax_list_matches_directory() {
        // CUSTOM_SYNTAXES 必须与 syntaxes/ 目录下的 .sublime-syntax 文件一一对应，
        // 防止新增/删除语法文件后忘记同步内嵌列表（仿 migrations 数组的编译期校验）。
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/syntaxes");
        let mut on_disk: Vec<String> = std::fs::read_dir(dir)
            .expect("syntaxes/ 目录应存在")
            .filter_map(|e| {
                let p = e.ok()?.path();
                if p.extension()? == "sublime-syntax" {
                    Some(p.file_stem()?.to_string_lossy().into_owned())
                } else {
                    None
                }
            })
            .collect();
        on_disk.sort();
        let mut embedded: Vec<&str> = CUSTOM_SYNTAXES.iter().map(|(name, _)| *name).collect();
        embedded.sort();
        assert_eq!(embedded, on_disk, "CUSTOM_SYNTAXES 与 syntaxes/ 目录不一致");
    }

    #[test]
    fn custom_syntaxes_are_loaded() {
        // 内嵌语法必须真正进入 SyntaxSet（守护生产环境回退纯文本的回归）。
        for (name, _) in CUSTOM_SYNTAXES {
            let lower = name.to_lowercase();
            assert!(
                SYNTAX_SET.find_syntax_by_name(name).is_some()
                    || SYNTAX_SET.find_syntax_by_extension(&lower).is_some(),
                "自定义语法 {} 未加载",
                name
            );
        }
    }

    #[test]
    fn highlight_code_rust() {
        let result = highlight_code("fn main() {}", Some("rust"));
        assert!(result.contains(r#"<span class="storage type function rust">fn</span>"#));
        assert!(result.contains(r#"<span class="entity name function rust">main</span>"#));
    }

    #[test]
    fn highlight_code_javascript_alias() {
        let result = highlight_code("console.log('hi')", Some("js"));
        assert!(result.contains(r#"<span class="support type object console js">console</span>"#));
        assert!(result.contains(r#"<span class="support function console js">log</span>"#));
    }

    #[test]
    fn highlight_code_python_alias() {
        let result = highlight_code("print('hi')", Some("python"));
        assert!(result.contains(r#"<span class="support function builtin python">print</span>"#));
    }

    #[test]
    fn highlight_code_unknown_language() {
        let result = highlight_code("some text", Some("brainfuck"));
        assert!(result.contains(r#"<span class="text plain">some text</span>"#));
    }

    #[test]
    fn highlight_code_none_language() {
        let result = highlight_code("plain text", None);
        assert!(result.contains(r#"<span class="text plain">plain text</span>"#));
    }

    #[test]
    fn highlight_code_empty() {
        let result = highlight_code("", None);
        assert!(result.is_empty());
    }

    #[test]
    fn highlight_code_produces_span_tags() {
        let result = highlight_code("let x = 1;", Some("rust"));
        assert!(result.contains(r#"<span class="storage type rust">let</span>"#));
        assert!(result.contains(r#"<span class="constant numeric integer decimal rust">1</span>"#));
    }

    #[test]
    fn highlight_code_haskell_by_full_name() {
        // Haskell 语法名首字母大写，扩展名为 hs；直接写 "haskell" 应能匹配。
        let code = "factorial :: Integer -> Integer\nfactorial 0 = 1";
        let result = highlight_code(code, Some("haskell"));
        assert!(
            !result.contains(r#"<span class="text plain">"#),
            "Haskell 不应回退到纯文本: {}",
            result
        );
        assert!(
            result.contains("source haskell"),
            "Haskell 应输出 source haskell: {}",
            result
        );
    }

    #[test]
    fn highlight_code_uppercase_language_falls_back_via_lowercase() {
        // 大写语言标识应通过小写回退路径匹配到对应语法。
        let lower = highlight_code("fn main() {}", Some("rust"));
        let upper = highlight_code("fn main() {}", Some("RUST"));
        // 大写标识的输出必须与小写标识完全一致，证明回退路径生效。
        assert_eq!(lower, upper);
        assert!(lower.contains(r#"<span class="storage type function rust">fn</span>"#));
    }

    #[test]
    fn highlight_code_resolves_golang_alias() {
        // 别名表中 "golang" 映射到 "go" 扩展名，输出应与直接用 "go" 一致。
        let by_alias = highlight_code("package main", Some("golang"));
        let by_ext = highlight_code("package main", Some("go"));
        assert_eq!(by_alias, by_ext);
        // 别名解析必须产出带 span 的高亮输出，而非纯文本。
        assert!(by_alias.contains("span"));
    }

    #[test]
    fn highlight_code_resolves_bash_alias() {
        // 别名表中 "bash" 映射到 "sh" 扩展名。
        let result = highlight_code("echo hello", Some("bash"));
        assert!(result.contains("span"));
    }

    #[test]
    fn highlight_code_resolves_yml_alias() {
        // 别名表中 "yml" 映射到 "yaml" 扩展名。
        let result = highlight_code("key: value", Some("yml"));
        assert!(!result.is_empty());
    }

    #[test]
    fn highlight_code_unknown_language_falls_back_to_plain_text() {
        // 无法识别的语言应回退到纯文本语法，仍能输出内容。
        let result = highlight_code("hello world", Some("totally-not-a-language-xyz"));
        assert!(result.contains("hello world"));
    }

    #[test]
    fn highlight_code_empty_language_string_falls_back_to_plain_text() {
        // 空字符串语言标识应走纯文本回退路径。
        let result = highlight_code("just text", Some(""));
        assert!(result.contains("just text"));
    }

    #[test]
    fn highlight_code_trims_surrounding_whitespace() {
        // 代码首尾的空白会被 trim 掉再高亮。
        let result = highlight_code("  \nfn main() {}\n  ", Some("rust"));
        assert!(result.contains(r#"<span class="storage type function rust">fn</span>"#));
    }

    #[test]
    fn highlight_code_multiline_output_spans_all_lines() {
        // 多行代码每一行都应被解析为带 span 的输出。
        let code = "fn a() {}\nfn b() {}";
        let result = highlight_code(code, Some("rust"));
        // 两处 fn 关键字都应出现
        assert_eq!(
            result
                .matches(r#"<span class="storage type function rust">fn</span>"#)
                .count(),
            2
        );
    }

    #[test]
    fn highlight_code_swift_keyword_and_func() {
        // Swift 关键字 func/import/let 应生成 declaration/keyword span，而不是纯文本。
        let code =
            "import Foundation\nfunc greet(person: String) -> String {\n    return \"Hi\"\n}";
        let result = highlight_code(code, Some("swift"));
        assert!(
            result.contains("keyword"),
            "Swift 输出缺少关键字高亮: {}",
            result
        );
        // 函数名应被识别为函数（声明名 entity name function 或调用 variable function）。
        assert!(
            result.contains("name function") || result.contains("variable function"),
            "Swift func 名缺少函数高亮: {}",
            result
        );
    }

    #[test]
    fn highlight_code_swift_types_and_strings() {
        // Swift 标准库类型与字符串字面量都应被识别。
        let code = "let count: Int = 42\nlet name = \"hello\"";
        let result = highlight_code(code, Some("swift"));
        assert!(
            result.contains("support type") || result.contains("entity name type"),
            "Swift Int 类型未被识别为类型: {}",
            result
        );
        assert!(
            result.contains("string"),
            "Swift 字符串未被识别: {}",
            result
        );
    }

    #[test]
    fn highlight_code_typescript_keywords_and_types() {
        // TS 关键字 interface/const/=> 与类型 string/number 应被识别。
        let code = "interface User { name: string; }\nconst x: number = 42;";
        let result = highlight_code(code, Some("typescript"));
        assert!(
            result.contains("keyword"),
            "TypeScript 关键字未被识别: {}",
            result
        );
        assert!(
            result.contains("support type") || result.contains("entity name type"),
            "TypeScript 类型未被识别: {}",
            result
        );
    }

    #[test]
    fn highlight_code_script_function_definitions_differ_from_calls() {
        // 函数定义与调用配色不同（定义用 clay，调用退为灰），scope 必须分开。
        let code = "function setup() {}\nconst run = () => setup();\nclass A { load(): void { if (ok(x)) {} } }";
        for lang in &["ts", "tsx", "jsx"] {
            let result = highlight_code(code, Some(lang));
            for name in ["setup", "run", "load"] {
                assert!(
                    result.contains(&format!(r#"entity name function {lang}">{name}</span>"#)),
                    "{lang} 函数定义 {name} 未识别: {result}"
                );
            }
            for name in ["setup", "ok"] {
                assert!(
                    result.contains(&format!(r#"variable function {lang}">{name}</span>"#)),
                    "{lang} 函数调用 {name} 未识别: {result}"
                );
            }
            assert!(
                result.contains(&format!(r#"keyword control {lang}">if</span>"#)),
                "{lang} 控制语句被识别为方法定义: {result}"
            );
        }
    }

    #[test]
    fn highlight_code_jsx_tags_and_attributes() {
        // JSX 标签名与属性名都应被识别。
        let code = "const el = <Button title=\"save\" onClick={fn}>OK</Button>;";
        for lang in &["jsx", "tsx"] {
            let result = highlight_code(code, Some(lang));
            assert!(
                result.contains("entity name tag"),
                "{lang} JSX 标签名未识别: {result}"
            );
            assert!(
                result.contains("attribute"),
                "{lang} JSX 属性名未识别: {result}"
            );
        }
    }

    #[test]
    fn highlight_code_typescript_resolves_ts_alias() {
        // 别名 "ts" 与 "typescript" 输出应一致。
        let code = "const x: number = 1;";
        let by_ext = highlight_code(code, Some("ts"));
        let by_name = highlight_code(code, Some("typescript"));
        assert_eq!(by_ext, by_name);
        assert!(by_ext.contains("keyword"));
    }

    #[test]
    fn highlight_code_zig_keywords_and_fn() {
        // Zig 关键字 const/fn/pub 与内建函数 @import 都应被高亮。
        let code = "const std = @import(\"std\");\npub fn main() void {}";
        let result = highlight_code(code, Some("zig"));
        assert!(result.contains("keyword"), "Zig 关键字未被识别: {}", result);
        assert!(
            result.contains("name function"),
            "Zig 函数名未被识别: {}",
            result
        );
        assert!(
            result.contains("builtin") || result.contains("support function"),
            "Zig 内建函数 @import 未被识别: {}",
            result
        );
    }

    #[test]
    fn highlight_code_zig_types_and_strings() {
        // Zig 整数类型、字符串字面量与十六进制数字应被识别。
        let code = "const x: u32 = 0xFF;\nconst s = \"hello\"";
        let result = highlight_code(code, Some("zig"));
        assert!(
            result.contains("support type") || result.contains("keyword"),
            "Zig u32 类型未被识别: {}",
            result
        );
        assert!(result.contains("string"), "Zig 字符串未被识别: {}", result);
        assert!(result.contains("numeric"), "Zig 数字未被识别: {}", result);
    }

    #[test]
    fn highlight_code_vue_sfc() {
        // Vue SFC 三段(template HTML + script JS + style CSS)都应被识别,
        // 不回退到纯文本(text plain)。
        let code = "\
<template>
  <div class=\"hello\" @click=\"onClick\">{{ message }}</div>
</template>

<script setup>
import { ref } from 'vue'
const message = ref('Hello Vue!')
</script>

<style scoped>
.hello { color: #42b983; }
</style>";
        let result = highlight_code(code, Some("vue"));
        assert!(
            !result.contains(r#"<span class="text plain">"#),
            "Vue 不应回退到纯文本: {}",
            result
        );
        assert!(
            result.contains("entity name tag"),
            "Vue template 标签未被识别: {}",
            result
        );
        assert!(
            result.contains("source js"),
            "Vue script 段未嵌入 JS 高亮: {}",
            result
        );
        assert!(
            result.contains("source css"),
            "Vue style 段未嵌入 CSS 高亮: {}",
            result
        );
        assert!(
            result.contains("entity other attribute-name"),
            "Vue 指令/属性未被识别: {}",
            result
        );
    }

    #[test]
    fn highlight_code_vue_script_lang_ts() {
        // <script lang="ts"> 应嵌入 TypeScript(scope source ts),而非 JS。
        let code = "<script lang=\"ts\">\nconst x: number = 42\n</script>";
        let result = highlight_code(code, Some("vue"));
        assert!(
            result.contains("source ts"),
            "Vue lang=ts 应嵌入 TS: {}",
            result
        );
    }

    #[test]
    fn highlight_code_vue_resolves_vue_alias() {
        // 别名表 "vue" 与扩展名 "vue" 输出应一致。
        let code = "<template><p>{{ msg }}</p></template>";
        let by_alias = highlight_code(code, Some("vue"));
        let by_upper = highlight_code(code, Some("Vue"));
        // 大写标识经别名表 eq_ignore_ascii_case 回退,输出须与小写一致。
        assert_eq!(by_alias, by_upper);
    }

    #[test]
    fn highlight_code_resolves_bun_alias_to_typescript() {
        // bun 运行器跑 TypeScript；别名表把 "bun" 归一为 "ts" 扩展名。
        // 用类型注解（TS 特有语法）验证命中的是 TypeScript 语法而非纯 JS。
        let code = "const x: number = 1;";
        let result = highlight_code(code, Some("bun"));
        // 不应回退纯文本（纯文本无 <span> 高亮 span）。
        assert!(
            result.contains("<span"),
            "bun 别名应触发语法高亮, got: {}",
            result
        );
        // 与直接传 ts 的输出一致——别名表正确归一。
        let by_ts = highlight_code(code, Some("ts"));
        assert_eq!(result, by_ts);
    }

    #[test]
    fn highlight_code_json_produces_spans() {
        // MCP 客户端配置页的 JSON 片段经此路径高亮；验证 json 解析为彩色语法而非纯文本。
        let code = r#"{"mcpServers": {"yggdrasil": {"type": "http"}}}"#;
        let result = highlight_code(code, Some("json"));
        assert!(
            result.contains("<span"),
            "json 应触发语法高亮, got: {}",
            result
        );
        assert!(
            !result.contains(r#"<span class="text plain">"#),
            "json 不应回退纯文本: {}",
            result
        );
    }

    #[test]
    fn highlight_code_bash_alias_produces_spans() {
        // MCP 客户端配置页的 CLI 一行命令经 bash 别名高亮。
        let code = "claude mcp add --transport http yggdrasil https://example.com/mcp";
        let result = highlight_code(code, Some("bash"));
        assert!(
            result.contains("<span"),
            "bash 别名应触发语法高亮, got: {}",
            result
        );
        // bash 别名与直接传 sh 的输出须一致（别名表 "bash" -> "sh"）。
        let by_sh = highlight_code(code, Some("sh"));
        assert_eq!(result, by_sh);
    }
    #[test]
    fn highlight_code_toml() {
        // 官方 sublimehq TOML 语法（version: 2），覆盖表头/键/字符串/数字/布尔/注释。
        let code = "\
[package]
name = \"yggdrasil\"
version = \"1.0\"
edition = 2021

[dependencies]
syntect = { version = \"5\", features = [\"html\"] }
published = true
# a comment";
        let result = highlight_code(code, Some("toml"));
        // 根作用域 source.toml —— 证明命中真正的 TOML 语法而非纯文本。
        assert!(
            result.contains(r#"<span class="source toml">"#),
            "TOML 未命中 source.toml 根作用域: {}",
            result
        );
        assert!(
            !result.contains(r#"<span class="text plain">"#),
            "TOML 不应回退到纯文本: {}",
            result
        );
        // [package] 表头 -> entity name section
        assert!(
            result.contains("entity name section"),
            "TOML 表头未识别为 section: {}",
            result
        );
        // 键 name/version/edition -> string unquoted
        assert!(
            result.contains("string unquoted"),
            "TOML 裸键未被识别: {}",
            result
        );
        // "yggdrasil" 字符串值 -> string quoted double
        assert!(
            result.contains("string quoted double"),
            "TOML 字符串值未被识别: {}",
            result
        );
        // 2021 -> constant numeric
        assert!(
            result.contains("constant numeric"),
            "TOML 数字未被识别: {}",
            result
        );
        // true -> constant language boolean
        assert!(
            result.contains("constant language boolean"),
            "TOML 布尔值未被识别: {}",
            result
        );
        // # a comment -> comment line number-sign
        assert!(
            result.contains("comment line number-sign"),
            "TOML 注释未被识别: {}",
            result
        );
    }

    #[test]
    fn highlight_code_toml_uppercase_matches_lowercase() {
        // 语法注册了 .toml 扩展名；大写标识经小写回退路径应与 toml 等价。
        let code = "[package]\nname = \"x\"";
        let lower = highlight_code(code, Some("toml"));
        let upper = highlight_code(code, Some("TOML"));
        assert_eq!(lower, upper);
        assert!(lower.contains(r#"<span class="source toml">"#));
    }
    #[test]
    fn highlight_code_wgsl() {
        // relrelb/sublime-wgsl 语法，覆盖 attribute/fn/类型/字面量/控制流/struct/注释。
        let code = "\
@vertex
fn vs_main(@location(0) pos: vec3<f32>) -> @builtin(position) vec4<f32> {
  let x: f32 = 1.5;
  return vec4<f32>(pos, x);
}
// comment
struct Uniforms { m: mat4x4<f32> }";
        let result = highlight_code(code, Some("wgsl"));
        // 根作用域 source.wgsl —— 证明命中真正的 WGSL 语法而非纯文本。
        assert!(
            result.contains(r#"<span class="source wgsl">"#),
            "WGSL 未命中 source.wgsl 根作用域: {}",
            result
        );
        assert!(
            !result.contains(r#"<span class="text plain">"#),
            "WGSL 不应回退到纯文本: {}",
            result
        );
        // @vertex -> entity name attribute
        assert!(
            result.contains("entity name attribute"),
            "WGSL @attribute 未识别: {}",
            result
        );
        // fn -> keyword other fn
        assert!(
            result.contains("keyword other fn"),
            "WGSL fn 未识别: {}",
            result
        );
        // vec3/f32/mat4x4 -> storage type
        assert!(
            result.contains("storage type"),
            "WGSL 类型(vec3/f32/mat4x4)未识别: {}",
            result
        );
        // 1.5 -> constant numeric float
        assert!(
            result.contains("constant numeric float"),
            "WGSL 浮点字面量未识别: {}",
            result
        );
        // 0 -> constant numeric
        assert!(
            result.contains("constant numeric decimal"),
            "WGSL 整数字面量未识别: {}",
            result
        );
        // return -> keyword control
        assert!(
            result.contains("keyword control"),
            "WGSL 控制关键字未识别: {}",
            result
        );
        // struct -> keyword declaration struct
        assert!(
            result.contains("keyword declaration struct"),
            "WGSL struct 未识别: {}",
            result
        );
        // Uniforms -> entity name type
        assert!(
            result.contains("entity name type"),
            "WGSL 自定义类型名未识别: {}",
            result
        );
        // // comment -> comment line double-slash
        assert!(
            result.contains("comment line double-slash"),
            "WGSL 行注释未识别: {}",
            result
        );
        // pos/x/m -> variable other
        assert!(
            result.contains("variable other"),
            "WGSL 变量未识别: {}",
            result
        );
    }

    #[test]
    fn highlight_code_wgsl_uppercase_matches_lowercase() {
        // 语法注册了 .wgsl 扩展名；大写标识经小写回退路径应与 wgsl 等价。
        let code = "@compute @workgroup_size(1)\nfn main() {}";
        let lower = highlight_code(code, Some("wgsl"));
        let upper = highlight_code(code, Some("WGSL"));
        assert_eq!(lower, upper);
        assert!(lower.contains(r#"<span class="source wgsl">"#));
    }
}
