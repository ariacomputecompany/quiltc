mod api;
mod db;
mod services;
mod types;

use anyhow::Result;
use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use api::nodes::AppState;
use services::{heartbeat_monitor, SimpleIPAM, SimpleScheduler};

#[derive(Parser, Debug)]
#[command(name = "quilt-mesh-control")]
#[command(about = "Quilt Mesh control plane", long_about = None)]
struct Args {
    /// Bind address for HTTP server
    #[arg(long, default_value = "0.0.0.0:8080")]
    bind: String,

    /// Database file path
    #[arg(long)]
    db_path: Option<PathBuf>,

    /// Log level
    #[arg(long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging
    let log_level = match args.log_level.to_lowercase().as_str() {
        "trace" => Level::TRACE,
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::INFO,
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(log_level)
        .finish();

    tracing::subscriber::set_global_default(subscriber)?;

    info!("Starting Quilt Mesh Control Plane");

    // Initialize database
    let db = db::init_db(args.db_path)?;

    // Initialize IPAM (find highest allocated subnet)
    let max_subnet_id = db::execute_async(&db, |conn| {
        services::node_registry::get_max_subnet_id(conn)
    })
    .await?;

    let ipam = if max_subnet_id > 0 {
        info!("Initializing IPAM from database (max subnet ID: {})", max_subnet_id);
        Arc::new(SimpleIPAM::init_from_db(max_subnet_id))
    } else {
        info!("Initializing fresh IPAM");
        Arc::new(SimpleIPAM::new())
    };

    // Create scheduler
    let scheduler = Arc::new(SimpleScheduler::new());

    // Create application state
    let state = Arc::new(AppState {
        db: db.clone(),
        ipam,
        scheduler,
    });

    // Start heartbeat monitor in background
    tokio::spawn(async move {
        if let Err(e) = heartbeat_monitor(db).await {
            tracing::error!("Heartbeat monitor failed: {}", e);
        }
    });

    // Create router
    let app = api::create_router(state);

    // Parse bind address
    let addr: SocketAddr = args.bind.parse()?;
    info!("Listening on http://{}", addr);

    // Start server
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
