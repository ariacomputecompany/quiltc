use anyhow::{Context, Result};
use rusqlite::Connection;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use crate::types::Container;

/// Create a container record in the database
pub fn create_container(
    conn: &Connection,
    node_id: &str,
    name: &str,
    namespace: &str,
    image: &str,
) -> Result<String> {
    let container_id = Uuid::new_v4().to_string();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    conn.execute(
        "INSERT INTO containers (container_id, node_id, name, namespace, image, created_at, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending')",
        rusqlite::params![container_id, node_id, name, namespace, image, now],
    )
    .context("Failed to insert container")?;

    Ok(container_id)
}

/// Update container IP address
pub fn update_container_ip(conn: &Connection, container_id: &str, ip_address: &str) -> Result<()> {
    let rows_updated = conn
        .execute(
            "UPDATE containers SET ip_address = ? WHERE container_id = ?",
            rusqlite::params![ip_address, container_id],
        )
        .context("Failed to update container IP")?;

    if rows_updated == 0 {
        anyhow::bail!("Container not found: {}", container_id);
    }

    Ok(())
}

/// Update container status
pub fn update_container_status(conn: &Connection, container_id: &str, status: &str) -> Result<()> {
    let rows_updated = conn
        .execute(
            "UPDATE containers SET status = ? WHERE container_id = ?",
            rusqlite::params![status, container_id],
        )
        .context("Failed to update container status")?;

    if rows_updated == 0 {
        anyhow::bail!("Container not found: {}", container_id);
    }

    Ok(())
}

/// Get a container by ID
pub fn get_container(conn: &Connection, container_id: &str) -> Result<Container> {
    let container = conn
        .query_row(
            "SELECT container_id, node_id, name, namespace, image, ip_address, created_at, status
             FROM containers WHERE container_id = ?",
            rusqlite::params![container_id],
            |row| {
                Ok(Container {
                    container_id: row.get(0)?,
                    node_id: row.get(1)?,
                    name: row.get(2)?,
                    namespace: row.get(3)?,
                    image: row.get(4)?,
                    ip_address: row.get(5)?,
                    created_at: row.get(6)?,
                    status: row.get(7)?,
                })
            },
        )
        .context("Failed to get container")?;

    Ok(container)
}

/// List all containers
pub fn list_containers(conn: &Connection) -> Result<Vec<Container>> {
    let mut stmt = conn
        .prepare(
            "SELECT container_id, node_id, name, namespace, image, ip_address, created_at, status
             FROM containers ORDER BY created_at DESC",
        )
        .context("Failed to prepare statement")?;

    let containers = stmt
        .query_map([], |row| {
            Ok(Container {
                container_id: row.get(0)?,
                node_id: row.get(1)?,
                name: row.get(2)?,
                namespace: row.get(3)?,
                image: row.get(4)?,
                ip_address: row.get(5)?,
                created_at: row.get(6)?,
                status: row.get(7)?,
            })
        })
        .context("Failed to query containers")?
        .collect::<Result<Vec<_>, _>>()
        .context("Failed to collect containers")?;

    Ok(containers)
}

/// Delete a container
pub fn delete_container(conn: &Connection, container_id: &str) -> Result<()> {
    let rows_deleted = conn
        .execute(
            "DELETE FROM containers WHERE container_id = ?",
            rusqlite::params![container_id],
        )
        .context("Failed to delete container")?;

    if rows_deleted == 0 {
        anyhow::bail!("Container not found: {}", container_id);
    }

    Ok(())
}
