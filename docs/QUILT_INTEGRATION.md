# Quilt Integration Requirements for Quilt Mesh

## Overview

Quilt Mesh requires **3 new gRPC RPCs** to be added to the Quilt runtime to support multi-host container networking. These RPCs allow the mesh agent to:
1. Configure per-node subnet allocation
2. Inject routes for remote container subnets
3. Remove routes when nodes leave the cluster

## Estimated Implementation Effort

**~200-300 lines of Rust code** split across:
- gRPC service implementation (~100 lines)
- Route management logic (~80 lines)
- IPAM subnet configuration (~40 lines)
- Integration glue (~40 lines)

## Protocol Buffer Definitions

The proto file is located at `agent/proto/quilt.proto`. Here are the 3 RPCs needed:

```protobuf
service QuiltRuntime {
  rpc ConfigureNodeSubnet(ConfigureNodeSubnetRequest) returns (ConfigureNodeSubnetResponse);
  rpc InjectRoute(InjectRouteRequest) returns (InjectRouteResponse);
  rpc RemoveRoute(RemoveRouteRequest) returns (RemoveRouteResponse);
}
```

## RPC 1: ConfigureNodeSubnet

### Purpose
Tells Quilt to narrow its container IP allocation to a specific /24 subnet within the cluster CIDR (10.42.0.0/16).

### When Called
Once on agent startup, after the node registers with the control plane and receives its assigned subnet.

### Request
```protobuf
message ConfigureNodeSubnetRequest {
  string subnet = 1;  // e.g., "10.42.1.0/24"
}
```

### Response
```protobuf
message ConfigureNodeSubnetResponse {
  bool success = 1;
  string error = 2;  // Error message if success=false
}
```

### Implementation Requirements

**What Quilt needs to do:**
1. Parse the subnet CIDR (e.g., "10.42.1.0/24")
2. Update the IPAM module to only allocate IPs from this range
3. Validate the subnet is within the cluster CIDR (10.42.0.0/16)
4. Return success=true if configured, or success=false + error message

**Pseudocode:**
```rust
async fn configure_node_subnet(request: ConfigureNodeSubnetRequest) -> Result<ConfigureNodeSubnetResponse> {
    let subnet = request.subnet.parse::<Ipv4Net>()?;

    // Validate subnet is /24 and within cluster CIDR
    if subnet.prefix_len() != 24 {
        return Ok(ConfigureNodeSubnetResponse {
            success: false,
            error: "Subnet must be /24".to_string(),
        });
    }

    // Update IPAM to use this subnet
    ipam_manager.set_allocation_range(subnet)?;

    Ok(ConfigureNodeSubnetResponse {
        success: true,
        error: String::new(),
    })
}
```

## RPC 2: InjectRoute

### Purpose
Adds a kernel route for a remote node's container subnet via the VXLAN interface. This ensures traffic destined for containers on other nodes goes through the overlay network.

### When Called
- Once for each existing node when the agent starts
- Whenever a new node joins the cluster
- Called by the peer sync loop (every 5s) for new peers

### Request
```protobuf
message InjectRouteRequest {
  string destination = 1;  // e.g., "10.42.2.0/24"
  string via_interface = 2;  // e.g., "vxlan100"
}
```

### Response
```protobuf
message InjectRouteResponse {
  bool success = 1;
  string error = 2;
}
```

### Implementation Requirements

**What Quilt needs to do:**
1. Parse the destination subnet
2. Add a kernel route using netlink or `ip route add`
3. Handle idempotency (adding the same route twice should succeed)

**Equivalent command:**
```bash
ip route add 10.42.2.0/24 dev vxlan100
```

**Pseudocode:**
```rust
async fn inject_route(request: InjectRouteRequest) -> Result<InjectRouteResponse> {
    let dest = request.destination.parse::<Ipv4Net>()?;
    let interface = request.via_interface;

    // Add route using netlink
    match route_manager.add_route(dest, &interface).await {
        Ok(_) => Ok(InjectRouteResponse {
            success: true,
            error: String::new(),
        }),
        Err(e) if e.kind() == ErrorKind::AlreadyExists => {
            // Idempotent: route already exists is OK
            Ok(InjectRouteResponse { success: true, error: String::new() })
        }
        Err(e) => Ok(InjectRouteResponse {
            success: false,
            error: e.to_string(),
        }),
    }
}
```

**Using rtnetlink crate:**
```rust
use rtnetlink::{new_connection, Handle};

async fn add_route_via_netlink(dest: Ipv4Net, interface: &str) -> Result<()> {
    let (connection, handle, _) = new_connection()?;
    tokio::spawn(connection);

    // Get interface index
    let mut links = handle.link().get().match_name(interface.to_string()).execute();
    let link = links.try_next().await?.context("Interface not found")?;
    let if_index = link.header.index;

    // Add route
    handle
        .route()
        .add()
        .v4()
        .destination_prefix(dest.addr(), dest.prefix_len())
        .output_interface(if_index)
        .execute()
        .await?;

    Ok(())
}
```

## RPC 3: RemoveRoute

### Purpose
Removes a kernel route for a remote node's subnet when that node leaves the cluster.

### When Called
Whenever a node is marked as "down" by the control plane's heartbeat monitor, or when an agent detects a peer is no longer in the node list.

### Request
```protobuf
message RemoveRouteRequest {
  string destination = 1;  // e.g., "10.42.2.0/24"
}
```

### Response
```protobuf
message RemoveRouteResponse {
  bool success = 1;
  string error = 2;
}
```

### Implementation Requirements

**What Quilt needs to do:**
1. Parse the destination subnet
2. Remove the kernel route using netlink or `ip route del`
3. Handle idempotency (removing a non-existent route should succeed)

**Equivalent command:**
```bash
ip route del 10.42.2.0/24
```

**Pseudocode:**
```rust
async fn remove_route(request: RemoveRouteRequest) -> Result<RemoveRouteResponse> {
    let dest = request.destination.parse::<Ipv4Net>()?;

    match route_manager.delete_route(dest).await {
        Ok(_) => Ok(RemoveRouteResponse {
            success: true,
            error: String::new(),
        }),
        Err(e) if e.kind() == ErrorKind::NotFound => {
            // Idempotent: route doesn't exist is OK
            Ok(RemoveRouteResponse { success: true, error: String::new() })
        }
        Err(e) => Ok(RemoveRouteResponse {
            success: false,
            error: e.to_string(),
        }),
    }
}
```

## Integration Checklist

- [ ] Add `tonic` and `prost` dependencies to Quilt's Cargo.toml
- [ ] Add `tonic-build` to build-dependencies
- [ ] Create build.rs to compile proto file
- [ ] Copy `agent/proto/quilt.proto` to Quilt repo
- [ ] Implement `ConfigureNodeSubnet` RPC
- [ ] Implement `InjectRoute` RPC
- [ ] Implement `RemoveRoute` RPC
- [ ] Add gRPC server to Quilt main.rs (default port: 50051)
- [ ] Test with Quilt Mesh agent

## Testing

Once implemented, test with:

```bash
# Start Quilt runtime with gRPC server
./quilt-runtime --grpc-port 50051

# In another terminal, start Quilt Mesh agent
./quilt-mesh-agent --control-plane http://CONTROL_IP:8080 --host-ip NODE_IP

# Agent should successfully call:
# 1. ConfigureNodeSubnet on startup
# 2. InjectRoute for each peer node
# 3. RemoveRoute when peers leave
```

## Example gRPC Server Setup in Quilt

```rust
// In Quilt's main.rs or network module

use tonic::{transport::Server, Request, Response, Status};
use quilt_proto::quilt_runtime_server::{QuiltRuntime, QuiltRuntimeServer};

pub struct QuiltRuntimeService {
    ipam: Arc<IpamManager>,
    route_manager: Arc<RouteManager>,
}

#[tonic::async_trait]
impl QuiltRuntime for QuiltRuntimeService {
    async fn configure_node_subnet(
        &self,
        request: Request<ConfigureNodeSubnetRequest>,
    ) -> Result<Response<ConfigureNodeSubnetResponse>, Status> {
        // Implementation here
    }

    async fn inject_route(
        &self,
        request: Request<InjectRouteRequest>,
    ) -> Result<Response<InjectRouteResponse>, Status> {
        // Implementation here
    }

    async fn remove_route(
        &self,
        request: Request<RemoveRouteRequest>,
    ) -> Result<Response<RemoveRouteResponse>, Status> {
        // Implementation here
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let service = QuiltRuntimeService::new(ipam, route_manager);

    Server::builder()
        .add_service(QuiltRuntimeServer::new(service))
        .serve("127.0.0.1:50051".parse()?)
        .await?;

    Ok(())
}
```

## Dependencies to Add

```toml
[dependencies]
tonic = "0.12"
prost = "0.13"
tokio = { version = "1", features = ["full"] }

[build-dependencies]
tonic-build = "0.12"
```

## Questions?

Contact the Quilt Mesh team for clarification or assistance with the integration.
