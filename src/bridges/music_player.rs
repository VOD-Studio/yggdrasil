//! 前台全局音乐播放器的布局级句柄。

#[cfg(target_arch = "wasm32")]
mod wasm {
    use crate::models::music_player::MusicPlayerConfig;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen::JsCast;

    #[wasm_bindgen]
    extern "C" {
        type MusicPlayerModule;

        #[wasm_bindgen(method, catch)]
        fn create(
            this: &MusicPlayerModule,
            container_id: &str,
            config: &JsValue,
        ) -> Result<Option<PlayerInstance>, JsValue>;

        type PlayerInstance;

        #[wasm_bindgen(method)]
        fn destroy(this: &PlayerInstance);
    }

    pub struct PlayerHandle(PlayerInstance);

    impl PlayerHandle {
        /// 调用前必须等待 music-player 浏览器库加载完成。
        pub fn create(container_id: &str, config: &MusicPlayerConfig) -> Result<Self, JsValue> {
            let window = web_sys::window().ok_or_else(|| JsValue::from_str("浏览器窗口不可用"))?;
            let module: MusicPlayerModule =
                js_sys::Reflect::get(&window, &"YggdrasilMusicPlayer".into())?.unchecked_into();
            let config = serde_wasm_bindgen::to_value(config)
                .map_err(|error| JsValue::from_str(&error.to_string()))?;
            module
                .create(container_id, &config)?
                .map(Self)
                .ok_or_else(|| JsValue::from_str("音乐播放器容器未就绪"))
        }
    }

    impl Drop for PlayerHandle {
        fn drop(&mut self) {
            self.0.destroy();
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;
