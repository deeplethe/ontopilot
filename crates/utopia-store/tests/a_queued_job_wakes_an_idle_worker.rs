//! #517：入队的任务不等轮询，空闲的 worker 立刻醒。
//!
//! worker 空闲时不再干睡两秒，而是等 `utopia_jobs` 频道的通知，两秒轮询只做兜底
//! （`run_at` 在未来的重试、重连期间丢掉的通知、几个 worker 被一条通知叫醒后没抢到的）。
//! 这里守的是 worker 真正用的那个等待函数 `jobs::wait_for_work`，不起完整 worker：
//! 完整 worker 会认领库里任何排队任务，共用一个测试库时会替别的测试把活干掉。
//!
//! 三件事：
//! 1. **入队叫得醒。** 通知在轮询间隔之内到达，等待以 `Notified` 结束。
//! 2. **通知只在提交之后。** 插入还在未提交的事务里时，等待只能靠轮询结束；提交后才醒。
//!    这是 #517 点名的顺序隐患：先醒后见行的话，醒来一无所获，又回到轮询。
//! 3. **没有通知也会醒。** 轮询那一臂还在，等满间隔以 `Polled` 结束。
//!
//! 三个测试共用一个频道，串行跑：并行的话彼此的通知会互相叫醒。
//! 要对着一个没有别的进程往里写任务的库跑。

use sqlx::PgPool;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use utopia_store::jobs::{self, Wake};

/// 短轮询：测试只关心「通知比轮询快」，不必等真正的两秒。
const POLL: Duration = Duration::from_millis(300);
static SERIAL: Mutex<()> = Mutex::const_new(());

async fn connect() -> anyhow::Result<Option<PgPool>> {
    let Some(url) = utopia_store::test_db::url() else {
        return Ok(None);
    };
    Ok(Some(PgPool::connect(&url).await?))
}

async fn listener(pool: &PgPool) -> anyhow::Result<sqlx::postgres::PgListener> {
    jobs::listen_for_jobs(pool)
        .await
        .ok_or_else(|| anyhow::anyhow!("the wake-up listener could not be built"))
}

async fn remove(pool: &PgPool, id: i64) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM jobs WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

#[tokio::test]
async fn a_queued_job_wakes_an_idle_worker() -> anyhow::Result<()> {
    let _serial = SERIAL.lock().await;
    let Some(pool) = connect().await? else {
        return Ok(());
    };
    let mut listener = listener(&pool).await?;

    let started = Instant::now();
    let id = jobs::enqueue(&pool, "wake_test", serde_json::json!({})).await?;
    let wake = jobs::wait_for_work(Some(&mut listener), POLL).await;
    let waited = started.elapsed();
    remove(&pool, id).await?;

    assert_eq!(
        wake,
        Wake::Notified,
        "an enqueue must end the idle wait with a notification"
    );
    assert!(
        waited < POLL,
        "woke after {waited:?}, which is not faster than the {POLL:?} poll"
    );
    Ok(())
}

#[tokio::test]
async fn a_notification_never_arrives_before_the_row() -> anyhow::Result<()> {
    let _serial = SERIAL.lock().await;
    let Some(pool) = connect().await? else {
        return Ok(());
    };
    let mut listener = listener(&pool).await?;

    let mut tx = pool.begin().await?;
    let id =
        jobs::enqueue_with_max_attempts_tx(&mut tx, "wake_test", serde_json::json!({}), 3).await?;
    let before_commit = jobs::wait_for_work(Some(&mut listener), POLL).await;
    tx.commit().await?;
    let after_commit = jobs::wait_for_work(Some(&mut listener), POLL).await;
    remove(&pool, id).await?;

    assert_eq!(
        before_commit,
        Wake::Polled,
        "nothing may wake a worker while the row is still invisible"
    );
    assert_eq!(
        after_commit,
        Wake::Notified,
        "the commit is what wakes the worker"
    );
    Ok(())
}

#[tokio::test]
async fn the_poll_still_fires_without_a_notification() -> anyhow::Result<()> {
    let _serial = SERIAL.lock().await;
    let Some(pool) = connect().await? else {
        return Ok(());
    };
    let mut listener = listener(&pool).await?;

    let started = Instant::now();
    let wake = jobs::wait_for_work(Some(&mut listener), POLL).await;
    let waited = started.elapsed();

    assert_eq!(wake, Wake::Polled);
    assert!(
        waited >= POLL,
        "the poll arm returned after {waited:?}, before its {POLL:?}"
    );
    Ok(())
}
