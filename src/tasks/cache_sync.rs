//! PostgreSQL 通知驱动双机缓存失效，持久化版本与定时核对负责断线补偿。
use std::time::Duration;

use tokio_postgres::{AsyncMessage, Client, NoTls};

const CHANNEL: &str = "ygg_content_changed";
const RECHECK_INTERVAL: Duration = Duration::from_secs(5);

struct ConnectionTask(tokio::task::JoinHandle<()>);
impl Drop for ConnectionTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ContentRevision {
    relation_oid: u32,
    revision: i64,
}

async fn refresh_revision(
    client: &Client,
    observed: &mut Option<ContentRevision>,
) -> Result<(), tokio_postgres::Error> {
    let row = client
        .query_one(
            "SELECT tableoid AS relation_oid, revision FROM content_cache_revision WHERE singleton",
            &[],
        )
        .await?;
    let revision = ContentRevision {
        relation_oid: row.get("relation_oid"),
        revision: row.get("revision"),
    };
    if *observed != Some(revision) {
        crate::cache::invalidate_shared_content();
        *observed = Some(revision);
        tracing::debug!(
            revision = revision.revision,
            relation_oid = revision.relation_oid,
            "共享内容缓存已同步失效"
        );
    }
    Ok(())
}

async fn listen(
    config: &tokio_postgres::Config,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (client, mut connection) = config.connect(NoTls).await?;
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    let _driver = ConnectionTask(tokio::spawn(async move {
        while let Some(message) = futures::future::poll_fn(|cx| connection.poll_message(cx)).await {
            match message {
                Ok(AsyncMessage::Notification(notification))
                    if notification.channel() == CHANNEL =>
                {
                    // 版本查询会合并突发变更；通道满时无需再排队。
                    let _ = tx.try_send(());
                }
                Err(_) => break,
                _ => {}
            }
        }
    }));
    // 先监听再读取版本，避免读取版本和 LISTEN 之间漏掉提交。
    client.batch_execute("LISTEN ygg_content_changed").await?;
    let mut observed = None;
    refresh_revision(&client, &mut observed).await?;
    tracing::info!("双机内容缓存通知已连接");
    let mut interval = tokio::time::interval(RECHECK_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            signal = rx.recv() => {
                if signal.is_none() { return Err(std::io::Error::other("cache notification connection closed").into()); }
            }
            _ = interval.tick() => {}
        }
        refresh_revision(&client, &mut observed).await?;
    }
}

pub async fn run() {
    let Ok(mut config) = crate::db::pool::build_pg_config() else {
        tracing::error!("无法初始化内容缓存通知：数据库配置无效");
        return;
    };
    config.connect_timeout(Duration::from_secs(5));
    config.application_name("yggdrasil-cache-sync");
    config.options("-c statement_timeout=5000");
    loop {
        if let Err(error) = listen(&config).await {
            tracing::warn!(%error, "内容缓存通知断开，5 秒后重连；HTML TTL 继续兜底");
            crate::cache::invalidate_shared_content();
        }
        tokio::time::sleep(RECHECK_INTERVAL).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires disposable DATABASE_URL database ygg_mcp_test"]
    fn content_revision_database_detects_recreated_table() {
        crate::db::TEST_DATABASE_RUNTIME
            .block_on(content_revision_database_detects_recreated_table_impl());
    }

    async fn content_revision_database_detects_recreated_table_impl() {
        let _guard = crate::db::TEST_DATABASE_LOCK.lock().await;
        // 独占连接使临时表在失败退出时也随连接清理，不污染连接池。
        let (client, connection) = crate::db::pool::build_pg_config()
            .unwrap()
            .connect(NoTls)
            .await
            .unwrap();
        let _driver = ConnectionTask(tokio::spawn(async move {
            connection.await.unwrap();
        }));
        let database: String = client
            .query_one("SELECT current_database()", &[])
            .await
            .unwrap()
            .get(0);
        assert_eq!(database, "ygg_mcp_test", "requires isolated test database");
        client
            .batch_execute(
                "CREATE TEMP TABLE content_cache_revision (
                    singleton BOOLEAN PRIMARY KEY, revision BIGINT NOT NULL
                 );
                 INSERT INTO content_cache_revision VALUES (TRUE, 7);",
            )
            .await
            .unwrap();
        let before = crate::cache::feed_generation();
        let mut observed = None;
        refresh_revision(&client, &mut observed).await.unwrap();
        let first = observed.unwrap();
        let generation = crate::cache::feed_generation();
        assert_ne!(generation, before);
        refresh_revision(&client, &mut observed).await.unwrap();
        assert_eq!(crate::cache::feed_generation(), generation);

        client
            .batch_execute("UPDATE content_cache_revision SET revision=8")
            .await
            .unwrap();
        refresh_revision(&client, &mut observed).await.unwrap();
        let incremented = observed.unwrap();
        assert_eq!(incremented.relation_oid, first.relation_oid);
        assert_eq!(incremented.revision, 8);
        let next = crate::cache::feed_generation();
        assert_ne!(next, generation);

        client
            .batch_execute(
                "DROP TABLE pg_temp.content_cache_revision;
                 CREATE TEMP TABLE content_cache_revision (
                    singleton BOOLEAN PRIMARY KEY, revision BIGINT NOT NULL
                 );
                 INSERT INTO content_cache_revision VALUES (TRUE, 8);",
            )
            .await
            .unwrap();
        refresh_revision(&client, &mut observed).await.unwrap();
        let recreated = observed.unwrap();
        assert_eq!(recreated.revision, incremented.revision);
        assert_ne!(recreated.relation_oid, incremented.relation_oid);
        let recreated_generation = crate::cache::feed_generation();
        assert_ne!(recreated_generation, next);
        refresh_revision(&client, &mut observed).await.unwrap();
        assert_eq!(crate::cache::feed_generation(), recreated_generation);
        client
            .batch_execute("DROP TABLE pg_temp.content_cache_revision")
            .await
            .unwrap();
    }
}
