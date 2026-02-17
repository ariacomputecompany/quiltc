use anyhow::{bail, Context, Result};
use ipnet::Ipv4Net;
use std::collections::HashSet;
use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// IPAM manager for container IP allocation
///
/// Manages IP allocation within a configured subnet range
pub struct IpamManager {
    state: Arc<RwLock<IpamState>>,
}

struct IpamState {
    /// Configured subnet for this node (e.g., 10.42.1.0/24)
    subnet: Option<Ipv4Net>,
    /// Set of allocated IPs
    allocated: HashSet<Ipv4Addr>,
}

impl IpamManager {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(IpamState {
                subnet: None,
                allocated: HashSet::new(),
            })),
        }
    }

    /// Configure the node's subnet for IP allocation
    ///
    /// # Arguments
    /// * `subnet_str` - CIDR notation (e.g., "10.42.1.0/24")
    ///
    /// # Returns
    /// Ok(()) if configuration succeeds, Err with reason if it fails
    pub async fn configure_subnet(&self, subnet_str: &str) -> Result<()> {
        // Parse subnet
        let subnet: Ipv4Net = subnet_str.parse().context("Invalid subnet CIDR format")?;

        // Validate subnet is /24
        if subnet.prefix_len() != 24 {
            bail!("Subnet must be /24, got /{}", subnet.prefix_len());
        }

        // Validate subnet is within cluster CIDR (10.42.0.0/16)
        let cluster_cidr: Ipv4Net = "10.42.0.0/16".parse().unwrap();
        if !cluster_cidr.contains(&subnet.network()) {
            bail!(
                "Subnet {} must be within cluster CIDR {}",
                subnet,
                cluster_cidr
            );
        }

        let mut state = self.state.write().await;
        state.subnet = Some(subnet);
        state.allocated.clear(); // Reset allocations when subnet changes

        info!("IPAM configured with subnet: {}", subnet);
        Ok(())
    }

    /// Allocate an IP address from the configured subnet
    ///
    /// Returns the next available IP, or error if subnet not configured or exhausted
    pub async fn allocate_ip(&self) -> Result<Ipv4Addr> {
        let mut state = self.state.write().await;

        let subnet = state
            .subnet
            .context("Subnet not configured - call configure_subnet first")?;

        // Iterate through IPs in the subnet (skip network and broadcast)
        for host in subnet.hosts() {
            if !state.allocated.contains(&host) {
                state.allocated.insert(host);
                return Ok(host);
            }
        }

        bail!("No available IPs in subnet {}", subnet);
    }

    /// Release an IP address back to the pool
    pub async fn release_ip(&self, ip: Ipv4Addr) -> Result<()> {
        let mut state = self.state.write().await;
        state.allocated.remove(&ip);
        Ok(())
    }

    /// Get the currently configured subnet
    pub async fn get_subnet(&self) -> Option<Ipv4Net> {
        let state = self.state.read().await;
        state.subnet
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_configure_subnet() {
        let ipam = IpamManager::new();

        // Valid subnet
        assert!(ipam.configure_subnet("10.42.1.0/24").await.is_ok());

        // Invalid prefix length
        assert!(ipam.configure_subnet("10.42.1.0/16").await.is_err());

        // Outside cluster CIDR
        assert!(ipam.configure_subnet("192.168.1.0/24").await.is_err());
    }

    #[tokio::test]
    async fn test_allocate_ip() {
        let ipam = IpamManager::new();

        // Should fail before configuration
        assert!(ipam.allocate_ip().await.is_err());

        // Configure and allocate
        ipam.configure_subnet("10.42.1.0/24").await.unwrap();
        let ip1 = ipam.allocate_ip().await.unwrap();
        let ip2 = ipam.allocate_ip().await.unwrap();

        // IPs should be different
        assert_ne!(ip1, ip2);

        // IPs should be in range
        let subnet: Ipv4Net = "10.42.1.0/24".parse().unwrap();
        assert!(subnet.contains(&ip1));
        assert!(subnet.contains(&ip2));
    }

    #[tokio::test]
    async fn test_release_ip() {
        let ipam = IpamManager::new();
        ipam.configure_subnet("10.42.1.0/24").await.unwrap();

        let ip = ipam.allocate_ip().await.unwrap();
        ipam.release_ip(ip).await.unwrap();

        // Should be able to allocate the same IP again
        let ip2 = ipam.allocate_ip().await.unwrap();
        assert_eq!(ip, ip2);
    }
}
