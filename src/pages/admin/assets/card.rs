//! Asset thumbnail, selection, metadata editing and deletion actions.
#[cfg(target_arch = "wasm32")]
use crate::api::assets::{delete_asset, update_asset_alt};
use crate::components::forms::FormInput;
use crate::components::ui::MEDIA_BADGE_BASE;
use crate::models::asset::AssetDto;
use crate::utils::format_bytes;
use dioxus::prelude::*;
use std::collections::HashSet;

/// Handles remain shared so editing and deletion confirmation stay exclusive across cards.
#[derive(Clone, Copy, PartialEq)]
pub(super) struct AssetCardState {
    pub selected_ids: Signal<HashSet<String>>,
    pub batch_closing: Signal<bool>,
    pub confirm_delete: Signal<Option<String>>,
    pub editing_alt: Signal<Option<String>>,
    pub alt_input: Signal<String>,
    pub refs_popover: Signal<Option<(String, i32, i32, &'static str)>>,
    pub reload: Signal<i32>,
    pub show_op: Callback<String>,
    pub dismiss_batch: Callback<()>,
}

#[component]
#[cfg_attr(not(target_arch = "wasm32"), allow(unused_mut, unused_variables))]
pub(super) fn AssetCard(asset: AssetDto, index: usize, state: AssetCardState) -> Element {
    let AssetCardState {
        mut selected_ids,
        mut batch_closing,
        mut confirm_delete,
        mut editing_alt,
        mut alt_input,
        mut refs_popover,
        mut reload,
        show_op,
        dismiss_batch,
    } = state;
    let any_selected = !selected_ids().is_empty();
    let a = &asset.asset;
    let thumb = format!("/uploads/{}?thumb=300x300", a.path);
    let placeholder = format!("/uploads/{}?w=20", a.path);
    let img_alt = a.alt.clone().unwrap_or_else(|| a.filename.clone());
    let is_orphan = asset.ref_count == 0;
    let is_selected = selected_ids().contains(&a.id);
    // 选中卡片加主题色描边（ring 与原有 border 叠加，不动布局）。
    let card_ring = if is_selected {
        "ring-2 ring-[var(--color-paper-accent)]"
    } else {
        ""
    };
    // 阶梯入场 delay：30ms 步长、300ms 封顶——前 10 张阶梯、其余同播，
    // 60 张/页时尾部不晚于 ~750ms 全部到位（对齐 asset_upload 的 idx*30 先例）。
    let enter_delay = (index * 30).min(300);
    // 勾选框：未引用素材才可删可选。选中或有任何选择时常显，
    // 否则随卡片 hover 显现（与卡片操作按钮同一显现语言）。
    let checkbox_class = if is_selected {
        "absolute top-2 right-2 z-10 w-6 h-6 flex items-center justify-center rounded-full text-xs cursor-pointer transition-all bg-[var(--color-paper-accent)] text-white border border-[var(--color-paper-accent)]"
    } else if any_selected {
        "absolute top-2 right-2 z-10 w-6 h-6 flex items-center justify-center rounded-full text-xs cursor-pointer transition-all bg-black/40 backdrop-blur-sm text-white border border-white/60"
    } else {
        "absolute top-2 right-2 z-10 w-6 h-6 flex items-center justify-center rounded-full text-xs cursor-pointer transition-all bg-black/40 backdrop-blur-sm text-white border border-white/60 opacity-0 group-hover:opacity-100"
    };
    // z-10：.blur-img-full 带 z-index:1，不提升会被展示层盖住（灯箱改造的回归）。
    let badge_tone = if is_orphan {
        "bg-amber-500/80 text-white"
    } else {
        "bg-black/50 text-white"
    };
    rsx! {
        div {
            key: "{a.id}",
            class: "group relative rounded-2xl overflow-hidden border border-[var(--color-paper-border)] bg-[var(--color-paper-entry)] shadow-sm hover:shadow-md hover:-translate-y-1 transition-all duration-300 animate-row-enter {card_ring}",
            style: "animation-delay: {enter_delay}ms",
            // blur-img 双层结构（对齐前台正文图）：?w=20 模糊占位 +
            // data-src 展示层（IO 懒加载）；点击由 lightbox.js 接管为灯箱
            // （图集模式，原图 = data-src 去 query）。不加 lightbox-single。
            // data-error-text：缩略图重试耗尽仍失败（本地文件丢失等）时
            // 卡片占位与灯箱错误态显示的定制文案。
            div {
                class: "blur-img !rounded-none aspect-square m-0 cursor-pointer bg-[var(--color-paper-theme)]",
                "data-error-text": "本地文件已丢失",
                img { class: "blur-img-placeholder", src: "{placeholder}", alt: "" }
                img {
                    class: "blur-img-full",
                    "data-src": "{thumb}",
                    alt: "{img_alt}",
                }
            }
            // 引用徽标：未引用静态展示；引用中可点击 →
            // 引用明细浮层（stop_propagation 防触发灯箱）。
            if is_orphan {
                span {
                    class: "absolute top-2 left-2 z-10 {MEDIA_BADGE_BASE} backdrop-blur-sm {badge_tone}",
                    "未引用"
                }
            } else {
                button {
                    class: "absolute top-2 left-2 z-10 {MEDIA_BADGE_BASE} backdrop-blur-sm {badge_tone} cursor-pointer hover:brightness-125 transition",
                    title: "查看被谁引用",
                    onclick: {
                        let id = a.id.clone();
                        move |evt| {
                            evt.stop_propagation();
                            #[cfg(target_arch = "wasm32")]
                            {
                                let coords = evt.client_coordinates();
                                let x = coords.x as i32;
                                let y = coords.y as i32;
                                // 动态 align：靠视口左/右缘的卡片让面板
                                // 朝内侧延伸（240px ≈ 面板宽 w-80 的 3/4）。
                                let vw = web_sys::window()
                                    .and_then(|w| w.inner_width().ok())
                                    .and_then(|v| v.as_f64())
                                    .unwrap_or(1280.0)
                                    as i32;
                                let align = if x < 240 {
                                    "start"
                                } else if x > vw - 240 {
                                    "end"
                                } else {
                                    "center"
                                };
                                refs_popover.set(Some((id.clone(), x, y, align)));
                            }
                        }
                    },
                    "被 {asset.ref_count} 处引用"
                }
            }
            // 多选勾选框（仅未引用素材；stop_propagation 防触发灯箱）
            if is_orphan {
                button {
                    class: "{checkbox_class}",
                    title: if is_selected { "取消选择" } else { "选择" },
                    onclick: {
                        let id = a.id.clone();
                        move |evt| {
                            evt.stop_propagation();
                            let mut s = selected_ids();
                            if s.contains(&id) {
                                s.remove(&id);
                            } else {
                                s.insert(id.clone());
                            }
                            if s.is_empty() {
                                // 取消最后一个勾选 → 收起批量条（延迟清空播退出动画）
                                dismiss_batch(());
                            } else {
                                batch_closing.set(false);
                                selected_ids.set(s);
                            }
                        }
                    },
                    if is_selected {
                        "✓"
                    }
                }
            }
            div { class: "p-3",
                p {
                    class: "text-xs font-medium truncate text-[var(--color-paper-primary)]",
                    title: "{a.filename}",
                    "{a.filename}"
                }
                p { class: "text-[10px] font-mono text-[var(--color-paper-tertiary)] mt-0.5",
                    "{a.width}×{a.height} · {format_bytes(a.size_bytes)}"
                }
                if let Some(alt_text) = &a.alt {
                    p {
                        class: "text-[10px] truncate text-[var(--color-paper-secondary)] mt-0.5",
                        title: "{alt_text}",
                        "alt: {alt_text}"
                    }
                }

                // 操作区：确认删除 / alt 编辑 / 常规三按钮 三态互斥
                if confirm_delete().as_deref() == Some(a.id.as_str()) {
                    div { class: "flex items-center gap-2 mt-2",
                        button {
                            class: "text-[10px] font-medium cursor-pointer px-2 py-1 rounded-full bg-red-500 text-white hover:bg-red-600 transition-colors",
                            onclick: {
                                let id = a.id.clone();
                                move |_| {
                                    confirm_delete.set(None);
                                    let id = id.clone();
                                    #[cfg(target_arch = "wasm32")]
                                    spawn(async move {
                                        match delete_asset(id).await {
                                            Ok(resp) => {
                                                // 行已不在 DB（refs 为空的业务拒绝 = 素材不存在）
                                                // 说明网格是过期数据，同样触发刷新自愈。
                                                let stale = !resp.success && resp.refs.is_empty();
                                                show_op(resp.message);
                                                if resp.success || stale {
                                                    reload.set(reload() + 1);
                                                }
                                            }
                                            Err(e) => show_op(format!("删除失败：{e}")),
                                        }
                                    });
                                }
                            },
                            "确认删除"
                        }
                        button {
                            class: "text-[10px] cursor-pointer px-2 py-1 rounded-full border border-[var(--color-paper-border)] text-[var(--color-paper-secondary)] hover:text-[var(--color-paper-primary)] transition-colors",
                            onclick: move |_| confirm_delete.set(None),
                            "取消"
                        }
                    }
                } else if editing_alt().as_deref() == Some(a.id.as_str()) {
                    div { class: "flex items-center gap-1 mt-2",
                        FormInput {
                            r#type: "text",
                            placeholder: "alt 文本",
                            value: alt_input(),
                            class: "flex-1 min-w-0 text-[10px] px-2 py-1 rounded-full border border-paper-border bg-paper-entry text-paper-primary placeholder:text-paper-tertiary focus:outline-none focus:border-paper-accent transition-colors",
                            oninput: move |v: String| alt_input.set(v),
                        }
                        button {
                            class: "text-[10px] font-medium cursor-pointer px-2 py-1 rounded-full bg-[var(--color-paper-primary)] text-[var(--color-paper-theme)] hover:opacity-80 transition-opacity",
                            onclick: {
                                let id = a.id.clone();
                                move |_| {
                                    let id = id.clone();
                                    let alt = alt_input();
                                    editing_alt.set(None);
                                    #[cfg(target_arch = "wasm32")]
                                    spawn(async move {
                                        match update_asset_alt(id, alt).await {
                                            Ok(resp) => {
                                                show_op(resp.message);
                                                if resp.success {
                                                    reload.set(reload() + 1);
                                                }
                                            }
                                            Err(e) => show_op(format!("保存失败：{e}")),
                                        }
                                    });
                                }
                            },
                            "存"
                        }
                        button {
                            class: "text-[10px] cursor-pointer px-2 py-1 rounded-full border border-[var(--color-paper-border)] text-[var(--color-paper-secondary)] hover:text-[var(--color-paper-primary)] transition-colors",
                            onclick: move |_| editing_alt.set(None),
                            "×"
                        }
                    }
                } else {
                    div { class: "flex items-center gap-2 mt-2 opacity-0 group-hover:opacity-100 transition-opacity",
                        button {
                            class: "text-[10px] cursor-pointer text-[var(--color-paper-secondary)] hover:text-[var(--color-paper-primary)] transition-colors",
                            title: "复制图片相对路径",
                            onclick: {
                                let url = format!("/uploads/{}", a.path);
                                move |_| {
                                    #[cfg(target_arch = "wasm32")]
                                    if let Some(window) = web_sys::window() {
                                        let _ = window
                                            .navigator()
                                            .clipboard()
                                            .write_text(&url);
                                        show_op(format!("已复制 {url}"));
                                    }
                                }
                            },
                            "复制路径"
                        }
                        button {
                            class: "text-[10px] cursor-pointer text-[var(--color-paper-secondary)] hover:text-[var(--color-paper-primary)] transition-colors",
                            title: "编辑 alt",
                            onclick: {
                                let id = a.id.clone();
                                let current_alt = a.alt.clone().unwrap_or_default();
                                move |_| {
                                    alt_input.set(current_alt.clone());
                                    editing_alt.set(Some(id.clone()));
                                }
                            },
                            "alt"
                        }
                        if asset.ref_count > 0 {
                            {
                                let refs_tip = asset
                                    .refs
                                    .iter()
                                    .map(|r| r.describe())
                                    .collect::<Vec<_>>()
                                    .join("、");
                                rsx! {
                                    span {
                                        class: "text-[10px] text-[var(--color-paper-tertiary)] cursor-not-allowed",
                                        title: "被引用：{refs_tip}",
                                        "删除"
                                    }
                                }
                            }
                        } else {
                            button {
                                class: "text-[10px] cursor-pointer text-red-500/70 hover:text-red-500 transition-colors",
                                onclick: {
                                    let id = a.id.clone();
                                    move |_| confirm_delete.set(Some(id.clone()))
                                },
                                "删除"
                            }
                        }
                    }
                }
            }
        }
    }
}
