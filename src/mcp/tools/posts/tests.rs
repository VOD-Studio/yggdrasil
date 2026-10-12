use super::*;
use crate::mcp::{auth::McpPrincipal, server::YggMcpServer};

fn web_context() -> dioxus::fullstack::FullstackContext {
    let (parts, _) = http::Request::builder()
        .header("cookie", "session=post-write-regression")
        .body(())
        .unwrap()
        .into_parts();
    dioxus::fullstack::FullstackContext::new(parts)
}

#[test]
#[ignore = "requires disposable DATABASE_URL database ygg_mcp_test"]
fn post_write_service_database() {
    crate::db::TEST_DATABASE_RUNTIME.block_on(post_write_service_database_impl());
}

async fn post_write_service_database_impl() {
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
        INSERT INTO users(id,username,email,password_hash,role) VALUES
        (1,'post-owner','owner@test.invalid','unused','admin'),
        (2,'post-other','other@test.invalid','unused','blocked');",
        )
        .await
        .unwrap();
    let hash = crate::auth::session::hash_token("post-write-regression");
    client.execute("INSERT INTO sessions(user_id,token_hash,expires_at) VALUES(1,$1,NOW()+INTERVAL '1 hour')", &[&hash]).await.unwrap();
    let asset_id = uuid::Uuid::new_v4();
    let asset_path = format!("2026/10/12/{asset_id}.webp");
    let cover = format!("/uploads/{asset_path}");
    client.execute("INSERT INTO assets(id,path,filename,mime,size_bytes,width,height) VALUES($1,$2,'fixture.webp','image/webp',1,1,1)", &[&asset_id,&asset_path]).await.unwrap();

    // The browser and MCP adapters produce the same normalized article fields.
    let web = web_context()
        .scope(crate::api::posts::create_post(
            "  共同标题  ".into(),
            Some("web-shared".into()),
            Some("  摘要  ".into()),
            "# 正文\n\n共同内容".into(),
            "draft".into(),
            vec![" tag ".into(), "tag".into()],
            Some(cover.clone()),
        ))
        .await
        .unwrap();
    assert!(web.success);
    let web_id = web.post_id.unwrap();
    let mcp = result_json(
        YggMcpServer
            .create_post(
                Parameters(CreatePostParams {
                    title: "  共同标题  ".into(),
                    slug: Some("mcp-shared".into()),
                    summary: Some("  摘要  ".into()),
                    content_md: "# 正文\n\n共同内容".into(),
                    status: "draft".into(),
                    tags: vec![" tag ".into(), "tag".into()],
                    cover_image: Some(cover.clone()),
                    published_at: None,
                }),
                Extension(parts(Some(TokenScope::Write))),
            )
            .await
            .unwrap(),
    );
    let mcp_id = mcp["post_id"].as_i64().unwrap() as i32;
    let values = client.query("SELECT title,summary,content_md,content_html,word_count,reading_time,cover_image FROM posts WHERE id=ANY($1) ORDER BY id", &[&vec![web_id,mcp_id]]).await.unwrap();
    for name in [
        "title",
        "summary",
        "content_md",
        "content_html",
        "cover_image",
    ] {
        assert_eq!(
            values[0].get::<_, Option<String>>(name),
            values[1].get::<_, Option<String>>(name),
            "{name}"
        );
    }
    for name in ["word_count", "reading_time"] {
        assert_eq!(values[0].get::<_, i32>(name), values[1].get::<_, i32>(name));
    }
    assert_eq!(
        crate::api::posts::helpers::fetch_post_tags(&client, web_id)
            .await
            .unwrap(),
        vec!["tag"]
    );

    // Both entry points reject blank updates before any persistent change.
    for (title, content) in [(" ", "正文"), ("标题", " \n")] {
        let rejected = web_context()
            .scope(crate::api::posts::update_post(
                web_id,
                title.into(),
                Some("web-shared".into()),
                None,
                content.into(),
                "draft".into(),
                vec![],
                None,
            ))
            .await
            .unwrap();
        assert!(!rejected.success);
        let p: UpdatePostParams = serde_json::from_value(
            serde_json::json!({"post_id":mcp_id,"title":title,"content_md":content}),
        )
        .unwrap();
        assert!(YggMcpServer
            .update_post(Parameters(p), Extension(parts(Some(TokenScope::Write))))
            .await
            .is_err());
    }
    assert_eq!(
        client
            .query_one("SELECT title FROM posts WHERE id=$1", &[&web_id])
            .await
            .unwrap()
            .get::<_, String>(0),
        "共同标题"
    );

    // PATCH omission preserves正文/封面; an explicitly empty cover removes its asset reference.
    let patch: UpdatePostParams =
        serde_json::from_value(serde_json::json!({"post_id":mcp_id,"title":"改标题"})).unwrap();
    YggMcpServer
        .update_post(Parameters(patch), Extension(parts(Some(TokenScope::Write))))
        .await
        .unwrap();
    let row = client
        .query_one(
            "SELECT content_md,cover_image FROM posts WHERE id=$1",
            &[&mcp_id],
        )
        .await
        .unwrap();
    assert_eq!(row.get::<_, String>(0), "# 正文\n\n共同内容");
    assert_eq!(row.get::<_, Option<String>>(1), Some(cover.clone()));
    let clear: UpdatePostParams =
        serde_json::from_value(serde_json::json!({"post_id":mcp_id,"cover_image":""})).unwrap();
    YggMcpServer
        .update_post(Parameters(clear), Extension(parts(Some(TokenScope::Write))))
        .await
        .unwrap();
    assert_eq!(
        client
            .query_one(
                "SELECT COUNT(*) FROM asset_refs WHERE post_id=$1",
                &[&mcp_id]
            )
            .await
            .unwrap()
            .get::<_, i64>(0),
        0
    );

    // Status-only changes invalidate tag pages even when tags were omitted from PATCH.
    crate::cache::set_posts_by_tag("tag", vec![], 0).await;
    YggMcpServer
        .publish_post(
            Parameters(PostIdParams { post_id: mcp_id }),
            Extension(parts(Some(TokenScope::Write))),
        )
        .await
        .unwrap();
    assert!(crate::cache::get_posts_by_tag("tag").await.is_none());
    let published: chrono::DateTime<chrono::Utc> = client
        .query_one("SELECT published_at FROM posts WHERE id=$1", &[&mcp_id])
        .await
        .unwrap()
        .get(0);
    YggMcpServer
        .publish_post(
            Parameters(PostIdParams { post_id: mcp_id }),
            Extension(parts(Some(TokenScope::Write))),
        )
        .await
        .unwrap();
    assert_eq!(
        client
            .query_one("SELECT published_at FROM posts WHERE id=$1", &[&mcp_id])
            .await
            .unwrap()
            .get::<_, chrono::DateTime<chrono::Utc>>(0),
        published
    );
    let dated: UpdatePostParams =
        serde_json::from_value(serde_json::json!({"post_id":mcp_id,"published_at":"2020-01-02"}))
            .unwrap();
    YggMcpServer
        .update_post(Parameters(dated), Extension(parts(Some(TokenScope::Write))))
        .await
        .unwrap();
    let dates = client
        .query_one(
            "SELECT published_at=created_at FROM posts WHERE id=$1",
            &[&mcp_id],
        )
        .await
        .unwrap();
    assert!(dates.get::<_, bool>(0));

    // Updating正文 with omitted cover preserves it; explicit dates/tags can be cleared.
    let set_cover: UpdatePostParams =
        serde_json::from_value(serde_json::json!({"post_id":mcp_id,"cover_image":cover})).unwrap();
    YggMcpServer
        .update_post(
            Parameters(set_cover),
            Extension(parts(Some(TokenScope::Write))),
        )
        .await
        .unwrap();
    let created: chrono::DateTime<chrono::Utc> = client
        .query_one("SELECT created_at FROM posts WHERE id=$1", &[&mcp_id])
        .await
        .unwrap()
        .get(0);
    let patch: UpdatePostParams = serde_json::from_value(
        serde_json::json!({"post_id":mcp_id,"published_at":"","tags":[],"content_md":"替换正文"}),
    )
    .unwrap();
    YggMcpServer
        .update_post(Parameters(patch), Extension(parts(Some(TokenScope::Write))))
        .await
        .unwrap();
    let row = client
        .query_one(
            "SELECT published_at,created_at,cover_image,summary FROM posts WHERE id=$1",
            &[&mcp_id],
        )
        .await
        .unwrap();
    assert!(row
        .get::<_, Option<chrono::DateTime<chrono::Utc>>>(0)
        .is_none());
    assert_eq!(row.get::<_, chrono::DateTime<chrono::Utc>>(1), created);
    assert_eq!(row.get::<_, Option<String>>(2), Some(cover.clone()));
    assert_eq!(row.get::<_, Option<String>>(3), Some("替换正文".into()));
    assert!(crate::api::posts::helpers::fetch_post_tags(&client, mcp_id)
        .await
        .unwrap()
        .is_empty());

    // A database failure after the article UPDATE rolls back article, tags, and asset refs.
    let failure = web_context()
        .scope(crate::api::posts::update_post(
            web_id,
            "不应落库".into(),
            Some("web-shared".into()),
            None,
            "新正文".into(),
            "draft".into(),
            vec!["x".repeat(300)],
            None,
        ))
        .await;
    assert!(failure.is_err());
    let row = client
        .query_one(
            "SELECT title,content_md,cover_image FROM posts WHERE id=$1",
            &[&web_id],
        )
        .await
        .unwrap();
    assert_eq!(row.get::<_, String>(0), "共同标题");
    assert_eq!(row.get::<_, String>(1), "# 正文\n\n共同内容");
    assert_eq!(row.get::<_, Option<String>>(2), Some(cover.clone()));
    assert_eq!(
        crate::api::posts::helpers::fetch_post_tags(&client, web_id)
            .await
            .unwrap(),
        vec!["tag"]
    );
    assert_eq!(
        client
            .query_one(
                "SELECT COUNT(*) FROM asset_refs WHERE post_id=$1",
                &[&web_id]
            )
            .await
            .unwrap()
            .get::<_, i64>(0),
        1
    );

    // Successful complete replacement regenerates slug/summary, clears nullable fields
    // and preserves publication/creation dates for an already published article.
    let published = web_context()
        .scope(crate::api::posts::update_post(
            web_id,
            "共同标题".into(),
            Some("web-shared".into()),
            None,
            "# 正文\n\n共同内容".into(),
            "published".into(),
            vec!["tag".into()],
            Some(cover),
        ))
        .await
        .unwrap();
    assert!(published.success);
    let before = client
        .query_one(
            "SELECT published_at,created_at FROM posts WHERE id=$1",
            &[&web_id],
        )
        .await
        .unwrap();
    crate::cache::set_post_by_slug("web-shared", None).await;
    crate::cache::set_posts_by_tag("tag", vec![], 0).await;
    let replaced = web_context()
        .scope(crate::api::posts::update_post(
            web_id,
            "新完整标题".into(),
            None,
            None,
            "新的正文".into(),
            "published".into(),
            vec![],
            None,
        ))
        .await
        .unwrap();
    assert!(replaced.success);
    assert_eq!(replaced.slug.as_deref(), Some("xin-wan-zheng-biao-ti"));
    let after = client
        .query_one(
            "SELECT summary,cover_image,published_at,created_at FROM posts WHERE id=$1",
            &[&web_id],
        )
        .await
        .unwrap();
    assert_eq!(after.get::<_, Option<String>>(0), Some("新的正文".into()));
    assert!(after.get::<_, Option<String>>(1).is_none());
    assert_eq!(
        after.get::<_, Option<chrono::DateTime<chrono::Utc>>>(2),
        before.get::<_, Option<chrono::DateTime<chrono::Utc>>>(0)
    );
    assert_eq!(
        after.get::<_, chrono::DateTime<chrono::Utc>>(3),
        before.get::<_, chrono::DateTime<chrono::Utc>>(1)
    );
    assert!(crate::api::posts::helpers::fetch_post_tags(&client, web_id)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        client
            .query_one(
                "SELECT COUNT(*) FROM asset_refs WHERE post_id=$1",
                &[&web_id]
            )
            .await
            .unwrap()
            .get::<_, i64>(0),
        0
    );
    assert!(crate::cache::get_post_by_slug("web-shared").await.is_none());
    assert!(crate::cache::get_posts_by_tag("tag").await.is_none());

    let foreign = service::create(
        2,
        PostDraft {
            title: "其他作者".into(),
            slug: Some("foreign".into()),
            summary: None,
            content_md: "正文".into(),
            status: "draft".into(),
            tags: vec![],
            cover_image: None,
        },
        None,
    )
    .await
    .unwrap();
    assert!(service::update(
        1,
        foreign.post_id,
        UpdatePost::Patch(PostPatch {
            title: Some("越权".into()),
            ..Default::default()
        })
    )
    .await
    .is_err());
    let denied = web_context()
        .scope(crate::api::posts::update_post(
            foreign.post_id,
            "越权".into(),
            None,
            None,
            "正文".into(),
            "draft".into(),
            vec![],
            None,
        ))
        .await
        .unwrap();
    assert!(!denied.success);
    let denied: UpdatePostParams =
        serde_json::from_value(serde_json::json!({"post_id":foreign.post_id,"title":"越权"}))
            .unwrap();
    assert!(YggMcpServer
        .update_post(
            Parameters(denied),
            Extension(parts(Some(TokenScope::Write)))
        )
        .await
        .is_err());
    YggMcpServer
        .trash_post(
            Parameters(PostIdParams { post_id: mcp_id }),
            Extension(parts(Some(TokenScope::Write))),
        )
        .await
        .unwrap();
    let deleted: UpdatePostParams =
        serde_json::from_value(serde_json::json!({"post_id":mcp_id,"title":"回收站不能编辑"}))
            .unwrap();
    assert!(YggMcpServer
        .update_post(
            Parameters(deleted),
            Extension(parts(Some(TokenScope::Write)))
        )
        .await
        .is_err());
    client
        .execute("DELETE FROM assets WHERE id=$1", &[&asset_id])
        .await
        .unwrap();
}

fn parts(scope: Option<TokenScope>) -> http::request::Parts {
    let (mut parts, _) = http::Request::new(()).into_parts();
    if let Some(scope) = scope {
        parts.extensions.insert(McpPrincipal {
            user_id: 1,
            scope,
            token_id: "test".into(),
        });
    }
    parts
}

fn result_json(result: CallToolResult) -> serde_json::Value {
    serde_json::from_str(&result.content[0].as_text().unwrap().text).unwrap()
}

/// Run only against a disposable database named ygg_mcp_test.
#[test]
#[ignore = "requires disposable DATABASE_URL database ygg_mcp_test"]
fn private_post_workflow_database() {
    crate::db::TEST_DATABASE_RUNTIME.block_on(private_post_workflow_database_impl());
}

async fn private_post_workflow_database_impl() {
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
            "TRUNCATE users CASCADE;
    INSERT INTO users (id, username, email, password_hash, role) VALUES
        (1, 'mcp-owner', 'owner@test.invalid', 'unused', 'admin'),
        (2, 'mcp-other', 'other@test.invalid', 'unused', 'blocked');
    INSERT INTO posts (id, author_id, title, slug, content_md, content_html, status) VALUES
        (101, 1, 'Draft 100%_', 'mcp-draft', 'draft body', '<p>draft body</p>', 'draft'),
        (102, 2, 'Other draft', 'mcp-other', 'private', '<p>private</p>', 'draft'),
        (103, 1, 'Published', 'mcp-published', 'public', '<p>public</p>', 'published');",
        )
        .await
        .unwrap();
    let p = ListPostsParams {
        status: Some("draft".into()),
        query: Some("100%_".into()),
        ..Default::default()
    };
    let list = result_json(
        YggMcpServer
            .list_posts(Parameters(p), Extension(parts(Some(TokenScope::Write))))
            .await
            .unwrap(),
    );
    assert_eq!(list["total"], 1);
    assert_eq!(list["posts"][0]["id"], 101);
    let post = result_json(
        YggMcpServer
            .get_post_by_id(
                Parameters(PostIdParams { post_id: 101 }),
                Extension(parts(Some(TokenScope::Write))),
            )
            .await
            .unwrap(),
    );
    assert_eq!(post["content_md"], "draft body");
    assert!(post["updated_at"].is_string());
    assert!(YggMcpServer
        .get_post_by_id(
            Parameters(PostIdParams { post_id: 102 }),
            Extension(parts(Some(TokenScope::Write)))
        )
        .await
        .is_err());
    assert!(crate::mcp::tools::read::get_published_by_slug("mcp-draft")
        .await
        .unwrap()
        .is_none());
    client
        .batch_execute(
            "UPDATE posts SET deleted_at = NOW() WHERE id IN (101, 102);
    INSERT INTO posts (id, author_id, title, slug, content_md, status) VALUES
    (104, 1, 'Slug collision', 'mcp-draft', 'body', 'draft');",
        )
        .await
        .unwrap();
    let trash = result_json(
        YggMcpServer
            .list_trashed_posts(
                Parameters(ListPostsParams::default()),
                Extension(parts(Some(TokenScope::Write))),
            )
            .await
            .unwrap(),
    );
    assert_eq!(trash["total"], 1);
    assert_eq!(trash["posts"][0]["id"], 101);
    assert!(trash["posts"][0]["deleted_at"].is_string());
    let denied = result_json(
        YggMcpServer
            .restore_post(
                Parameters(PostIdParams { post_id: 102 }),
                Extension(parts(Some(TokenScope::Write))),
            )
            .await
            .unwrap(),
    );
    assert_eq!(denied["success"], false);
    let restored = result_json(
        YggMcpServer
            .restore_post(
                Parameters(PostIdParams { post_id: 101 }),
                Extension(parts(Some(TokenScope::Write))),
            )
            .await
            .unwrap(),
    );
    assert_eq!(restored["success"], true);
    assert_ne!(restored["slug"], "mcp-draft");
    let row = client
        .query_one("SELECT status, deleted_at FROM posts WHERE id = 101", &[])
        .await
        .unwrap();
    assert_eq!(row.get::<_, String>(0), "draft");
    assert!(row
        .get::<_, Option<chrono::DateTime<chrono::Utc>>>(1)
        .is_none());
    let before: chrono::DateTime<chrono::Utc> = client
        .query_one("SELECT updated_at FROM posts WHERE id = 101", &[])
        .await
        .unwrap()
        .get(0);
    let update = |title: &str, version: chrono::DateTime<chrono::Utc>| {
        serde_json::from_value::<UpdatePostParams>(serde_json::json!({
            "post_id": 101, "title": title, "expected_updated_at": version.to_rfc3339()
        }))
        .unwrap()
    };
    // Two writers using the same version must not both succeed.
    let (a, b) = tokio::join!(
        YggMcpServer.update_post(
            Parameters(update("writer-a", before)),
            Extension(parts(Some(TokenScope::Write)))
        ),
        YggMcpServer.update_post(
            Parameters(update("writer-b", before)),
            Extension(parts(Some(TokenScope::Write)))
        ),
    );
    assert_ne!(a.is_ok(), b.is_ok());
    let (winner, conflict) = match (a, b) {
        (Ok(winner), Err(conflict)) | (Err(conflict), Ok(winner)) => (winner, conflict),
        _ => panic!("exactly one concurrent writer must succeed"),
    };
    assert!(conflict.message.contains("conflict"));
    let winner = result_json(winner);
    let after = client
        .query_one("SELECT title, updated_at FROM posts WHERE id = 101", &[])
        .await
        .unwrap();
    assert_eq!(
        winner["updated_at"],
        serde_json::to_value(after.get::<_, chrono::DateTime<chrono::Utc>>(1)).unwrap()
    );
    assert!(YggMcpServer
        .update_post(
            Parameters(update("stale overwrite", before)),
            Extension(parts(Some(TokenScope::Write)))
        )
        .await
        .is_err());
    assert_eq!(
        client
            .query_one("SELECT title FROM posts WHERE id = 101", &[])
            .await
            .unwrap()
            .get::<_, String>(0),
        after.get::<_, String>(0)
    );
    // An edit from the web/backend also invalidates the previously read version.
    let version = after.get::<_, chrono::DateTime<chrono::Utc>>(1);
    client
        .execute(
            "UPDATE posts SET title = 'web edit', updated_at = clock_timestamp() WHERE id = 101",
            &[],
        )
        .await
        .unwrap();
    assert!(YggMcpServer
        .update_post(
            Parameters(update("stale web overwrite", version)),
            Extension(parts(Some(TokenScope::Write)))
        )
        .await
        .is_err());
    let legacy = serde_json::from_value::<UpdatePostParams>(
        serde_json::json!({"post_id":101,"title":"legacy update"}),
    )
    .unwrap();
    assert!(YggMcpServer
        .update_post(
            Parameters(legacy),
            Extension(parts(Some(TokenScope::Write)))
        )
        .await
        .is_ok());
}

#[test]
fn expected_version_requires_precise_timestamp() {
    assert!(parse_expected_updated_at(None).unwrap().is_none());
    assert!(parse_expected_updated_at(Some("2026-09-16")).is_err());
    assert!(parse_expected_updated_at(Some("invalid")).is_err());
    assert_eq!(
        parse_expected_updated_at(Some("2026-09-16T08:00:00.123456+08:00")).unwrap(),
        parse_expected_updated_at(Some("2026-09-16T00:00:00.123456Z")).unwrap()
    );
}

#[test]
fn list_validates_status_bounds_paging_and_escapes_search() {
    let mut p = ListPostsParams::default();
    assert_eq!(p.normalize().unwrap(), (1, 20, "%".repeat(2)));
    p.page = Some(i32::MIN);
    p.per_page = Some(i32::MAX);
    p.query = Some("  100%_  ".into());
    assert_eq!(p.normalize().unwrap(), (1, 50, r"%100\%\_%".into()));
    p.status = Some("unknown".into());
    assert!(p.normalize().is_err());
}

#[tokio::test]
async fn private_reads_reject_missing_and_read_only_principals_before_io() {
    for scope in [None, Some(TokenScope::Read)] {
        assert!(YggMcpServer
            .list_trashed_posts(
                Parameters(ListPostsParams::default()),
                Extension(parts(scope))
            )
            .await
            .is_err());
        assert!(YggMcpServer
            .restore_post(
                Parameters(PostIdParams { post_id: 101 }),
                Extension(parts(scope))
            )
            .await
            .is_err());
        assert!(YggMcpServer
            .list_posts(
                Parameters(ListPostsParams::default()),
                Extension(parts(scope))
            )
            .await
            .is_err());
        assert!(YggMcpServer
            .get_post_by_id(
                Parameters(PostIdParams { post_id: 1 }),
                Extension(parts(scope))
            )
            .await
            .is_err());
    }
}
