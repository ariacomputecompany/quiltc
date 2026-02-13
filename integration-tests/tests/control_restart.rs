use crate::common::{ListNodesResponse, RegisterNodeResponse, TestCluster};
use std::time::Duration;

#[tokio::test]
async fn test_control_plane_restart_preserves_state() {
    let mut cluster = TestCluster::new().await;

    // Register a node directly via API (no agent needed for this test)
    let client = reqwest::Client::new();
    let resp = client
        .post(&format!("{}/api/nodes/register", cluster.control_url()))
        .json(&serde_json::json!({
            "hostname": "test-node",
            "host_ip": "192.168.1.10",
            "cpu_cores": 4,
            "ram_mb": 8192
        }))
        .send()
        .await
        .unwrap()
        .json::<RegisterNodeResponse>()
        .await
        .unwrap();

    let original_node_id = resp.node_id.clone();
    let original_subnet = resp.subnet.clone();

    // Restart control plane (same DB)
    let new_port = cluster.restart_control().await;
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Node data should be preserved
    let resp = reqwest::get(&format!(
        "http://127.0.0.1:{}/api/nodes",
        new_port
    ))
    .await
    .unwrap()
    .json::<ListNodesResponse>()
    .await
    .unwrap();

    assert_eq!(resp.nodes.len(), 1, "Node should persist after restart");
    assert_eq!(resp.nodes[0].node_id, original_node_id);
    assert_eq!(resp.nodes[0].subnet, original_subnet);

    // Register another node — IPAM should continue from correct offset
    let resp = client
        .post(&format!(
            "http://127.0.0.1:{}/api/nodes/register",
            new_port
        ))
        .json(&serde_json::json!({
            "hostname": "test-node-2",
            "host_ip": "192.168.1.11",
            "cpu_cores": 2,
            "ram_mb": 4096
        }))
        .send()
        .await
        .unwrap()
        .json::<RegisterNodeResponse>()
        .await
        .unwrap();

    // Second node should get a different subnet
    assert_ne!(
        resp.subnet, original_subnet,
        "Second node should get a different subnet after restart"
    );
}
