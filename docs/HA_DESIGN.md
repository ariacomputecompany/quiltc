# Quilt Mesh Control Plane: High Availability Design

## Problem Statement

The control plane is a single process with an embedded SQLite database. If it goes down:

- No new nodes can register with the cluster
- Heartbeats fail silently (agents log warnings and retry)
- Peer discovery stalls (agents use their last-known peer list)
- Container scheduling is unavailable

**Impact during outage**: The data plane (VXLAN overlay, existing routes) continues functioning — existing node-to-node connectivity is unaffected. The blast radius is limited to control plane operations: registration, peer discovery of *new* nodes, and container scheduling. Nodes that depart during the outage will not be detected until recovery.

---

## Current Architecture

```
┌─────────────────────────┐
│    Control Plane (1x)   │
│    ┌─────────────────┐  │
│    │  axum HTTP API   │  │  ← Single process
│    │  ┌─────────────┐ │  │
│    │  │   SQLite    │ │  │  ← Single-file database
│    │  │  (r2d2 pool)│ │  │
│    │  └─────────────┘ │  │
│    │  IPAM (AtomicU8) │  │  ← In-memory counter
│    │  Scheduler       │  │  ← In-memory round-robin
│    │  Heartbeat Mon.  │  │  ← Background task
│    └─────────────────┘  │
└─────────────────────────┘
```

**State that must be replicated**:
- Nodes table (node_id, hostname, host_ip, subnet, status, heartbeat)
- Containers table (container_id, node_id, name, namespace, image, ip, status)
- IPAM subnet counter (next available subnet ID)
- Scheduler round-robin index

---

## Option A: PostgreSQL Backend + Active-Passive (Recommended Phase 1)

### Architecture

```
                    ┌──────────────┐
                    │ Load Balancer│
                    └──────┬───────┘
                    ┌──────┴───────┐
              ┌─────┴─────┐ ┌─────┴─────┐
              │ Control 1  │ │ Control 2  │
              │  (active)  │ │ (standby)  │
              └─────┬──────┘ └─────┬──────┘
                    └──────┬───────┘
              ┌────────────┴────────────┐
              │   PostgreSQL Primary    │
              │   (streaming repl.)     │
              │      ┌──────────┐       │
              │      │ Standby  │       │
              │      └──────────┘       │
              └─────────────────────────┘
```

### How It Works

1. **Replace SQLite with PostgreSQL**: Swap `rusqlite`/`r2d2_sqlite` for `sqlx` with the `postgres` feature. PostgreSQL handles concurrent writes from multiple control plane instances.

2. **Run 2+ control plane instances** behind a TCP load balancer (HAProxy, nginx, or cloud LB). Both instances connect to the same PostgreSQL database.

3. **IPAM becomes a database sequence**: Replace `AtomicU8` with `SELECT nextval('subnet_seq')` or a `SELECT ... FOR UPDATE` row lock pattern.

4. **Heartbeat monitor coordination**: Only one instance should run the heartbeat monitor. Use PostgreSQL advisory locks (`pg_advisory_lock`) so that only the instance holding the lock runs the background task.

5. **PostgreSQL HA via streaming replication**: Run a primary + one or more standbys with synchronous replication. Use `pg_basebackup` for initial setup. Automatic failover via Patroni, repmgr, or cloud-managed PostgreSQL.

### Code Changes Required

| File | Change |
|------|--------|
| `control/Cargo.toml` | Replace `rusqlite`, `r2d2`, `r2d2_sqlite` with `sqlx = { version = "0.8", features = ["postgres", "runtime-tokio", "macros"] }` |
| `control/src/db/mod.rs` | Replace `r2d2::Pool<SqliteConnectionManager>` with `sqlx::PgPool`. Remove `execute_async` wrapper (sqlx is natively async). |
| `control/src/services/node_registry.rs` | Rewrite SQL queries to use `sqlx::query!()` macros. Replace `rusqlite::params![]` with `$1, $2` bind params. |
| `control/src/services/container_registry.rs` | Same SQL migration. |
| `control/src/services/heartbeat_monitor.rs` | Acquire advisory lock before running. Release on drop. |
| `control/src/services/ipam.rs` | Replace `AtomicU8` with `SELECT nextval('subnet_seq')` sequence. |
| `control/src/main.rs` | Add `--database-url` CLI arg. Replace `init_db()` with `PgPool::connect()`. |
| `control/migrations/` | Convert SQL to PostgreSQL syntax (minor: `INTEGER` → `INT`, `AUTOINCREMENT` → `SERIAL`, add sequence). |

### Advantages

- Lowest-risk migration path — business logic unchanged
- PostgreSQL handles all write serialization and consistency
- Well-understood operational model (backups, monitoring, replication)
- No custom consensus implementation

### Disadvantages

- External dependency (PostgreSQL must be deployed and managed)
- Database becomes the new SPOF unless replicated
- Network latency to PostgreSQL adds overhead to API calls

### Migration Effort: Moderate

The database module and SQL queries need rewriting, but the HTTP API, VXLAN overlay, and agent code are completely unaffected. Estimated 2-3 days of focused work.

---

## Option B: Raft Consensus with Embedded State

### Architecture

```
              ┌──────────────────────┐
              │   Control Plane x3   │
              │                      │
              │  ┌──────┐ ┌──────┐  │
              │  │Node 1│ │Node 2│  │
              │  │Leader│ │Follow│  │
              │  └──┬───┘ └──┬───┘  │
              │     │  Raft   │      │
              │  ┌──┴────────┴──┐   │
              │  │   Node 3     │   │
              │  │  Follower    │   │
              │  └──────────────┘   │
              └──────────────────────┘
```

### How It Works

1. Use the `openraft` crate to maintain replicated state across 3 or 5 nodes.
2. All writes go through the Raft leader. The leader replicates the write to a quorum before responding.
3. Reads can be served by any node (eventual consistency) or only the leader (strong consistency).
4. Leader election is automatic — if the leader dies, a new leader is elected within seconds.

### Implementation Requirements

- Implement `openraft::RaftStorage` for the SQLite backend
- Implement Raft RPC transport (over gRPC or HTTP)
- Handle leader forwarding in API handlers
- Implement log compaction and snapshot transfer
- Add peer discovery for Raft cluster membership

### Advantages

- No external dependencies — fully self-contained
- Strong consistency guarantees
- Automatic leader election and failover

### Disadvantages

- Significant implementation complexity (4-6 weeks estimated)
- Minimum 3 nodes for quorum (cannot run a single-node cluster without special config)
- Split-brain is prevented by Raft but requires careful network partition handling
- Operational complexity (Raft cluster management, log compaction tuning)

### Migration Effort: High

Requires implementing Raft state machine, log storage, snapshot management, and modifying all API handlers to distinguish leader vs. follower. Major architectural change.

---

## Option C: External State Store (etcd/Consul)

### Architecture

```
              ┌──────────────────────┐
              │ Control Plane x N    │
              │ (stateless proxies)  │
              └──────────┬───────────┘
                         │
              ┌──────────┴───────────┐
              │   etcd cluster (3x)  │
              │   (Raft-based HA)    │
              └──────────────────────┘
```

### How It Works

1. Move all state (node registry, IPAM, containers) into etcd key-value store.
2. Control plane instances become stateless HTTP proxies to etcd.
3. IPAM uses etcd transactions (compare-and-swap) for atomic subnet allocation.
4. Agents could use etcd watches instead of polling (push-based peer discovery).

### Advantages

- Battle-tested HA from etcd (used by Kubernetes)
- Control plane is stateless and trivially horizontally scalable
- Watch-based notifications eliminate the 5-second polling delay
- etcd provides its own backup and restore

### Disadvantages

- Heaviest external dependency (etcd cluster must be deployed)
- Complete rewrite of the data layer
- etcd is a complex system to operate
- Overkill for small clusters

### Migration Effort: High

Requires a complete rewrite of the data layer and potentially the agent communication model.

---

## Recommendation

### Phase 1 (Near-Term): Option A — PostgreSQL

**Why**: Lowest risk, fastest to implement, and PostgreSQL is a well-understood operational component. The code changes are well-scoped (database module replacement only), and managed PostgreSQL is available on every cloud provider.

**Timeline**: 2-3 days

**Key decisions**:
- Use `sqlx` with compile-time checked queries
- Use PostgreSQL sequences for IPAM
- Use advisory locks for heartbeat monitor coordination
- Use cloud-managed PostgreSQL (RDS, Cloud SQL) for HA, or self-hosted with Patroni

### Phase 2 (Future): Evaluate Option B (openraft)

**When to consider**: If the project needs to be fully self-contained (no external database dependency), or if PostgreSQL becomes a bottleneck at scale.

**Prerequisite**: The SQLite data model from Phase 1 (or the PostgreSQL schema) can be lifted into an openraft state machine.

---

## Agent Resilience During Failover

Agents already handle control plane unavailability gracefully:

| Agent Behavior | During Outage | After Recovery |
|---------------|---------------|----------------|
| Heartbeat | Logs warning, retries every 10s | Resumes normally |
| Peer sync | Logs warning, retries every 5s, uses last-known peers | Gets updated peer list |
| VXLAN/Routes | Unchanged — data plane continues | Updated with any new/departed peers |
| Registration | Fails (new nodes cannot join) | New nodes can register |

**Risk**: Peers that depart during the outage are not detected until recovery. The heartbeat monitor only runs on the control plane, so a 10-minute outage means up to 10 minutes of stale peer state.

**Mitigation**: Agents could implement local peer health checks (direct ping/TCP check to peers via the VXLAN overlay) as a secondary detection mechanism. This would be independent of the control plane.

---

## Split-Brain Prevention

### With PostgreSQL (Option A)

- PostgreSQL serializes all writes. Two control plane instances cannot produce conflicting state.
- IPAM uses database sequences — two simultaneous registrations cannot get the same subnet.
- Heartbeat monitor uses advisory locks — only one instance runs it at a time.
- No split-brain risk at the data level.

### With Raft (Option B)

- Raft consensus prevents split-brain by requiring a quorum for all writes.
- In a 3-node cluster, at most 1 node can be isolated. The remaining 2 still form a quorum.
- The isolated node refuses writes until it rejoins the cluster.

### Load Balancer Considerations

- Use health check endpoints (`GET /health`) to route traffic only to healthy instances.
- For PostgreSQL: Both instances are healthy as long as they can reach the database.
- For Raft: Only the leader should handle writes. Followers redirect write requests to the leader.
