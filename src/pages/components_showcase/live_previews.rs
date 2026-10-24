//! 图鉴里的实时预览：直接渲染生产 UI 组件，状态只留在本页，不发任何请求。

use chrono::{DateTime, Utc};
use leptos::ev::{MouseEvent, SubmitEvent};
use leptos::prelude::*;

use super::AlertBoxDemo;
use crate::components::admin_palette::{AdminPaletteView, Item, PostSearch};
use crate::components::forms::{
    FormInput, FormLabel, FormSelect, FormTextarea, NumberInput, TimePicker, ToggleSwitch,
};
use crate::components::palette::{contains_ci, use_search_palette_provider};
use crate::components::ui::{
    use_sheet_close, use_toast, BottomSheet, Breadcrumb, Button, ButtonSize, ButtonVariant,
    Checkbox, Composer, ComposerInput, ComposerSend, ComposerSize, ConfirmDialog, Crumb, Icon,
    IconButton, IconName, ImagePlaceholder, LoadError, Menu, MenuItem, ModalShell, Pagination,
    Popover, Radio, SegmentedControl, SegmentedItem, SegmentedKind, SegmentedSize, SideSheet,
    StatusBadge, StatusDot, TablePager, TagChip, ToastTone, Tooltip, UserAvatar,
};
use crate::models::post::{Post, PostListItem, PostStatus};
use crate::routes::Route;

fn segmented_mode_items() -> Vec<SegmentedItem<String>> {
    vec![
        SegmentedItem::new("light".to_string(), "浅色").icon(IconName::Sun),
        SegmentedItem::new("dark".to_string(), "深色").icon(IconName::Moon),
        SegmentedItem::new("system".to_string(), "跟随系统").icon(IconName::Monitor),
    ]
}

#[component]
pub(super) fn LiveComponentPreview(
    name: String,
    detail: bool,
    #[prop(optional)] checked: Option<RwSignal<bool>>,
    #[prop(optional)] danger: Option<RwSignal<bool>>,
    #[prop(optional)] alert_variant: Option<RwSignal<String>>,
    #[prop(optional)] alert_message: Option<RwSignal<String>>,
) -> impl IntoView {
    let checked = checked.unwrap_or_else(|| RwSignal::new(true));
    let danger = danger.unwrap_or_else(|| RwSignal::new(false));
    let alert_variant = alert_variant.unwrap_or_else(|| RwSignal::new("success".to_string()));
    let alert_message =
        alert_message.unwrap_or_else(|| RwSignal::new("已保存，新的想法正在生长。".to_string()));
    let switch_on = RwSignal::new(true);
    let selected_radio = RwSignal::new("public".to_string());
    let select_value = RwSignal::new("leaf".to_string());
    let time_value = RwSignal::new("08:30".to_string());
    let input_value = RwSignal::new(String::new());
    let textarea_value = RwSignal::new(String::new());
    let tab_value = RwSignal::new("all".to_string());
    let seg_mode = RwSignal::new("light".to_string());
    let page = RwSignal::new(1i32);
    let number_value = RwSignal::new(14i64);
    let clicked = RwSignal::new(false);
    let popover_open = RwSignal::new(false);
    let popover_x = RwSignal::new(550i32);
    let popover_y = RwSignal::new(330i32);
    let modal_open = RwSignal::new(false);
    let modal_closing = RwSignal::new(false);
    let confirm_open = RwSignal::new(false);
    let filter = RwSignal::new(false);

    match name.as_str() {
        "Checkbox" => view! {
            <div class="showcase-preview-stack">
                <label class=move || {
                    if danger.get() { "showcase-check-row danger" } else { "showcase-check-row" }
                }>
                    <Checkbox
                        checked=checked
                        danger=danger
                        onchange=move |value: bool| checked.set(value)
                    />
                    "记住这个小小的选择"
                </label>
                {(!detail)
                    .then(|| {
                        view! {
                            <label class="showcase-check-row">
                                <Checkbox
                                    checked=filter
                                    onchange=move |value: bool| filter.set(value)
                                />
                                "接收新的生长消息"
                            </label>
                            <label class="showcase-check-row danger">
                                <Checkbox checked=true danger=true onchange=move |_: bool| {}/>
                                "确认移除这条记录"
                            </label>
                        }
                    })}
            </div>
        }
        .into_any(),
        "Radio" => view! {
            <div class="showcase-preview-stack" role="radiogroup" aria-label="分享范围">
                {[("public", "公开分享"), ("private", "仅自己可见")]
                    .into_iter()
                    .map(|(value, label)| {
                        view! {
                            <label class="showcase-check-row">
                                <Radio
                                    name="showcase-visibility"
                                    value=value
                                    checked=move || selected_radio.get() == value
                                    onchange=move |value: String| selected_radio.set(value)
                                />
                                {label}
                            </label>
                        }
                    })
                    .collect_view()}
                <label class="showcase-check-row">
                    <Radio
                        name="showcase-visibility"
                        value="unavailable"
                        checked=false
                        disabled=true
                        onchange=move |_: String| {}
                    />
                    "暂不可用"
                </label>
            </div>
        }
        .into_any(),
        "ToggleSwitch" => view! {
            <div class="showcase-preview-stack">
                <div class="showcase-switch-row">
                    <span>"接收更新"</span>
                    <ToggleSwitch
                        checked=switch_on
                        ontoggle=move |()| switch_on.update(|on| *on = !*on)
                    />
                </div>
                <div class="showcase-switch-row">
                    <span>"自动备份"</span>
                    <ToggleSwitch
                        checked=filter
                        ontoggle=move |()| filter.update(|on| *on = !*on)
                    />
                </div>
            </div>
        }
        .into_any(),
        "FormSelect" => view! {
            <div class="showcase-preview-stack">
                <div class="showcase-preview-field">
                    <FormLabel label="素材排序" html_for="showcase-sort"/>
                    <FormSelect
                        id="showcase-sort"
                        value=select_value
                        options=vec![
                            ("leaf".to_string(), "最新上传"),
                            ("design".to_string(), "体积最大"),
                            ("daily".to_string(), "引用最多"),
                            ("name".to_string(), "按文件名"),
                        ]
                        disabled_options=vec![("name".to_string(), "需要先重建索引")]
                        onchange=move |value: String| select_value.set(value)
                    />
                </div>
                <div class="showcase-preview-field">
                    <FormLabel label="主题标签（多于 8 项时可筛选）" html_for="showcase-tag"/>
                    <FormSelect
                        id="showcase-tag"
                        value=tab_value
                        options=vec![
                            ("all".to_string(), "全部标签"),
                            ("rust".to_string(), "Rust"),
                            ("dioxus".to_string(), "Dioxus"),
                            ("pg".to_string(), "PostgreSQL"),
                            ("wasm".to_string(), "WebAssembly"),
                            ("design".to_string(), "设计"),
                            ("arch".to_string(), "架构"),
                            ("note".to_string(), "读书笔记"),
                            ("emu".to_string(), "模拟器"),
                            ("mcp".to_string(), "MCP"),
                        ]
                        onchange=move |value: String| tab_value.set(value)
                    />
                </div>
            </div>
        }
        .into_any(),
        "TimePicker" => view! {
            <TimePicker
                value=time_value
                onchange=move |value: String| time_value.set(value)
            />
        }
        .into_any(),
        "FormInput" => view! {
            <FormInput
                r#type="text"
                placeholder="一个新的开始…"
                value=input_value
                oninput=move |value: String| input_value.set(value)
            />
        }
        .into_any(),
        "FormTextarea" => view! {
            <FormTextarea
                rows=4
                placeholder="写下这篇文章想说的事…"
                value=textarea_value
                oninput=move |value: String| textarea_value.set(value)
            />
        }
        .into_any(),
        "FormLabel" => view! {
            <div class="showcase-preview-field">
                <FormLabel label="文章标题" html_for="showcase-title"/>
                <FormInput
                    id="showcase-title"
                    r#type="text"
                    placeholder="点击标签也能聚焦这里"
                    value=input_value
                    oninput=move |value: String| input_value.set(value)
                />
            </div>
        }
        .into_any(),
        "Button" => view! {
            <div class="flex flex-col gap-4">
                <div class="showcase-preview-row flex-wrap">
                    <Button onclick=move |_: MouseEvent| clicked.set(true)>
                        {move || if clicked.get() { "已发布" } else { "发布文章" }}
                    </Button>
                    <Button variant=ButtonVariant::Secondary icon=IconName::Save>
                        "保存草稿"
                    </Button>
                    <Button variant=ButtonVariant::Outline>"预览"</Button>
                    <Button variant=ButtonVariant::Ghost>"取消"</Button>
                    <Button variant=ButtonVariant::Danger>"永久删除"</Button>
                    <Button variant=ButtonVariant::Secondary loading=true>
                        "处理中"
                    </Button>
                </div>
                <div class="showcase-preview-row flex-wrap items-center">
                    <Button size=ButtonSize::Xs variant=ButtonVariant::Secondary>
                        "XS"
                    </Button>
                    <Button size=ButtonSize::Sm variant=ButtonVariant::Secondary>
                        "SM"
                    </Button>
                    <Button variant=ButtonVariant::Secondary>"MD"</Button>
                    <Button size=ButtonSize::Lg variant=ButtonVariant::Secondary>
                        "LG"
                    </Button>
                    <Button variant=ButtonVariant::DangerSoft icon=IconName::Trash>
                        "清空回收站"
                    </Button>
                </div>
            </div>
        }
        .into_any(),
        "IconButton" => view! {
            <div class="showcase-preview-row">
                <IconButton icon=IconName::Copy label="复制链接"/>
                <IconButton
                    icon=IconName::Search
                    label="搜索"
                    shortcut="⌘K"
                    variant=ButtonVariant::Secondary
                />
                <IconButton
                    icon=IconName::PanelLeft
                    label="收起侧栏"
                    pressed=Signal::derive(move || Some(clicked.get()))
                    onclick=move |_: MouseEvent| clicked.update(|on| *on = !*on)
                />
                <IconButton icon=IconName::Trash label="删除" disabled=true/>
            </div>
        }
        .into_any(),
        "NumberInput" => view! {
            <div class="showcase-preview-field">
                <FormLabel label="日志保留" html_for="showcase-retention"/>
                <NumberInput
                    id="showcase-retention"
                    value=number_value
                    min=1
                    max=90
                    unit="天"
                    hint="超过保留期的日志由后台任务自动清理。".to_string()
                    onchange=move |value: i64| number_value.set(value)
                />
            </div>
        }
        .into_any(),
        "Menu" => view! {
            <div class="flex flex-col items-center gap-3">
                <Menu
                    label="更多操作"
                    items=vec![
                        MenuItem::new("edit", "编辑").icon(IconName::Pencil),
                        MenuItem::new("copy", "复制链接").icon(IconName::Link).shortcut("⌘C"),
                        MenuItem::new("archive", "归档")
                            .icon(IconName::Archive)
                            .disabled("已发布的文章不能归档"),
                        MenuItem::new("trash", "移入回收站")
                            .icon(IconName::Trash)
                            .danger()
                            .separated(),
                    ]
                    on_select=move |id: &'static str| input_value.set(id.to_string())
                />
                {move || {
                    (!input_value.get().is_empty())
                        .then(|| {
                            view! {
                                <p class="text-xs text-fg-3 font-mono">
                                    {move || format!("on_select: {}", input_value.get())}
                                </p>
                            }
                        })
                }}
            </div>
        }
        .into_any(),
        "ToastRegion" => view! { <ToastDemo/> }.into_any(),
        "BottomSheet" => view! { <BottomSheetDemo/> }.into_any(),
        "Breadcrumb" => view! {
            <Breadcrumb items=vec![
                Crumb::link("关于", Route::About),
                Crumb::link("组件图鉴", Route::ComponentShowcase),
                Crumb::current("复选框"),
            ]/>
        }
        .into_any(),
        "SearchPalette" => view! { <SearchPaletteDemo/> }.into_any(),
        "SideSheet" => view! { <SideSheetDemo/> }.into_any(),
        "Composer" => view! { <ComposerDemo/> }.into_any(),
        "ComposerInput" => view! { <ComposerInputDemo/> }.into_any(),
        "ComposerSend" => view! { <ComposerSendDemo/> }.into_any(),
        "AdminPalette" => view! { <AdminPaletteDemo/> }.into_any(),
        "SegmentedControl" => view! {
            <div class="showcase-preview-stack w-full">
                <SegmentedControl
                    items=vec![
                        SegmentedItem::new("all".to_string(), "全部"),
                        SegmentedItem::new("published".to_string(), "已发布"),
                        SegmentedItem::new("draft".to_string(), "草稿"),
                    ]
                    value=tab_value
                    aria_label="文章状态示例"
                    on_change=move |value: String| tab_value.set(value)
                />
                {detail
                    .then(|| {
                        view! {
                            <div class="w-full">
                                <SegmentedControl
                                    items=segmented_mode_items()
                                    value=seg_mode
                                    kind=SegmentedKind::Radio
                                    size=SegmentedSize::Sm
                                    aria_label="外观示例（宽）"
                                    on_change=move |value: String| seg_mode.set(value)
                                />
                            </div>
                            <div style="width: 11rem">
                                <SegmentedControl
                                    items=segmented_mode_items()
                                    value=seg_mode
                                    kind=SegmentedKind::Radio
                                    size=SegmentedSize::Sm
                                    aria_label="外观示例（窄）"
                                    on_change=move |value: String| seg_mode.set(value)
                                />
                            </div>
                        }
                    })}
            </div>
        }
        .into_any(),
        "Pagination" => view! {
            <div class="w-full max-w-md" inert="true">
                <Pagination
                    current_page=2
                    total=80_i64
                    per_page=10
                    prev_route=Route::Home.to_string()
                    next_route=Route::HomePage { page: 3 }.to_string()
                />
            </div>
        }
        .into_any(),
        "TablePager" => view! {
            <div class="admin-panel w-full max-w-md">
                <TablePager
                    page=page
                    total=57_i64
                    per_page=20
                    unit="篇"
                    on_change=move |value: i32| page.set(value)
                />
            </div>
        }
        .into_any(),
        "Tooltip" => view! {
            <div class="showcase-preview-row">
                <Tooltip tip="复制链接">
                    <button class="btn btn-icon btn-secondary" aria-label="复制链接">
                        <Icon name=IconName::Copy/>
                    </button>
                </Tooltip>
                <Tooltip tip="搜索" shortcut="⌘K">
                    <button class="btn btn-icon btn-secondary" aria-label="搜索">
                        <Icon name=IconName::Search/>
                    </button>
                </Tooltip>
                <Tooltip tip="向右旋转" shortcut="R" placement="bottom">
                    <button class="btn btn-icon btn-secondary" aria-label="向右旋转">
                        <Icon name=IconName::RotateCw/>
                    </button>
                </Tooltip>
            </div>
        }
        .into_any(),
        "Popover" => view! {
            <div class="showcase-popover-example">
                <button
                    class="showcase-demo-button secondary"
                    on:click=move |event: MouseEvent| {
                        event.stop_propagation();
                        popover_x.set(event.client_x());
                        popover_y.set(event.client_y() - event.offset_y());
                        popover_open.update(|open| *open = !*open);
                    }
                >
                    "试试浮层 ↗"
                </button>
                <Popover
                    open=popover_open
                    anchor_x=popover_x
                    anchor_y=popover_y
                    placement="top"
                    align="center"
                    on_close=move |()| popover_open.set(false)
                >
                    <div class="showcase-popover-content">
                        <p>"要保留这片小小的灵感吗？"</p>
                        <div class="showcase-popover-actions">
                            <button
                                class="showcase-demo-button secondary"
                                on:click=move |_| popover_open.set(false)
                            >
                                "取消"
                            </button>
                            <button
                                class="showcase-demo-button"
                                on:click=move |_| popover_open.set(false)
                            >
                                "保留"
                            </button>
                        </div>
                    </div>
                </Popover>
            </div>
        }
        .into_any(),
        "ModalShell" => view! {
            <div>
                <button
                    class="showcase-demo-button secondary"
                    on:click=move |_| modal_open.set(true)
                >
                    "打开一扇小窗 ↗"
                </button>
                <ModalShell visible=modal_open closing=modal_closing title="留住一片灵感">
                    <div class="showcase-demo-modal-content">
                        <h2>"留住一片灵感。"</h2>
                        <p>"让内容拥有一点专注的空间。"</p>
                        <button
                            class="showcase-demo-button"
                            on:click=move |_| {
                                modal_closing.set(true);
                                modal_open.set(false);
                            }
                        >
                            "收好了"
                        </button>
                    </div>
                </ModalShell>
            </div>
        }
        .into_any(),
        "ConfirmDialog" => view! {
            <div>
                <button
                    class="showcase-demo-button secondary"
                    on:click=move |_| confirm_open.set(true)
                >
                    "清空回收站"
                </button>
                <ConfirmDialog
                    visible=confirm_open
                    title="清空回收站？"
                    description="3 篇文章会被彻底删除，无法恢复。被这些文章引用的素材会变为未引用。"
                    confirm_label="彻底删除"
                    on_confirm=move |()| {}
                />
            </div>
        }
        .into_any(),
        "AlertBox" => view! {
            <AlertBoxDemo detail=detail alert_variant=alert_variant alert_message=alert_message/>
        }
        .into_any(),
        "StatusBadge" => view! {
            <div class="showcase-preview-row">
                <StatusBadge color_class="badge-success" label="已发布"/>
                <StatusBadge color_class="" label="草稿"/>
                <StatusBadge color_class="badge-warning" label="待审核"/>
                <StatusBadge color_class="badge-danger" label="3 天后清除"/>
                <StatusBadge color_class="badge-clay" label="新"/>
            </div>
        }
        .into_any(),
        "StatusDot" => view! {
            <div class="showcase-preview-row">
                <StatusDot label="已发布" tone="clay"/>
                <StatusDot label="草稿"/>
                <StatusDot label="Docker   沙箱" tone="success"/>
                <StatusDot label="即将过期" tone="warning"/>
                <StatusDot label="已吊销" tone="danger"/>
            </div>
        }
        .into_any(),
        "TagChip" => view! {
            <div class="showcase-preview-row">
                <TagChip
                    label="设计"
                    to=Route::TagDetail { tag: "设计".to_string() }.to_string()
                    variant="outline"
                    count=12
                />
                <TagChip
                    label="灵感"
                    to=Route::TagDetail { tag: "灵感".to_string() }.to_string()
                    variant="solid"
                />
            </div>
        }
        .into_any(),
        "UserAvatar" => {
            let no_avatar: Option<String> = None;
            view! {
            <div class="showcase-preview-row">
                <UserAvatar
                    name="叶子"
                    avatar_url=no_avatar
                    class="w-14 h-14 rounded-full text-xl"
                />
                <UserAvatar
                    name="Yggdrasil"
                    avatar_url=Some("/missing-avatar-for-showcase.png".to_string())
                    class="w-10 h-10 rounded-full text-sm"
                />
            </div>
            }
            .into_any()
        }
        "ImagePlaceholder" => view! { <ImagePlaceholder class="w-10 h-10 text-fg-3"/> }.into_any(),
        "Icon" => view! {
            <div class="grid grid-cols-8 gap-3 text-fg-2">
                {IconName::ALL
                    .into_iter()
                    .map(|icon| {
                        view! {
                            <span class="grid place-items-center" title=icon.slug()>
                                <Icon name=icon size=18/>
                            </span>
                        }
                    })
                    .collect_view()}
            </div>
        }
        .into_any(),
        "EmptyState" => view! {
            <div class="showcase-empty-preview">
                <crate::components::empty_state::EmptyState
                    title="这里，等待第一颗种子。"
                    description="写下一个新的开始。"
                />
            </div>
        }
        .into_any(),
        "LoadError" => view! {
            <div class="w-full max-w-xl">
                <LoadError
                    title="文章暂时没能加载"
                    retrying=clicked
                    on_retry=move |()| clicked.set(true)
                />
            </div>
        }
        .into_any(),
        _ => panic!("基础组件缺少预览: {name}"),
    }
}

/// 图鉴示例用的固定文章，保证预览不随真实时间变化。
pub(super) fn sample_post() -> Post {
    let created_at = DateTime::parse_from_rfc3339("2026-09-23T08:00:00Z")
        .expect("固定的图鉴示例日期必须有效")
        .with_timezone(&Utc);
    Post {
        id: 0,
        author_id: 0,
        title: "让想法生根".to_string(),
        slug: "showcase-example".to_string(),
        summary: Some("在时间的缝隙里，种下一些文字。".to_string()),
        content_md: "一片叶子。".to_string(),
        content_html: Some("<p>一片叶子，也记得来时的风。</p>".to_string()),
        status: PostStatus::Published,
        published_at: Some(created_at),
        created_at,
        updated_at: created_at,
        deleted_at: None,
        tags: vec!["设计".to_string()],
        cover_image: None,
        reading_time: 3,
        word_count: 620,
        toc_html: None,
        prev_post: None,
        next_post: None,
    }
}

/// 不请求服务、不修改数据的业务组件使用固定样例资料展示真实结构。
#[component]
pub(super) fn SceneComponentPreview(name: String) -> impl IntoView {
    let cover_preview = name == "PostCover";
    Effect::new(move |_| {
        #[cfg(target_arch = "wasm32")]
        if cover_preview {
            if let Some(window) = web_sys::window() {
                let selectors = js_sys::Array::of1(&".showcase-cover-window".into());
                crate::utils::js::invoke_optional_global(
                    &window,
                    "__initLightbox",
                    &[selectors.into()],
                );
            }
        }
    });
    let inner = match name.as_str() {
        "PostCover" => view! {
            <crate::components::post::post_cover::PostCover src="/images/samples/clay-circle.webp"/>
        }
        .into_any(),
        "SearchIconLink" => view! { <crate::components::header::SearchIconLink/> }.into_any(),
        _ => panic!("场景组件缺少预览: {name}"),
    };
    view! {
        <div
            class=if cover_preview {
                "showcase-scene-window showcase-cover-window"
            } else {
                "showcase-scene-window"
            }
            aria-label=format!("{name} 的静态样例")
        >
            <div class="showcase-skeleton-window-bar">
                <span></span>
                <span></span>
                <span></span>
            </div>
            <div class="showcase-scene-canvas" inert="true">
                {inner}
            </div>
        </div>
    }
}

/// 搜索面板演示：自带句柄与面板实例，不注册全局快捷键，不影响页面上的正式面板。
#[component]
fn SearchPaletteDemo() -> impl IntoView {
    let palette = use_search_palette_provider();
    view! {
        <div class="showcase-preview-row">
            <Button
                variant=ButtonVariant::Secondary
                icon=IconName::Search
                onclick=move |_: MouseEvent| palette.open()
            >
                "打开搜索面板"
            </Button>
        </div>
        <crate::components::search_palette::SearchPalette shortcut=false/>
    }
}

/// 提示演示：三种语气各发一条，走正式的全站提示区。
#[component]
fn ToastDemo() -> impl IntoView {
    let toast = use_toast();
    view! {
        <div class="showcase-preview-row flex-wrap">
            <Button
                variant=ButtonVariant::Secondary
                onclick=move |_: MouseEvent| toast.show(ToastTone::Info, "草稿已自动保存")
            >
                "说明"
            </Button>
            <Button
                variant=ButtonVariant::Secondary
                onclick=move |_: MouseEvent| toast.success("设置已保存")
            >
                "成功"
            </Button>
            <Button
                variant=ButtonVariant::Secondary
                onclick=move |_: MouseEvent| toast.error("保存失败：网络连接中断，请稍后重试")
            >
                "错误"
            </Button>
        </div>
    }
}

/// 图鉴里的抽屉只放演示用的按钮，选中后关闭，不跳转。
#[component]
fn BottomSheetDemo() -> impl IntoView {
    let open = RwSignal::new(false);
    view! {
        <button
            id="showcase-sheet-trigger"
            class="btn btn-secondary"
            type="button"
            aria-haspopup="dialog"
            aria-expanded=move || open.get().to_string()
            aria-controls="showcase-bottom-sheet"
            on:click=move |_| open.set(true)
        >
            "打开抽屉"
        </button>
        <BottomSheet
            open=open
            id="showcase-bottom-sheet"
            trigger_id="showcase-sheet-trigger"
            label="演示抽屉"
        >
            <BottomSheetDemoBody/>
        </BottomSheet>
    }
}

/// 左侧抽屉演示：内容与底部抽屉演示相同，选中后关闭。
#[component]
fn SideSheetDemo() -> impl IntoView {
    let open = RwSignal::new(false);
    view! {
        <button
            id="showcase-side-sheet-trigger"
            class="btn btn-secondary"
            type="button"
            aria-haspopup="dialog"
            aria-expanded=move || open.get().to_string()
            aria-controls="showcase-side-sheet"
            on:click=move |_| open.set(true)
        >
            "打开左侧抽屉"
        </button>
        <SideSheet
            open=open
            id="showcase-side-sheet"
            trigger_id="showcase-side-sheet-trigger"
            label="演示抽屉"
        >
            <div class="p-3">
                <BottomSheetDemoBody/>
            </div>
        </SideSheet>
    }
}

/// 输入栏演示：单行搜索式，提交后只在下方回显，不请求、不跳转。
#[component]
fn ComposerDemo() -> impl IntoView {
    let query = RwSignal::new(String::new());
    let submitted = RwSignal::new(String::new());
    view! {
        <div class="showcase-preview-stack w-full max-w-md">
            <form
                role="search"
                on:submit=move |event: SubmitEvent| {
                    event.prevent_default();
                    submitted.set(query.get_untracked().trim().to_string());
                }
            >
                <Composer>
                    <Icon name=IconName::Search size=18/>
                    <ComposerInput
                        search=true
                        aria_label="输入栏示例"
                        placeholder="搜索文章、标签…"
                        value=query
                        oninput=move |value: String| query.set(value)
                    />
                    <ComposerSend label="搜索"/>
                </Composer>
            </form>
            {move || {
                (!submitted.get().is_empty())
                    .then(|| {
                        view! {
                            <p class="text-sm text-fg-2" role="status">
                                {move || format!("已提交：{}", submitted.get())}
                            </p>
                        }
                    })
            }}
        </div>
    }
}

/// 输入栏输入框演示：多行输入，⌘/Ctrl+Enter 把内容追加到下方的本地列表。
#[component]
fn ComposerInputDemo() -> impl IntoView {
    let text = RwSignal::new(String::new());
    let entries = RwSignal::new(Vec::<String>::new());
    view! {
        <div class="showcase-preview-stack w-full max-w-md">
            <Composer size=ComposerSize::Sm>
                <ComposerInput
                    multiline=true
                    aria_label="多行输入示例"
                    placeholder="写点什么…（⌘↵ 提交）"
                    value=text
                    oninput=move |value: String| text.set(value)
                    on_submit=move |()| {
                        let entry = text.get_untracked().trim().to_string();
                        if !entry.is_empty() {
                            entries.update(|list| list.push(entry));
                            text.set(String::new());
                        }
                    }
                />
            </Composer>
            <ul class="flex flex-col gap-1 text-sm text-fg-2" aria-label="已提交内容">
                {move || {
                    entries
                        .get()
                        .into_iter()
                        .map(|entry| view! { <li>{entry}</li> })
                        .collect_view()
                }}
            </ul>
        </div>
    }
}

/// 发送键演示：可用与不可用并排，点击只在本地计数。
#[component]
fn ComposerSendDemo() -> impl IntoView {
    let sent = RwSignal::new(0u32);
    view! {
        <div class="showcase-preview-stack">
            <div class="showcase-preview-row">
                <ComposerSend
                    label="发送"
                    onclick=move |_: MouseEvent| sent.update(|count| *count += 1)
                />
                <ComposerSend label="发送（不可用）" disabled=true/>
            </div>
            {move || {
                (sent.get() > 0)
                    .then(|| {
                        view! {
                            <p class="text-sm text-fg-2" role="status">
                                {move || format!("已点击 {} 次", sent.get())}
                            </p>
                        }
                    })
            }}
        </div>
    }
}

/// 后台命令面板演示：固定文章在浏览器内过滤，选中项只在下方回显，不请求、不跳转。
#[component]
fn AdminPaletteDemo() -> impl IntoView {
    let palette = use_search_palette_provider();
    let picked = RwSignal::new(String::new());
    let created_at = sample_post().created_at;
    let posts: Vec<PostListItem> = [
        (1, "让想法生根", PostStatus::Published),
        (2, "Dioxus 服务端渲染的水合细节", PostStatus::Draft),
        (3, "用 Rust 写一个 6502 模拟器", PostStatus::Published),
    ]
    .into_iter()
    .map(|(id, title, status)| PostListItem {
        id,
        author_id: 0,
        title: title.to_string(),
        slug: format!("showcase-{id}"),
        summary: None,
        published_at: (status == PostStatus::Published).then_some(created_at),
        status,
        created_at,
        updated_at: created_at,
        deleted_at: None,
        tags: Vec::new(),
        cover_image: None,
        reading_time: 3,
        word_count: 620,
    })
    .collect();
    let search = move |query: String| -> PostSearch {
        let hits = posts
            .iter()
            .filter(|post| contains_ci(&post.title, &query))
            .cloned()
            .collect();
        Box::pin(std::future::ready(Some(hits)))
    };
    view! {
        <div class="showcase-preview-row flex-wrap">
            <Button
                variant=ButtonVariant::Secondary
                icon=IconName::Search
                onclick=move |_: MouseEvent| palette.open()
            >
                "打开命令面板"
            </Button>
            {move || {
                (!picked.get().is_empty())
                    .then(|| {
                        view! {
                            <p class="text-xs text-fg-3 font-mono">
                                {move || format!("on_activate: {}", picked.get())}
                            </p>
                        }
                    })
            }}
        </div>
        <AdminPaletteView
            search=search
            on_activate=move |item: Item| picked.set(item.title())
        />
    }
}

#[component]
fn BottomSheetDemoBody() -> impl IntoView {
    let sheet = use_sheet_close();
    view! {
        <ul class="flex flex-col gap-0.5">
            {["设计系统", "Dioxus 实践", "Rust 异步"]
                .into_iter()
                .map(|label| {
                    view! {
                        <li>
                            <button
                                class="sheet-nav-item"
                                type="button"
                                on:click=move |_| sheet.close()
                            >
                                {label}
                            </button>
                        </li>
                    }
                })
                .collect_view()}
        </ul>
    }
}
