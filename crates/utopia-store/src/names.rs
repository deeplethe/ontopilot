//! 名字是关于实体的一条事实（0041 决定 1）。
//!
//! 一个实体的每个名字——本名、简称、曾用名、另一种文字的写法——都是内建属性
//! `known_as` 上的一条值事实：`object_value = {"value": 名字}`，`object_id` 为空。
//! 它有出处（证据引文）、两个时钟、可以有有效期（更名），能被撤回，合并时跟着
//! 其它事实一起搬、撤回合并时一起搬回去。**它是值，不是节点**：画布只画带
//! `object_id` 的边。
//!
//! 读的时候要分清两件事，这个模块给两种 SQL 片段：
//! - 召回：`has_name_in` —— 这个实体有没有叫这些名字之一的（含曾用名：世界轴上
//!   结束了的名字仍然认得出旧文档里的它，所以只看记录轴 `invalidated_at`）；
//! - 列事实：`not_a_name` —— 实体面板、审阅卡、度数、规则这些地方，名字不算
//!   一条「事实」，不能冒充证据，也不能把计数撑大。

use crate::graph::{add_evidence, insert_value_fact, Validity};
use crate::resolution::normalize_name;
use sqlx::PgPool;
use utopia_core::models::{NameView, RelationType};
use utopia_core::AppResult;
use uuid::Uuid;

/// 内建属性的 key。与 `is_a` 同一个做法：`builtin = TRUE`，建库时不铺，第一次要时建
pub const KNOWN_AS: &str = "known_as";

/// 这条关系类型是不是名字属性。给模型看的清单、本体引导、本体向量索引都要把它拿掉：
/// 名字走自己的通道（抽取回复里的 `names`，服务端核对它在原文里），不能当一条普通属性
/// 被模型直接写进来——那样就绕过了核对
pub fn is_name_attribute(r: &RelationType) -> bool {
    r.builtin && r.key == KNOWN_AS
}

/// 取（没有就建）这个库的 `known_as` 属性。
pub async fn ensure_known_as(pool: &PgPool, kb_id: Uuid) -> AppResult<Uuid> {
    if let Some((id,)) =
        sqlx::query_as::<_, (Uuid,)>("SELECT id FROM relation_types WHERE kb_id = $1 AND key = $2")
            .bind(kb_id)
            .bind(KNOWN_AS)
            .fetch_optional(pool)
            .await?
    {
        return Ok(id);
    }
    sqlx::query(
        "INSERT INTO relation_types (id, kb_id, key, label, kind, datatype, temporal, builtin, description)
         VALUES ($1, $2, $3, 'known as', 'attribute', 'text', 'state', TRUE, $4)
         ON CONFLICT (kb_id, key) DO NOTHING",
    )
    .bind(Uuid::now_v7())
    .bind(kb_id)
    .bind(KNOWN_AS)
    .bind(
        "A name a text uses for this entity. Each name is a fact with its source and both clocks; \
         a name is a value and never becomes a node.",
    )
    .execute(pool)
    .await?;
    // ON CONFLICT 命中说明并发建过了，取回那一条
    let (id,): (Uuid,) =
        sqlx::query_as("SELECT id FROM relation_types WHERE kb_id = $1 AND key = $2")
            .bind(kb_id)
            .bind(KNOWN_AS)
            .fetch_one(pool)
            .await?;
    Ok(id)
}

/// 一个名字的出处：哪一块、引文。没有出处的名字（人在界面上写的、迁移补的）传 None
#[derive(Debug, Clone, Copy)]
pub struct NameSource<'a> {
    pub chunk_id: Uuid,
    pub quote: &'a str,
}

/// 记下「这段文字管这个实体叫这个名字」。返回名字事实的 id；名字是空的返回 None。
///
/// 同一个实体同一个名字只有一行（`insert_value_fact` 按主语、谓词、值去重），
/// 再被提到只是多一条证据、证据日期往早挪。
pub async fn record(
    pool: &PgPool,
    kb_id: Uuid,
    entity_id: Uuid,
    name: &str,
    source: Option<NameSource<'_>>,
    attested_at: Option<chrono::DateTime<chrono::Utc>>,
) -> AppResult<Option<Uuid>> {
    let name = normalize_name(name);
    if name.is_empty() {
        return Ok(None);
    }
    let attr = ensure_known_as(pool, kb_id).await?;
    let (fact_id, _) = insert_value_fact(
        pool,
        kb_id,
        entity_id,
        Some(attr),
        &serde_json::json!({ "value": name }),
        Validity {
            attested_at,
            ..Default::default()
        },
        1.0,
    )
    .await?;
    if let Some(s) = source {
        add_evidence(pool, fact_id, s.chunk_id, Some(s.quote), Some(KNOWN_AS)).await?;
    }
    Ok(Some(fact_id))
}

/// 召回用：实体 `{entity}` 有一条现行的名字事实，小写值在第 `${param}` 个参数（text[]）里。
///
/// 只看记录轴：世界轴上结束了的名字（曾用名）照样召回——更名之前的文档还在用它。
pub fn has_name_in(entity: &str, param: usize) -> String {
    format!(
        "EXISTS (SELECT 1 FROM facts nf
                   JOIN relation_types nr ON nr.id = nf.predicate_id
                  WHERE nf.subject_id = {entity}.id AND nr.builtin AND nr.key = '{KNOWN_AS}'
                    AND nf.invalidated_at IS NULL
                    AND lower(nf.object_value->>'value') = ANY(${param}))"
    )
}

/// 查找用：实体 `{entity}` 有一条现行的名字事实 ILIKE 第 `${param}` 个参数。
pub fn has_name_like(entity: &str, param: usize) -> String {
    format!(
        "EXISTS (SELECT 1 FROM facts nf
                   JOIN relation_types nr ON nr.id = nf.predicate_id
                  WHERE nf.subject_id = {entity}.id AND nr.builtin AND nr.key = '{KNOWN_AS}'
                    AND nf.invalidated_at IS NULL
                    AND nf.object_value->>'value' ILIKE ${param})"
    )
}

/// 列事实用：事实 `{fact}` 不是名字事实。
pub fn not_a_name(fact: &str) -> String {
    format!(
        "NOT EXISTS (SELECT 1 FROM relation_types nr
                      WHERE nr.id = {fact}.predicate_id AND nr.builtin AND nr.key = '{KNOWN_AS}')"
    )
}

/// 一个实体的名字栏：本名在前，其余按首次记下的先后。`as_of` 回放到当时库里记着的名字。
pub async fn for_entity(
    pool: &PgPool,
    kb_id: Uuid,
    entity_id: Uuid,
    as_of: Option<chrono::DateTime<chrono::Utc>>,
) -> AppResult<Vec<NameView>> {
    Ok(sqlx::query_as(&format!(
        "SELECT f.id AS fact_id, f.object_value->>'value' AS name,
                lower(f.object_value->>'value') = lower(e.canonical_name) AS canonical,
                f.recorded_at, f.valid_from, f.valid_from_precision, f.valid_to, f.valid_to_precision,
                ARRAY(SELECT DISTINCT fe.document_id FROM fact_evidence fe
                       WHERE fe.fact_id = f.id AND fe.document_id IS NOT NULL
                       ORDER BY fe.document_id) AS document_ids,
                (SELECT count(*) FROM fact_evidence fe WHERE fe.fact_id = f.id) AS evidence_count
           FROM facts f
           JOIN relation_types r ON r.id = f.predicate_id
           JOIN entities e ON e.id = $2
          WHERE f.kb_id = $1 AND r.builtin AND r.key = '{KNOWN_AS}'
            AND {owner} = $2 AND {held}
          ORDER BY canonical DESC, f.recorded_at",
        owner = crate::record_axis::owner_at("f", "subject_id", as_of.map(|_| 3), false),
        held = crate::record_axis::facts_held_at("f", 3),
    ))
    .bind(kb_id)
    .bind(entity_id)
    .bind(as_of)
    .fetch_all(pool)
    .await?)
}
