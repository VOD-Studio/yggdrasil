//! 关于页面：以世界树、引文与年轮呈现「书写抵抗遗忘」的主题。
//! 内容直接 SSR；树冠只有本地开合状态，不请求或存储访客数据。

use dioxus::prelude::*;

use crate::router::Route;

/// 静态文档不属于 SPA 路由，保留普通外链入口。
const LINKS: &[(&str, &str, &str)] = &[(
    "/doc/yggdrasil/index.html",
    "站点文档",
    "这棵树是如何长成的",
)];

#[component]
pub fn About() -> Element {
    rsx! {
        article { class: "about-page",
            header { class: "about-hero",
                div { class: "about-hero-copy",
                    p { class: "about-eyebrow", span {} "ABOUT / 关于这里" }
                    h1 {
                        "世界遗忘的，"
                        span { "树记得。" }
                    }
                    p { class: "about-hero-description",
                        "在时间的缝隙里，种下一些文字。"
                        br {}
                        "让想法生根，让记忆有迹可循。"
                    }
                    Link { class: "about-reading-link", to: Route::Archives {},
                        "翻阅这里的故事"
                        span { class: "about-arrow", aria_hidden: "true", "↗" }
                    }
                }
                WorldTree {}
            }

            section { class: "about-story about-enter", style: "--about-delay: 120ms",
                aria_labelledby: "about-name-title",
                div { class: "about-section-label",
                    span { class: "about-index", aria_hidden: "true", "01 / ROOTS" }
                    h2 { id: "about-name-title", "名字的根须" }
                }
                div { class: "about-story-copy",
                    p { class: "about-name", "Yggdrasil" span { "世界树" } }
                    p { "北欧神话中的世界树，根须贯穿九界，枝叶承载记忆。" }
                    p { "这里是它的一小片枝叶。写下所见、所想，也收留那些不想忘记的瞬间。" }
                }
            }

            section { class: "about-memory about-enter", style: "--about-delay: 200ms",
                aria_label: "关于记忆与书写",
                span { class: "about-quote-mark", aria_hidden: "true", "“" }
                blockquote {
                    p { "世界……遗忘我……" }
                    footer { "—— 大慈树王" }
                }
                div { class: "about-memory-response",
                    span { class: "about-small-rule", aria_hidden: "true" }
                    p { "人会遗忘，也终将被遗忘。" }
                    p { "而写下的，会替我们记得。" }
                }
                details { class: "about-footnote",
                    summary {
                        span { "这句引文，与这棵树" }
                        span { class: "about-plus", aria_hidden: "true" }
                    }
                    div { class: "about-footnote-copy",
                        p { "这句话来自大慈树王将自己从世界树（Irminsul）中抹除时的请求。" }
                        p { "一个故事里的世界树选择遗忘，另一个名字里的世界树收留记忆。于是，这里有了一个小小的回应：世界遗忘的，树记得。" }
                    }
                }
            }

            section { class: "about-rings about-enter", style: "--about-delay: 280ms",
                aria_labelledby: "about-rings-title",
                div { class: "about-rings-heading",
                    div { class: "about-section-label",
                        span { class: "about-index", aria_hidden: "true", "02 / GROWTH" }
                        h2 { id: "about-rings-title", "生长的年轮" }
                    }
                    p { "每一次记录，都留下一圈纹路。" }
                }
                div { class: "about-link-grid",
                    Link { class: "about-ring-card", to: Route::Changelog {},
                        span { class: "about-card-top", aria_hidden: "true",
                            span { class: "about-ring-symbol", span {} span {} span {} }
                            span { class: "about-card-kicker", "THE CHANGELOG" }
                        }
                        span { class: "about-card-title", "更新日志" }
                        span { class: "about-card-description", "每一圈年轮，都是一次生长" }
                        span { class: "about-card-arrow about-arrow", aria_hidden: "true", "→" }
                    }
                    for (href, name, description) in LINKS.iter().copied() {
                        a {
                            key: "{href}",
                            class: "about-ring-card about-doc-card",
                            href,
                            target: "_blank",
                            rel: "noopener noreferrer",
                            aria_label: "{name}：{description}（在新标签页打开）",
                            span { class: "about-card-top", aria_hidden: "true",
                                svg { class: "about-book-symbol", view_box: "0 0 40 40", fill: "none",
                                    path { d: "M20 10C15 6 9 6 4 8V31C9 29 15 29 20 33C25 29 31 29 36 31V8C31 6 25 6 20 10ZM20 10V33M9 13C12 12 15 13 16 14M9 19C12 18 15 19 16 20M25 14C28 12 30 12 32 13", stroke: "currentColor", stroke_width: "1.3", stroke_linecap: "round", stroke_linejoin: "round" }
                                }
                                span { class: "about-card-kicker", "FIELD NOTES" }
                            }
                            span { class: "about-card-title", "{name}" }
                            span { class: "about-card-description", "{description}" }
                            span { class: "about-card-arrow about-arrow", aria_hidden: "true", "↗" }
                        }
                    }
                }
            }

            footer { class: "about-colophon about-enter", style: "--about-delay: 340ms",
                span { class: "about-colophon-star", aria_hidden: "true", "✳" }
                p { "生长未完，故事继续。" }
                span { class: "about-colophon-name", "YGGDRASIL · A PLACE TO REMEMBER" }
            }
        }
    }
}

/// 原生按钮支持触屏、Enter 与空格；点击展开新叶，再次点击回到初态。
#[component]
fn WorldTree() -> Element {
    let mut grown = use_signal(|| false);
    let leaves = [
        (200, 80, -40),
        (184, 99, -110),
        (218, 106, -10),
        (168, 120, -135),
        (235, 128, -5),
        (143, 139, -135),
        (256, 146, 5),
        (114, 159, -145),
        (281, 169, 15),
        (154, 171, -100),
        (244, 181, -5),
        (177, 151, -120),
        (222, 159, -25),
        (132, 190, -150),
        (263, 204, 20),
        (100, 206, -150),
        (298, 218, 25),
        (168, 210, -120),
        (229, 219, -15),
        (144, 226, -160),
        (250, 239, 25),
        (186, 188, -110),
        (212, 198, -30),
        (199, 126, -55),
    ];

    rsx! {
        figure { class: "about-tree-figure about-enter", style: "--about-delay: 100ms",
            button {
                class: "about-tree",
                r#type: "button",
                aria_label: "让记忆生长",
                aria_pressed: "{grown}",
                aria_describedby: "about-tree-caption",
                "data-grown": "{grown}",
                onclick: move |_| grown.toggle(),
                span { class: "about-tree-glow", aria_hidden: "true" }
                svg {
                    class: "about-tree-art",
                    view_box: "0 0 400 440",
                    fill: "none",
                    "aria-hidden": "true",
                    g { class: "about-tree-orbits", stroke: "currentColor", stroke_width: "0.7",
                        circle { cx: "200", cy: "208", r: "154" }
                        circle { cx: "200", cy: "208", r: "137", stroke_dasharray: "2 7" }
                        ellipse { cx: "200", cy: "208", rx: "170", ry: "115", transform: "rotate(-42 200 208)" }
                        path { d: "M200 40V51M200 365V376M32 208H43M357 208H368" }
                    }
                    g { class: "about-tree-stars", fill: "currentColor",
                        path { d: "M307 81L309 88L316 90L309 92L307 99L305 92L298 90L305 88ZM79 274L81 279L86 281L81 283L79 288L77 283L72 281L77 279Z" }
                        circle { cx: "94", cy: "111", r: "2.5" }
                        circle { cx: "330", cy: "247", r: "2" }
                        circle { cx: "263", cy: "62", r: "1.5" }
                    }
                    g { class: "about-tree-roots", stroke: "currentColor", stroke_width: "1.2", stroke_linecap: "round",
                        path { d: "M198 284C193 308 177 310 156 327L133 341M202 284C207 307 227 315 251 332L267 341M199 292L191 324L177 348M203 297L216 329L221 353M200 314V359M184 306L160 311L143 320M221 310L245 314L265 326M163 322L156 342M240 326L247 348M191 324L181 330M216 329L233 339" }
                        path { class: "about-tree-ground", d: "M102 286C155 282 243 282 298 286" }
                    }
                    g { class: "about-tree-branches", stroke: "currentColor", stroke_linecap: "round", stroke_linejoin: "round",
                        path { stroke_width: "3", d: "M198 287C202 263 194 244 201 221C207 198 193 182 200 159C205 136 195 111 200 84" }
                        path { stroke_width: "1.7", d: "M201 263C180 240 150 245 128 224L104 209M199 246C220 232 261 241 281 226L296 219M200 224C180 207 157 207 145 188L117 162M200 206C222 195 258 198 278 174M198 183C172 171 163 148 146 142M202 170C217 159 239 154 253 149M200 149C179 140 178 128 171 122M201 137L231 131M198 113L187 102M199 123L215 110" }
                        path { stroke_width: "1.1", d: "M158 240L146 229M165 241L170 214M256 239L252 241M233 235L230 223M147 189L134 192M159 202L155 174M247 197L246 184M259 195L263 204M179 167L179 154M218 161L221 161M192 201L187 191M203 219L211 202" }
                    }
                    g { class: "about-tree-leaves",
                        for (index, (x, y, angle)) in leaves.iter().enumerate() {
                            g { key: "{index}", transform: "translate({x} {y}) rotate({angle})",
                                path {
                                    class: if index % 3 == 0 { "about-tree-leaf about-tree-new-leaf" } else { "about-tree-leaf" },
                                    style: "--leaf-delay: {index % 6 * 45}ms",
                                    d: "M0 0C-6-10-4-22 0-27C8-20 10-8 0 0Z",
                                }
                            }
                        }
                    }
                    circle { class: "about-tree-seed", cx: "200", cy: "284", r: "4", fill: "currentColor" }
                }
                span { class: "about-tree-touch", aria_hidden: "true", "✳" }
            }
            figcaption { id: "about-tree-caption", class: "about-tree-caption", aria_live: "polite",
                if grown() {
                    "一片新叶，一段被留下的记忆"
                } else {
                    "轻触这棵树，让记忆生长"
                }
            }
        }
    }
}
