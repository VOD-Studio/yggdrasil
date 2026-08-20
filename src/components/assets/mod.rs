//! 素材相关组件模块
//!
//! 提供素材选择 modal（封面 / 头像 / 友链图标「从素材库选择」）与
//! 素材上传 modal（素材管理页内上传）。

/// 素材选择 modal（封面上「从素材库选择」）。
pub mod asset_picker;
/// 素材上传 modal（素材管理页内上传）。
pub mod asset_upload;

pub use asset_picker::{AssetPickerModal, AssetSelection};
pub use asset_upload::AssetUploadModal;
