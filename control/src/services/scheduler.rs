use std::sync::atomic::{AtomicUsize, Ordering};

use crate::types::Node;

/// Simple round-robin scheduler for container placement
pub struct SimpleScheduler {
    current_index: AtomicUsize,
}

impl SimpleScheduler {
    pub fn new() -> Self {
        Self {
            current_index: AtomicUsize::new(0),
        }
    }

    /// Pick a node for container placement using round-robin
    /// Only considers nodes with status="up"
    pub fn pick_node<'a>(&self, nodes: &'a [Node]) -> Option<&'a Node> {
        if nodes.is_empty() {
            return None;
        }

        let idx = self.current_index.fetch_add(1, Ordering::Relaxed);
        Some(&nodes[idx % nodes.len()])
    }
}

impl Default for SimpleScheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_node(id: &str) -> Node {
        Node {
            node_id: id.to_string(),
            hostname: format!("node-{}", id),
            host_ip: format!("192.168.1.{}", id),
            subnet: format!("10.42.{}.0/24", id),
            cpu_cores: Some(4),
            ram_mb: Some(8192),
            status: "up".to_string(),
            registered_at: 0,
            last_heartbeat: 0,
        }
    }

    #[test]
    fn test_round_robin() {
        let scheduler = SimpleScheduler::new();
        let nodes = vec![
            create_test_node("1"),
            create_test_node("2"),
            create_test_node("3"),
        ];

        // Should cycle through nodes
        assert_eq!(scheduler.pick_node(&nodes).unwrap().node_id, "1");
        assert_eq!(scheduler.pick_node(&nodes).unwrap().node_id, "2");
        assert_eq!(scheduler.pick_node(&nodes).unwrap().node_id, "3");
        assert_eq!(scheduler.pick_node(&nodes).unwrap().node_id, "1");
    }

    #[test]
    fn test_empty_nodes() {
        let scheduler = SimpleScheduler::new();
        assert!(scheduler.pick_node(&[]).is_none());
    }
}
