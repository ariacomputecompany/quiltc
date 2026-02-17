# Quiltc CLI Live Verification Results

Date: 2026-02-17

This document records a live, production-style verification of the `quiltc` CLI against the Quilt backend HTTP API. The goal is to validate that `quiltc` can act as a cluster/container management CLI by successfully calling the backend control-plane and runtime endpoints (no backend functionality is implemented here).

## Environment

- Backend base URL used: `https://backend.quilt.sh`
- Auth used for most tests: tenant API key via `X-Api-Key` (from local `.env`, not committed)
- Local env handling:
  - Repo-root `.env` exists locally and is gitignored.
  - `.env` file permissions were set to `0600`.

Notes on secrets:
- This report redacts all secret material (API keys, JWTs, node tokens).
- Some backend responses include API key values in JSON; those are not reproduced here.

## Tooling

- CLI binary: `quiltc` (Rust)
- Key env vars:
  - `QUILT_BASE_URL=https://backend.quilt.sh`
  - `QUILT_API_KEY=<REDACTED>`

## Verification Artifacts

Raw captures were written to (local machine only):
- `/tmp/quiltc_live_verify` (earlier run)
- `/tmp/quiltc_live_verify2` (earlier run)
- `/tmp/quiltc_live_verify3` (latest run used for most evidence below)

These captures may contain secret-bearing JSON fields (notably API key creation responses). Do not share them without redaction.

## Results Summary

### PASS (works with `X-Api-Key`)

- Health
  - `GET /health` returned 200.

- Containers (runtime)
  - `GET /api/containers` (list)
  - `POST /api/containers` (create)
  - `GET /api/containers/:id` (get)
  - `GET /api/containers/:id/logs` (logs)
  - `POST /api/containers/:id/exec` (exec)
  - `GET /api/containers/:id/network` (network get)
  - `GET /api/containers/:id/metrics` (metrics)
  - `POST /api/containers/:id/stop` (stop)
  - `POST /api/containers/:id/start` (start)
  - `POST /api/containers/:id/kill` (kill)
  - `DELETE /api/containers/:id` (delete)

- Container route injection (tenant-safe, container netns only)
  - `POST /api/containers/:id/routes` (route add)
  - `DELETE /api/containers/:id/routes` (route del)

- Events (SSE)
  - `GET /api/events` streamed events (e.g. `container_update`, `process_monitor_update`).

- API keys
  - `GET /api/api-keys` (list)
  - `POST /api/api-keys` (create)
  - `DELETE /api/api-keys/:id` (delete)

- Volumes (partial)
  - `GET /api/volumes` (list)
  - `POST /api/volumes` (create)
  - `GET /api/volumes/:name` (get)
  - `DELETE /api/volumes/:name` (delete)
  - File upload/download (JSON + base64):
    - `POST /api/volumes/:name/files`
    - `GET /api/volumes/:name/files/*path`
  - Archive upload/extract (JSON + base64):
    - `POST /api/volumes/:name/archive`

### Notes / Corrections

- Earlier runs attempted volume endpoints `POST /api/volumes/:name/upload` and `GET /api/volumes/:name/download`. Those endpoints are not part of the production backend surface; the correct endpoints are under `/api/volumes/:name/files` and `/api/volumes/:name/archive` (JSON + base64).
- The CLI has been updated to use the correct volume file endpoints and re-verified (see Retest sections).

## Evidence (Redacted)

### Health

- Request: `GET https://backend.quilt.sh/health`
- Evidence files: `/tmp/quiltc_live_verify3/health.headers`, `/tmp/quiltc_live_verify3/health.body`

### Containers

- Create response (example fields):
  - `container_id`: `<UUID>`
  - `ip_address`: `10.42.0.x` (allocated)
- Evidence files:
  - Create: `/tmp/quiltc_live_verify3/container_create.json`
  - Exec output: `/tmp/quiltc_live_verify3/container_exec.json`
  - Logs: `/tmp/quiltc_live_verify3/container_logs.txt`
  - Metrics: `/tmp/quiltc_live_verify3/container_metrics.json`
  - Network: `/tmp/quiltc_live_verify3/container_network.json`
  - Delete: `/tmp/quiltc_live_verify3/container_delete.json`

### Route Injection

- Route add succeeded:
  - `{ "success": true, "message": "Route 10.96.0.0/16 injected inside container network namespace" }`
- Route del succeeded:
  - `{ "success": true, "message": "Route 10.96.0.0/16 removed inside container network namespace" }`
- Evidence files:
  - `/tmp/quiltc_live_verify3/route_add.json`
  - `/tmp/quiltc_live_verify3/route_del.json`

### Events (SSE)

- Stream contained events including `container_update` and `process_monitor_update`.
- Evidence file:
  - `/tmp/quiltc_live_verify3/events.txt` (large)

### API Keys

- `POST /api/api-keys` succeeded and returned an object including an `id` and a generated `key`.
  - The returned `key` value is a secret and is not included here.
- Evidence files:
  - List: `/tmp/quiltc_live_verify3/api_keys_list.json` (contains secret values; do not share unredacted)
  - Create: `/tmp/quiltc_live_verify3/api_key_create.json` (contains a secret key; do not share unredacted)
  - Delete: `/tmp/quiltc_live_verify3/api_key_delete.json`

### Volumes

- Create/list/get/delete all succeeded using `X-Api-Key`.
- File upload/download succeeded using the `/files` endpoint (JSON + base64).
- Evidence files:
  - Create: `/tmp/quiltc_live_verify3/volume_create.json`
  - List: `/tmp/quiltc_live_verify3/volumes_list.json`
  - Get: `/tmp/quiltc_live_verify3/volume_get.json`
  - Delete: `/tmp/quiltc_live_verify3/volume_delete.json`
  - File upload/download retest: `/tmp/quiltc_live_verify6_path.txt`

## Conclusion

- `quiltc` is able to manage containers end-to-end (create, inspect, exec, logs, metrics, network, routes, lifecycle actions, delete) against `https://backend.quilt.sh` using a tenant API key.
- `quiltc` can stream tenant events via SSE.
- `quiltc` can manage API keys and volumes including file upload/download (via `/api/volumes/:name/files`) with an API key.

## Retest (Post-Fix Claim)

Date: 2026-02-17

After a report that the earlier 401s were fixed, the following rechecks were run using the same tenant API key (`X-Api-Key`). Evidence: a new local capture folder created via `mktemp` (path recorded in `/tmp/quiltc_live_verify4_path.txt`).

- `GET /api/clusters` now succeeds with `X-Api-Key`.
  - Example response: `{ "clusters": [] }`

 - Volume upload/download should be considered verified via the correct production endpoints under `/api/volumes/:name/files` (see next section).

## Retest (Volume File Endpoints)

Date: 2026-02-17

The CLI was updated to use the production volume file endpoints:

- Upload single file (JSON + base64):
  - `POST /api/volumes/:name/files` with `{ "path": "...", "content": "<base64>", "mode": 420 }`
- Download single file (JSON + base64):
  - `GET /api/volumes/:name/files/*path` returning `{ "path": "...", "content": "<base64>", ... }`

Recheck outcome:

- Upload succeeded (example response: `{ "success": true, "path": "/hello.txt", "size": 14 }`).
- Download succeeded and the decoded file contents matched the original (SHA-256 match).

Evidence: `/tmp/quiltc_live_verify6_path.txt` points to the local capture folder for this retest.

## Next Actions

1. Decide the intended production auth contract:
   - Confirm whether tenant API keys are intended to work for all tenant endpoints (clusters, workloads, placements, volumes files).

2. If JWT is required for clusters, perform a follow-up verification run using:
   - `quiltc auth login ...` then `quiltc clusters ...`
   - `quiltc volumes upload/download ...`
