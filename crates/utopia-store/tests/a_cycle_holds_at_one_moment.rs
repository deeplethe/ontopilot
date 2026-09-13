//! 传递环要整条路径同一时刻成立（#636），打在真库上。
//!
//! 纯逻辑那部分——两两重叠而三者无交、剪枝丢不了真环——在 `utopia-reason` 里跑。
//! 这里钉的是只有过一遍取数与落库才看得见的两样：
//!
//! 1. **时间上错开的环不进 Review。** 区间是 `timed_edges` 按谓词的时间语义从库里
//!    读出来的；从前 `cycles` 只看形状，A 先属于 B、后来 B 属于 A 也报成环
//! 2. **改对了时间，环就消失。** 人在 Review 里看到环，去把其中一条的结束日期补上
//!    （302 的 `correct_interval`，作废 + 改写），重跑那一行被清掉。从前改完照样算出
//!    同一个环——人按提示修了数据，队列却纹丝不动
//!
//! 没有 `UTOPIA_DATABASE_URL` 时跳过而不是失败。自建自拆，绝不碰已有的库。

use sqlx::PgPool;
use utopia_store::graph::Validity;
use utopia_store::reasoning;
use uuid::Uuid;

struct Fixture {
    org: Uuid,
    kb: Uuid,
    etype: Uuid,
    /// state + transitive
    part_of: Uuid,
}

async fn seed(pool: &PgPool) -> anyhow::Result<Fixture> {
    let (org, ws, kb) = (Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7());
    let (etype, part_of) = (Uuid::now_v7(), Uuid::now_v7());
    sqlx::query("INSERT INTO organizations (id, name) VALUES ($1, 'cycle-time-test')")
        .bind(org)
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO workspaces (id, org_id, name) VALUES ($1, $2, 'cycle-time-test')")
        .bind(ws)
        .bind(org)
        .execute(pool)
        .await?;
    sqlx::query(
        "INSERT INTO knowledge_bases (id, workspace_id, name) VALUES ($1, $2, 'cycle-time-test')",
    )
    .bind(kb)
    .bind(ws)
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO entity_types (id, kb_id, key, label) VALUES ($1, $2, 'thing', 'Thing')",
    )
    .bind(etype)
    .bind(kb)
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO relation_types (id, kb_id, key, label, temporal, is_transitive)
         VALUES ($1, $2, 'part_of', 'part_of', 'state', TRUE)",
    )
    .bind(part_of)
    .bind(kb)
    .execute(pool)
    .await?;
    Ok(Fixture {
        org,
        kb,
        etype,
        part_of,
    })
}

async fn entity(pool: &PgPool, f: &Fixture, name: &str) -> anyhow::Result<Uuid> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO entities (id, kb_id, type_id, canonical_name) VALUES ($1, $2, $3, $4)",
    )
    .bind(id)
    .bind(f.kb)
    .bind(f.etype)
    .bind(name)
    .execute(pool)
    .await?;
    Ok(id)
}

fn day(d: &str) -> chrono::DateTime<chrono::Utc> {
    format!("{d}T00:00:00Z").parse().unwrap()
}

/// `s part_of o`，区间 `[from, to)` 按天；`to` 不给就是还在持续
async fn part_of(
    pool: &PgPool,
    f: &Fixture,
    s: Uuid,
    o: Uuid,
    from: &str,
    to: Option<&str>,
) -> anyhow::Result<Uuid> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO facts (id, kb_id, subject_id, predicate_id, object_id,
                            valid_from, valid_from_precision, valid_to, valid_to_precision)
         VALUES ($1, $2, $3, $4, $5, $6, 'day', $7, CASE WHEN $7 IS NULL THEN NULL ELSE 'day' END)",
    )
    .bind(id)
    .bind(f.kb)
    .bind(s)
    .bind(f.part_of)
    .bind(o)
    .bind(day(from))
    .bind(to.map(day))
    .execute(pool)
    .await?;
    Ok(id)
}

async fn open_cycles(pool: &PgPool, f: &Fixture) -> anyhow::Result<Vec<Vec<Uuid>>> {
    Ok(sqlx::query_scalar(
        "SELECT path FROM axiom_violations
          WHERE kb_id = $1 AND kind = 'cycle' AND status = 'open'",
    )
    .bind(f.kb)
    .fetch_all(pool)
    .await?)
}

#[tokio::test]
async fn a_cycle_is_one_moment_and_fixing_the_time_clears_it() -> anyhow::Result<()> {
    let Some(url) = utopia_store::test_db::url() else {
        return Ok(());
    };
    let pool = PgPool::connect(&url).await?;
    let f = seed(&pool).await?;

    let result = async {
        // ---- 一、时间上错开的环：两段前后接上，三段两两重叠而三者无交
        let (a, b) = (
            entity(&pool, &f, "Acme").await?,
            entity(&pool, &f, "Beta").await?,
        );
        part_of(&pool, &f, a, b, "2019-01-01", Some("2021-01-01")).await?;
        part_of(&pool, &f, b, a, "2022-01-01", None).await?;
        let (d, e, g) = (
            entity(&pool, &f, "Delta").await?,
            entity(&pool, &f, "Echo").await?,
            entity(&pool, &f, "Golf").await?,
        );
        part_of(&pool, &f, d, e, "2020-01-01", Some("2022-01-01")).await?;
        part_of(&pool, &f, e, g, "2021-01-01", Some("2023-01-01")).await?;
        part_of(&pool, &f, g, d, "2022-01-01", Some("2024-01-01")).await?;

        let r = reasoning::run(&pool, f.kb).await?;
        assert_eq!(r.edges, 5);
        assert!(
            open_cycles(&pool, &f).await?.is_empty(),
            "没有哪一刻整条成立的环不该进 Review"
        );

        // ---- 二、真正同时成立的环照报
        let (x, y) = (
            entity(&pool, &f, "Xray").await?,
            entity(&pool, &f, "Yankee").await?,
        );
        let xy = part_of(&pool, &f, x, y, "2020-01-01", None).await?;
        let yx = part_of(&pool, &f, y, x, "2021-06-01", None).await?;
        reasoning::run(&pool, f.kb).await?;
        let cycles = open_cycles(&pool, &f).await?;
        assert_eq!(cycles.len(), 1, "{cycles:?}");
        let mut expected = vec![xy, yx];
        expected.sort();
        assert_eq!(cycles[0], expected);

        // ---- 三、人去把 X part_of Y 的结束日期补上：它 2021-06-01 结束，正是
        // Y part_of X 开始的那天。改完重跑，那一行被清掉
        utopia_store::temporal::correct_interval(
            &pool,
            xy,
            Validity {
                from: Some(day("2020-01-01")),
                from_precision: Some("day"),
                to: Some(day("2021-06-01")),
                to_precision: Some("day"),
                ..Default::default()
            },
        )
        .await?
        .expect("这条还活着，应当改得动");
        let after = reasoning::run(&pool, f.kb).await?;
        assert!(
            open_cycles(&pool, &f).await?.is_empty(),
            "改对了时间，环就不在了"
        );
        assert_eq!(after.cleared, 1, "上一轮那一行是陈的");
        Ok::<_, anyhow::Error>(())
    }
    .await;
    sqlx::query("DELETE FROM organizations WHERE id = $1")
        .bind(f.org)
        .execute(&pool)
        .await?;
    result
}
