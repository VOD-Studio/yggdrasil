//! 启动期代码运行器就绪探测（server-only）。
//!
//! 博客本身不依赖 Docker——代码运行是可选功能。本模块在启动后台 best-effort 探测
//! Docker daemon 是否可达、已启用的运行器镜像是否已构建，缺失时打印可操作日志，
//! **不阻塞启动、不 exit**。让 issue #20 那类「socket 其实通了但镜像没构建」的失败
//! 从完全不可见变得可见且可操作。
//!
//! 不自动构建镜像、不新增运行时依赖（用户已选「诊断 + 改善错误信息」）。

#![cfg(feature = "server")]

use std::collections::HashSet;

use bollard::query_parameters::ListImagesOptions;

use crate::api::code_runner::languages::{is_supported_lang, LANGUAGES};
use crate::infra::docker::DOCKER_CLIENT;

/// 启动后台调用：探测 Docker daemon 与已启用运行器镜像的就绪度，缺失时打日志。
///
/// 全程 best-effort：任何一步失败只记日志后 return，绝不 panic、不阻塞启动。
/// 尊重 `CODE_RUNNER_LANGUAGES` 白名单——只探测 [`is_supported_lang`] 为真的语言，
/// 不为运维收窄掉的镜像报「缺失」。
pub async fn log_runner_readiness() {
    let docker = match DOCKER_CLIENT.as_ref() {
        Some(d) => d,
        None => {
            tracing::warn!(
                "代码运行器已禁用：Docker daemon 未连接。请启动 Docker 并检查 socket 的访问权限，\
                 自定义路径可通过 DOCKER_SOCKET_PATH 设置"
            );
            return;
        }
    };

    // 探活 daemon：version 仅用于判断「daemon 是否响应」，失败即 warn 后 return。
    if let Err(e) = docker.version().await {
        tracing::warn!("Docker daemon 可达性检查失败：{e}，代码运行可能不可用");
        return;
    }

    // 列举本机镜像 tag 集合（repo_tags 是 Vec<String>，可能含多个 tag）。
    let images = match docker.list_images(None::<ListImagesOptions>).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("Docker 镜像列表查询失败：{e}，跳过运行器镜像就绪检查");
            return;
        }
    };
    let tags: HashSet<String> = images
        .iter()
        .flat_map(|i| i.repo_tags.iter().cloned())
        .collect();

    // 仅探测已启用（命中白名单）的语言镜像，避免为运维收窄掉的语言报缺失。
    let registered: Vec<(&String, &str)> = LANGUAGES
        .iter()
        .filter(|(k, _)| is_supported_lang(k))
        .map(|(k, def)| (k, def.image.as_str()))
        .collect();

    if registered.is_empty() {
        tracing::warn!("无已注册运行器语言");
        return;
    }

    let missing: Vec<&str> = registered
        .iter()
        .filter_map(|(_, img)| {
            if tags.contains(*img) {
                None
            } else {
                Some(*img)
            }
        })
        .collect();

    let lang_names: Vec<&str> = registered.iter().map(|(k, _)| k.as_str()).collect();

    if missing.is_empty() {
        tracing::info!(
            "代码运行器就绪：{}/{} 镜像已构建（{}）",
            registered.len(),
            registered.len(),
            lang_names.join("/")
        );
    } else {
        tracing::warn!(
            "代码运行器镜像未构建：{}。请在宿主执行 `bash docker/build-runners.sh` 构建后再使用代码运行功能",
            missing.join("、")
        );
    }
}
