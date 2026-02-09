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
    let migrations = [
        include_str!("../../migrations/001_nodes.sql"),
        include_str!("../../migrations/002_containers.sql"),
    ];

    for (i, migration) in migrations.iter().enumerate() {
        info!("Running migration {}", i + 1);
        conn.execute_batch(migration)
            .with_context(|| format!("Failed to run migration {}", i + 1))?;
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
