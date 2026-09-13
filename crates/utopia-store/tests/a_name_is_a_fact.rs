//! 名字是关于实体的一条事实（0041 决定 1）。
//!
//! 建实体时本名就是一条名字事实；记下的别名让后来的提及找得到它；名字不进事实列表、
//! 不算度数；合并把名字当普通事实搬走，撤回合并再搬回来。

use sqlx::PgPool;
use utopia_store::{graph, names, resolution};
use uuid::Uuid;

struct Fixture {
    org: Uuid,
    kb: Uuid,
    equipment: Uuid,
}

async fn seed(pool: &PgPool) -> anyhow::Result<Fixture> {
    let (org, ws, kb, equipment) = (
        Uuid::now_v7(),
        Uuid::now_v7(),
        Uuid::now_v7(),
        Uuid::now_v7(),
    );
    sqlx::query("INSERT INTO organizations (id, name) VALUES ($1, 'names-test')")
        .bind(org)
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO workspaces (id, org_id, name) VALUES ($1, $2, 'names-test')")
        .bind(ws)
        .bind(org)
        .execute(pool)
        .await?;
    sqlx::query(
        "INSERT INTO knowledge_bases (id, workspace_id, name) VALUES ($1, $2, 'names-test')",
    )
    .bind(kb)
    .bind(ws)
    .execute(pool)
    .await?;
    sqlx::query("INSERT INTO entity_types (id, kb_id, key, label) VALUES ($1, $2, 'equipment', 'Equipment')")
        .bind(equipment)
        .bind(kb)
        .execute(pool)
        .await?;
    Ok(Fixture { org, kb, equipment })
}

async fn teardown(pool: &PgPool, f: &Fixture) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM knowledge_bases WHERE id = $1")
        .bind(f.kb)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM organizations WHERE id = $1")
        .bind(f.org)
        .execute(pool)
        .await?;
    Ok(())
}

async fn mention(pool: &PgPool, f: &Fixture, name: &str) -> anyhow::Result<resolution::Resolution> {
    // 不给向量：召回到候选就走「并到事实最多的那个」，量的正是召回找不找得到
    Ok(resolution::resolve_mention(pool, f.kb, Some(f.equipment), name, None, None, &[]).await?)
}

fn values(v: &[utopia_core::models::NameView]) -> Vec<String> {
    let mut out: Vec<String> = v.iter().map(|n| n.name.clone()).collect();
    out.sort();
    out
}

#[tokio::test]
async fn a_new_entity_carries_its_name_and_an_alias_brings_a_later_mention_home(
) -> anyhow::Result<()> {
    let Some(url) = utopia_store::test_db::url() else {
        return Ok(());
    };
    let pool = PgPool::connect(&url).await?;
    let f = seed(&pool).await?;
    let run = async {
        let probe = mention(&pool, &f, "海洋探测器1号").await?;
        assert!(probe.created);
        let n = names::for_entity(&pool, f.kb, probe.entity_id, None).await?;
        assert_eq!(values(&n), vec!["海洋探测器1号"], "本名是一条名字事实");
        assert!(n[0].canonical);

        // 没有桥的简称：另起一个实体（这正是 0041 要修的，这一刀靠记下名字修）
        let before = mention(&pool, &f, "海探2").await?;
        assert!(before.created && before.entity_id != probe.entity_id);

        names::record(&pool, f.kb, probe.entity_id, "海探1", None, None).await?;
        let later = mention(&pool, &f, "海探1").await?;
        assert!(!later.created, "记下的别名让后来的提及找得到它");
        assert_eq!(later.entity_id, probe.entity_id);
        assert_eq!(
            resolution::existing_by_name(&pool, f.kb, "海探1").await?,
            Some(probe.entity_id)
        );

        // 同一个名字再记一次只有一行
        names::record(&pool, f.kb, probe.entity_id, " 海探1 ", None, None).await?;
        let n = names::for_entity(&pool, f.kb, probe.entity_id, None).await?;
        assert_eq!(values(&n), vec!["海探1", "海洋探测器1号"]);
        assert_eq!(
            names::record(&pool, f.kb, probe.entity_id, "  ", None, None).await?,
            None
        );
        anyhow::Ok(())
    }
    .await;
    teardown(&pool, &f).await?;
    run
}

#[tokio::test]
async fn a_name_is_neither_a_listed_fact_nor_a_degree() -> anyhow::Result<()> {
    let Some(url) = utopia_store::test_db::url() else {
        return Ok(());
    };
    let pool = PgPool::connect(&url).await?;
    let f = seed(&pool).await?;
    let run = async {
        let probe = mention(&pool, &f, "海洋探测器1号").await?;
        names::record(&pool, f.kb, probe.entity_id, "海探1", None, None).await?;
        let (node, facts) = graph::entity_detail(&pool, f.kb, probe.entity_id, None, None).await?;
        assert!(facts.is_empty(), "名字不进事实列表：{facts:?}");
        assert_eq!(node.degree, 0, "名字不算度数");
        anyhow::Ok(())
    }
    .await;
    teardown(&pool, &f).await?;
    run
}

#[tokio::test]
async fn merging_moves_names_and_reverting_brings_them_back() -> anyhow::Result<()> {
    let Some(url) = utopia_store::test_db::url() else {
        return Ok(());
    };
    let pool = PgPool::connect(&url).await?;
    let f = seed(&pool).await?;
    let run = async {
        let full = mention(&pool, &f, "海洋探测器1号").await?.entity_id;
        let short = mention(&pool, &f, "海探1").await?.entity_id;
        assert_ne!(full, short);

        let merge = resolution::merge_entities(&pool, f.kb, short, full, None, "test").await?;
        assert_eq!(
            values(&names::for_entity(&pool, f.kb, full, None).await?),
            vec!["海探1", "海洋探测器1号"],
            "被并方的名字随它的事实搬过来"
        );
        assert_eq!(
            resolution::existing_by_name(&pool, f.kb, "海探1").await?,
            Some(full),
            "合并之后按简称找到的是存活者"
        );

        resolution::revert_merge(&pool, f.kb, merge).await?;
        assert_eq!(
            values(&names::for_entity(&pool, f.kb, short, None).await?),
            vec!["海探1"]
        );
        assert_eq!(
            values(&names::for_entity(&pool, f.kb, full, None).await?),
            vec!["海洋探测器1号"],
            "撤回合并，名字搬回去"
        );
        anyhow::Ok(())
    }
    .await;
    teardown(&pool, &f).await?;
    run
}

#[tokio::test]
async fn a_name_another_entity_already_has_queues_the_pair_without_merging() -> anyhow::Result<()> {
    let Some(url) = utopia_store::test_db::url() else {
        return Ok(());
    };
    let pool = PgPool::connect(&url).await?;
    let f = seed(&pool).await?;
    let run = async {
        // 倒序到达：只写简称的那篇先建出「海探1」，写着全称的那篇后到
        let short = mention(&pool, &f, "海探1").await?.entity_id;
        let full = mention(&pool, &f, "海洋探测器1号").await?.entity_id;
        names::record(&pool, f.kb, full, "海探1", None, None).await?;
        assert_eq!(
            names::pair_shared_name(&pool, f.kb, full, "海探1").await?,
            1
        );
        let (reason, stage, status): (String, String, String) = sqlx::query_as(
            "SELECT reason, stage, status FROM resolution_reviews
              WHERE kb_id = $1 AND least(left_id, right_id) = least($2, $3)
                AND greatest(left_id, right_id) = greatest($2, $3)",
        )
        .bind(f.kb)
        .bind(short)
        .bind(full)
        .fetch_one(&pool)
        .await?;
        assert_eq!(reason, "shared_name|海探1");
        assert_eq!(
            (stage.as_str(), status.as_str()),
            ("adjudicating", "pending")
        );
        let merged: Option<Uuid> =
            sqlx::query_scalar("SELECT merged_into FROM entities WHERE id = $1")
                .bind(short)
                .fetch_one(&pool)
                .await?;
        assert_eq!(merged, None, "只排队，不合并");
        // 再报一次同一个名字不重复排
        assert_eq!(
            names::pair_shared_name(&pool, f.kb, full, "海探1").await?,
            1
        );
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM resolution_reviews WHERE kb_id = $1")
            .bind(f.kb)
            .fetch_one(&pool)
            .await?;
        assert_eq!(n, 1);
        anyhow::Ok(())
    }
    .await;
    teardown(&pool, &f).await?;
    run
}

#[tokio::test]
async fn the_adjudicator_sees_the_other_names_and_never_the_shared_one() -> anyhow::Result<()> {
    let Some(url) = utopia_store::test_db::url() else {
        return Ok(());
    };
    let pool = PgPool::connect(&url).await?;
    let f = seed(&pool).await?;
    let run = async {
        let full = mention(&pool, &f, "海洋探测器1号").await?.entity_id;
        names::record(&pool, f.kb, full, "海探1", None, None).await?;
        let lines = resolution::entity_fact_lines(&pool, f.kb, full, 4).await?;
        assert_eq!(lines, vec!["also known as: 海探1".to_string()]);
        let bare = mention(&pool, &f, "海探2").await?.entity_id;
        assert!(
            resolution::entity_fact_lines(&pool, f.kb, bare, 4)
                .await?
                .is_empty(),
            "只有本名的实体没有「又名」这一行"
        );
        anyhow::Ok(())
    }
    .await;
    teardown(&pool, &f).await?;
    run
}
