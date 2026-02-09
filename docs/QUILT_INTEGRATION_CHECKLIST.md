# Integration Checklist for Actual Quilt Runtime

## Overview

This checklist describes what needs to be added to the **actual Quilt container runtime** to support Quilt Mesh networking.

Our `runtime/` directory is a **reference implementation** that demonstrates these requirements. The real Quilt runtime (presumably at `ariacomputecompany/quilt-cloud` per your CLAUDE.md) needs to implement the same 3 gRPC RPCs.

---

## What RPCs Need to Be Added to Quilt

The actual Quilt container runtime needs to implement **3 gRPC RPCs** defined in `agent/proto/quilt.proto`:

### 1. ConfigureNodeSubnet RPC

**Purpose**: Tell Quilt to narrow its container IP allocation to a specific /24 subnet.

**When Called**: Once on agent startup, after receiving subnet assignment from control plane.

**Request**:
```protobuf
message ConfigureNodeSubnetRequest {
  string subnet = 1;  // e.g., "10.42.1.0/24"
}
```

**Response**:
```protobuf
message ConfigureNodeSubnetResponse {
  bool success = 1;
  string error = 2;
}
```

**What Quilt Must Do**:
1. Parse the subnet CIDR string
2. Validate it's a /24 subnet within 10.42.0.0/16
3. Update Quilt's IPAM module to only allocate container IPs from this range
4. Return `success=true` or `success=false` + error message

**Reference Implementation**: `runtime/src/ipam.rs:49-67`

---

### 2. InjectRoute RPC

**Purpose**: Add a kernel route for a remote node's container subnet via the VXLAN interface.

**When Called**:
- Once for each existing node when agent starts
- Whenever a new node joins the cluster
- Every 5s during peer sync (idempotent)

**Request**:
```protobuf
message InjectRouteRequest {
  string destination = 1;     // e.g., "10.42.2.0/24"
  string via_interface = 2;   // e.g., "vxlan100"
}
```

**Response**:
```protobuf
message InjectRouteResponse {
  bool success = 1;
  string error = 2;
}
```

**What Quilt Must Do**:
1. Parse the destination subnet
2. Look up the interface index for `via_interface`
3. Add kernel route: `ip route add <destination> dev <via_interface>`
4. Handle idempotency (adding existing route should succeed, not error)
5. Return `success=true` or `success=false` + error message

**Equivalent Command**: `ip route add 10.42.2.0/24 dev vxlan100`

**Reference Implementation**: `runtime/src/route_manager.rs:68-138`

---

### 3. RemoveRoute RPC

**Purpose**: Remove a kernel route when a remote node leaves the cluster.

**When Called**: Whenever a node is marked as "down" by heartbeat monitor or disappears from node list.

**Request**:
```protobuf
message RemoveRouteRequest {
  string destination = 1;  // e.g., "10.42.2.0/24"
}
```

**Response**:
```protobuf
message RemoveRouteResponse {
  bool success = 1;
  string error = 2;
}
```

**What Quilt Must Do**:
1. Parse the destination subnet
2. Remove kernel route: `ip route del <destination>`
3. Handle idempotency (removing non-existent route should succeed, not error)
4. Return `success=true` or `success=false` + error message

**Equivalent Command**: `ip route del 10.42.2.0/24`

**Reference Implementation**: `runtime/src/route_manager.rs:177-223`

---

## Step-by-Step Integration Guide

### Step 1: Add Dependencies to Quilt's Cargo.toml

```toml
[dependencies]
# gRPC framework
tonic = "0.12"
prost = "0.13"

# Network types
ipnet = "2.10"

# Linux-only: netlink for route management
[target.'cfg(target_os = "linux")'.dependencies]
rtnetlink = "0.14"
futures = "0.3"

[build-dependencies]
tonic-build = "0.12"
```

### Step 2: Copy Proto File

```bash
# Copy proto definition to Quilt repo
cp agent/proto/quilt.proto /path/to/quilt-runtime/proto/
```

### Step 3: Create build.rs

Create `build.rs` in Quilt's root:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::compile_protos("proto/quilt.proto")?;
    Ok(())
}
```

### Step 4: Implement IPAM Module

Create a module to handle subnet configuration (or extend existing IPAM):

**File**: `src/ipam.rs` or similar

**Key Functions**:
- `configure_subnet(subnet: &str) -> Result<()>`
  - Validate subnet is /24
  - Validate subnet is within 10.42.0.0/16
  - Store configured subnet
  - Update IP allocation pool

**Reference**: See `runtime/src/ipam.rs` for full implementation

### Step 5: Implement Route Manager

Create a module to handle kernel route manipulation:

**File**: `src/route_manager.rs` or `src/network/routes.rs`

**Key Functions**:
- `add_route(destination: &str, interface: &str) -> Result<()>`
  - Parse subnet CIDR
  - Get interface index via netlink
  - Add route using `rtnetlink`
  - Handle `EEXIST` (route already exists) → return Ok

- `remove_route(destination: &str) -> Result<()>`
  - Parse subnet CIDR
  - Delete route using `rtnetlink`
  - Handle `ENOENT`/`ESRCH` (route doesn't exist) → return Ok

**Using rtnetlink**:
```rust
use rtnetlink::{new_connection, Handle};
use futures::TryStreamExt;

async fn add_route_example(dest: Ipv4Net, interface: &str) -> Result<()> {
    let (connection, handle, _) = new_connection()?;
    tokio::spawn(connection);

    // Get interface index
    let mut links = handle.link().get().match_name(interface.to_string()).execute();
    let link = links.try_next().await?.context("Interface not found")?;
    let if_index = link.header.index;

    // Add route
    handle.route().add().v4()
        .destination_prefix(dest.network(), dest.prefix_len())
        .output_interface(if_index)
        .execute()
        .await?;

    Ok(())
}
```

**Reference**: See `runtime/src/route_manager.rs:68-138` for full implementation

### Step 6: Implement gRPC Service

Create gRPC service that implements the 3 RPCs:

**File**: `src/grpc_service.rs` or `src/api/grpc.rs`

```rust
use tonic::{Request, Response, Status};

// Include generated proto code
pub mod quilt {
    tonic::include_proto!("quilt");
}

use quilt::quilt_runtime_server::QuiltRuntime;
use quilt::{
    ConfigureNodeSubnetRequest, ConfigureNodeSubnetResponse,
    InjectRouteRequest, InjectRouteResponse,
    RemoveRouteRequest, RemoveRouteResponse,
};

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
        let subnet = request.into_inner().subnet;

        match self.ipam.configure_subnet(&subnet).await {
            Ok(_) => Ok(Response::new(ConfigureNodeSubnetResponse {
                success: true,
                error: String::new(),
            })),
            Err(e) => Ok(Response::new(ConfigureNodeSubnetResponse {
                success: false,
                error: e.to_string(),
            })),
        }
    }

    async fn inject_route(
        &self,
        request: Request<InjectRouteRequest>,
    ) -> Result<Response<InjectRouteResponse>, Status> {
        let req = request.into_inner();

        match self.route_manager.add_route(&req.destination, &req.via_interface).await {
            Ok(_) => Ok(Response::new(InjectRouteResponse {
                success: true,
                error: String::new(),
            })),
            Err(e) => Ok(Response::new(InjectRouteResponse {
                success: false,
                error: e.to_string(),
            })),
        }
    }

    async fn remove_route(
        &self,
        request: Request<RemoveRouteRequest>,
    ) -> Result<Response<RemoveRouteResponse>, Status> {
        let req = request.into_inner();

        match self.route_manager.remove_route(&req.destination).await {
            Ok(_) => Ok(Response::new(RemoveRouteResponse {
                success: true,
                error: String::new(),
            })),
            Err(e) => Ok(Response::new(RemoveRouteResponse {
                success: false,
                error: e.to_string(),
            })),
        }
    }
}
```

**Reference**: See `runtime/src/service.rs` for full implementation

### Step 7: Start gRPC Server in Quilt's main.rs

Add gRPC server startup to Quilt's main initialization:

```rust
use tonic::transport::Server;
use quilt::quilt_runtime_server::QuiltRuntimeServer;

#[tokio::main]
async fn main() -> Result<()> {
    // ... existing Quilt initialization ...

    // Create IPAM and route manager
    let ipam = Arc::new(IpamManager::new());
    let route_manager = Arc::new(RouteManager::new().await?);

    // Create gRPC service
    let grpc_service = QuiltRuntimeService::new(ipam.clone(), route_manager.clone());

    // Start gRPC server in background
    let grpc_addr = "127.0.0.1:50051".parse().unwrap();
    tokio::spawn(async move {
        info!("Starting gRPC server on {}", grpc_addr);
        Server::builder()
            .add_service(QuiltRuntimeServer::new(grpc_service))
            .serve(grpc_addr)
            .await
            .expect("gRPC server failed");
    });

    // ... rest of Quilt runtime initialization ...

    Ok(())
}
```

### Step 8: Add CLI Flag for gRPC Port

Add command-line option to configure gRPC listen address:

```rust
use clap::Parser;

#[derive(Parser)]
struct Args {
    /// gRPC API listen address
    #[arg(long, default_value = "127.0.0.1:50051")]
    grpc_addr: String,

    // ... other Quilt args ...
}
```

---

## Testing the Integration

### Test 1: Start Quilt Runtime

```bash
./quilt-runtime --grpc-addr 127.0.0.1:50051
```

**Expected**: gRPC server starts and listens on port 50051

### Test 2: Start Quilt Mesh Agent

```bash
./quilt-mesh-agent \
  --control-plane http://localhost:8080 \
  --host-ip 192.168.1.10
```

**Expected Agent Logs**:
```
INFO: Connecting to Quilt runtime at http://127.0.0.1:50051
INFO: Successfully connected to Quilt runtime
INFO: Calling ConfigureNodeSubnet(subnet=10.42.1.0/24)
INFO: ConfigureNodeSubnet succeeded
```

**Expected Quilt Logs**:
```
INFO: Starting gRPC server on 127.0.0.1:50051
INFO: RPC: ConfigureNodeSubnet(subnet=10.42.1.0/24)
INFO: IPAM configured with subnet: 10.42.1.0/24
INFO: Successfully configured subnet: 10.42.1.0/24
```

### Test 3: Verify Route Injection (Linux only)

When a second node joins:

```bash
# On first node, check routes
ip route show | grep vxlan100

# Expected output:
# 10.42.2.0/24 dev vxlan100 scope link
```

**Expected Agent Logs**:
```
INFO: New peer discovered: subnet=10.42.2.0/24, host_ip=192.168.1.11
INFO: Calling InjectRoute(destination=10.42.2.0/24, via=vxlan100)
INFO: InjectRoute succeeded
```

**Expected Quilt Logs**:
```
INFO: RPC: InjectRoute(destination=10.42.2.0/24, via=vxlan100)
INFO: Added route: 10.42.2.0/24 dev vxlan100 (index: 5)
INFO: Successfully injected route: 10.42.2.0/24 dev vxlan100
```

### Test 4: Verify Route Removal

When second node stops:

```bash
# On first node, check routes again
ip route show | grep vxlan100

# Expected: route should be gone
```

**Expected Agent Logs**:
```
INFO: Peer removed: subnet=10.42.2.0/24
INFO: Calling RemoveRoute(destination=10.42.2.0/24)
INFO: RemoveRoute succeeded
```

**Expected Quilt Logs**:
```
INFO: RPC: RemoveRoute(destination=10.42.2.0/24)
INFO: Removed route: 10.42.2.0/24
INFO: Successfully removed route: 10.42.2.0/24
```

---

## Integration Checklist

Copy this checklist to track progress:

- [ ] Add `tonic`, `prost`, `ipnet` dependencies to Cargo.toml
- [ ] Add `rtnetlink` and `futures` for Linux target
- [ ] Add `tonic-build` to build-dependencies
- [ ] Create `build.rs` to compile proto file
- [ ] Copy `agent/proto/quilt.proto` to Quilt repo
- [ ] Implement IPAM subnet configuration logic
  - [ ] Parse CIDR strings
  - [ ] Validate /24 prefix length
  - [ ] Validate within 10.42.0.0/16 cluster CIDR
  - [ ] Store configured subnet
  - [ ] Update IP allocation pool
- [ ] Implement route manager
  - [ ] Create netlink connection (rtnetlink)
  - [ ] Implement `add_route()` with interface lookup
  - [ ] Handle idempotency for route add (EEXIST)
  - [ ] Implement `remove_route()`
  - [ ] Handle idempotency for route remove (ENOENT/ESRCH)
- [ ] Implement gRPC service
  - [ ] Include generated proto code
  - [ ] Implement `ConfigureNodeSubnet` RPC handler
  - [ ] Implement `InjectRoute` RPC handler
  - [ ] Implement `RemoveRoute` RPC handler
  - [ ] Return proper success/error responses
- [ ] Integrate gRPC server into main.rs
  - [ ] Initialize IPAM and route manager
  - [ ] Create QuiltRuntimeService
  - [ ] Start Tonic server on background task
  - [ ] Add CLI flag for gRPC address
- [ ] Test with Quilt Mesh agent
  - [ ] Verify ConfigureNodeSubnet is called on startup
  - [ ] Verify InjectRoute is called for new peers
  - [ ] Verify RemoveRoute is called when peers leave
  - [ ] Verify routes actually appear in kernel routing table

---

## Common Issues and Solutions

### Issue: "Failed to connect to Quilt runtime"

**Cause**: Quilt runtime gRPC server not started or wrong address

**Solution**:
- Ensure Quilt starts gRPC server on 127.0.0.1:50051
- Check Quilt logs for "Starting gRPC server" message
- Verify no other service is using port 50051

### Issue: "Interface 'vxlan100' not found"

**Cause**: VXLAN interface not created before route injection

**Solution**:
- Ensure agent creates vxlan100 before calling InjectRoute
- Check `ip link show vxlan100`
- Agent creates VXLAN in `overlay/vxlan.rs:setup_vxlan()`

### Issue: "EACCES (Permission denied)" when adding routes

**Cause**: Insufficient privileges to modify routing table

**Solution**:
- Run Quilt runtime as root or with `CAP_NET_ADMIN` capability
- Use `sudo ./quilt-runtime` or `setcap CAP_NET_ADMIN=+ep ./quilt-runtime`

### Issue: "Subnet must be /24" error

**Cause**: Control plane assigned non-/24 subnet

**Solution**:
- Verify control plane IPAM allocates /24 subnets
- Check `control/src/services/ipam.rs:allocate_subnet()`
- Should return subnets like "10.42.1.0/24", not "10.42.1.0/16"

---

## Estimated Effort

Based on our reference implementation:

| Task | Lines of Code | Estimated Time |
|------|---------------|----------------|
| IPAM module | ~150 lines | 2-3 hours |
| Route manager | ~200 lines | 3-4 hours |
| gRPC service | ~100 lines | 1-2 hours |
| Integration/testing | ~50 lines | 2-3 hours |
| **Total** | **~500 lines** | **8-12 hours** |

This assumes familiarity with Rust, async/await, and the Quilt codebase.

---

## Reference Implementation

The complete reference implementation is in `runtime/`:

- `runtime/src/ipam.rs` - IPAM with validation and tests
- `runtime/src/route_manager.rs` - Route management via rtnetlink
- `runtime/src/service.rs` - gRPC service implementation
- `runtime/src/main.rs` - Server startup and CLI

**Use this as a guide** when implementing in the actual Quilt runtime.

---

## Questions?

If you have questions during integration:

1. Check the reference implementation in `runtime/`
2. Review logs from our quilt-runtime with `--log-level debug`
3. Test with `grpcurl` to verify RPCs work:
   ```bash
   grpcurl -plaintext localhost:50051 list
   grpcurl -plaintext localhost:50051 quilt.QuiltRuntime/ConfigureNodeSubnet
   ```

The Quilt Mesh agent is ready to use these RPCs once they're implemented in the actual Quilt runtime!
