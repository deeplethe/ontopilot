//! 分页查询的稳定排序（#646）——同一次大批量插入所有行 `detected_at`
//! 相同，仅靠原 ORDER BY 无法稳定分页。加 `id DESC` 作为 tiebreaker 之后，
//! 同样的 LIMIT/OFFSET 翻页不会漏行也不会重复。
//!
//! 没有 `UTOPIA_DATABASE_URL` 时跳过而不是失败。自建自拆，绝不碰已有的库。

use sqlx::PgPool;
use utopia_store::reasoning;
use uuid::Uuid;

#[tokio::test]
async fn paged_violations_cover_every_inserted_row_exactly_once() -> anyhow::Result<()> {
    let Some(url) = utopia_store::test_db::url() else {
        return Ok(());
    };
    let pool = PgPool::connect(&url).await?;
    let (org, ws, kb) = (Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7());
    let rtype = Uuid::now_v7();
    let pred = Uuid::now_v7();
    let l: i64 = 256;

    sqlx::query("INSERT INTO organizations (id, name) VALUES ($1, 'paged-tiebreak-test')")
        .bind(org)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO workspaces (id, org_id, name) VALUES ($1, $2, 'paged-tiebreak-test')")
        .bind(ws)
        .bind(org)
        .execute(&pool)
        .await?;
    sqlx::query(
        "INSERT INTO knowledge_bases (id, workspace_id, name) VALUES ($1, $2, 'paged-tiebreak-test')",
    )
    .bind(kb)
    .bind(ws)
    .execute(&pool)
    .await?;
    sqlx::query("INSERT INTO entity_types (id, kb_id, key, label) VALUES ($1, $2, 'thing', 'Thing')")
        .bind(rtype)
        .bind(kb)
        .execute(&pool)
        .await?;
    sqlx::query(
        "INSERT INTO relation_types (id, kb_id, key, label, temporal) \
         VALUES ($1, $2, 'loops', 'loops', 'state')",
    )
    .bind(pred)
    .bind(kb)
    .execute(&pool)
    .await?;

    // 一次事务内插入 256 条 self_loop 违规。`date_trunc('second', now())` 在同事务内
    // 返回同一秒的值，把「同一 detected_at」的 tie 拉满——这正是修复前会丢行/重复
    // 行的情形。子秒精度被 `facts_from_precision_matches_date` 拒掉（#0033），所以
    // 要用精度='second' + date_trunc 这一对。`facts` 表要求 subject/object 都引用
    // `entities`，predicate 引用 `relation_types`，所以三者都要先建出来。
    let mut tx = pool.begin().await?;
    let mut inserted: Vec<Uuid> = Vec::with_capacity(l as usize);
    for _ in 0..l {
        let fact = Uuid::now_v7();
        let entity = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO entities (id, kb_id, type_id, canonical_name)
             VALUES ($1, $2, $3, 'loop-' || substr($1::text, 1, 8))",
        )
        .bind(entity)
        .bind(kb)
        .bind(rtype)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO facts (id, kb_id, subject_id, predicate_id, object_id, object_value,
                               valid_from, valid_from_precision, valid_to, confidence)
             VALUES ($1, $2, $3, $4, $3, '{\"x\":1}'::jsonb,
                     date_trunc('second', now()), 'second', NULL, 0.5)",
        )
        .bind(fact)
        .bind(kb)
        .bind(entity)
        .bind(pred)
        .execute(&mut *tx)
        .await?;
        let vid = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO axiom_violations (id, kb_id, kind, left_fact, right_fact, path, status,
                                           detected_at)
             VALUES ($1, $2, 'self_loop', $3, $3, '{}'::uuid[], 'open',
                     date_trunc('second', now()))",
        )
        .bind(vid)
        .bind(kb)
        .bind(fact)
        .execute(&mut *tx)
        .await?;
        inserted.push(vid);
    }
    tx.commit().await?;

    // 用 repo 的分页 API 把全部 open 违规捞出来——逐页加进集合，断言总数与
    // 插入的相同，且每行恰好出现一次。
    let page = 50_i64;
    let mut seen: Vec<Uuid> = Vec::with_capacity(l as usize);
    let mut offset: i64 = 0;
    loop {
        let rows = reasoning::open_violations(&pool, kb, page, offset).await?;
        if rows.is_empty() {
            break;
        }
        for v in rows {
            seen.push(v.id);
        }
        if seen.len() as i64 == l {
            break;
        }
        offset += page;
        // 兜底：万一排序出错导致死循环，256 / 50 = 6 页之后必须停止
        if offset > l * 2 {
            anyhow::bail!("paging did not terminate within 2x the row count");
        }
    }

    seen.sort();
    inserted.sort();
    assert_eq!(seen.len(), inserted.len(), "rows differ in count");
    assert_eq!(seen, inserted, "page order or duplicates disagree");

    sqlx::query("DELETE FROM axiom_violations WHERE kb_id = $1")
        .bind(kb)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM facts WHERE kb_id = $1")
        .bind(kb)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM entities WHERE kb_id = $1")
        .bind(kb)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM relation_types WHERE kb_id = $1")
        .bind(kb)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM entity_types WHERE kb_id = $1")
        .bind(kb)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM knowledge_bases WHERE id = $1")
        .bind(kb)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM workspaces WHERE id = $1")
        .bind(ws)
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM organizations WHERE id = $1")
        .bind(org)
        .execute(&pool)
        .await?;

    Ok(())
}
