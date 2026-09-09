//! 向量索引由任务来建，不是一条编号迁移（0035）。
//!
//! 列是无维的（`embedding vector`，没有 `(N)`）：维度跟着工作区选的嵌入模型走，
//! 迁移跑的时候还不知道，pgvector 也建不了未知维度的 HNSW。另一头，
//! `CREATE INDEX CONCURRENTLY` 进不了 sqlx 给迁移套的事务，而不带 CONCURRENTLY
//! 就要在 `chunks` 上持 ACCESS EXCLUSIVE 锁到建完。所以：第一次写下某个维度的
//! 向量时排一条任务，任务在事务外建一个**按维度的部分表达式索引**，
//! `IF NOT EXISTS` 让重排无害。
//!
//! 读路径上有三条规矩，缺一条索引就悄悄不生效或悄悄少回：
//! 1. **维度写成字面量。** `vector_dims(col) = $2` 绑参数时自定义计划能用索引，
//!    generic plan 退回顺扫；sqlx 的预处理语句跑五次就切 generic，第六次起索引
//!    悄悄失效。用 [`same_dims`] / [`distance`] 把整数写进 SQL。
//! 2. **两侧都 cast 到 `vector(N)`。** 索引建在表达式 `col::vector(N)` 上，
//!    `ORDER BY` 必须一字不差地写同一个表达式。
//! 3. **`hnsw.iterative_scan = relaxed_order`。** HNSW 先取 `ef_search` 个候选再过
//!    WHERE；一张按 kb 分租的表上，小库的行在候选里占不到几个，`LIMIT 10` 会回
//!    三行甚至零行（实测 1 万行的库在 6 万行的表上：关着回 3/24，开着回满）。
//!    iterative_scan 让它继续往下走到凑够为止。`hnsw.max_scan_tuples` 留默认的
//!    20,000：占表不到万分之五的库才会撞到顶，那种库规划器本来就走精确路径。
//!
//! 走不走索引由规划器定：有 `chunks_kb_idx` 时小库、中库它自己选精确路径，只有
//! 占表大头的库才走 HNSW（实测 6 万行：20 行和 1 万行的库走精确，5 万的走索引）。
//! 应用侧不设阈值——阈值是对规划器的猜测，猜错了两边都慢。

use sqlx::{Executor, PgPool, Postgres, Transaction};
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
use utopia_core::{AppError, AppResult};

/// 任务种类，`main.rs` 的分发按这个名字认
pub const JOB_KIND: &str = "build_vector_index";

/// pgvector 的 HNSW 对 `vector` 类型的上限。超过的维度（text-embedding-3-large
/// 是 3072）不建索引，查询照常走精确路径。`halfvec` 能到 4000，但那是另一种
/// 精度，等有人用到再说
pub const MAX_DIMS: usize = 2000;

/// 哪一列。两张表同一套机制，名字不同而已
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Target {
    /// `chunks.embedding`：文档分块，会无界增长的那张
    Chunks,
    /// `entities.profile_embedding`：实体画像，类型消解按主语逐个扫它（#514）
    EntityProfiles,
}

impl Target {
    pub fn table(self) -> &'static str {
        match self {
            Target::Chunks => "chunks",
            Target::EntityProfiles => "entities",
        }
    }

    pub fn column(self) -> &'static str {
        match self {
            Target::Chunks => "embedding",
            Target::EntityProfiles => "profile_embedding",
        }
    }

    /// 任务载荷里的名字
    pub fn key(self) -> &'static str {
        self.table()
    }

    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "chunks" => Some(Target::Chunks),
            "entities" => Some(Target::EntityProfiles),
            _ => None,
        }
    }
}

/// 索引名：`chunks_embedding_hnsw_1024`
pub fn index_name(target: Target, dims: usize) -> String {
    format!("{}_{}_hnsw_{dims}", target.table(), target.column())
}

/// 部分索引的谓词；查询里要原样出现（规矩 1）
pub fn same_dims(column: &str, dims: usize) -> String {
    format!("vector_dims({column}) = {dims}")
}

/// `<=>` 两侧都 cast 到字面维度（规矩 1、2）；`param` 是查询向量的参数号
pub fn distance(column: &str, param: usize, dims: usize) -> String {
    format!("{column}::vector({dims}) <=> ${param}::vector({dims})")
}

/// 索引现在的状态：`None` 没有；`Some(valid)` 有，`false` 是上次建到一半留下的
pub async fn status(pool: &PgPool, target: Target, dims: usize) -> AppResult<Option<bool>> {
    let row: Option<(bool,)> = sqlx::query_as(
        "SELECT i.indisvalid FROM pg_class c
           JOIN pg_index i ON i.indexrelid = c.oid
          WHERE c.relname = $1 AND c.relkind = 'i'",
    )
    .bind(index_name(target, dims))
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(v,)| v))
}

/// 进程里记住的「已经在了」的索引：往后每次写入只是一次查找
fn known() -> &'static Mutex<HashSet<String>> {
    static KNOWN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    KNOWN.get_or_init(|| Mutex::new(HashSet::new()))
}

fn remember(name: &str) {
    known().lock().unwrap().insert(name.to_string());
}

fn forget(name: &str) {
    known().lock().unwrap().remove(name);
}

fn is_known(name: &str) -> bool {
    known().lock().unwrap().contains(name)
}

/// 写下某个维度的向量时叫一声：索引不在就排一条建索引任务。
/// 返回 `Some(job id)` = 这一次排上了。
///
/// 索引在的话记在进程里，往后只是一次 HashSet 查找；不在的时候每次写入一次目录
/// 查询加一次「没排着才排」的插入——建索引那一两分钟里写入照常，代价可忽略。
/// 超过 [`MAX_DIMS`] 的维度什么都不排：建不了，查询走精确路径
pub async fn request(pool: &PgPool, target: Target, dims: usize) -> AppResult<Option<i64>> {
    if dims == 0 || dims > MAX_DIMS {
        return Ok(None);
    }
    let name = index_name(target, dims);
    if is_known(&name) {
        return Ok(None);
    }
    if status(pool, target, dims).await? == Some(true) {
        remember(&name);
        return Ok(None);
    }
    crate::jobs::enqueue_unless_queued(
        pool,
        JOB_KIND,
        serde_json::json!({ "table": target.key(), "dims": dims }),
    )
    .await
}

/// 一次 [`build`] 的结果
#[derive(Debug)]
pub struct Built {
    pub name: String,
    /// `false` = 本来就在
    pub created: bool,
    pub seconds: f64,
}

/// 任务本体。**事务外**（CONCURRENTLY 的要求）、**串行**（并行建索引要共享内存，
/// Docker 默认 64 MB 的 `/dev/shm` 会让它报 could not resize shared memory segment；
/// 串行 6 万行 1024 维约 90 秒，到处能跑）。上一次建到一半留下的无效索引先删——
/// `IF NOT EXISTS` 看见它会以为已经建好。
pub async fn build(pool: &PgPool, target: Target, dims: usize) -> AppResult<Built> {
    if dims == 0 || dims > MAX_DIMS {
        return Err(AppError::Validation(format!(
            "{dims} dims: HNSW on vector holds up to {MAX_DIMS}"
        )));
    }
    let name = index_name(target, dims);
    let started = std::time::Instant::now();
    let mut conn = pool.acquire().await?;
    let outcome = async {
        conn.execute("SET max_parallel_maintenance_workers = 0")
            .await?;
        let before = status(pool, target, dims).await?;
        if before == Some(false) {
            conn.execute(format!("DROP INDEX CONCURRENTLY IF EXISTS {name}").as_str())
                .await?;
        }
        let existed = before == Some(true);
        conn.execute(
            format!(
                "CREATE INDEX CONCURRENTLY IF NOT EXISTS {name} ON {table} \
                 USING hnsw (({col}::vector({dims})) vector_cosine_ops) WHERE {pred}",
                table = target.table(),
                col = target.column(),
                pred = same_dims(target.column(), dims),
            )
            .as_str(),
        )
        .await?;
        Ok::<bool, AppError>(!existed)
    }
    .await;
    // 会话级 SET 跟着连接回池，成败都复位
    let _ = conn.execute("RESET max_parallel_maintenance_workers").await;
    let created = outcome?;
    remember(&name);
    Ok(Built {
        name,
        created,
        seconds: started.elapsed().as_secs_f64(),
    })
}

/// 删掉（测试与手工维护用；写路径不会走到这里）
pub async fn drop(pool: &PgPool, target: Target, dims: usize) -> AppResult<()> {
    let name = index_name(target, dims);
    forget(&name);
    let mut conn = pool.acquire().await?;
    conn.execute(format!("DROP INDEX CONCURRENTLY IF EXISTS {name}").as_str())
        .await?;
    Ok(())
}

/// `hnsw.iterative_scan` 是 pgvector 0.8 才有的参数。旧版本第一次探一下、进程内
/// 记住：探不到就不设——查询还是对的，只是共享表上的小库可能少回几行，
/// 那正是 0.8 修的事
async fn iterative_scan_available(pool: &PgPool) -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    if let Some(v) = AVAILABLE.get() {
        return *v;
    }
    let seen: Result<Option<String>, _> =
        sqlx::query_scalar("SELECT current_setting('hnsw.iterative_scan', true)")
            .fetch_one(pool)
            .await;
    let ok = matches!(seen, Ok(Some(_)));
    let _ = AVAILABLE.set(ok);
    ok
}

/// 读路径的会话设置（规矩 3）。`SET LOCAL` 只在事务里生效，所以近邻查询都套一个事务
pub async fn relaxed_order(pool: &PgPool, tx: &mut Transaction<'_, Postgres>) -> AppResult<()> {
    if iterative_scan_available(pool).await {
        (&mut **tx)
            .execute("SET LOCAL hnsw.iterative_scan = relaxed_order")
            .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_index_name_carries_table_column_and_dims() {
        assert_eq!(
            index_name(Target::Chunks, 1024),
            "chunks_embedding_hnsw_1024"
        );
        assert_eq!(
            index_name(Target::EntityProfiles, 768),
            "entities_profile_embedding_hnsw_768"
        );
    }

    #[test]
    fn the_sql_writes_the_dimension_as_a_literal() {
        assert_eq!(
            same_dims("c.embedding", 1024),
            "vector_dims(c.embedding) = 1024"
        );
        assert_eq!(
            distance("c.embedding", 2, 1024),
            "c.embedding::vector(1024) <=> $2::vector(1024)"
        );
    }

    #[test]
    fn a_target_round_trips_through_the_payload() {
        for t in [Target::Chunks, Target::EntityProfiles] {
            assert_eq!(Target::parse(t.key()), Some(t));
        }
        assert_eq!(Target::parse("documents"), None);
    }
}
