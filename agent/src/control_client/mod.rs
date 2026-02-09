use anyhow::{Context, Result};
use reqwest::Client;
use tracing::{debug, info};

use crate::types::{ListNodesResponse, RegisterNodeRequest, RegisterNodeResponse};

pub struct ControlClient {
    base_url: String,
    client: Client,
}

impl ControlClient {
    pub fn new(base_url: String) -> Result<Self> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { base_url, client })
    }

    /// Register this node with the control plane
    pub async fn register_node(
        &self,
        hostname: String,
        host_ip: String,
        cpu_cores: Option<u32>,
        ram_mb: Option<u64>,
    ) -> Result<RegisterNodeResponse> {
        let url = format!("{}/api/nodes/register", self.base_url);
        info!("Registering node at {}", url);

        let req = RegisterNodeRequest {
            hostname,
            host_ip,
            cpu_cores,
            ram_mb,
        };

        let resp = self
            .client
            .post(&url)
            .json(&req)
            .send()
            .await
            .context("Failed to send registration request")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Registration failed ({}): {}", status, body);
        }

        let result = resp
            .json::<RegisterNodeResponse>()
            .await
            .context("Failed to parse registration response")?;

        info!("Node registered: node_id={}, subnet={}", result.node_id, result.subnet);
        Ok(result)
    }

    /// Send heartbeat for this node
    pub async fn heartbeat(&self, node_id: &str) -> Result<()> {
        let url = format!("{}/api/nodes/{}/heartbeat", self.base_url, node_id);
        debug!("Sending heartbeat to {}", url);

        let resp = self
            .client
            .post(&url)
            .send()
            .await
            .context("Failed to send heartbeat")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Heartbeat failed ({}): {}", status, body);
        }

        Ok(())
    }

    /// List all nodes in the cluster
    pub async fn list_nodes(&self) -> Result<ListNodesResponse> {
        let url = format!("{}/api/nodes", self.base_url);
        debug!("Listing nodes from {}", url);

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("Failed to list nodes")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("List nodes failed ({}): {}", status, body);
        }

        let result = resp
            .json::<ListNodesResponse>()
            .await
            .context("Failed to parse list nodes response")?;

        Ok(result)
    }
}
