//! A newly queued job wakes an idle worker without waiting for the poll interval.
//!
//! The worker still polls as a backstop for delayed retries and notifications lost
//! during reconnects. This test only checks the fast path: enqueueing a job must
//! publish a notification after the insert is committed.

use sqlx::postgres::PgListener;
use sqlx::PgPool;
use std::time::Duration;
use utopia_store::jobs;

#[tokio::test]
async fn a_queued_job_notifies_an_idle_worker() -> anyhow::Result<()> {
    let Some(url) = utopia_store::test_db::url() else {
        return Ok(());
    };
    let pool = PgPool::connect(&url).await?;
    let mut listener = PgListener::connect_with(&pool).await?;
    listener.listen(jobs::JOB_CHANNEL).await?;

    let payload = serde_json::json!({
        "test": "a_queued_job_notifies_an_idle_worker"
    });
    let id = jobs::enqueue(&pool, "notify_test", payload).await?;

    let notification = tokio::time::timeout(Duration::from_secs(1), listener.recv()).await;
    assert!(
        notification.is_ok(),
        "enqueue should wake a listening worker"
    );

    sqlx::query("DELETE FROM jobs WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await?;
    Ok(())
}
