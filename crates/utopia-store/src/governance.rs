//! 治理（0025）：agent 先读台账再裁。
//!
//! 三件事：**先例**——这个库里的人对这一对、这个名字、这种类型对做过什么；
//! **队列**——等人的重复对按先进先出排，队头带着它的簇（同名或同实体的其他对）
//! 一起走；**闸门**——一个纯函数，说这条判决是自己动手还是写成建议。模型调用在
//! server 的 `governance` 任务里，这里不碰模型。
//!
//! 先例只认人的决定（actor 不为空）。agent 自己裁过的不算——否则它引用自己，
//! 越判越自信。人接受了 agent 的建议算：那一笔是人签的，走的是人的裁决路径。

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use utopia_core::models::{AgentDecisionView, ReviewItem};
use utopia_core::{AppError, AppResult};
use uuid::Uuid;

use crate::resolution::{assemble_reviews, ReviewRow};

/// 自己动手的置信度线。比攒批裁决器的 0.8 高一点：那一档只看这一对，这里是替人做主
pub const AUTO_CONF: f32 = 0.85;
/// 历史同意时的线：人对这一对、这个名字或这种类型对做过同样的决定，agent 的把握可以低一点
pub const SUPPORTED_CONF: f32 = 0.75;
/// 类型对的习惯要有多少笔人的决定才算数
pub const TYPE_PAIR_MIN: i64 = 5;
/// 每族先例最多带几条进提示词
const PER_FAMILY: i64 = 8;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Precedent {
    pub event_id: Uuid,
    /// review.merge | review.keep | merge.manual | merge.revert
    pub action: String,
    pub left: String,
    pub right: String,
    pub at: DateTime<Utc>,
}

impl Precedent {
    /// 这一笔是把两个记录合了，还是分开了
    pub fn merged(&self) -> bool {
        matches!(self.action.as_str(), "review.merge" | "merge.manual")
    }
}

#[derive(Debug, Clone, Default, Serialize, sqlx::FromRow)]
pub struct TypePairStats {
    pub merged: i64,
    pub kept: i64,
    pub reverted: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Precedents {
    /// 人对这两个名字（不分左右）做过的决定
    pub same_pair: Vec<Precedent>,
    /// 其中一个名字对别的名字：这个名字是重名磁铁，还是一直被合
    pub same_name: Vec<Precedent>,
    /// 这个库对这种类型对的习惯；一侧没类型就没有
    pub type_pair: Option<TypePairStats>,
    /// 涉及任一名字的撤回：有一笔就只建议不动手
    pub reverts: Vec<Precedent>,
}

/// 台账里两族 detail 的写法：review.* 记 left/right，merge.* 记 source/target
const L: &str = "lower(COALESCE(detail->>'left', detail->>'source', ''))";
const R: &str = "lower(COALESCE(detail->>'right', detail->>'target', ''))";
const COLS: &str = "id AS event_id, action,
         COALESCE(detail->>'left', detail->>'source', '') AS \"left\",
         COALESCE(detail->>'right', detail->>'target', '') AS \"right\",
         created_at AS at";

pub async fn precedents_for(
    pool: &PgPool,
    kb_id: Uuid,
    item: &ReviewItem,
) -> AppResult<Precedents> {
    let (a, b) = (
        item.left.name.to_lowercase(),
        item.right.name.to_lowercase(),
    );
    let is_pair = format!("(({L} = $2 AND {R} = $3) OR ({L} = $3 AND {R} = $2))");
    let touches = format!("({L} IN ($2, $3) OR {R} IN ($2, $3))");

    let same_pair: Vec<Precedent> = sqlx::query_as(&format!(
        "SELECT {COLS} FROM audit_events
         WHERE kb_id = $1 AND actor_id IS NOT NULL
           AND action IN ('review.merge', 'review.keep', 'merge.manual') AND {is_pair}
         ORDER BY created_at DESC LIMIT $4"
    ))
    .bind(kb_id)
    .bind(&a)
    .bind(&b)
    .bind(PER_FAMILY)
    .fetch_all(pool)
    .await?;

    let same_name: Vec<Precedent> = sqlx::query_as(&format!(
        "SELECT {COLS} FROM audit_events
         WHERE kb_id = $1 AND actor_id IS NOT NULL
           AND action IN ('review.merge', 'review.keep', 'merge.manual')
           AND {touches} AND NOT {is_pair}
         ORDER BY created_at DESC LIMIT $4"
    ))
    .bind(kb_id)
    .bind(&a)
    .bind(&b)
    .bind(PER_FAMILY)
    .fetch_all(pool)
    .await?;

    let reverts: Vec<Precedent> = sqlx::query_as(&format!(
        "SELECT {COLS} FROM audit_events
         WHERE kb_id = $1 AND action = 'merge.revert' AND {touches}
         ORDER BY created_at DESC LIMIT $4"
    ))
    .bind(kb_id)
    .bind(&a)
    .bind(&b)
    .bind(PER_FAMILY)
    .fetch_all(pool)
    .await?;

    let types: Vec<(Uuid, Option<Uuid>)> =
        sqlx::query_as("SELECT id, type_id FROM entities WHERE id = ANY($1)")
            .bind(vec![item.left.id, item.right.id])
            .fetch_all(pool)
            .await?;
    let type_of = |id: Uuid| types.iter().find(|t| t.0 == id).and_then(|t| t.1);
    let type_pair = match (type_of(item.left.id), type_of(item.right.id)) {
        (Some(ta), Some(tb)) => Some(type_pair_stats(pool, kb_id, ta, tb).await?),
        _ => None,
    };

    Ok(Precedents {
        same_pair,
        same_name,
        type_pair,
        reverts,
    })
}

/// 这个库对这种类型对的习惯：人合了多少、分了多少、合了又撤回多少
async fn type_pair_stats(
    pool: &PgPool,
    kb_id: Uuid,
    ta: Uuid,
    tb: Uuid,
) -> AppResult<TypePairStats> {
    let (merged, kept): (i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE rr.status = 'merged'),
                count(*) FILTER (WHERE rr.status = 'kept')
         FROM resolution_reviews rr
         JOIN entities a ON a.id = rr.left_id
         JOIN entities b ON b.id = rr.right_id
         WHERE rr.kb_id = $1 AND rr.decided_by IS NOT NULL
           AND ((a.type_id = $2 AND b.type_id = $3) OR (a.type_id = $3 AND b.type_id = $2))",
    )
    .bind(kb_id)
    .bind(ta)
    .bind(tb)
    .fetch_one(pool)
    .await?;
    let reverted: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM entity_merges m
         JOIN entities s ON s.id = m.source_id
         JOIN entities t ON t.id = m.target_id
         WHERE m.kb_id = $1 AND m.reverted_at IS NOT NULL
           AND ((s.type_id = $2 AND t.type_id = $3) OR (s.type_id = $3 AND t.type_id = $2))",
    )
    .bind(kb_id)
    .bind(ta)
    .bind(tb)
    .fetch_one(pool)
    .await?;
    Ok(TypePairStats {
        merged,
        kept,
        reverted,
    })
}

/// 先例写成提示词里的行。英文：读者是模型
pub fn render_lines(p: &Precedents) -> Vec<String> {
    let verb = |x: &Precedent| if x.merged() { "merged" } else { "kept apart" };
    let day = |t: &DateTime<Utc>| t.format("%Y-%m-%d").to_string();
    let mut out = Vec::new();
    for x in &p.same_pair {
        out.push(format!(
            "this same pair was {} by a person on {}",
            verb(x),
            day(&x.at)
        ));
    }
    for x in &p.same_name {
        out.push(format!(
            "\"{}\" against \"{}\" was {} by a person on {}",
            x.left,
            x.right,
            verb(x),
            day(&x.at)
        ));
    }
    if let Some(t) = &p.type_pair {
        if t.merged + t.kept > 0 {
            out.push(format!(
                "for pairs of these two types in this base, people merged {}, kept {} apart, and later reverted {} merge(s)",
                t.merged, t.kept, t.reverted
            ));
        }
    }
    for x in &p.reverts {
        out.push(format!(
            "a merge of \"{}\" into \"{}\" was reverted by a person on {}",
            x.left,
            x.right,
            day(&x.at)
        ));
    }
    out
}

/// 落进 agent_decisions.precedents 的样子：每条带一个 family
pub fn precedents_json(p: &Precedents) -> serde_json::Value {
    let tag = |family: &str, xs: &[Precedent]| {
        xs.iter()
            .map(|x| {
                serde_json::json!({
                    "family": family, "event_id": x.event_id, "action": x.action,
                    "left": x.left, "right": x.right, "at": x.at,
                })
            })
            .collect::<Vec<_>>()
    };
    let mut all = tag("same_pair", &p.same_pair);
    all.extend(tag("same_name", &p.same_name));
    all.extend(tag("revert", &p.reverts));
    if let Some(t) = &p.type_pair {
        all.push(serde_json::json!({
            "family": "type_pair", "merged": t.merged, "kept": t.kept, "reverted": t.reverted,
        }));
    }
    serde_json::Value::Array(all)
}

#[derive(Debug, PartialEq, Eq)]
pub enum Gate {
    /// 自己动手：合并可撤，分开可再合
    Apply,
    /// 写成建议留给人
    Propose,
}

impl Precedents {
    /// 历史同意这个判决：这一对被人这么裁过、这个名字每次都这么裁、或这种类型对有
    /// 这个习惯（够 TYPE_PAIR_MIN 笔；合并的习惯还要求没撤回过）
    pub fn supports(&self, same: bool) -> bool {
        let pair = self.same_pair.iter().any(|x| x.merged() == same);
        let name = !self.same_name.is_empty() && self.same_name.iter().all(|x| x.merged() == same);
        let habit = self.type_pair.as_ref().is_some_and(|t| {
            t.merged + t.kept >= TYPE_PAIR_MIN
                && if same {
                    t.merged >= t.kept && t.reverted == 0
                } else {
                    t.kept > t.merged
                }
        });
        pair || name || habit
    }
}

/// agent 按自己的把握判，历史是参考（0025 决定 4，2026-09-06 修订）。历史反对就拦住：
/// 人分开过这一对却说合、合过却说分、涉及任一名字的撤回。历史同意就把线放低。其余
/// 按 agent 自己的把握。类型冲突永远不合——那是规则，不是把握
pub fn gate(same: Option<bool>, conf: f32, types_conflict: bool, p: &Precedents) -> Gate {
    let Some(same) = same else {
        return Gate::Propose;
    };
    if !p.reverts.is_empty() || p.same_pair.iter().any(|x| x.merged() != same) {
        return Gate::Propose;
    }
    if same && types_conflict {
        return Gate::Propose;
    }
    let bar = if p.supports(same) {
        SUPPORTED_CONF
    } else {
        AUTO_CONF
    };
    if conf >= bar {
        Gate::Apply
    } else {
        Gate::Propose
    }
}

/// 同一对实体，人裁过没有：Some(true) 合过、Some(false) 分过、None 没裁过。
/// 按实体 id 认，不按名字——名字一样的两个「张伟」不是同一个问题
pub async fn decided_before(
    pool: &PgPool,
    kb_id: Uuid,
    a: Uuid,
    b: Uuid,
) -> AppResult<Option<bool>> {
    let merged: Option<bool> = sqlx::query_scalar(
        "SELECT status = 'merged' FROM resolution_reviews
         WHERE kb_id = $1 AND decided_by IS NOT NULL AND status IN ('merged', 'kept')
           AND ((left_id = $2 AND right_id = $3) OR (left_id = $3 AND right_id = $2))
         ORDER BY decided_at DESC LIMIT 1",
    )
    .bind(kb_id)
    .bind(a)
    .bind(b)
    .fetch_optional(pool)
    .await?;
    Ok(merged)
}

/// 人已经定过的一对，照人的定：同一对实体裁过；或者名字不同的一对名字，人对这一对
/// 名字的决定一致（「OpenAI OpCo ≟ OpenAI」合过就是合）。同名的一对不算——那只说明
/// 库里有过重名，不说明眼前这两个是谁。有撤回的不走这条路
pub fn settled_by_people(
    left_name: &str,
    right_name: &str,
    p: &Precedents,
    prior: Option<bool>,
) -> Option<bool> {
    if !p.reverts.is_empty() {
        return None;
    }
    if let Some(m) = prior {
        return Some(m);
    }
    if left_name.to_lowercase() == right_name.to_lowercase() {
        return None;
    }
    let mut it = p.same_pair.iter();
    let first = it.next()?.merged();
    it.all(|x| x.merged() == first).then_some(first)
}

/// 有一条开着的建议的对不进队列：agent 已经问过了，等人答
const OPEN_PROPOSAL: &str = "NOT EXISTS (SELECT 1 FROM agent_decisions d
    WHERE d.target_kind = 'review' AND d.target_id = rr.id AND d.status = 'proposed')";

/// 等人的重复对，先进先出
pub async fn queue(pool: &PgPool, kb_id: Uuid, limit: i64) -> AppResult<Vec<ReviewItem>> {
    let rows: Vec<ReviewRow> = sqlx::query_as(&format!(
        "SELECT rr.id, rr.left_id, rr.right_id, rr.score, rr.reason, rr.stage, rr.created_at
         FROM resolution_reviews rr
         WHERE rr.kb_id = $1 AND rr.status = 'pending' AND {OPEN_PROPOSAL}
         ORDER BY rr.created_at, rr.id LIMIT $2"
    ))
    .bind(kb_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    assemble_reviews(pool, kb_id, rows).await
}

/// 队头的簇：与它同名或同实体的其他等人的对，同样先进先出
pub async fn cluster_of(
    pool: &PgPool,
    kb_id: Uuid,
    head: &ReviewItem,
    limit: i64,
) -> AppResult<Vec<ReviewItem>> {
    let rows: Vec<ReviewRow> = sqlx::query_as(&format!(
        "SELECT rr.id, rr.left_id, rr.right_id, rr.score, rr.reason, rr.stage, rr.created_at
         FROM resolution_reviews rr
         JOIN entities a ON a.id = rr.left_id
         JOIN entities b ON b.id = rr.right_id
         WHERE rr.kb_id = $1 AND rr.status = 'pending' AND rr.id <> $2 AND {OPEN_PROPOSAL}
           AND (rr.left_id = ANY($3) OR rr.right_id = ANY($3)
                OR lower(a.canonical_name) = ANY($4) OR lower(b.canonical_name) = ANY($4))
         ORDER BY rr.created_at, rr.id LIMIT $5"
    ))
    .bind(kb_id)
    .bind(head.id)
    .bind(vec![head.left.id, head.right.id])
    .bind(vec![
        head.left.name.to_lowercase(),
        head.right.name.to_lowercase(),
    ])
    .bind(limit)
    .fetch_all(pool)
    .await?;
    assemble_reviews(pool, kb_id, rows).await
}

pub struct NewDecision<'a> {
    pub run_id: Uuid,
    pub target_id: Uuid,
    /// merge | keep | unsure
    pub action: &'a str,
    pub confidence: f32,
    pub reason: Option<&'a str>,
    pub precedents: serde_json::Value,
    /// proposed | applied
    pub status: &'a str,
    pub merge_id: Option<Uuid>,
    /// defer 留给人的问题（第二刀）
    pub question: Option<&'a str>,
    /// 循环看了什么：[{tool, args, note}]
    pub trace: serde_json::Value,
    /// 循环里花的模型调用
    pub calls: i32,
}

pub async fn record(pool: &PgPool, kb_id: Uuid, d: NewDecision<'_>) -> AppResult<Uuid> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO agent_decisions
            (id, kb_id, run_id, target_kind, target_id, action, confidence, reason,
             precedents, status, merge_id, question, trace, calls)
         VALUES ($1, $2, $3, 'review', $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
    )
    .bind(id)
    .bind(kb_id)
    .bind(d.run_id)
    .bind(d.target_id)
    .bind(d.action)
    .bind(d.confidence)
    .bind(d.reason)
    .bind(d.precedents)
    .bind(d.status)
    .bind(d.merge_id)
    .bind(d.question)
    .bind(d.trace)
    .bind(d.calls)
    .execute(pool)
    .await?;
    Ok(id)
}

/// 今天这个库在循环里花了几次模型调用：每库每天的预算按它算
pub async fn loop_calls_today(pool: &PgPool, kb_id: Uuid) -> AppResult<i64> {
    let n: i64 = sqlx::query_scalar(
        "SELECT COALESCE(sum(calls), 0)::bigint FROM agent_decisions
         WHERE kb_id = $1 AND created_at >= date_trunc('day', now())",
    )
    .bind(kb_id)
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// 工具：提到这个实体的原文片段——它的事实是从哪句话里抽出来的，出自哪份文档。
/// 证据上有 quote 就用 quote，没有就截分块开头
pub async fn quotes_of(
    pool: &PgPool,
    kb_id: Uuid,
    entity_id: Uuid,
    limit: i64,
) -> AppResult<Vec<(String, String)>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT DISTINCT ON (fe.chunk_id) d.filename,
                COALESCE(NULLIF(fe.quote, ''), left(c.text, 240))
         FROM facts f
         JOIN fact_evidence fe ON fe.fact_id = f.id
         JOIN chunks c ON c.id = fe.chunk_id
         JOIN documents d ON d.id = c.document_id
         WHERE f.kb_id = $1 AND f.invalidated_at IS NULL
           AND (f.subject_id = $2 OR f.object_id = $2)
         ORDER BY fe.chunk_id, f.confidence DESC
         LIMIT $3",
    )
    .bind(kb_id)
    .bind(entity_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 工具：台账里人对某个名字做过的决定——名字包含 query 的合并、分开与撤回
pub async fn ledger_search(
    pool: &PgPool,
    kb_id: Uuid,
    query: &str,
    limit: i64,
) -> AppResult<Vec<Precedent>> {
    let like = format!("%{}%", query.trim().to_lowercase());
    let rows: Vec<Precedent> = sqlx::query_as(&format!(
        "SELECT {COLS} FROM audit_events
         WHERE kb_id = $1 AND actor_id IS NOT NULL
           AND action IN ('review.merge', 'review.keep', 'merge.manual', 'merge.revert')
           AND ({L} LIKE $2 OR {R} LIKE $2)
         ORDER BY created_at DESC LIMIT $3"
    ))
    .bind(kb_id)
    .bind(&like)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 工具：库里名字包含 query 的其他实体——叫这个名字的有几个、各是什么、各有多少事实
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Namesake {
    pub name: String,
    pub type_label: Option<String>,
    pub facts: i64,
    pub merged: bool,
}

pub async fn namesakes(
    pool: &PgPool,
    kb_id: Uuid,
    query: &str,
    limit: i64,
) -> AppResult<Vec<Namesake>> {
    let like = format!("%{}%", query.trim().to_lowercase());
    let rows: Vec<Namesake> = sqlx::query_as(
        "SELECT e.canonical_name AS name, t.label AS type_label,
                (SELECT count(*) FROM facts f
                  WHERE f.invalidated_at IS NULL
                    AND (f.subject_id = e.id OR f.object_id = e.id)) AS facts,
                (e.merged_into IS NOT NULL) AS merged
         FROM entities e
         LEFT JOIN entity_types t ON t.id = e.type_id
         WHERE e.kb_id = $1 AND lower(e.canonical_name) LIKE $2
         ORDER BY e.merged_into IS NOT NULL, facts DESC, e.canonical_name
         LIMIT $3",
    )
    .bind(kb_id)
    .bind(&like)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

const VIEW: &str = "SELECT d.id, d.run_id, d.target_kind, d.target_id, d.action, d.confidence,
        d.reason, d.precedents, d.status, d.merge_id, d.question, d.trace, d.calls,
        d.created_at, d.decided_at,
        u.display_name AS decided_by_name,
        a.canonical_name AS \"left\", b.canonical_name AS \"right\"
    FROM agent_decisions d
    LEFT JOIN users u ON u.id = d.decided_by
    LEFT JOIN resolution_reviews rr ON rr.id = d.target_id
    LEFT JOIN entities a ON a.id = rr.left_id
    LEFT JOIN entities b ON b.id = rr.right_id";

/// Agent 队列：最新的在前
pub async fn list(
    pool: &PgPool,
    kb_id: Uuid,
    limit: i64,
    offset: i64,
) -> AppResult<Vec<AgentDecisionView>> {
    let rows = sqlx::query_as(&format!(
        "{VIEW} WHERE d.kb_id = $1 ORDER BY d.created_at DESC, d.id DESC LIMIT $2 OFFSET $3"
    ))
    .bind(kb_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get(pool: &PgPool, kb_id: Uuid, id: Uuid) -> AppResult<AgentDecisionView> {
    sqlx::query_as(&format!("{VIEW} WHERE d.kb_id = $1 AND d.id = $2"))
        .bind(kb_id)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound)
}

pub async fn count_proposed(pool: &PgPool, kb_id: Uuid) -> AppResult<i64> {
    let n = sqlx::query_scalar(
        "SELECT count(*) FROM agent_decisions WHERE kb_id = $1 AND status = 'proposed'",
    )
    .bind(kb_id)
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// 人的回答：accepted / overridden 答一条建议，reverted / overridden 答一条自动裁决。
/// 已经答过的不能再答——那是另一个人刚做的决定
pub async fn settle(
    pool: &PgPool,
    kb_id: Uuid,
    id: Uuid,
    status: &str,
    user_id: Uuid,
) -> AppResult<()> {
    let n = sqlx::query(
        "UPDATE agent_decisions SET status = $3, decided_at = now(), decided_by = $4
         WHERE kb_id = $1 AND id = $2 AND status IN ('proposed', 'applied')",
    )
    .bind(kb_id)
    .bind(id)
    .bind(status)
    .bind(user_id)
    .execute(pool)
    .await?
    .rows_affected();
    if n == 0 {
        return Err(AppError::Conflict(
            "this decision has already been answered".into(),
        ));
    }
    Ok(())
}

/// 人在卡片上裁了一对：这一对上开着的建议就此有了答案——动作与建议相同是
/// accepted，不同是 overridden；没有开着的建议就什么都不做
pub async fn answer_open(
    pool: &PgPool,
    kb_id: Uuid,
    review_id: Uuid,
    action: &str,
    user_id: Uuid,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE agent_decisions
            SET status = CASE WHEN action = $3 THEN 'accepted' ELSE 'overridden' END,
                decided_at = now(), decided_by = $4
          WHERE kb_id = $1 AND target_kind = 'review' AND target_id = $2 AND status = 'proposed'",
    )
    .bind(kb_id)
    .bind(review_id)
    .bind(action)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 人裁了一对：同名或同实体的其他对上开着的建议过时了——那些建议是在没有这笔
/// 先例时写的。标成 superseded，那些对回到队列，下一轮带着新先例再看
pub async fn supersede_siblings(pool: &PgPool, kb_id: Uuid, review_id: Uuid) -> AppResult<u64> {
    let n = sqlx::query(
        "UPDATE agent_decisions d SET status = 'superseded', decided_at = now()
         FROM resolution_reviews me
              JOIN entities ma ON ma.id = me.left_id
              JOIN entities mb ON mb.id = me.right_id,
              resolution_reviews rr
              JOIN entities a ON a.id = rr.left_id
              JOIN entities b ON b.id = rr.right_id
         WHERE me.id = $2 AND me.kb_id = $1
           AND d.kb_id = $1 AND d.status = 'proposed'
           AND d.target_kind = 'review' AND d.target_id = rr.id AND rr.id <> me.id
           AND (rr.left_id IN (me.left_id, me.right_id) OR rr.right_id IN (me.left_id, me.right_id)
                OR lower(a.canonical_name) IN (lower(ma.canonical_name), lower(mb.canonical_name))
                OR lower(b.canonical_name) IN (lower(ma.canonical_name), lower(mb.canonical_name)))",
    )
    .bind(kb_id)
    .bind(review_id)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(n)
}

/// 保险丝（0025 决定 9）：多少天里撤回几次就跳
pub const FUSE_WINDOW_DAYS: i64 = 7;
pub const FUSE_REVERTS: i64 = 2;

/// 从某一刻起，人撤回了 agent 的自动合并几次
pub async fn reverts_since(pool: &PgPool, kb_id: Uuid, since: DateTime<Utc>) -> AppResult<i64> {
    let n = sqlx::query_scalar(
        "SELECT count(*) FROM agent_decisions
         WHERE kb_id = $1 AND status = 'reverted' AND decided_at >= $2",
    )
    .bind(kb_id)
    .bind(since)
    .fetch_one(pool)
    .await?;
    Ok(n)
}

/// 保险丝跳闸：开关开着就关掉。回 true 表示这一次真的关了——告警只发一次
pub async fn trip(pool: &PgPool, kb_id: Uuid) -> AppResult<bool> {
    let n = sqlx::query(
        "UPDATE knowledge_bases SET governance = FALSE, updated_at = now()
         WHERE id = $1 AND governance",
    )
    .bind(kb_id)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(n > 0)
}

/// 人从合并历史撤回了一条合并：如果那是 agent 自己裁的，那一行记成 reverted。
/// 回那一行的 id；不是 agent 的合并就是 None
pub async fn settle_by_merge(
    pool: &PgPool,
    kb_id: Uuid,
    merge_id: Uuid,
    user_id: Uuid,
) -> AppResult<Option<Uuid>> {
    let id = sqlx::query_scalar(
        "UPDATE agent_decisions SET status = 'reverted', decided_at = now(), decided_by = $3
         WHERE kb_id = $1 AND merge_id = $2 AND status = 'applied'
         RETURNING id",
    )
    .bind(kb_id)
    .bind(merge_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    Ok(id)
}

/// 开关开着、队列里还有 agent 没看过的对的库：定时扫描用
pub async fn due(pool: &PgPool) -> AppResult<Vec<Uuid>> {
    let ids = sqlx::query_scalar(&format!(
        "SELECT kb.id FROM knowledge_bases kb
         WHERE kb.governance AND EXISTS (
             SELECT 1 FROM resolution_reviews rr
             WHERE rr.kb_id = kb.id AND rr.status = 'pending' AND {OPEN_PROPOSAL})"
    ))
    .fetch_all(pool)
    .await?;
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(action: &str) -> Precedent {
        Precedent {
            event_id: Uuid::now_v7(),
            action: action.into(),
            left: "a".into(),
            right: "b".into(),
            at: Utc::now(),
        }
    }

    fn stats(merged: i64, kept: i64, reverted: i64) -> Option<TypePairStats> {
        Some(TypePairStats {
            merged,
            kept,
            reverted,
        })
    }

    fn with_stats(merged: i64, kept: i64, reverted: i64) -> Precedents {
        Precedents {
            type_pair: stats(merged, kept, reverted),
            ..Default::default()
        }
    }

    #[test]
    fn unsure_and_low_confidence_are_proposed() {
        let none = Precedents::default();
        assert_eq!(gate(None, 0.99, false, &none), Gate::Propose);
        assert_eq!(gate(Some(false), 0.6, false, &none), Gate::Propose);
        assert_eq!(gate(Some(true), 0.84, false, &none), Gate::Propose);
    }

    #[test]
    fn keeping_apart_needs_confidence_only() {
        let none = Precedents::default();
        assert_eq!(gate(Some(false), 0.9, false, &none), Gate::Apply);
        // 类型冲突的对也能自己分开：分开不改图
        assert_eq!(gate(Some(false), 0.9, true, &none), Gate::Apply);
    }

    #[test]
    fn the_agent_judges_and_history_moves_the_bar() {
        // 没有任何历史：agent 自己的把握够就合，不够就问
        let none = Precedents::default();
        assert_eq!(gate(Some(true), 0.9, false, &none), Gate::Apply);
        assert_eq!(gate(Some(true), 0.8, false, &none), Gate::Propose);
        // 历史同意：线降到 SUPPORTED_CONF
        let pair = Precedents {
            same_pair: vec![p("review.merge")],
            ..Default::default()
        };
        assert_eq!(gate(Some(true), 0.8, false, &pair), Gate::Apply);
        assert_eq!(gate(Some(true), 0.7, false, &pair), Gate::Propose);
        let name = Precedents {
            same_name: vec![p("merge.manual"), p("review.merge")],
            ..Default::default()
        };
        assert_eq!(gate(Some(true), 0.8, false, &name), Gate::Apply);
        // 同名的决定有合有分：不算同意，回到 agent 自己的线
        let mixed = Precedents {
            same_name: vec![p("review.merge"), p("review.keep")],
            ..Default::default()
        };
        assert_eq!(gate(Some(true), 0.8, false, &mixed), Gate::Propose);
        assert_eq!(gate(Some(true), 0.9, false, &mixed), Gate::Apply);
        // 类型对的习惯
        assert_eq!(
            gate(Some(true), 0.8, false, &with_stats(6, 2, 0)),
            Gate::Apply
        );
        assert_eq!(
            gate(Some(true), 0.8, false, &with_stats(3, 1, 0)),
            Gate::Propose,
            "习惯要够 {TYPE_PAIR_MIN} 笔才算"
        );
        assert_eq!(
            gate(Some(true), 0.8, false, &with_stats(6, 2, 1)),
            Gate::Propose,
            "撤回过的类型对不算合并的习惯"
        );
        // 分开那一边同样：人一直分开的名字，分开的线也低
        assert_eq!(gate(Some(false), 0.8, false, &none), Gate::Propose);
        let kept = Precedents {
            same_pair: vec![p("review.keep")],
            ..Default::default()
        };
        assert_eq!(gate(Some(false), 0.8, false, &kept), Gate::Apply);
        assert_eq!(
            gate(Some(false), 0.8, false, &with_stats(1, 8, 0)),
            Gate::Apply
        );
    }

    #[test]
    fn a_pair_people_decided_skips_the_agent() {
        let none = Precedents::default();
        assert_eq!(
            settled_by_people("Apple", "Apple Inc.", &none, Some(true)),
            Some(true)
        );
        assert_eq!(
            settled_by_people("Zhang Wei", "Zhang Wei", &none, Some(false)),
            Some(false)
        );
        assert_eq!(settled_by_people("Apple", "Apple Inc.", &none, None), None);
        let merged = Precedents {
            same_pair: vec![p("review.merge"), p("merge.manual")],
            ..Default::default()
        };
        assert_eq!(
            settled_by_people("Apple", "Apple Inc.", &merged, None),
            Some(true)
        );
        assert_eq!(
            settled_by_people("Zhang Wei", "Zhang Wei", &merged, None),
            None,
            "同名的一对：名字上的先例说不了眼前这两个是谁"
        );
        let mixed = Precedents {
            same_pair: vec![p("review.merge"), p("review.keep")],
            ..Default::default()
        };
        assert_eq!(settled_by_people("Apple", "Apple Inc.", &mixed, None), None);
        let reverted = Precedents {
            same_pair: vec![p("review.merge")],
            reverts: vec![p("merge.revert")],
            ..Default::default()
        };
        assert_eq!(
            settled_by_people("Apple", "Apple Inc.", &reverted, Some(true)),
            None
        );
    }

    #[test]
    fn hard_rules_come_first() {
        let pair = Precedents {
            same_pair: vec![p("review.merge")],
            ..Default::default()
        };
        assert_eq!(
            gate(Some(true), 0.99, true, &pair),
            Gate::Propose,
            "类型冲突不合"
        );
        let reverted = Precedents {
            same_pair: vec![p("review.merge")],
            reverts: vec![p("merge.revert")],
            ..Default::default()
        };
        assert_eq!(gate(Some(true), 0.99, false, &reverted), Gate::Propose);
        assert_eq!(gate(Some(false), 0.99, false, &reverted), Gate::Propose);
        let kept = Precedents {
            same_pair: vec![p("review.keep")],
            ..Default::default()
        };
        assert_eq!(
            gate(Some(true), 0.99, false, &kept),
            Gate::Propose,
            "人分开过这一对，模型说合，只建议"
        );
        let merged = Precedents {
            same_pair: vec![p("review.merge")],
            ..Default::default()
        };
        assert_eq!(
            gate(Some(false), 0.99, false, &merged),
            Gate::Propose,
            "人合过这一对，模型说分，只建议"
        );
    }
}
