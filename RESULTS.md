# Quiltc Production Verification (Backend API)

Date: 2026-02-17

This document verifies that `quiltc` (the CLI only) can operate Quilt’s Kubernetes-like control plane and container runtime by successfully calling the production Quilt backend HTTP API.

Secrets policy:
- No secrets are recorded in this doc (API keys, JWTs, node tokens).
- Local capture artifacts referenced below may contain secret-bearing JSON fields; redact before sharing.

## Environment

- Base URL: `https://backend.quilt.sh`
- Auth used for these runs: tenant API key via `X-Api-Key` (loaded from local `.env`, which is gitignored)

## Verified Capabilities

### 1. Container Runtime Session (Like “kubectl run/exec/logs/delete”)

Validated end-to-end with `X-Api-Key`:
- `GET /health`
- Containers:
  - `GET /api/containers`
  - `POST /api/containers`
  - `GET /api/containers/:id`
  - `GET /api/containers/:id/logs`
  - `POST /api/containers/:id/exec`
  - `GET /api/containers/:id/metrics`
  - `GET /api/containers/:id/network`
  - `POST /api/containers/:id/start`
  - `POST /api/containers/:id/stop`
  - `POST /api/containers/:id/kill`
  - `DELETE /api/containers/:id`
- Tenant-safe route programming inside the container netns:
  - `POST /api/containers/:id/routes`
  - `DELETE /api/containers/:id/routes`
- Events:
  - `GET /api/events` (SSE)

Evidence (local): `/tmp/quiltc_live_verify3`

### 2. API Key Management

Validated with `X-Api-Key`:
- `GET /api/api-keys`
- `POST /api/api-keys`
- `DELETE /api/api-keys/:id`

Evidence (local): `/tmp/quiltc_live_verify3` (contains secret-bearing JSON; do not share unredacted)

### 3. Volumes (Including File Push/Pull)

Validated with `X-Api-Key`:
- Volume lifecycle:
  - `GET /api/volumes`
  - `POST /api/volumes`
  - `GET /api/volumes/:name`
  - `DELETE /api/volumes/:name`
- Production file endpoints (JSON + base64):
  - Upload single file: `POST /api/volumes/:name/files`
  - Download single file: `GET /api/volumes/:name/files/*path` (CLI decodes base64 and writes bytes to disk)
- Production archive endpoint (implemented in CLI):
  - Upload/extract tar.gz: `POST /api/volumes/:name/archive`

Evidence (local): `/tmp/quiltc_live_verify6_path.txt` (points to the capture folder; includes SHA-256 match evidence)

### 4. Cluster Control Plane (Tenant Endpoints)

Validated with `X-Api-Key`:
- Clusters:
  - `POST /api/clusters`
  - `GET /api/clusters`
  - `GET /api/clusters/:cluster_id`
  - `POST /api/clusters/:cluster_id/reconcile`
  - `DELETE /api/clusters/:cluster_id`
- Workloads (CRUD):
  - `POST /api/clusters/:cluster_id/workloads`
  - `GET /api/clusters/:cluster_id/workloads`
  - `GET /api/clusters/:cluster_id/workloads/:workload_id`
  - `PUT /api/clusters/:cluster_id/workloads/:workload_id`
  - `DELETE /api/clusters/:cluster_id/workloads/:workload_id`
- Placements listing:
  - `GET /api/clusters/:cluster_id/placements`
- Nodes listing:
  - `GET /api/clusters/:cluster_id/nodes`

Evidence (local): `/tmp/quiltc_cluster_session1_path.txt`, `/tmp/quiltc_cluster_tenant_session_path.txt`

## What Remains To Verify For A Full “Kubernetes Session”

To be equivalent to “kubectl apply deployment; scheduler assigns; kubelet/agent materializes; status is reported”, we still need one live run that includes:

- Agent bootstrap + node registration:
  - `POST /api/agent/clusters/:cluster_id/nodes/register` (requires `QUILT_AGENT_KEY`)
  - `POST /api/agent/clusters/:cluster_id/nodes/:node_id/heartbeat` (requires node token)
  - `GET /api/agent/clusters/:cluster_id/nodes/:node_id/placements` (agent fetches assignments)
  - `POST /api/agent/clusters/:cluster_id/nodes/:node_id/placements/:placement_id/report` (agent reports status + container_id)
  - `POST /api/agent/clusters/:cluster_id/nodes/:node_id/deregister` (cleanup)

Once nodes are registered/ready, we can also validate:
- `POST /api/clusters/:cluster_id/nodes/:node_id/drain`
- `DELETE /api/clusters/:cluster_id/nodes/:node_id` (revokes node token)
- Scheduler behavior: placements created for replicas, scale up/down, and reschedule when a node is deleted.

