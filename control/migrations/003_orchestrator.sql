CREATE TABLE IF NOT EXISTS orchestrator_workload_policy (
    tenant_id TEXT NOT NULL,
    workload_id TEXT NOT NULL,
    max_concurrency INTEGER NOT NULL,
    hard_quota INTEGER NOT NULL,
    soft_burst INTEGER NOT NULL,
    absolute_limit INTEGER NOT NULL,
    priority INTEGER NOT NULL,
    cooldown_seconds INTEGER NOT NULL,
    hysteresis_pct REAL NOT NULL,
    burst_cpu_cap REAL NOT NULL,
    burst_mem_mb INTEGER NOT NULL,
    burst_ttl_seconds INTEGER NOT NULL,
    target_container_ids_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (tenant_id, workload_id)
);

CREATE TABLE IF NOT EXISTS orchestrator_workload_slo (
    tenant_id TEXT NOT NULL,
    workload_id TEXT NOT NULL,
    p95_latency_ms INTEGER NOT NULL,
    max_cold_start_pct REAL NOT NULL,
    max_reject_pct REAL NOT NULL,
    rto_seconds INTEGER NOT NULL,
    max_cost_per_compute_unit REAL NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (tenant_id, workload_id)
);

CREATE TABLE IF NOT EXISTS orchestrator_workload_observation (
    tenant_id TEXT NOT NULL,
    workload_id TEXT NOT NULL,
    node_group TEXT NOT NULL,
    queue_depth INTEGER NOT NULL,
    cpu_pressure REAL NOT NULL,
    mem_pressure REAL NOT NULL,
    io_pressure REAL NOT NULL,
    cold_start_pct REAL NOT NULL,
    invoke_p95_ms INTEGER NOT NULL,
    reject_pct REAL NOT NULL,
    active_compute_units INTEGER NOT NULL,
    cost_per_compute_unit REAL NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (tenant_id, workload_id)
);

CREATE TABLE IF NOT EXISTS orchestrator_node_group_observation (
    node_group TEXT PRIMARY KEY,
    cpu_pressure REAL NOT NULL,
    mem_pressure REAL NOT NULL,
    io_pressure REAL NOT NULL,
    warm_ready INTEGER NOT NULL,
    warm_hit_rate REAL NOT NULL,
    capacity_units INTEGER NOT NULL,
    used_units INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS orchestrator_intent (
    tenant_id TEXT NOT NULL,
    workload_id TEXT NOT NULL,
    target_concurrency INTEGER NOT NULL,
    burst_cpu_cap REAL NOT NULL,
    burst_mem_mb INTEGER NOT NULL,
    burst_ttl_seconds INTEGER NOT NULL,
    pool_min_ready INTEGER NOT NULL,
    pool_target_ready INTEGER NOT NULL,
    pool_max_ready INTEGER NOT NULL,
    preferred_node_group TEXT NOT NULL,
    anti_affinity INTEGER NOT NULL,
    reason_code TEXT NOT NULL,
    effective_at INTEGER NOT NULL,
    ttl_seconds INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (tenant_id, workload_id)
);

CREATE TABLE IF NOT EXISTS orchestrator_action (
    action_id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    workload_id TEXT NOT NULL,
    action_type TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    ttl_seconds INTEGER NOT NULL,
    rollback_action_json TEXT,
    parent_action_id TEXT,
    idempotency_key TEXT NOT NULL UNIQUE,
    decision_window_start INTEGER NOT NULL,
    status TEXT NOT NULL,
    reason_code TEXT,
    reason_message TEXT,
    effective_at INTEGER NOT NULL,
    outbound_requested_at INTEGER,
    runtime_operation_id TEXT,
    runtime_operation_type TEXT,
    terminal_status TEXT,
    terminal_at INTEGER,
    total_latency_ms INTEGER,
    attempt_count INTEGER NOT NULL,
    next_retry_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_orch_action_status_retry
    ON orchestrator_action(status, next_retry_at);

CREATE INDEX IF NOT EXISTS idx_orch_action_runtime_op
    ON orchestrator_action(runtime_operation_id);
