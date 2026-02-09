# Quilt Mesh - Multi-Host Container Networking

Quilt Mesh is a multi-host container networking system that provides seamless networking across multiple nodes using VXLAN overlays.

## Architecture

The system consists of three main components:

### 1. Control Plane (`control/`)
- Central coordinator for the mesh network
- Manages node registry and heartbeat monitoring
- Assigns unique /24 subnets to each node from the cluster CIDR (10.42.0.0/16)
- Provides REST API for node registration and status

### 2. Mesh Agent (`agent/`)
- Runs on each node in the cluster
- Manages VXLAN overlay network configuration
- Synchronizes with control plane for peer discovery
- Configures Quilt runtime via gRPC

### 3. Quilt Runtime (`runtime/`)
- Container runtime with gRPC API
- Implements 3 core RPCs for network management:
  - **ConfigureNodeSubnet**: Sets per-node subnet for container IP allocation
  - **InjectRoute**: Adds kernel routes for remote container subnets
  - **RemoveRoute**: Removes routes when nodes leave

## Components

### Control Plane Services
- **Node Registry**: Tracks all nodes and assigns subnets
- **IPAM**: Allocates /24 subnets from 10.42.0.0/16
- **Heartbeat Monitor**: Detects node failures (30s timeout)
- **Container Registry**: Tracks containers across the mesh (future)
- **Scheduler**: Container placement decisions (future)

### Agent Features
- Automatic VXLAN interface setup (vxlan100)
- FDB entry management for peer discovery
- 5-second peer sync loop
- 10-second heartbeat interval
- Automatic route injection/removal

### Runtime Features
- gRPC server on port 50051
- IPAM with /24 subnet enforcement
- Kernel route management via netlink
- Idempotent operations (safe to retry)
- Linux-only (uses rtnetlink)

## Building

```bash
# Build all components
cargo build --release

# Build specific component
cargo build -p quilt-mesh-control --release
cargo build -p quilt-mesh-agent --release
cargo build -p quilt-runtime --release

# Run tests
cargo test --workspace
```

## Usage

### 1. Start Control Plane

```bash
./target/release/quilt-mesh-control \
  --listen 0.0.0.0:8080 \
  --log-level info
```

### 2. Start Quilt Runtime (on each node)

```bash
./target/release/quilt-runtime \
  --grpc-addr 127.0.0.1:50051 \
  --log-level info
```

### 3. Start Mesh Agent (on each node)

```bash
./target/release/quilt-mesh-agent \
  --control-plane http://CONTROL_IP:8080 \
  --host-ip NODE_IP \
  --log-level info
```

## Example: 3-Node Cluster

### Node 1 (Control Plane)
```bash
# Terminal 1: Control plane
./target/release/quilt-mesh-control --listen 0.0.0.0:8080

# Terminal 2: Runtime
./target/release/quilt-runtime

# Terminal 3: Agent
./target/release/quilt-mesh-agent \
  --control-plane http://localhost:8080 \
  --host-ip 192.168.1.10
```

### Node 2
```bash
# Terminal 1: Runtime
./target/release/quilt-runtime

# Terminal 2: Agent
./target/release/quilt-mesh-agent \
  --control-plane http://192.168.1.10:8080 \
  --host-ip 192.168.1.11
```

### Node 3
```bash
# Terminal 1: Runtime
./target/release/quilt-runtime

# Terminal 2: Agent
./target/release/quilt-mesh-agent \
  --control-plane http://192.168.1.10:8080 \
  --host-ip 192.168.1.12
```

## Network Flow

1. **Node Registration**
   - Agent registers with control plane
   - Control plane assigns unique /24 subnet (e.g., 10.42.1.0/24)
   - Agent configures local Quilt runtime with subnet

2. **Peer Discovery**
   - Agent polls control plane every 5s for node list
   - Detects new peers and removed peers
   - Updates VXLAN FDB and routing table

3. **Route Management**
   - New peer: `ip route add 10.42.2.0/24 dev vxlan100`
   - Removed peer: `ip route del 10.42.2.0/24`
   - Routes injected via Quilt runtime gRPC

4. **Container Networking**
   - Quilt runtime allocates IPs from node's /24 subnet
   - Traffic to remote containers routes through vxlan100
   - VXLAN encapsulates and sends to peer node

## API Endpoints

### Control Plane REST API

**Register Node**
```http
POST /api/nodes/register
Content-Type: application/json

{
  "hostname": "node1",
  "host_ip": "192.168.1.10",
  "cpu_cores": 8,
  "ram_mb": 16384
}
```

**List Nodes**
```http
GET /api/nodes
```

**Node Heartbeat**
```http
POST /api/nodes/:node_id/heartbeat
```

### Quilt Runtime gRPC API

See `agent/proto/quilt.proto` for full definitions.

**ConfigureNodeSubnet**
```protobuf
message ConfigureNodeSubnetRequest {
  string subnet = 1;  // e.g., "10.42.1.0/24"
}
```

**InjectRoute**
```protobuf
message InjectRouteRequest {
  string destination = 1;     // e.g., "10.42.2.0/24"
  string via_interface = 2;   // e.g., "vxlan100"
}
```

**RemoveRoute**
```protobuf
message RemoveRouteRequest {
  string destination = 1;     // e.g., "10.42.2.0/24"
}
```

## Configuration

### Control Plane
- `--listen`: HTTP listen address (default: 127.0.0.1:8080)
- `--db-path`: SQLite database path (default: quilt-mesh.db)
- `--log-level`: trace|debug|info|warn|error (default: info)

### Agent
- `--control-plane`: Control plane URL (required)
- `--host-ip`: This node's IP address (required)
- `--hostname`: Hostname (default: system hostname)
- `--log-level`: trace|debug|info|warn|error (default: info)

### Runtime
- `--grpc-addr`: gRPC listen address (default: 127.0.0.1:50051)
- `--log-level`: trace|debug|info|warn|error (default: info)

## Network Requirements

- **Cluster CIDR**: 10.42.0.0/16 (254 nodes max)
- **Per-Node Subnet**: /24 (254 IPs per node)
- **VXLAN Port**: UDP 4789
- **VXLAN VNI**: 100
- **Control Plane**: TCP 8080
- **Runtime gRPC**: TCP 50051

Firewall rules needed:
```bash
# Allow VXLAN between nodes
iptables -A INPUT -p udp --dport 4789 -j ACCEPT

# Allow control plane access
iptables -A INPUT -p tcp --dport 8080 -j ACCEPT

# Allow gRPC (if runtime is remote)
iptables -A INPUT -p tcp --dport 50051 -j ACCEPT
```

## Troubleshooting

### Check Node Status
```bash
curl http://CONTROL_IP:8080/api/nodes | jq
```

### Verify VXLAN Interface
```bash
ip link show vxlan100
ip -d link show vxlan100  # Detailed info
```

### Check Routes
```bash
ip route show | grep vxlan100
```

### View FDB Entries
```bash
bridge fdb show dev vxlan100
```

### Test gRPC Connection
```bash
grpcurl -plaintext localhost:50051 list
```

### Logs
Enable debug logging:
```bash
./target/release/quilt-mesh-agent --log-level debug
./target/release/quilt-runtime --log-level debug
```

## Development

### Project Structure
```
quiltc/
├── control/          # Control plane
│   ├── src/
│   │   ├── api/      # REST API handlers
│   │   ├── db/       # SQLite database
│   │   └── services/ # Core services (IPAM, registry, etc.)
│   └── Cargo.toml
├── agent/            # Mesh agent
│   ├── src/
│   │   ├── control_client/  # Control plane HTTP client
│   │   ├── overlay/         # VXLAN management
│   │   └── quilt_client/    # Runtime gRPC client
│   ├── proto/        # Proto definitions
│   └── Cargo.toml
├── runtime/          # Quilt runtime
│   ├── src/
│   │   ├── ipam.rs          # IP allocation
│   │   ├── route_manager.rs # Route management
│   │   └── service.rs       # gRPC service
│   ├── proto/        # Proto definitions (copied from agent)
│   └── Cargo.toml
└── docs/
    └── QUILT_INTEGRATION.md  # Integration guide
```

### Adding Features

**New Control Plane API**:
1. Add handler in `control/src/api/`
2. Register route in `control/src/api/mod.rs`
3. Update database schema if needed

**New Agent Feature**:
1. Add logic to appropriate module (`overlay/`, `control_client/`, etc.)
2. Update `agent/src/main.rs` to wire it up

**New Runtime RPC**:
1. Update `proto/quilt.proto`
2. Add handler in `runtime/src/service.rs`
3. Implement logic in IPAM or route manager

## Testing

### Unit Tests
```bash
cargo test --workspace
```

### Integration Test
```bash
# Start control plane
./target/debug/quilt-mesh-control &

# Start runtime
./target/debug/quilt-runtime &

# Start agent
./target/debug/quilt-mesh-agent \
  --control-plane http://localhost:8080 \
  --host-ip 127.0.0.1

# Check logs for successful registration and route injection
```

## Implementation Notes

### IPAM
- Cluster CIDR: 10.42.0.0/16 (hardcoded)
- Per-node subnets: /24 (254 usable IPs)
- Subnet assignment: sequential (10.42.1.0/24, 10.42.2.0/24, ...)
- Thread-safe using RwLock

### Route Management
- Uses rtnetlink for kernel route manipulation
- Idempotent: adding existing route or removing non-existent route succeeds
- Routes stored in-memory for tracking
- Linux-only (macOS falls back to stub mode)

### VXLAN
- Interface name: vxlan100
- VNI: 100
- Port: UDP 4789 (IANA standard)
- FDB entries: one per peer
- Multicast: disabled (unicast-only)

### Heartbeat
- Interval: 10s
- Timeout: 30s
- Failed heartbeats mark node as "down"
- Routes automatically removed when node goes down

### gRPC
- Server: Tonic 0.12
- Protocol Buffers: prost 0.13
- Health checks: not implemented yet
- Authentication: not implemented yet

## Future Work

- [ ] Container runtime integration (actual container execution)
- [ ] Container API endpoints
- [ ] Scheduler implementation
- [ ] Multi-tenancy / namespace isolation
- [ ] Network policies
- [ ] gRPC authentication & TLS
- [ ] Metrics & observability
- [ ] Health checks for all components
- [ ] Graceful shutdown handling
- [ ] Persistent state for runtime (survive restarts)

## License

TBD

## Contact

For questions or issues, see the Quilt Mesh team.
