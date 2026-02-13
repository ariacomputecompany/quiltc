use crate::common::{ListNodesResponse, TestCluster};

#[tokio::test]
async fn test_node_registration_and_unique_subnets() {
    let mut cluster = TestCluster::new().await;
    cluster.add_node("192.168.1.10").await;
    cluster.add_node("192.168.1.11").await;

    // Wait for registration to propagate
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;

    let resp = reqwest::get(&format!("{}/api/nodes", cluster.control_url()))
        .await
        .expect("Failed to list nodes")
        .json::<ListNodesResponse>()
        .await
        .expect("Failed to parse response");

    assert_eq!(resp.nodes.len(), 2, "Expected 2 nodes, got {}", resp.nodes.len());

    // Subnets must be unique
    assert_ne!(
        resp.nodes[0].subnet, resp.nodes[1].subnet,
        "Nodes got the same subnet"
    );

    // Subnets must be in the 10.42.X.0/24 range
    for node in &resp.nodes {
        assert!(
            node.subnet.starts_with("10.42."),
            "Unexpected subnet: {}",
            node.subnet
        );
        assert!(
            node.subnet.ends_with("/24"),
            "Subnet should be /24: {}",
            node.subnet
        );
    }

    // Both nodes should be "up"
    for node in &resp.nodes {
        assert_eq!(node.status, "up", "Node {} should be up", node.node_id);
    }
}

#[tokio::test]
async fn test_single_node_registration() {
    let mut cluster = TestCluster::new().await;
    cluster.add_node("192.168.1.10").await;

    let resp = reqwest::get(&format!("{}/api/nodes", cluster.control_url()))
        .await
        .expect("Failed to list nodes")
        .json::<ListNodesResponse>()
        .await
        .expect("Failed to parse response");

    assert_eq!(resp.nodes.len(), 1);
    assert_eq!(resp.nodes[0].host_ip, "192.168.1.10");
    assert_eq!(resp.nodes[0].status, "up");
    assert_eq!(resp.nodes[0].subnet, "10.42.1.0/24");
}
