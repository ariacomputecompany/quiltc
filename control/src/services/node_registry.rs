use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::types::Node;

/// Register a new node in the database
pub fn register_node(
    conn: &Connection,
    hostname: &str,
    host_ip: &str,
    subnet: &str,
    cpu_cores: Option<u32>,
    ram_mb: Option<u64>,
) -> Result<String> {
    let node_id = Uuid::new_v4().to_string();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    conn.execute(
        "INSERT INTO nodes (node_id, hostname, host_ip, subnet, cpu_cores, ram_mb, status, registered_at, last_heartbeat)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'up', ?7, ?8)",
        rusqlite::params![node_id, hostname, host_ip, subnet, cpu_cores, ram_mb, now, now],
    )
    .context("Failed to insert node")?;

    Ok(node_id)
}

/// Update heartbeat timestamp for a node
pub fn update_heartbeat(conn: &Connection, node_id: &str) -> Result<()> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let rows_updated = conn
        .execute(
            "UPDATE nodes SET last_heartbeat = ?1, status = 'up' WHERE node_id = ?2",
            rusqlite::params![now, node_id],
        )
        .context("Failed to update heartbeat")?;

    if rows_updated == 0 {
        anyhow::bail!("Node not found: {}", node_id);
    }

    Ok(())
}

/// List all nodes
pub fn list_nodes(conn: &Connection) -> Result<Vec<Node>> {
    let mut stmt = conn
        .prepare("SELECT node_id, hostname, host_ip, subnet, cpu_cores, ram_mb, status, registered_at, last_heartbeat FROM nodes ORDER BY registered_at")
        .context("Failed to prepare statement")?;

    let nodes = stmt
        .query_map([], |row| {
            Ok(Node {
                node_id: row.get(0)?,
                hostname: row.get(1)?,
                host_ip: row.get(2)?,
                subnet: row.get(3)?,
                cpu_cores: row.get(4)?,
                ram_mb: row.get(5)?,
                status: row.get(6)?,
                registered_at: row.get(7)?,
                last_heartbeat: row.get(8)?,
            })
        })
        .context("Failed to query nodes")?
        .collect::<Result<Vec<_>, _>>()
        .context("Failed to collect nodes")?;

    Ok(nodes)
}

/// Get the maximum subnet ID allocated (for IPAM initialization)
pub fn get_max_subnet_id(conn: &Connection) -> Result<u8> {
    let result: Option<String> = conn
        .query_row("SELECT subnet FROM nodes ORDER BY subnet DESC LIMIT 1", [], |row| {
            row.get(0)
        })
        .optional()
        .context("Failed to query max subnet")?;

    if let Some(subnet) = result {
        // Parse "10.42.X.0/24" to extract X
        let parts: Vec<&str> = subnet.as_str().split('.').collect();
        if parts.len() >= 3 {
            return parts[2]
                .parse()
                .context("Failed to parse subnet ID");
        }
    }

    Ok(0)
}
