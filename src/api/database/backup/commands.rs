//! PostgreSQL 命令构造：通过 PG 环境变量传递连接，备份去除属主和 ACL。

use std::path::Path;

/// 子进程不接收完整数据库 URI；连接参数拆入 PG 环境变量。
fn pg_command_with_db_config(
    program: &str,
    db_url: &str,
) -> std::io::Result<std::process::Command> {
    use std::str::FromStr;
    use tokio_postgres::config::{Host, SslMode};

    let config = tokio_postgres::Config::from_str(db_url).map_err(|error| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("DATABASE_URL 解析失败: {error}"),
        )
    })?;
    let mut command = std::process::Command::new(program);
    // pg_dump/psql 不读取 DATABASE_URL；移除继承的敏感环境变量，只保留
    // 拆出的 PG* 连接参数，避免子进程环境再次暴露完整 URI。
    command.env_remove("DATABASE_URL");

    let hosts: Vec<String> = config
        .get_hosts()
        .iter()
        .map(|host| match host {
            Host::Tcp(host) => host.clone(),
            Host::Unix(path) => path.to_string_lossy().into_owned(),
        })
        .collect();
    if !hosts.is_empty() {
        command.env("PGHOST", hosts.join(","));
    }
    if !config.get_hostaddrs().is_empty() {
        command.env(
            "PGHOSTADDR",
            config
                .get_hostaddrs()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(","),
        );
    }
    if !config.get_ports().is_empty() {
        command.env(
            "PGPORT",
            config
                .get_ports()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(","),
        );
    }
    if let Some(user) = config.get_user() {
        command.env("PGUSER", user);
    }
    if let Some(password) = config.get_password() {
        command.env("PGPASSWORD", String::from_utf8_lossy(password).into_owned());
    }
    if let Some(dbname) = config.get_dbname() {
        command.env("PGDATABASE", dbname);
    }
    if let Some(options) = config.get_options() {
        command.env("PGOPTIONS", options);
    }
    if let Some(application_name) = config.get_application_name() {
        command.env("PGAPPNAME", application_name);
    }
    let ssl_mode = match config.get_ssl_mode() {
        SslMode::Disable => "disable",
        SslMode::Prefer => "prefer",
        SslMode::Require => "require",
        _ => "prefer",
    };
    command.env("PGSSLMODE", ssl_mode);
    if let Some(timeout) = config.get_connect_timeout() {
        command.env("PGCONNECT_TIMEOUT", timeout.as_secs().max(1).to_string());
    }
    Ok(command)
}

pub(super) fn pg_dump_command(db_url: &str) -> std::io::Result<std::process::Command> {
    let mut command = pg_command_with_db_config("pg_dump", db_url)?;
    command.args(["--clean", "--if-exists", "--no-owner", "--no-privileges"]);
    Ok(command)
}
pub(super) fn psql_restore_command(
    db_url: &str,
    path: &Path,
) -> std::io::Result<std::process::Command> {
    let mut command = pg_command_with_db_config("psql", db_url)?;
    command
        .args(["--single-transaction", "-v", "ON_ERROR_STOP=1", "-f"])
        .arg(path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped());
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pg_dump_is_owner_acl_neutral_without_db_url_in_args() {
        let db_url = "postgres://user:secret@example.com:5432/dbname";
        let command = pg_dump_command(db_url).expect("valid database URL");
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert!(args.iter().any(|arg| arg == "--no-owner"), "{args:?}");
        assert!(args.iter().any(|arg| arg == "--no-privileges"), "{args:?}");
        assert!(!args.iter().any(|arg| arg.contains("secret")), "{args:?}");
        assert!(command
            .get_envs()
            .any(|(key, value)| key == "PGPASSWORD" && value.is_some()));
    }

    #[test]
    fn psql_restore_is_atomic_without_db_url_in_args() {
        let db_url = "postgres://user:secret@example.com:5432/dbname";
        let command =
            psql_restore_command(db_url, Path::new("backup.sql")).expect("valid database URL");
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert!(
            args.iter().any(|arg| arg == "--single-transaction"),
            "{args:?}"
        );
        assert!(!args.iter().any(|arg| arg.contains("secret")), "{args:?}");
    }
}
