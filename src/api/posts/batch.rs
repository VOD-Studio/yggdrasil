//! 后台文章的批量操作：发布、转为草稿、移到回收站。
//!
//! 逐篇走文章写入业务（各自事务与缓存失效），返回成功与失败篇数；某一篇失败不影响其余。
//! Leptos server function，注册在 `/api` 路径下。

use leptos::prelude::*;

#[cfg(feature = "server")]
use super::helpers::get_current_admin_user;
use super::types::{BatchPostAction, BatchPostResult};

/// 单次批量操作的篇数上限。
#[cfg(feature = "server")]
const MAX_BATCH: usize = 200;

/// 对多篇文章执行同一操作。需要 admin 权限。
#[server(BatchPosts, endpoint = "batch_posts", input = leptos::server_fn::codec::Json)]
pub async fn batch_posts(
    post_ids: Vec<i32>,
    action: BatchPostAction,
) -> Result<BatchPostResult, ServerFnError> {
    #[cfg(feature = "server")]
    {
        use super::service::{act, PostAction};

        let _user = get_current_admin_user().await?;
        if post_ids.len() > MAX_BATCH {
            return Err(ServerFnError::new(format!("一次最多操作 {MAX_BATCH} 篇")));
        }
        let mut ids = post_ids;
        ids.sort_unstable();
        ids.dedup();
        let mut result = BatchPostResult { done: 0, failed: 0 };
        for id in ids {
            let action = match action {
                BatchPostAction::Publish => PostAction::Publish,
                BatchPostAction::Unpublish => PostAction::Unpublish,
                BatchPostAction::Trash => PostAction::Trash,
            };
            match act(id, None, action).await {
                Ok(_) => result.done += 1,
                Err(_) => result.failed += 1,
            }
        }
        Ok(result)
    }
    #[cfg(not(feature = "server"))]
    {
        let _ = (post_ids, action);
        unreachable!()
    }
}

#[cfg(all(test, feature = "server"))]
mod tests {
    use super::*;
    use crate::api::posts::{list_posts, AdminPostQuery};
    use crate::db::pool::get_conn;
    use crate::models::post::PostStatus;

    fn web_context() -> http::request::Parts {
        let (parts, _) = http::Request::builder()
            .header("cookie", "session=post-batch-regression")
            .body(())
            .unwrap()
            .into_parts();
        parts
    }

    #[test]
    #[ignore = "requires disposable DATABASE_URL database ygg_mcp_test"]
    fn admin_post_list_and_batch_database() {
        crate::db::TEST_DATABASE_RUNTIME.block_on(admin_post_list_and_batch_database_impl());
    }

    async fn admin_post_list_and_batch_database_impl() {
        let _guard = crate::db::TEST_DATABASE_LOCK.lock().await;
        let mut client = get_conn().await.unwrap();
        let database: String = client
            .query_one("SELECT current_database()", &[])
            .await
            .unwrap()
            .get(0);
        assert_eq!(database, "ygg_mcp_test", "requires isolated test database");
        crate::db::migrate::run_on_conn(&mut client).await.unwrap();
        client
            .batch_execute(
                "TRUNCATE users RESTART IDENTITY CASCADE;
                 TRUNCATE tags RESTART IDENTITY CASCADE;
                 INSERT INTO users(id,username,email,password_hash,role)
                   VALUES (1,'batch-owner','batch@test.invalid','unused','admin');
                 INSERT INTO posts(id,author_id,title,slug,content_md,status,published_at,created_at) VALUES
                   (1,1,'Rust 入门','rust-intro','正文','published',NOW()-INTERVAL '3 days',NOW()-INTERVAL '3 days'),
                   (2,1,'异步 Rust','async-rust','正文','published',NOW()-INTERVAL '2 days',NOW()-INTERVAL '2 days'),
                   (3,1,'草稿 100%_完成','draft-escape','正文','draft',NULL,NOW()-INTERVAL '1 day');
                 SELECT setval('posts_id_seq', 3);
                 INSERT INTO tags(name) VALUES ('Rust');
                 INSERT INTO post_tags(post_id,tag_id) SELECT 2, id FROM tags WHERE name='Rust';",
            )
            .await
            .unwrap();
        let hash = crate::auth::session::hash_token("post-batch-regression");
        client
            .execute(
                "INSERT INTO sessions(user_id,token_hash,expires_at) VALUES(1,$1,NOW()+INTERVAL '1 hour')",
                &[&hash],
            )
            .await
            .unwrap();
        let list = |query: AdminPostQuery| async move {
            crate::request_ctx::scope_with_request(web_context(), list_posts(1, 20, query))
                .await
                .unwrap()
        };
        let ids = |res: &crate::api::posts::PostListResponse| {
            res.posts.iter().map(|p| p.id).collect::<Vec<_>>()
        };

        // 筛选：状态、标签、标题；LIKE 通配符按字面匹配；新建在前。
        assert_eq!(ids(&list(AdminPostQuery::default()).await), [3, 2, 1]);
        let drafts = list(AdminPostQuery {
            status: Some(PostStatus::Draft),
            ..Default::default()
        })
        .await;
        assert_eq!((ids(&drafts), drafts.total), (vec![3], 1));
        let tagged = list(AdminPostQuery {
            tag: Some("Rust".into()),
            ..Default::default()
        })
        .await;
        assert_eq!(ids(&tagged), [2]);
        assert_eq!(tagged.posts[0].tags, ["Rust"]);
        assert_eq!(ids(&list(AdminPostQuery::search("rust")).await), [2, 1]);
        assert_eq!(ids(&list(AdminPostQuery::search("100%_")).await), [3]);
        assert!(list(AdminPostQuery::search("0%x")).await.posts.is_empty());

        // 转为草稿保留首次发布时间；移到回收站后不再出现在列表。
        let first_published: chrono::DateTime<chrono::Utc> = client
            .query_one("SELECT published_at FROM posts WHERE id=1", &[])
            .await
            .unwrap()
            .get(0);
        let res = crate::request_ctx::scope_with_request(
            web_context(),
            batch_posts(vec![1, 1, 2, 99], BatchPostAction::Unpublish),
        )
        .await
        .unwrap();
        assert_eq!(res, BatchPostResult { done: 2, failed: 1 });
        let row = client
            .query_one("SELECT status, published_at FROM posts WHERE id=1", &[])
            .await
            .unwrap();
        assert_eq!(row.get::<_, String>("status"), "draft");
        assert_eq!(
            row.get::<_, Option<chrono::DateTime<chrono::Utc>>>("published_at"),
            Some(first_published)
        );
        // 发布草稿：补上首次发布时间。
        let res = crate::request_ctx::scope_with_request(
            web_context(),
            batch_posts(vec![3], BatchPostAction::Publish),
        )
        .await
        .unwrap();
        assert_eq!(res, BatchPostResult { done: 1, failed: 0 });
        let row = client
            .query_one("SELECT status, published_at FROM posts WHERE id=3", &[])
            .await
            .unwrap();
        assert_eq!(row.get::<_, String>("status"), "published");
        assert!(row
            .get::<_, Option<chrono::DateTime<chrono::Utc>>>("published_at")
            .is_some());
        let res = crate::request_ctx::scope_with_request(
            web_context(),
            batch_posts(vec![2, 3], BatchPostAction::Trash),
        )
        .await
        .unwrap();
        assert_eq!(res, BatchPostResult { done: 2, failed: 0 });
        assert_eq!(ids(&list(AdminPostQuery::default()).await), [1]);
        // 已在回收站的文章不能再转为草稿。
        let res = crate::request_ctx::scope_with_request(
            web_context(),
            batch_posts(vec![2], BatchPostAction::Unpublish),
        )
        .await
        .unwrap();
        assert_eq!(res, BatchPostResult { done: 0, failed: 1 });
    }
}
