use anyhow::{Context, Result};
use reqwest::Client;
use tracing::{debug, info};

use crate::types::{ListNodesResponse, RegisterNodeRequest, RegisterNodeResponse, TlsConfig};

pub struct ControlClient {
    base_url: String,
    client: Client,
}

impl ControlClient {
    pub fn new(base_url: String, tls: Option<&TlsConfig>) -> Result<Self> {
        let mut builder = Client::builder().timeout(std::time::Duration::from_secs(10));

        if let Some(tls) = tls {
            // Load CA certificate for server verification
            let ca_pem = std::fs::read(&tls.ca_cert)
                .with_context(|| format!("Failed to read CA cert: {:?}", tls.ca_cert))?;
            let ca_cert = reqwest::Certificate::from_pem(&ca_pem)
                .context("Failed to parse CA certificate")?;
            builder = builder.add_root_certificate(ca_cert);

            // Load client identity for mTLS
            if let (Some(cert_path), Some(key_path)) = (&tls.client_cert, &tls.client_key) {
                let cert_pem = std::fs::read(cert_path)
                    .with_context(|| format!("Failed to read client cert: {:?}", cert_path))?;
                let key_pem = std::fs::read(key_path)
                    .with_context(|| format!("Failed to read client key: {:?}", key_path))?;
                let mut identity_pem = cert_pem;
                identity_pem.extend_from_slice(&key_pem);
                let identity = reqwest::Identity::from_pem(&identity_pem)
                    .context("Failed to parse client identity")?;
                builder = builder.identity(identity);
            }
        }

        let client = builder.build().context("Failed to create HTTP client")?;

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

        info!(
            "Node registered: node_id={}, subnet={}",
            result.node_id, result.subnet
        );
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

    /// Deregister this node from the control plane (graceful shutdown)
    pub async fn deregister(&self, node_id: &str) -> Result<()> {
        let url = format!("{}/api/nodes/{}/deregister", self.base_url, node_id);
        info!("Deregistering node at {}", url);

        let resp = self
            .client
            .post(&url)
            .send()
            .await
            .context("Failed to send deregister request")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Deregister failed ({}): {}", status, body);
        }

        info!("Node deregistered successfully");
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
