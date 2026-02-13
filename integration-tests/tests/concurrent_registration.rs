use crate::common::{RegisterNodeResponse, TestCluster};
use std::collections::HashSet;

#[tokio::test]
async fn test_concurrent_node_registration() {
    let cluster = TestCluster::new().await;
    let client = reqwest::Client::new();

    // Register 10 nodes concurrently
    let mut handles = Vec::new();
    for i in 0..10 {
        let url = format!("{}/api/nodes/register", cluster.control_url());
        let client = client.clone();
        handles.push(tokio::spawn(async move {
            client
                .post(&url)
                .json(&serde_json::json!({
                    "hostname": format!("node-{}", i),
                    "host_ip": format!("192.168.1.{}", 10 + i),
                    "cpu_cores": 4,
                    "ram_mb": 8192
                }))
                .send()
                .await
                .expect("Failed to register")
                .json::<RegisterNodeResponse>()
                .await
                .expect("Failed to parse response")
        }));
    }

    let results: Vec<RegisterNodeResponse> = futures::future::join_all(handles)
        .await
        .into_iter()
        .map(|r| r.expect("Task panicked"))
        .collect();

    // All should have unique node IDs
    let node_ids: HashSet<&str> = results.iter().map(|r| r.node_id.as_str()).collect();
    assert_eq!(node_ids.len(), 10, "Expected 10 unique node IDs");

    // All should have unique subnets
    let subnets: HashSet<&str> = results.iter().map(|r| r.subnet.as_str()).collect();
    assert_eq!(subnets.len(), 10, "Expected 10 unique subnets");
}
