# What RPCs Need to Be Added to Quilt

This document answers the question: **"What needs to be added to the actual Quilt container runtime?"**

---

## Quick Answer

The actual Quilt runtime needs to implement **3 gRPC RPCs** that allow the Quilt Mesh agent to configure networking. These RPCs are already defined in `agent/proto/quilt.proto`.

---

## The 3 RPCs

### 1. ConfigureNodeSubnet

**Purpose**: Tell Quilt which /24 subnet to use for container IP allocation on this node.

**When called**: Once at agent startup, right after node registration.

**Request**:
```protobuf
message ConfigureNodeSubnetRequest {
  string subnet = 1;  // Example: "10.42.1.0/24"
}
```

**Response**:
```protobuf
message ConfigureNodeSubnetResponse {
  bool success = 1;
  string error = 2;  // Only set if success=false
}
```

**What Quilt does**:
1. Parse the subnet string (e.g., "10.42.1.0/24")
2. Validate it's a /24 subnet
3. Validate it's within 10.42.0.0/16 (cluster CIDR)
4. Configure IPAM to only allocate container IPs from this range
5. Return success or error

**Example**:
```
Agent calls: ConfigureNodeSubnet("10.42.1.0/24")
Quilt responds: { success: true, error: "" }
Result: Future containers get IPs like 10.42.1.10, 10.42.1.11, etc.
```

**Reference code**: `runtime/src/ipam.rs:49-67`

---

### 2. InjectRoute

**Purpose**: Add a kernel route so traffic to remote containers goes through the VXLAN overlay.

**When called**:
- Once for each existing node when agent starts
- Whenever a new node joins
- Every 5s during peer sync (idempotent)

**Request**:
```protobuf
message InjectRouteRequest {
  string destination = 1;     // Example: "10.42.2.0/24"
  string via_interface = 2;   // Example: "vxlan100"
}
```

**Response**:
```protobuf
message InjectRouteResponse {
  bool success = 1;
  string error = 2;
}
```

**What Quilt does**:
1. Parse the destination subnet
2. Look up the interface index for `via_interface` (e.g., vxlan100)
3. Add kernel route using netlink
4. **Important**: If route already exists, return success (idempotent)
5. Return success or error

**Equivalent shell command**:
```bash
ip route add 10.42.2.0/24 dev vxlan100
```

**Example**:
```
Agent calls: InjectRoute("10.42.2.0/24", "vxlan100")
Quilt responds: { success: true, error: "" }
Result: Traffic to 10.42.2.x goes through vxlan100 interface
```

**Reference code**: `runtime/src/route_manager.rs:68-138`

---

### 3. RemoveRoute

**Purpose**: Remove a kernel route when a remote node leaves the cluster.

**When called**: Whenever a node goes down or disappears from the node list.

**Request**:
```protobuf
message RemoveRouteRequest {
  string destination = 1;  // Example: "10.42.2.0/24"
}
```

**Response**:
```protobuf
message RemoveRouteResponse {
  bool success = 1;
  string error = 2;
}
```

**What Quilt does**:
1. Parse the destination subnet
2. Remove kernel route using netlink
3. **Important**: If route doesn't exist, return success (idempotent)
4. Return success or error

**Equivalent shell command**:
```bash
ip route del 10.42.2.0/24
```

**Example**:
```
Agent calls: RemoveRoute("10.42.2.0/24")
Quilt responds: { success: true, error: "" }
Result: Route to 10.42.2.x is removed from routing table
```

**Reference code**: `runtime/src/route_manager.rs:177-223`

---

## Implementation Requirements

### Dependencies Needed

Add to Quilt's `Cargo.toml`:

```toml
[dependencies]
tonic = "0.12"      # gRPC framework
prost = "0.13"      # Protocol buffers
ipnet = "2.10"      # IP/CIDR parsing

[target.'cfg(target_os = "linux")'.dependencies]
rtnetlink = "0.14"  # Netlink for route management
futures = "0.3"     # Async utilities

[build-dependencies]
tonic-build = "0.12"  # Compile .proto files
```

### Proto File

Copy `agent/proto/quilt.proto` to Quilt's repo and create `build.rs`:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::compile_protos("proto/quilt.proto")?;
    Ok(())
}
```

### Core Logic

You need to implement two modules:

**1. IPAM Module** (~150 lines)
- Store configured subnet
- Validate subnets (must be /24, within 10.42.0.0/16)
- Update IP allocation pool

**2. Route Manager Module** (~200 lines)
- Add routes via rtnetlink: `handle.route().add().v4()...`
- Remove routes via rtnetlink: `handle.route().del().v4()...`
- Handle idempotency (EEXIST for add, ENOENT for remove)

**3. gRPC Service** (~100 lines)
- Implement `QuiltRuntime` trait from generated proto code
- Wire up the 3 RPC handlers to call IPAM and route manager

### Server Startup

Add to Quilt's `main.rs`:

```rust
use tonic::transport::Server;

#[tokio::main]
async fn main() -> Result<()> {
    // ... existing Quilt startup ...

    // Create service
    let ipam = Arc::new(IpamManager::new());
    let route_manager = Arc::new(RouteManager::new().await?);
    let service = QuiltRuntimeService::new(ipam, route_manager);

    // Start gRPC server
    tokio::spawn(async move {
        Server::builder()
            .add_service(QuiltRuntimeServer::new(service))
            .serve("127.0.0.1:50051".parse()?)
            .await?;
    });

    // ... rest of Quilt runtime ...
}
```

---

## How the Agent Uses These RPCs

Here's the complete flow:

```
1. Agent starts
   ↓
2. Registers with control plane
   → Gets assigned subnet (e.g., "10.42.1.0/24")
   ↓
3. Calls Quilt: ConfigureNodeSubnet("10.42.1.0/24")
   → Quilt configures IPAM to use this subnet
   ↓
4. Agent polls for other nodes every 5s
   ↓
5. New peer appears (node 2 with subnet 10.42.2.0/24)
   ↓
6. Calls Quilt: InjectRoute("10.42.2.0/24", "vxlan100")
   → Quilt adds route: ip route add 10.42.2.0/24 dev vxlan100
   ↓
7. Peer node goes down
   ↓
8. Calls Quilt: RemoveRoute("10.42.2.0/24")
   → Quilt removes route: ip route del 10.42.2.0/24
```

---

## Testing the Integration

Once implemented, test like this:

```bash
# Terminal 1: Start control plane
./quilt-mesh-control --listen 0.0.0.0:8080

# Terminal 2: Start Quilt runtime with gRPC
./quilt-runtime --grpc-addr 127.0.0.1:50051

# Terminal 3: Start agent
./quilt-mesh-agent \
  --control-plane http://localhost:8080 \
  --host-ip 192.168.1.10

# Terminal 4: Verify routes (Linux only)
ip route show | grep vxlan100
```

**Expected logs from Quilt**:
```
INFO: Starting gRPC server on 127.0.0.1:50051
INFO: RPC: ConfigureNodeSubnet(subnet=10.42.1.0/24)
INFO: IPAM configured with subnet: 10.42.1.0/24
INFO: RPC: InjectRoute(destination=10.42.2.0/24, via=vxlan100)
INFO: Added route: 10.42.2.0/24 dev vxlan100
```

---

## Reference Implementation

We've built a **complete reference implementation** in `runtime/` that you can use as a template:

- `runtime/src/ipam.rs` - Subnet validation and management
- `runtime/src/route_manager.rs` - Route add/remove via rtnetlink
- `runtime/src/service.rs` - gRPC service implementation
- `runtime/src/main.rs` - Server startup

**Use this as your guide** when adding these RPCs to the actual Quilt runtime.

---

## Full Integration Guide

For complete step-by-step instructions, see:
- **`docs/QUILT_INTEGRATION_CHECKLIST.md`** - Detailed checklist with code examples
- **`docs/RUNTIME_IMPLEMENTATION.md`** - Reference implementation documentation

---

## Estimated Effort

| Task | Time | Lines of Code |
|------|------|---------------|
| Add dependencies | 15 min | ~15 lines |
| Setup proto compilation | 15 min | ~5 lines |
| Implement IPAM | 2-3 hours | ~150 lines |
| Implement route manager | 3-4 hours | ~200 lines |
| Implement gRPC service | 1-2 hours | ~100 lines |
| Integration & testing | 2-3 hours | ~50 lines |
| **Total** | **8-12 hours** | **~520 lines** |

---

## Summary

**What Quilt needs**: Add a gRPC server with 3 RPCs for network configuration.

**Why**: Allows Quilt Mesh agent to:
- Configure per-node subnet allocation
- Inject routes for remote containers
- Clean up routes when nodes leave

**How**: Use our reference implementation in `runtime/` as a template.

**Effort**: ~8-12 hours of development time.

**Result**: Full multi-host container networking with automatic route management.

The agent is already built and ready to use these RPCs once they're implemented in Quilt!
