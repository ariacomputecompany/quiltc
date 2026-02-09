-- Containers table tracks containers across the cluster
CREATE TABLE IF NOT EXISTS containers (
    container_id TEXT PRIMARY KEY,
    node_id TEXT NOT NULL REFERENCES nodes(node_id),
    name TEXT NOT NULL,
    namespace TEXT NOT NULL DEFAULT 'default',
    image TEXT NOT NULL,
    ip_address TEXT,
    created_at INTEGER NOT NULL,
    status TEXT NOT NULL,  -- "pending" | "running" | "stopped" | "failed"
    UNIQUE(namespace, name)
);

CREATE INDEX IF NOT EXISTS idx_containers_node ON containers(node_id);
CREATE INDEX IF NOT EXISTS idx_containers_namespace ON containers(namespace);
CREATE INDEX IF NOT EXISTS idx_containers_status ON containers(status);
