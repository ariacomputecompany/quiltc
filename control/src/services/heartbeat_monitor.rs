use anyhow::{Context, Result};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::{error, info};

use crate::db::DbPool;

/// Background task that marks nodes as down if they haven't sent a heartbeat in 30s
pub async fn heartbeat_monitor(pool: DbPool) -> Result<()> {
    info!("Starting heartbeat monitor");

    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;

        match mark_stale_nodes(&pool).await {
            Ok(count) if count > 0 => {
                info!("Marked {} node(s) as down due to missing heartbeat", count);
            }
            Err(e) => {
                error!("Heartbeat monitor error: {}", e);
            }
            _ => {}
        }
    }
}

async fn mark_stale_nodes(pool: &DbPool) -> Result<usize> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let threshold = now - 30; // 30 seconds

    crate::db::execute_async(pool, move |conn| {
        let rows = conn
            .execute(
                "UPDATE nodes SET status = 'down' WHERE last_heartbeat < ?1 AND status = 'up'",
                rusqlite::params![threshold],
            )
            .context("Failed to mark stale nodes")?;

        Ok(rows)
    })
    .await
}
