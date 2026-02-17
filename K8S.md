# Kubernetes Parity: What `quiltc` Verifies (And What It Does Not)

Date: 2026-02-17

This document describes the **verified Kubernetes-like functionality** of `quiltc` when used against the Quilt backend (`https://backend.quilt.sh`), and the **exact scope** where Quilt’s cluster primitives provide parity with common Kubernetes workflows.

This is intentionally precise: it describes what was exercised live and what semantics are (and are not) present.

Related: `RESULTS.md` contains the concrete live verification evidence pointers.

## Mental Model Mapping (Kubernetes -> Quilt)

- **Cluster** (K8s): A control plane managing nodes and workloads.
  - **Quilt**: `cluster` object under `/api/clusters`.

- **Node** (K8s): A machine/VM running an agent (kubelet) that heartbeats and hosts pods.
  - **Quilt**: `node` under `/api/clusters/:cluster_id/nodes/*` + agent surface under `/api/agent/...`.

- **Deployment / ReplicaSet** (K8s): Desired-state “N replicas of this pod template”, plus a reconciliation loop to ensure N running.
  - **Quilt**: `workload` under `/api/clusters/:cluster_id/workloads/*` with `replicas` in the spec.

- **Pod** (K8s): A scheduled unit of work with lifecycle + status, bound to a node.
  - **Quilt**: `placement` under `/api/clusters/:cluster_id/placements` (replica_index -> node assignment + reported status).

- **Container Runtime** (K8s): Container create/exec/logs/kill/delete.
  - **Quilt**: `/api/containers/*`.

## Verified “Kubernetes-Like” Control Loop (Full Session)

The following loop was run end-to-end:

1. Create a cluster.
2. Register nodes (bootstrap key), heartbeat them to `ready`.
3. Create a workload with `replicas > 1`.
4. Reconcile: backend creates/maintains placements and assigns replicas to nodes.
5. Agent fetches placements for its node.
6. Agent materializes placements by creating containers and reporting placement status (`running` + `container_id`).
7. Delete a node and reconcile: placements are rescheduled off the deleted node (state resets to `assigned`).
8. Cleanup (delete workload, delete cluster, delete created containers).

This matches the **structural** Kubernetes pattern:
- desired state declared (workload spec + replicas)
- scheduler assigns (placements with node_id)
- kubelet materializes and reports status (agent placements + report)
- node failure/drain -> reschedule

## Parity Matrix (Verified)

The table below lists areas where we have **verified parity** with common K8s workflows and what the Quilt equivalents are.

### Workload Lifecycle (Deployment-like)

- **Create desired state**:
  - K8s: `kubectl apply -f deployment.yaml`
  - Quilt: `POST /api/clusters/:cluster_id/workloads` (spec includes `replicas`, `command`, resource fields)

- **Scale**:
  - K8s: `kubectl scale deploy X --replicas=N`
  - Quilt: `PUT /api/clusters/:cluster_id/workloads/:workload_id` with updated `replicas`

- **Observe desired state**:
  - K8s: `kubectl get deploy/rs/pods`
  - Quilt: `GET /api/clusters/:cluster_id/workloads` and `GET /api/clusters/:cluster_id/placements`

### Scheduling / Assignment (Replica -> Node)

- **Scheduler assigns work to nodes**:
  - K8s: scheduler binds pods to nodes
  - Quilt: reconcile assigns placements to `node_id` and persists them

- **Reschedule when node removed**:
  - K8s: node becomes NotReady/removed -> pods rescheduled (depending on controllers)
  - Quilt: deleting a node + reconcile moves placements to eligible nodes and sets placement state back to `assigned`

### Node Lifecycle (Kubelet-like)

- **Bootstrap registration**:
  - K8s: node joins cluster (bootstrap tokens / certs)
  - Quilt: `POST /api/agent/clusters/:cluster_id/nodes/register` using `QUILT_AGENT_KEY`

- **Heartbeat / readiness**:
  - K8s: kubelet posts node status; control plane tracks Ready/NotReady
  - Quilt: `POST /api/agent/.../heartbeat` updates node state and last_heartbeat_at

- **Drain**:
  - K8s: `kubectl drain node`
  - Quilt: `POST /api/clusters/:cluster_id/nodes/:node_id/drain` (marks draining; reconciliation behavior depends on backend)

- **Delete/deregister**:
  - K8s: delete node object / revoke credentials
  - Quilt: tenant delete node revokes token; agent deregister endpoint exists for agent-driven removal

### Runtime Operations (kubectl run/exec/logs/delete)

On a running container:

- **Exec**:
  - K8s: `kubectl exec -it pod -- cmd`
  - Quilt: `POST /api/containers/:id/exec`

- **Logs**:
  - K8s: `kubectl logs pod`
  - Quilt: `GET /api/containers/:id/logs`

- **Delete**:
  - K8s: `kubectl delete pod` (and controllers recreate if desired)
  - Quilt: `DELETE /api/containers/:id` (controller behavior depends on placement + agent loop)

### Events / Watch

- K8s: `kubectl get events`, watch APIs
- Quilt: `GET /api/events` SSE stream (verified streaming)

### Volume “kubectl cp”-like File Movement (Verified)

- K8s: `kubectl cp local -> pod:path` and `kubectl cp pod:path -> local`
- Quilt: volume file endpoints:
  - upload file: `POST /api/volumes/:name/files` (base64)
  - download file: `GET /api/volumes/:name/files/*path` (base64)
  - archive upload/extract: `POST /api/volumes/:name/archive` (base64 tar.gz)

## Where We Do NOT Have Kubernetes Parity (Not Verified or Not Implemented)

These are common Kubernetes features that are either out of scope for Quilt’s current primitives, not represented as first-class APIs, or not validated in the production session:

- **Service abstraction / ClusterIP / kube-proxy behavior**
- **Ingress / LoadBalancers**
- **DNS + service discovery semantics**
- **Namespaces**
- **RBAC / service accounts**
- **Secrets/ConfigMaps as first-class objects**
- **PodSecurity / PSP-style policy**
- **Health probes (liveness/readiness/startup)**
- **Rolling updates / surge/unavailable semantics**
- **Jobs/CronJobs**
- **Horizontal Pod Autoscaling**
- **PersistentVolume/PersistentVolumeClaim object model**
- **NetworkPolicy**
- **CNI-equivalent host networking programming as a tenant API** (host routing remains agent-only / separate concerns)
- **Multi-cluster federation**

## Summary

`quiltc` has verified parity with Kubernetes in the core control-loop sense for a defined subset:

- Desired-state replicas (workloads) -> scheduler assignments (placements) -> node agent materialization + status reporting.
- Node registration/heartbeat/readiness and reschedule on node removal.
- Day-2 operations on runtime containers (exec/logs/kill/delete) and a watch-like event stream.
- Volume file transfer that covers common `kubectl cp` use cases (via volume file endpoints).

Everything beyond that subset should be treated as “not Kubernetes parity” unless explicitly implemented and separately verified.

