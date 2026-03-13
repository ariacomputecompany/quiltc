use anyhow::{Context, Result};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::Connection;
use std::path::PathBuf;
use tracing::info;

pub type DbPool = Pool<SqliteConnectionManager>;

/// Initialize database with connection pool and run migrations
pub fn init_db(db_path: Option<PathBuf>) -> Result<DbPool> {
    let path = db_path.unwrap_or_else(|| {
        let mut path = dirs::data_local_dir().expect("Cannot determine data directory");
        path.push("quilt-mesh");
        std::fs::create_dir_all(&path).expect("Cannot create data directory");
        path.push("control.db");
        path
    });

    info!("Initializing database at: {:?}", path);

    let manager = SqliteConnectionManager::file(&path);
    let pool = Pool::builder()
        .max_size(10)
        .build(manager)
        .context("Failed to create connection pool")?;

    // Run migrations
    let conn = pool.get().context("Failed to get database connection")?;
    run_migrations(&conn)?;

    info!("Database initialized successfully");
    Ok(pool)
}

fn run_migrations(conn: &Connection) -> Result<()> {
    // Enable foreign keys
    conn.execute_batch("PRAGMA foreign_keys = ON;")
        .context("Failed to enable foreign keys")?;

    // Run migration files in order
    let migrations = [include_str!("../../migrations/003_orchestrator.sql")];

    for (i, migration) in migrations.iter().enumerate() {
        info!("Running migration {}", i + 1);
        conn.execute_batch(migration)
            .with_context(|| format!("Failed to run migration {}", i + 1))?;
    }

    validate_schema(conn)?;

    Ok(())
}

fn validate_schema(conn: &Connection) -> Result<()> {
    fn columns_for(conn: &Connection, table: &str) -> Result<std::collections::HashSet<String>> {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({})", table))
            .with_context(|| format!("Failed to inspect schema for table {}", table))?;
        let cols = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<std::collections::HashSet<_>, _>>()
            .with_context(|| format!("Failed to read columns for table {}", table))?;
        Ok(cols)
    }

    let policy = columns_for(conn, "orchestrator_workload_policy")?;
    if !policy.contains("runtime_function_id") {
        anyhow::bail!(
            "Schema mismatch: orchestrator_workload_policy.runtime_function_id missing. \
             This build is not backward compatible; use a fresh control DB."
        );
    }

    let action = columns_for(conn, "orchestrator_action")?;
    for required in [
        "idempotency_key",
        "decision_window_start",
        "runtime_operation_id",
        "terminal_status",
        "total_latency_ms",
    ] {
        if !action.contains(required) {
            anyhow::bail!(
                "Schema mismatch: orchestrator_action.{} missing. \
                 This build is not backward compatible; use a fresh control DB.",
                required
            );
        }
    }

    Ok(())
}

/// Helper for async database operations (spawn_blocking wrapper)
pub async fn execute_async<F, T>(pool: &DbPool, f: F) -> Result<T>
where
    F: FnOnce(&Connection) -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    let pool = pool.clone();
    tokio::task::spawn_blocking(move || {
        let conn = pool.get().context("Failed to get database connection")?;
        f(&conn)
    })
    .await
    .context("Task join error")?
}
