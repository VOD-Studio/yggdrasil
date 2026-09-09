// 与 posts 模块一致：Dioxus `#[server]` 宏触发 deprecated/unit/too_many_arguments
// 提示，按项目惯例放行（限流/运行器等配置项天然参数多）。
#![allow(clippy::unused_unit, deprecated, clippy::too_many_arguments)]

use dioxus::prelude::*;

#[cfg(feature = "server")]
use crate::api::auth::get_current_admin_user;
#[cfg(feature = "server")]
use crate::api::error::AppError;
#[cfg(feature = "server")]
use crate::db::pool::get_conn;
use crate::models::settings::TrashSettings;

/// Web 与 MCP 共用的回收站配置读取；缺失或非法值回退默认值。
#[cfg(feature = "server")]
pub(crate) async fn load_trash_settings(
    client: &tokio_postgres::Client,
) -> Result<TrashSettings, AppError> {
    let enabled: bool = client
        .query_opt(
            "SELECT value FROM settings WHERE key = 'trash_auto_purge_enabled'",
            &[],
        )
        .await
        .map_err(AppError::query)?
        .and_then(|r| r.get::<_, String>("value").parse().ok())
        .unwrap_or(crate::models::settings::DEFAULT_AUTO_PURGE_ENABLED);

    let days: i32 = client
        .query_opt(
            "SELECT value FROM settings WHERE key = 'trash_retention_days'",
            &[],
        )
        .await
        .map_err(AppError::query)?
        .and_then(|r| r.get::<_, String>("value").parse().ok())
        .unwrap_or(crate::models::settings::DEFAULT_RETENTION_DAYS);

    Ok(TrashSettings {
        auto_purge_enabled: enabled,
        retention_days: TrashSettings::clamp_retention(days),
    })
}

/// Web 与 MCP 共用的配置写入；保留天数统一钳制后写入并返回。
#[cfg(feature = "server")]
pub(crate) async fn save_trash_settings(
    client: &tokio_postgres::Client,
    auto_purge_enabled: bool,
    retention_days: i32,
) -> Result<TrashSettings, AppError> {
    let retention_days = TrashSettings::clamp_retention(retention_days);

    client
        .execute(
            "INSERT INTO settings (key, value, updated_at) VALUES ('trash_auto_purge_enabled', $1, NOW())
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()",
            &[&auto_purge_enabled.to_string()],
        )
        .await
        .map_err(AppError::query)?;

    client
        .execute(
            "INSERT INTO settings (key, value, updated_at) VALUES ('trash_retention_days', $1, NOW())
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()",
            &[&retention_days.to_string()],
        )
        .await
        .map_err(AppError::query)?;

    Ok(TrashSettings {
        auto_purge_enabled,
        retention_days,
    })
}

/// 读取回收站配置。
///
/// settings 表缺失键时回退到默认值，保证向后兼容。
#[server(GetTrashSettings, "/api")]
pub async fn get_trash_settings() -> Result<TrashSettings, ServerFnError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;
        Ok(load_trash_settings(&client).await?)
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(TrashSettings::default())
    }
}

/// 更新回收站配置。
///
/// retention_days 会被 clamp 到合法范围后写入。
#[server(UpdateTrashSettings, "/api")]
pub async fn update_trash_settings(
    auto_purge_enabled: bool,
    retention_days: i32,
) -> Result<TrashSettings, ServerFnError> {
    let _user = get_current_admin_user().await?;

    #[cfg(feature = "server")]
    {
        let client = get_conn().await.map_err(AppError::db_conn)?;
        let settings = save_trash_settings(&client, auto_purge_enabled, retention_days).await?;

        tracing::info!(
            "Trash settings updated: auto_purge={}, retention_days={}",
            settings.auto_purge_enabled,
            settings.retention_days
        );

        Ok(settings)
    }

    #[cfg(not(feature = "server"))]
    {
        Ok(TrashSettings {
            auto_purge_enabled,
            retention_days,
        })
    }
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires YGGDRASIL_TEST_DATABASE_URL; uses only a connection-local temporary table"]
    async fn trash_settings_database_roundtrip() {
        let url = std::env::var("YGGDRASIL_TEST_DATABASE_URL")
            .expect("set YGGDRASIL_TEST_DATABASE_URL to run this database test");
        let (client, connection) = tokio_postgres::connect(&url, tokio_postgres::NoTls)
            .await
            .unwrap();
        let connection = tokio::spawn(connection);
        client
            .batch_execute(
                "CREATE TEMP TABLE settings (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL,
                    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
                )",
            )
            .await
            .unwrap();

        assert_eq!(
            load_trash_settings(&client).await.unwrap(),
            TrashSettings::default()
        );
        client
            .batch_execute(
                "INSERT INTO settings (key, value) VALUES
                    ('trash_auto_purge_enabled', 'invalid'),
                    ('trash_retention_days', 'invalid')",
            )
            .await
            .unwrap();
        assert_eq!(
            load_trash_settings(&client).await.unwrap(),
            TrashSettings::default()
        );

        for (enabled, input_days, expected_days) in [(true, -5, 1), (false, 366, 365), (true, 7, 7)]
        {
            let expected = TrashSettings {
                auto_purge_enabled: enabled,
                retention_days: expected_days,
            };
            assert_eq!(
                save_trash_settings(&client, enabled, input_days)
                    .await
                    .unwrap(),
                expected
            );
            assert_eq!(load_trash_settings(&client).await.unwrap(), expected);
            let stored: String = client
                .query_one(
                    "SELECT value FROM settings WHERE key = 'trash_retention_days'",
                    &[],
                )
                .await
                .unwrap()
                .get(0);
            assert_eq!(stored, expected_days.to_string());
        }

        for (stored, expected_days) in [("-5", 1), ("366", 365)] {
            client
                .execute(
                    "UPDATE settings SET value = $1 WHERE key = 'trash_retention_days'",
                    &[&stored],
                )
                .await
                .unwrap();
            assert_eq!(
                load_trash_settings(&client).await.unwrap().retention_days,
                expected_days
            );
        }

        client
            .batch_execute("ALTER TABLE pg_temp.settings RENAME COLUMN value TO invalid_value")
            .await
            .unwrap();
        assert!(matches!(
            load_trash_settings(&client).await,
            Err(AppError::Query(_))
        ));
        assert!(matches!(
            save_trash_settings(&client, true, 30).await,
            Err(AppError::Query(_))
        ));
        drop(client);
        connection.await.unwrap().unwrap();
    }
}
