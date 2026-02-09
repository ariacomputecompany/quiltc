use anyhow::{bail, Context, Result};
use ipnet::Ipv4Net;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

#[cfg(target_os = "linux")]
use {
    futures::TryStreamExt,
    rtnetlink::{new_connection, Handle},
    std::io::ErrorKind,
};

/// Route manager for kernel routing table manipulation
///
/// Manages routes for remote container subnets via overlay interfaces
pub struct RouteManager {
    state: Arc<RwLock<RouteState>>,
    #[cfg(target_os = "linux")]
    handle: Handle,
}

struct RouteState {
    /// Map of destination subnet -> interface name
    routes: HashMap<String, String>,
}

impl RouteManager {
    #[cfg(target_os = "linux")]
    pub async fn new() -> Result<Self> {
        let (connection, handle, _) = new_connection().context("Failed to create netlink connection")?;

        // Spawn the connection in the background
        tokio::spawn(connection);

        Ok(Self {
            state: Arc::new(RwLock::new(RouteState {
                routes: HashMap::new(),
            })),
            handle,
        })
    }

    #[cfg(not(target_os = "linux"))]
    pub async fn new() -> Result<Self> {
        Ok(Self {
            state: Arc::new(RwLock::new(RouteState {
                routes: HashMap::new(),
            })),
        })
    }

    /// Add a route for a destination subnet via an interface
    ///
    /// # Arguments
    /// * `destination` - Destination subnet in CIDR notation (e.g., "10.42.2.0/24")
    /// * `interface` - Interface name to route through (e.g., "vxlan100")
    ///
    /// # Returns
    /// Ok(()) if route added successfully (idempotent - adding existing route is OK)
    #[cfg(target_os = "linux")]
    pub async fn add_route(&self, destination: &str, interface: &str) -> Result<()> {
        // Parse destination subnet
        let subnet: Ipv4Net = destination
            .parse()
            .context("Invalid destination subnet")?;

        // Check if route already exists
        {
            let state = self.state.read().await;
            if let Some(existing_if) = state.routes.get(destination) {
                if existing_if == interface {
                    info!("Route already exists: {} dev {}", destination, interface);
                    return Ok(());
                } else {
                    warn!(
                        "Route for {} exists via different interface: {} (requested: {})",
                        destination, existing_if, interface
                    );
                }
            }
        }

        // Get interface index
        let mut links = self
            .handle
            .link()
            .get()
            .match_name(interface.to_string())
            .execute();

        let link = links
            .try_next()
            .await
            .context("Failed to query interface")?
            .context(format!("Interface '{}' not found", interface))?;

        let if_index = link.header.index;

        // Add route
        match self
            .handle
            .route()
            .add()
            .v4()
            .destination_prefix(subnet.network(), subnet.prefix_len())
            .output_interface(if_index)
            .execute()
            .await
        {
            Ok(_) => {
                info!("Added route: {} dev {} (index: {})", destination, interface, if_index);

                // Track route
                let mut state = self.state.write().await;
                state.routes.insert(destination.to_string(), interface.to_string());

                Ok(())
            }
            Err(e) => {
                // Check if route already exists (EEXIST)
                if let Some(io_err) = e.downcast_ref::<std::io::Error>() {
                    if io_err.kind() == ErrorKind::AlreadyExists {
                        info!("Route already exists (idempotent): {} dev {}", destination, interface);

                        // Track route even if it already existed
                        let mut state = self.state.write().await;
                        state.routes.insert(destination.to_string(), interface.to_string());

                        return Ok(());
                    }
                }
                bail!("Failed to add route: {}", e);
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    pub async fn add_route(&self, destination: &str, interface: &str) -> Result<()> {
        warn!(
            "STUB: add_route({}, {}) - route management only available on Linux",
            destination, interface
        );

        let mut state = self.state.write().await;
        state.routes.insert(destination.to_string(), interface.to_string());

        Ok(())
    }

    /// Remove a route for a destination subnet
    ///
    /// # Arguments
    /// * `destination` - Destination subnet in CIDR notation
    ///
    /// # Returns
    /// Ok(()) if route removed successfully (idempotent - removing non-existent route is OK)
    #[cfg(target_os = "linux")]
    pub async fn remove_route(&self, destination: &str) -> Result<()> {
        // Parse destination subnet
        let subnet: Ipv4Net = destination
            .parse()
            .context("Invalid destination subnet")?;

        // Check if we're tracking this route
        let interface = {
            let state = self.state.read().await;
            state.routes.get(destination).cloned()
        };

        if interface.is_none() {
            info!("Route not tracked (may not exist): {}", destination);
            // Continue anyway - idempotent removal
        }

        // Delete route
        match self
            .handle
            .route()
            .del()
            .v4()
            .destination_prefix(subnet.network(), subnet.prefix_len())
            .execute()
            .await
        {
            Ok(_) => {
                info!("Removed route: {}", destination);

                // Untrack route
                let mut state = self.state.write().await;
                state.routes.remove(destination);

                Ok(())
            }
            Err(e) => {
                // Check if route doesn't exist (ESRCH/NotFound)
                if let Some(io_err) = e.downcast_ref::<std::io::Error>() {
                    if io_err.kind() == ErrorKind::NotFound
                        || io_err.raw_os_error() == Some(3) // ESRCH
                    {
                        info!("Route doesn't exist (idempotent): {}", destination);

                        // Untrack route even if it didn't exist
                        let mut state = self.state.write().await;
                        state.routes.remove(destination);

                        return Ok(());
                    }
                }
                bail!("Failed to remove route: {}", e);
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    pub async fn remove_route(&self, destination: &str) -> Result<()> {
        warn!(
            "STUB: remove_route({}) - route management only available on Linux",
            destination
        );

        let mut state = self.state.write().await;
        state.routes.remove(destination);

        Ok(())
    }

    /// Get all tracked routes
    pub async fn get_routes(&self) -> HashMap<String, String> {
        let state = self.state.read().await;
        state.routes.clone()
    }
}
