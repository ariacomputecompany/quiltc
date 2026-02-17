use anyhow::{bail, Context, Result};
use tracing::info;

use crate::types::TlsConfig;

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
    pub async fn new(quilt_endpoint: String, tls: Option<&TlsConfig>) -> Result<Self> {
        info!("Connecting to Quilt runtime at {}", quilt_endpoint);

        let channel = if let Some(tls) = tls {
            let ca_pem = std::fs::read(&tls.ca_cert)
                .with_context(|| format!("Failed to read CA cert: {:?}", tls.ca_cert))?;
            let ca = tonic::transport::Certificate::from_pem(ca_pem);

            let mut tls_config = tonic::transport::ClientTlsConfig::new().ca_certificate(ca);

            if let (Some(cert_path), Some(key_path)) = (&tls.client_cert, &tls.client_key) {
                let cert_pem = std::fs::read(cert_path)
                    .with_context(|| format!("Failed to read client cert: {:?}", cert_path))?;
                let key_pem = std::fs::read(key_path)
                    .with_context(|| format!("Failed to read client key: {:?}", key_path))?;
                let identity = tonic::transport::Identity::from_pem(cert_pem, key_pem);
                tls_config = tls_config.identity(identity);
            }

            tonic::transport::Channel::from_shared(quilt_endpoint)?
                .tls_config(tls_config)?
                .connect()
                .await
                .context("Failed to connect to Quilt runtime (TLS)")?
        } else {
            tonic::transport::Channel::from_shared(quilt_endpoint)?
                .connect()
                .await
                .context("Failed to connect to Quilt runtime")?
        };

        let client = QuiltRuntimeClient::new(channel);

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
        info!(
            "Calling InjectRoute(destination={}, via={})",
            destination, via_interface
        );

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
