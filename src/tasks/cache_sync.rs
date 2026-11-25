//! PostgreSQL 通知驱动双机缓存失效，持久化版本与定时核对负责断线补偿。
use std::time::Duration;

use tokio_postgres::{AsyncMessage, Client, NoTls};

const CHANNEL: &str = "ygg_content_changed";
const RECHECK_INTERVAL: Duration = Duration::from_secs(5);
// 串行读取并应用版本，本机写路径和通知监听器只需应用同一个已提交版本一次。
static OBSERVED: tokio::sync::Mutex<Option<ContentRevision>> = tokio::sync::Mutex::const_new(None);

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
    posts: i64,
    public_posts: i64,
    comments: i64,
    friends: i64,
    settings: i64,
    public_notes: Option<i64>,
    notebooks: Option<i64>,
}

/// 提交后等待本机应用已提交的版本。监听器也可能先完成这一步。
pub(crate) async fn sync_after_write(client: &Client) {
    let mut observed = OBSERVED.lock().await;
    if let Err(error) = refresh_revision(client, &mut observed).await {
        tracing::warn!(%error, "提交后读取内容缓存版本失败，已清除本机缓存");
        *observed = None;
        crate::cache::invalidate_shared_content();
    }
}

async fn refresh_revision(
    client: &Client,
    observed: &mut Option<ContentRevision>,
) -> Result<(), tokio_postgres::Error> {
    // 恢复早期备份后只有全局版本，各内容域随全局版本一起失效，直到重启补齐迁移。
    let row = client
        .query_one(
            "SELECT tableoid AS relation_oid, revision,
                    COALESCE((to_jsonb(r)->>'posts_revision')::bigint, revision) AS posts_revision,
                    (to_jsonb(r)->>'public_posts_revision')::bigint AS public_posts_revision,
                    COALESCE((to_jsonb(r)->>'comments_revision')::bigint, revision) AS comments_revision,
                    COALESCE((to_jsonb(r)->>'friends_revision')::bigint, revision) AS friends_revision,
                    COALESCE((to_jsonb(r)->>'settings_revision')::bigint, revision) AS settings_revision,
                    (to_jsonb(r)->>'public_notes_revision')::bigint AS public_notes_revision,
                    (to_jsonb(r)->>'notebooks_revision')::bigint AS notebooks_revision
             FROM content_cache_revision r WHERE singleton",
            &[],
        )
        .await?;
    let posts = row.get("posts_revision");
    let revision = ContentRevision {
        relation_oid: row.get("relation_oid"),
        revision: row.get("revision"),
        posts,
        public_posts: row
            .get::<_, Option<i64>>("public_posts_revision")
            .unwrap_or(posts),
        comments: row.get("comments_revision"),
        friends: row.get("friends_revision"),
        settings: row.get("settings_revision"),
        public_notes: row.get("public_notes_revision"),
        notebooks: row.get("notebooks_revision"),
    };
    if *observed != Some(revision) {
        if let Some(previous) = observed.filter(|previous| {
            previous.relation_oid == revision.relation_oid && previous.revision < revision.revision
        }) {
            let posts_changed = previous.posts != revision.posts;
            let public_posts_changed = previous.public_posts != revision.public_posts;
            let comments_changed = previous.comments != revision.comments;
            let friends_changed = previous.friends != revision.friends;
            let settings_changed = previous.settings != revision.settings;
            let public_notes_changed =
                revision.public_notes.is_none() || previous.public_notes != revision.public_notes;
            if public_notes_changed {
                crate::cache::invalidate_note_stats();
            }
            if public_notes_changed
                || revision.notebooks.is_none()
                || previous.notebooks != revision.notebooks
            {
                crate::cache::invalidate_public_notebooks();
            }
            if public_posts_changed {
                crate::cache::invalidate_all_post_caches();
            } else if posts_changed {
                crate::cache::invalidate_post_stats();
                crate::cache::invalidate_all_tags();
            }
            if comments_changed {
                crate::cache::invalidate_all_comments();
            }
            if friends_changed {
                crate::cache::invalidate_friend_links();
            }
            if settings_changed {
                crate::cache::invalidate_shared_settings();
            }
            if public_posts_changed
                || comments_changed
                || friends_changed
                || settings_changed
                || revision.public_notes.is_none()
            {
                crate::ssr_cache::invalidate_ssr_all_public();
            } else if public_notes_changed {
                crate::ssr_cache::invalidate_home();
            }
        } else {
            crate::cache::invalidate_shared_content();
        }
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
    refresh_revision(&client, &mut *OBSERVED.lock().await).await?;
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
        refresh_revision(&client, &mut *OBSERVED.lock().await).await?;
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
            let mut observed = OBSERVED.lock().await;
            *observed = None;
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
    fn draft_posts_database_preserves_public_caches() {
        crate::db::TEST_DATABASE_RUNTIME
            .block_on(draft_posts_database_preserves_public_caches_impl());
    }

    async fn draft_posts_database_preserves_public_caches_impl() {
        let _guard = crate::db::TEST_DATABASE_LOCK.lock().await;
        let mut writer = crate::db::pool::get_conn().await.unwrap();
        let database: String = writer
            .query_one("SELECT current_database()", &[])
            .await
            .unwrap()
            .get(0);
        assert_eq!(database, "ygg_mcp_test", "requires isolated test database");
        crate::db::migrate::run_on_conn(&mut writer).await.unwrap();
        writer
            .batch_execute(
                "TRUNCATE users, tags RESTART IDENTITY CASCADE;
             INSERT INTO users(id, username, email, password_hash, role)
             VALUES (1, 'draft-owner', 'draft@test.invalid', 'unused', 'admin');",
            )
            .await
            .unwrap();

        // 从第二条连接读取已提交版本，覆盖对端合并通知或断线后核对版本的路径。
        let reader = crate::db::pool::get_conn().await.unwrap();
        let mut observed = None;
        refresh_revision(&reader, &mut observed).await.unwrap();
        let generations = || {
            (
                crate::ssr_cache::current_generation("/"),
                crate::ssr_cache::current_generation("/post/test"),
                crate::cache::feed_generation(),
            )
        };
        let initial = generations();
        assert_eq!(
            crate::api::posts::load_post_stats().await.unwrap().drafts,
            0
        );
        assert!(crate::api::posts::list_tags()
            .await
            .unwrap()
            .tags
            .is_empty());

        writer
            .batch_execute(
                "INSERT INTO posts(id, author_id, title, slug, content_md) VALUES
                (1, 1, '草稿', 'test', '草稿正文'), (2, 1, '草稿二', 'test-2', '');
             INSERT INTO tags(name) VALUES ('草稿标签');
             INSERT INTO post_tags VALUES (1, 1);
             UPDATE posts SET title='修改草稿' WHERE id=1;",
            )
            .await
            .unwrap();
        refresh_revision(&reader, &mut observed).await.unwrap();
        assert_eq!(generations(), initial);
        assert_eq!(
            crate::api::posts::load_post_stats().await.unwrap().drafts,
            2
        );
        assert_eq!(
            crate::api::posts::list_tags().await.unwrap().tags[0].post_count,
            0
        );
        assert!(crate::pages::archives::list_public_tags()
            .await
            .unwrap()
            .tags
            .is_empty());

        for (sql, public_changed) in [
            ("DELETE FROM post_tags WHERE post_id=1", false),
            (
                "INSERT INTO tags(name) VALUES ('草稿标签') ON CONFLICT (name) DO NOTHING;
              INSERT INTO post_tags VALUES (1, 1)",
                false,
            ),
            (
                "BEGIN; UPDATE posts SET status='published' WHERE id=1; ROLLBACK",
                false,
            ),
            (
                "UPDATE posts SET status='published' WHERE id=1;
              UPDATE posts SET title='独立的草稿' WHERE id=2",
                true,
            ),
            (
                "UPDATE posts SET content_md='新正文' WHERE id IN (1, 2)",
                true,
            ),
            ("DELETE FROM post_tags WHERE post_id=1", true),
            ("INSERT INTO post_tags VALUES (1, 1)", true),
            ("UPDATE tags SET name='公开标签' WHERE id=1", true),
            ("UPDATE posts SET deleted_at=NOW() WHERE id=1", true),
            ("UPDATE posts SET deleted_at=NULL WHERE id=1", true),
            ("UPDATE posts SET status='draft' WHERE id=1", true),
            ("DELETE FROM posts WHERE id=1", false),
            (
                "INSERT INTO posts(id, author_id, title, slug, content_md, status)
              VALUES (3, 1, '直接发布', 'published', '正文', 'published')",
                true,
            ),
            ("DELETE FROM posts WHERE id=3", true),
        ] {
            let before = generations();
            writer.batch_execute(sql).await.unwrap();
            refresh_revision(&reader, &mut observed).await.unwrap();
            let after = generations();
            assert_eq!(
                (
                    after.0 != before.0,
                    after.1 != before.1,
                    after.2 != before.2
                ),
                (public_changed, public_changed, public_changed),
                "{sql}"
            );
        }
    }

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
                    singleton BOOLEAN PRIMARY KEY, revision BIGINT NOT NULL,
                    posts_revision BIGINT NOT NULL DEFAULT 0,
                    comments_revision BIGINT NOT NULL DEFAULT 0,
                    friends_revision BIGINT NOT NULL DEFAULT 0,
                    settings_revision BIGINT NOT NULL DEFAULT 0
                 );
                 INSERT INTO content_cache_revision (singleton, revision, posts_revision) VALUES (TRUE, 7, 7);",
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

        let site = crate::models::settings::SiteSettings {
            home_title: "缓存域测试".into(),
            ..Default::default()
        };
        crate::cache::set_site_settings(site.clone()).await;
        crate::cache::set_comments_by_post(42, Vec::new()).await;
        client
            .batch_execute("UPDATE content_cache_revision SET revision=8, posts_revision=8")
            .await
            .unwrap();
        refresh_revision(&client, &mut observed).await.unwrap();
        let incremented = observed.unwrap();
        assert_eq!(incremented.relation_oid, first.relation_oid);
        assert_eq!(incremented.revision, 8);
        let next = crate::cache::feed_generation();
        assert_ne!(next, generation);
        assert_eq!(crate::cache::get_site_settings().await, Some(site));
        assert!(crate::cache::get_comments_by_post(42).await.is_some());

        // 从早期备份恢复时，同一数值版本也会因表 OID 改变触发全量失效。
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
        assert!(crate::cache::get_site_settings().await.is_none());
        assert!(crate::cache::get_comments_by_post(42).await.is_none());
        refresh_revision(&client, &mut observed).await.unwrap();
        assert_eq!(crate::cache::feed_generation(), recreated_generation);
        client
            .batch_execute("DROP TABLE pg_temp.content_cache_revision")
            .await
            .unwrap();
    }
}
