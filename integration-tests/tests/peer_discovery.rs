use crate::common::{ListNodesResponse, TestCluster};
use std::time::Duration;

#[tokio::test]
async fn test_peer_discovery_after_new_node() {
    let mut cluster = TestCluster::new().await;

    // Start first node
    cluster.add_node("192.168.1.10").await;

    // Wait for first node to complete initial sync
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Add second node
    cluster.add_node("192.168.1.11").await;

    // Wait for peer sync cycle (5s) + buffer
    tokio::time::sleep(Duration::from_secs(8)).await;

    // Both nodes should be up and visible
    let resp = reqwest::get(&format!("{}/api/nodes", cluster.control_url()))
        .await
        .expect("Failed to list nodes")
        .json::<ListNodesResponse>()
        .await
        .expect("Failed to parse response");

    assert_eq!(resp.nodes.len(), 2);
    assert!(resp.nodes.iter().all(|n| n.status == "up"));
}
