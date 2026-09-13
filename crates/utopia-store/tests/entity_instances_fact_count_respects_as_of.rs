//! 实体面板右侧的实例列表 fact_count 按记录轴回放（#307 / 0019）。
//!
//! 之前 `entity_instances` 写死 `f.invalidated_at IS NULL`：三月入库的
//! 事实，三月时 `fact_count` 应该数；今天再去看同一个实体，`fact_count`
//! 只数今天还活着的那几条——同一库不同时间，数却一样。两个实体的对称检验：
//!
//! - 一个实体上**写一条、立刻作废**——今天看是 0；as_of 在作废**之前**看是 1
//! - 另一个实体上**写一条、没作废**——今天看是 1；as_of 在写入**之前**看是 0
//!
//! 没有 `UTOPIA_DATABASE_URL` 时跳过而不是失败。自建自拆，绝不碰已有的库。

use sqlx::PgPool;
use uuid::Uuid;

fn t(s: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(s).unwrap().to_utc()
}

#[tokio::test]
async fn entity_instances_fact_count_respects_as_of() -> anyhow::Result<()> {
    let Some(url) = utopia_store::test_db::url() else {
        return Ok(());
    };
    let pool = PgPool::connect(&url).await?;
    let (org, ws, kb, etype) = (
        Uuid::now_v7(),
        Uuid::now_v7(),
        Uuid::now_v7(),
        Uuid::now_v7(),
    );
    let (e_retracted, e_live, peer) = (Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7());
    let rtype = Uuid::now_v7();

    sqlx::query("INSERT INTO organizations (id, name) VALUES ($1, 'as-of-factcount-test')")
        .bind(org)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO workspaces (id, org_id, name) VALUES ($1, $2, 'as-of-factcount-test')")
        .bind(ws)
        .bind(org)
        .execute(&pool)
        .await?;
    sqlx::query(
        "INSERT INTO knowledge_bases (id, workspace_id, name) VALUES ($1, $2, 'as-of-factcount-test')",
    )
    .bind(kb)
    .bind(ws)
    .execute(&pool)
    .await?;
    sqlx::query("INSERT INTO entity_types (id, kb_id, key, label) VALUES ($1, $2, 'person', 'Person')")
        .bind(etype)
        .bind(kb)
        .execute(&pool)
        .await?;
    for id in [e_retracted, e_live, peer] {
        sqlx::query(
            "INSERT INTO entities (id, kb_id, type_id, canonical_name)
             VALUES ($1, $2, $3, 'e-' || substr($1::text, 1, 8))",
        )
        .bind(id)
        .bind(kb)
        .bind(etype)
        .execute(&pool)
        .await?;
    }
    sqlx::query(
        "INSERT INTO relation_types (id, kb_id, key, label) VALUES ($1, $2, 'knows', 'knows')",
    )
    .bind(rtype)
    .bind(kb)
    .execute(&pool)
    .await?;

    // 2 月 e_retracted 写下"认识 peer"，3 月作废
    let f_retracted = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO facts (id, kb_id, subject_id, predicate_id, object_id,
                           confidence, recorded_at)
         VALUES ($1, $2, $3, $4, $5, 0.9, $6)",
    )
    .bind(f_retracted)
    .bind(kb)
    .bind(e_retracted)
    .bind(rtype)
    .bind(peer)
    .bind(t("2026-02-15T00:00:00Z"))
    .execute(&pool)
    .await?;
    sqlx::query(
        "UPDATE facts SET invalidated_at = $2
          WHERE id = $1 AND invalidated_at IS NULL",
    )
    .bind(f_retracted)
    .bind(t("2026-03-10T00:00:00Z"))
    .execute(&pool)
    .await?;
    // 4 月 e_live 写下"认识 peer"，没有作废
    sqlx::query(
        "INSERT INTO facts (id, kb_id, subject_id, predicate_id, object_id,
                           confidence, recorded_at)
         VALUES ($1, $2, $3, $4, $5, 0.9, $6)",
    )
    .bind(Uuid::now_v7())
    .bind(kb)
    .bind(e_live)
    .bind(rtype)
    .bind(peer)
    .bind(t("2026-04-15T00:00:00Z"))
    .execute(&pool)
    .await?;

    // 现状：今天看，e_retracted 的 fact_count = 0，e_live 的 fact_count = 1
    let now_rows = utopia_store::ontology::entity_instances(&pool, kb, etype, 50, 0, None).await?;
    let fc_now = |id: Uuid| -> i64 {
        now_rows
            .0
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.fact_count)
            .unwrap_or(-1)
    };
    assert_eq!(fc_now(e_retracted), 0, "已作废的实体今天 fact_count=0");
    assert_eq!(fc_now(e_live), 1, "未作废的实体今天 fact_count=1");

    // as_of 在 2 月与 3 月之间：e_retracted 已记下、未作废→1；e_live 还没写→0
    let feb_rows = utopia_store::ontology::entity_instances(
        &pool,
        kb,
        etype,
        50,
        0,
        Some(t("2026-02-20T00:00:00Z")),
    )
    .await?;
    let fc_feb = |id: Uuid| -> i64 {
        feb_rows
            .0
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.fact_count)
            .unwrap_or(-1)
    };
    assert_eq!(fc_feb(e_retracted), 1, "2 月那条当时还活着");
    assert_eq!(fc_feb(e_live), 0, "4 月那条 2 月时还没记下");

    // as_of 在 4 月 1 日：e_retracted 已作废→0；e_live 还没记下→0
    let mar_rows = utopia_store::ontology::entity_instances(
        &pool,
        kb,
        etype,
        50,
        0,
        Some(t("2026-04-01T00:00:00Z")),
    )
    .await?;
    let fc_mar = |id: Uuid| -> i64 {
        mar_rows
            .0
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.fact_count)
            .unwrap_or(-1)
    };
    assert_eq!(fc_mar(e_retracted), 0, "3 月已作废，回放 4 月时为 0");
    assert_eq!(fc_mar(e_live), 0, "4 月那条当时还没记下");

    // as_of 在 4 月之后：e_live 已记下→1
    let may_rows = utopia_store::ontology::entity_instances(
        &pool,
        kb,
        etype,
        50,
        0,
        Some(t("2026-05-01T00:00:00Z")),
    )
    .await?;
    let fc_may = |id: Uuid| -> i64 {
        may_rows
            .0
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.fact_count)
            .unwrap_or(-1)
    };
    assert_eq!(fc_may(e_live), 1, "4 月之后回放，未作废的实体为 1");

    sqlx::query("DELETE FROM organizations WHERE id = $1")
        .bind(org)
        .execute(&pool)
        .await?;
    Ok(())
}
