//! 索引由任务建（0035）：写入一侧只负责「排一条」，且排一次就够。
//!
//! - 第一次写下某个维度：排上一条 `build_vector_index`
//! - 再写：不再排（队列里已经有一条）
//! - 建好之后再写：什么都不排（进程里记住了）
//! - 超过 HNSW 上限的维度：不排，也建不了——这种失败不该重试
//!
//! 建索引本身与查询的一致性在 `the_nearest_chunk_is_found_however_it_is_reached` 里。

use sqlx::PgPool;
use utopia_store::vector_index::{self, Target, JOB_KIND, MAX_DIMS};

/// 这个测试独占的维度
const DIMS: usize = 9;

async fn queued(pool: &PgPool, dims: usize) -> anyhow::Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT count(*) FROM jobs WHERE kind = $1 AND payload = $2 AND status = 'queued'",
    )
    .bind(JOB_KIND)
    .bind(serde_json::json!({ "table": "chunks", "dims": dims }))
    .fetch_one(pool)
    .await?)
}

#[tokio::test]
async fn the_first_write_of_a_dimension_queues_one_build() -> anyhow::Result<()> {
    let Some(url) = utopia_store::test_db::url() else {
        return Ok(());
    };
    let pool = PgPool::connect(&url).await?;
    // 上一次跑留下的都清掉，从「什么都没有」开始
    vector_index::drop(&pool, Target::Chunks, DIMS).await?;
    sqlx::query("DELETE FROM jobs WHERE kind = $1 AND payload->>'dims' = $2")
        .bind(JOB_KIND)
        .bind(DIMS.to_string())
        .execute(&pool)
        .await?;

    let run = async {
        let first = vector_index::request(&pool, Target::Chunks, DIMS).await?;
        assert!(first.is_some(), "第一次写下 9 维：排上一条");
        assert_eq!(queued(&pool, DIMS).await?, 1);
        let second = vector_index::request(&pool, Target::Chunks, DIMS).await?;
        assert_eq!(second, None, "队列里已经有一条，不再排");
        assert_eq!(queued(&pool, DIMS).await?, 1);

        let built = vector_index::build(&pool, Target::Chunks, DIMS).await?;
        assert!(built.created);
        assert_eq!(
            vector_index::status(&pool, Target::Chunks, DIMS).await?,
            Some(true)
        );
        // 任务跑完那条会被标 done；这里模拟一下，好让下一问只靠「进程里记住了」
        sqlx::query("UPDATE jobs SET status = 'done' WHERE kind = $1 AND payload->>'dims' = $2")
            .bind(JOB_KIND)
            .bind(DIMS.to_string())
            .execute(&pool)
            .await?;
        let third = vector_index::request(&pool, Target::Chunks, DIMS).await?;
        assert_eq!(third, None, "建好了：什么都不排");
        assert_eq!(queued(&pool, DIMS).await?, 0);

        // 超过上限的维度
        let too_wide = MAX_DIMS + 1;
        assert_eq!(
            vector_index::request(&pool, Target::Chunks, too_wide).await?,
            None,
            "建不了的不排"
        );
        assert_eq!(queued(&pool, too_wide).await?, 0);
        let err = vector_index::build(&pool, Target::Chunks, too_wide)
            .await
            .expect_err("HNSW on vector holds up to 2000 dims");
        assert!(
            matches!(err, utopia_core::AppError::Validation(_)),
            "是 Validation，分发层据此判 Terminal：{err:?}"
        );
        Ok::<_, anyhow::Error>(())
    }
    .await;

    let _ = vector_index::drop(&pool, Target::Chunks, DIMS).await;
    sqlx::query("DELETE FROM jobs WHERE kind = $1 AND payload->>'dims' = $2")
        .bind(JOB_KIND)
        .bind(DIMS.to_string())
        .execute(&pool)
        .await?;
    run
}
