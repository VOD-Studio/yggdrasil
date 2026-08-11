use serde::{Deserialize, Serialize};
#[cfg(feature = "server")]
use std::env;
#[cfg(feature = "server")]
use std::sync::LazyLock;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
pub struct ResourceLimits {
    pub cpu_cores: f64,
    pub memory_mb: u64,
    pub timeout_secs: u64,
    pub output_bytes: u64,
    pub allow_network: bool,
}

#[cfg(feature = "server")]
pub struct RunnerConfig {
    pub max_cpu_cores: f64,
    pub max_memory_mb: u64,
    pub max_timeout_secs: u64,
    pub max_output_bytes: u64,
    pub max_source_bytes: u64,
    pub allow_network: bool,
    pub max_concurrent: usize,
    pub queue_timeout_secs: u64,
    pub task_ttl_secs: u64,
    pub docker_socket_path: String,
    /// 语言白名单。`None` 表示不限制——注册表里的所有语言均视为支持；
    /// `Some(list)` 表示收窄到列表内（仍须同时在 LANGUAGES 注册表存在）。
    pub languages: Option<Vec<String>>,
}

#[cfg(all(test, feature = "server"))]
fn parse_allow_network(v: &str) -> bool {
    let l = v.to_lowercase();
    l == "true" || l == "1" || l == "yes"
}

#[cfg(feature = "server")]
pub static RUNNER_CONFIG: LazyLock<RunnerConfig> = LazyLock::new(|| {
    let cfg = crate::config::runner();
    RunnerConfig {
        max_cpu_cores: cfg.max_cpu_cores,
        max_memory_mb: cfg.max_memory_mb as u64,
        max_timeout_secs: cfg.max_timeout_secs as u64,
        max_output_bytes: cfg.max_output_bytes,
        max_source_bytes: cfg.max_source_bytes,
        allow_network: cfg.allow_network,
        max_concurrent: cfg.max_concurrent as usize,
        queue_timeout_secs: cfg.queue_timeout_secs as u64,
        task_ttl_secs: cfg.task_ttl_secs as u64,
        docker_socket_path: env::var("DOCKER_SOCKET_PATH")
            .unwrap_or_else(|_| "/var/run/docker.sock".to_string()),
        // languages 是逗号分隔字符串，拆成 Vec<String>。
        languages: cfg
            .languages
            .map(|s| s.split(',').map(|t| t.trim().to_lowercase()).filter(|t| !t.is_empty()).collect()),
    }
});

#[cfg(feature = "server")]
pub fn clamp_limits(merged: ResourceLimits, lang_allows_network: bool) -> ResourceLimits {
    clamp_limits_impl(merged, lang_allows_network, &RUNNER_CONFIG)
}

#[cfg(feature = "server")]
fn clamp_limits_impl(
    merged: ResourceLimits,
    lang_allows_network: bool,
    config: &RunnerConfig,
) -> ResourceLimits {
    let max_cpu = if config.max_cpu_cores.is_nan() {
        2.0
    } else {
        config.max_cpu_cores
    };
    let min_cpu = 0.1f64.min(max_cpu);
    let cpu_cores = if merged.cpu_cores.is_nan() {
        min_cpu
    } else {
        merged.cpu_cores.clamp(min_cpu, max_cpu)
    };

    let min_mem = 16.min(config.max_memory_mb);
    let memory_mb = merged.memory_mb.clamp(min_mem, config.max_memory_mb);

    let min_timeout = 1.min(config.max_timeout_secs);
    let timeout_secs = merged
        .timeout_secs
        .clamp(min_timeout, config.max_timeout_secs);

    ResourceLimits {
        cpu_cores,
        memory_mb,
        timeout_secs,
        output_bytes: merged.output_bytes.min(config.max_output_bytes),
        allow_network: merged.allow_network && config.allow_network && lang_allows_network,
    }
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    #[test]
    fn test_clamp_limits() {
        let raw = ResourceLimits {
            cpu_cores: 5.0,
            memory_mb: 4096,
            timeout_secs: 120,
            output_bytes: 9999999,
            allow_network: true,
        };
        let clamped = clamp_limits(raw, true);
        assert!(clamped.cpu_cores <= 2.0);
        assert!(clamped.memory_mb <= 1024);
        assert!(clamped.timeout_secs <= 30);
        assert!(clamped.output_bytes <= 1048576);
        assert!(!clamped.allow_network);
    }

    #[test]
    fn test_clamp_limits_safeguarded() {
        let config = RunnerConfig {
            max_cpu_cores: 0.05,
            max_memory_mb: 8,
            max_timeout_secs: 0,
            max_output_bytes: 100,
            max_source_bytes: 100,
            allow_network: true,
            max_concurrent: 1,
            queue_timeout_secs: 1,
            task_ttl_secs: 1,
            docker_socket_path: "".to_string(),
            languages: None,
        };
        let raw = ResourceLimits {
            cpu_cores: 1.0,
            memory_mb: 64,
            timeout_secs: 10,
            output_bytes: 50,
            allow_network: true,
        };
        let clamped = clamp_limits_impl(raw, true, &config);
        assert_eq!(clamped.cpu_cores, 0.05);
        assert_eq!(clamped.memory_mb, 8);
        assert_eq!(clamped.timeout_secs, 0);
        assert_eq!(clamped.output_bytes, 50);
        assert!(clamped.allow_network);
    }

    #[test]
    fn test_parse_allow_network() {
        assert!(parse_allow_network("true"));
        assert!(parse_allow_network("TRUE"));
        assert!(parse_allow_network("True"));
        assert!(parse_allow_network("1"));
        assert!(parse_allow_network("yes"));
        assert!(parse_allow_network("YES"));
        assert!(parse_allow_network("Yes"));
        assert!(!parse_allow_network("false"));
        assert!(!parse_allow_network("0"));
        assert!(!parse_allow_network("no"));
    }
}
