use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{error, info};

use crate::ipam::IpamManager;
use crate::route_manager::RouteManager;

// Include generated proto code
pub mod quilt {
    tonic::include_proto!("quilt");
}

use quilt::quilt_runtime_server::QuiltRuntime;
use quilt::{
    ConfigureNodeSubnetRequest, ConfigureNodeSubnetResponse, InjectRouteRequest,
    InjectRouteResponse, RemoveRouteRequest, RemoveRouteResponse,
};

/// Quilt Runtime gRPC service implementation
///
/// Implements the 3 RPCs required for Quilt Mesh integration:
/// 1. ConfigureNodeSubnet - Configure IPAM for per-node subnet allocation
/// 2. InjectRoute - Add kernel routes for remote container subnets
/// 3. RemoveRoute - Remove routes when nodes leave
pub struct QuiltRuntimeService {
    ipam: Arc<IpamManager>,
    route_manager: Arc<RouteManager>,
}

impl QuiltRuntimeService {
    pub fn new(ipam: Arc<IpamManager>, route_manager: Arc<RouteManager>) -> Self {
        Self {
            ipam,
            route_manager,
        }
    }
}

#[tonic::async_trait]
impl QuiltRuntime for QuiltRuntimeService {
    async fn configure_node_subnet(
        &self,
        request: Request<ConfigureNodeSubnetRequest>,
    ) -> Result<Response<ConfigureNodeSubnetResponse>, Status> {
        let req = request.into_inner();
        let subnet = req.subnet;

        info!("RPC: ConfigureNodeSubnet(subnet={})", subnet);

        match self.ipam.configure_subnet(&subnet).await {
            Ok(_) => {
                info!("Successfully configured subnet: {}", subnet);
                Ok(Response::new(ConfigureNodeSubnetResponse {
                    success: true,
                    error: String::new(),
                }))
            }
            Err(e) => {
                error!("Failed to configure subnet {}: {}", subnet, e);
                Ok(Response::new(ConfigureNodeSubnetResponse {
                    success: false,
                    error: format!("Failed to configure subnet: {}", e),
                }))
            }
        }
    }

    async fn inject_route(
        &self,
        request: Request<InjectRouteRequest>,
    ) -> Result<Response<InjectRouteResponse>, Status> {
        let req = request.into_inner();
        let destination = req.destination;
        let via_interface = req.via_interface;

        info!(
            "RPC: InjectRoute(destination={}, via={})",
            destination, via_interface
        );

        match self
            .route_manager
            .add_route(&destination, &via_interface)
            .await
        {
            Ok(_) => {
                info!("Successfully injected route: {} dev {}", destination, via_interface);
                Ok(Response::new(InjectRouteResponse {
                    success: true,
                    error: String::new(),
                }))
            }
            Err(e) => {
                error!("Failed to inject route {} dev {}: {}", destination, via_interface, e);
                Ok(Response::new(InjectRouteResponse {
                    success: false,
                    error: format!("Failed to inject route: {}", e),
                }))
            }
        }
    }

    async fn remove_route(
        &self,
        request: Request<RemoveRouteRequest>,
    ) -> Result<Response<RemoveRouteResponse>, Status> {
        let req = request.into_inner();
        let destination = req.destination;

        info!("RPC: RemoveRoute(destination={})", destination);

        match self.route_manager.remove_route(&destination).await {
            Ok(_) => {
                info!("Successfully removed route: {}", destination);
                Ok(Response::new(RemoveRouteResponse {
                    success: true,
                    error: String::new(),
                }))
            }
            Err(e) => {
                error!("Failed to remove route {}: {}", destination, e);
                Ok(Response::new(RemoveRouteResponse {
                    success: false,
                    error: format!("Failed to remove route: {}", e),
                }))
            }
        }
    }
}
