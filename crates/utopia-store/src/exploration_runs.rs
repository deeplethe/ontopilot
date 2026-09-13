//! 一轮映射探索的账：扫了多大的东西、丢了什么、覆盖了多少（#503）。
//!
//! 从前一轮探索只留下两样：`concept_mappings` 里若干行，以及「一条都没提出来」
//! 时的一条告警。**覆盖率没有任何地方说**——十一条提议对着一张八十列的宽表，
//! 与十一条刚好覆盖完一个小库，页面上一模一样。
//!
//! 丢弃更是静默的。`explore_mappings` 对每条提议有四处 `continue`，四处都不计数。
//! 实测踩过一次：源叫 `tpch-2026-09-08-12-30`，模型看着 schema 回的是 `tpch`，
//! 十二条一条不剩地被吞掉，任务照样 `done`。
//!
//! 与 `extraction_drops` 的分工照那张表自己的话说：那边答「这些事实没落地」，
//! 读者是传文档的人；这边答「这一轮看见了多大的东西、覆盖了多少」，读者是
//! 管数据源的人，动作是给列加注释或者再跑一轮。

use sqlx::PgPool;
use utopia_core::models::ExplorationRun;
use utopia_core::AppResult;
use uuid::Uuid;

/// 丢弃原因码。前端按这个查文案，所以是稳定契约，不要改字面量
/// （与 `extraction_drops::reason` 同一约定）。
pub mod drop_reason {
    /// 模型回的 `source` 对不上任何挂载源的名字。**最常踩的一个**：源名带了
    /// 环境后缀或时间戳，而模型照着 schema 回一个短名
    pub const SOURCE: &str = "source";
    /// `kind` 不是 metric / dimension
    pub const KIND: &str = "kind";
    /// 概念类查不到——本体里没有 Metric / Dimension，`ensure_concept_types`
    /// 之后不该再发生
    pub const TYPE: &str = "type";
    /// `definition` 不是一个对象，或者拿不出可执行的定义
    pub const DEFINITION: &str = "definition";
    /// 模型回的条数超过这一轮的上限，超出的没看。上限按表数定，回得比它多，
    /// 多半是模型把列当成了口径——宽表上常见
    pub const CAP: &str = "cap";
    /// 这条概念在这个源上已经有人表过态（确认或拒绝）。提议不覆盖决定，
    /// 所以下一轮探索算出同一条时它既不回到待看，也不算这一轮写入的
    pub const DECIDED: &str = "decided";
}

/// 开一轮。**先开行再干活**，跑挂了那一轮也有账可查——
/// 失败与「跑了但什么都没提」在页面上从前都是「没有新提议」。
pub async fn start(pool: &PgPool, kb_id: Uuid) -> AppResult<Uuid> {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO mapping_exploration_runs (id, kb_id) VALUES ($1, $2)")
        .bind(id)
        .bind(kb_id)
        .execute(pool)
        .await?;
    Ok(id)
}

/// 读完 schema、还没问模型时先记下规模。**问模型可能要一分钟**，
/// 这期间页面上该看得见「正在扫的是多大的东西」。
pub async fn scanned(
    pool: &PgPool,
    id: Uuid,
    sources: &[String],
    tables: i32,
    columns: i32,
    truncated: bool,
    cap: i32,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE mapping_exploration_runs
            SET sources = $2, tables_scanned = $3, columns_scanned = $4,
                schema_truncated = $5, cap = $6
          WHERE id = $1",
    )
    .bind(id)
    .bind(sources)
    .bind(tables)
    .bind(columns)
    .bind(truncated)
    .bind(cap)
    .execute(pool)
    .await?;
    Ok(())
}

/// 收一轮。`dropped` 的形状见 [`drop_reason`]：每个原因一个
/// `{"n": 计数, "example": "一条例子"}`——**光有计数诊断不动**，
/// 「十二条源名对不上」要配上「模型说的是 tpch，挂的是 tpch-2026-09-08」
/// 才知道该改什么。
pub async fn finish(
    pool: &PgPool,
    id: Uuid,
    returned: i32,
    accepted: i32,
    dropped: serde_json::Value,
    tables_covered: &[String],
) -> AppResult<()> {
    sqlx::query(
        "UPDATE mapping_exploration_runs
            SET finished_at = now(), returned = $2, accepted = $3,
                dropped = $4, tables_covered = $5
          WHERE id = $1",
    )
    .bind(id)
    .bind(returned)
    .bind(accepted)
    .bind(dropped)
    .bind(tables_covered)
    .execute(pool)
    .await?;
    Ok(())
}

/// 这一轮挂了。记下来就收摊——**报错路径上的失败不该淹掉它要报的那件事**，
/// 调用方一律 `let _ =`（与 `extraction_drops` 同一约定）。
pub async fn fail(pool: &PgPool, id: Uuid, error: &str) -> AppResult<()> {
    sqlx::query(
        "UPDATE mapping_exploration_runs
            SET finished_at = now(), error = left($2, 500) WHERE id = $1",
    )
    .bind(id)
    .bind(error)
    .execute(pool)
    .await?;
    Ok(())
}

/// 最近几轮，新的在前。数据映射页读它。
pub async fn recent(pool: &PgPool, kb_id: Uuid, limit: i64) -> AppResult<Vec<ExplorationRun>> {
    Ok(sqlx::query_as(
        "SELECT id, started_at, finished_at, sources, tables_scanned, columns_scanned,
                schema_truncated, cap, returned, accepted, dropped, tables_covered, error
           FROM mapping_exploration_runs
          WHERE kb_id = $1
          ORDER BY started_at DESC
          LIMIT $2",
    )
    .bind(kb_id)
    .bind(limit)
    .fetch_all(pool)
    .await?)
}
