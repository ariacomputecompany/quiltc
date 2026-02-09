use anyhow::{bail, Context, Result};
use tracing::info;

// Include generated proto code
pub mod quilt {
    tonic::include_proto!("quilt");
}

use quilt::quilt_runtime_client::QuiltRuntimeClient;
use quilt::{ConfigureNodeSubnetRequest, InjectRouteRequest, RemoveRouteRequest};

/// Client for Quilt runtime gRPC API
pub struct QuiltClient {
    client: QuiltRuntimeClient<tonic::transport::Channel>,
}

impl QuiltClient {
    /// Create a new Quilt client
    pub async fn new(quilt_endpoint: String) -> Result<Self> {
        info!("Connecting to Quilt runtime at {}", quilt_endpoint);

        let client = QuiltRuntimeClient::connect(quilt_endpoint)
            .await
            .context("Failed to connect to Quilt runtime")?;

        info!("Successfully connected to Quilt runtime");

        Ok(Self { client })
    }

    /// Configure the node's subnet for container IP allocation
    pub async fn configure_node_subnet(&mut self, subnet: String) -> Result<()> {
        info!("Calling ConfigureNodeSubnet(subnet={})", subnet);

        let req = tonic::Request::new(ConfigureNodeSubnetRequest { subnet });
        let resp = self.client.configure_node_subnet(req).await?.into_inner();

        if !resp.success {
            bail!("ConfigureNodeSubnet failed: {}", resp.error);
        }

        info!("ConfigureNodeSubnet succeeded");
        Ok(())
    }

    /// Inject a route for a remote subnet
    pub async fn inject_route(&mut self, destination: String, via_interface: String) -> Result<()> {
        info!("Calling InjectRoute(destination={}, via={})", destination, via_interface);

        let req = tonic::Request::new(InjectRouteRequest {
            destination,
            via_interface,
        });
        let resp = self.client.inject_route(req).await?.into_inner();

        if !resp.success {
            bail!("InjectRoute failed: {}", resp.error);
        }

        info!("InjectRoute succeeded");
        Ok(())
    }

    /// Remove a route for a remote subnet
    pub async fn remove_route(&mut self, destination: String) -> Result<()> {
        info!("Calling RemoveRoute(destination={})", destination);

        let req = tonic::Request::new(RemoveRouteRequest { destination });
        let resp = self.client.remove_route(req).await?.into_inner();

        if !resp.success {
            bail!("RemoveRoute failed: {}", resp.error);
        }

        info!("RemoveRoute succeeded");
        Ok(())
    }
}
