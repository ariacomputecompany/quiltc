-- Nodes table tracks registered cluster nodes
CREATE TABLE IF NOT EXISTS nodes (
    node_id TEXT PRIMARY KEY,
    hostname TEXT NOT NULL,
    host_ip TEXT NOT NULL,
    subnet TEXT NOT NULL,           -- e.g., "10.42.1.0/24"
    cpu_cores INTEGER,
    ram_mb INTEGER,
    status TEXT NOT NULL,            -- "up" | "down"
    registered_at INTEGER NOT NULL,  -- Unix timestamp
    last_heartbeat INTEGER NOT NULL, -- Unix timestamp
    UNIQUE(host_ip)
);

CREATE INDEX IF NOT EXISTS idx_nodes_status ON nodes(status);
CREATE INDEX IF NOT EXISTS idx_nodes_heartbeat ON nodes(last_heartbeat);
