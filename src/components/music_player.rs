//! 仅前台布局挂载的音乐播放器；页面路由与主题变化不重建播放核心。

use crate::api::music_player::get_music_player_config;
use crate::bridges::library::{use_browser_library, LibraryLoadError};
use dioxus::prelude::*;

#[component]
pub(crate) fn MusicPlayer() -> Element {
    let config_resource = use_server_future(get_music_player_config)?;
    let config = use_memo(move || {
        config_resource
            .read()
            .as_ref()
            .and_then(|result| result.as_ref().ok())
            .cloned()
            .flatten()
    });
    let library = use_browser_library("music-player", move || config().is_some());
    #[allow(unused_mut)]
    let mut init_error = use_signal(|| None::<String>);

    #[cfg(target_arch = "wasm32")]
    {
        use crate::bridges::library::library_ready;
        use crate::bridges::music_player::PlayerHandle;

        let mut handle = use_signal(|| None::<PlayerHandle>);
        use_drop(move || handle.set(None));
        use_effect(move || {
            let Some(config) = config() else {
                return;
            };
            if !library_ready(library) || init_error().is_some() || handle.peek().is_some() {
                return;
            }
            match PlayerHandle::create("frontend-music-player", &config) {
                Ok(instance) => handle.set(Some(instance)),
                Err(error) => {
                    web_sys::console::error_1(&error);
                    init_error.set(Some("音乐播放器初始化失败，请检查服务配置或重试。".into()));
                }
            }
        });
    }

    rsx! {
        if config().is_some() {
            div { id: "frontend-music-player",
                LibraryLoadError { library }
                if let Some(message) = init_error() {
                    div { role: "alert", class: "flex items-center gap-3 p-3 text-sm text-red-500 dark:text-red-400",
                        span { "{message}" }
                        button {
                            r#type: "button",
                            class: "underline underline-offset-4",
                            onclick: move |_| init_error.set(None),
                            "重新加载"
                        }
                    }
                }
            }
        }
    }
}
