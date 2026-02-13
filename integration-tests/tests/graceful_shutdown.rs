use crate::common::{ListNodesResponse, TestCluster};
use std::time::Duration;

#[tokio::test]
async fn test_agent_graceful_shutdown_deregisters() {
    let mut cluster = TestCluster::new().await;
    cluster.add_node("192.168.1.10").await;

    // Verify node is up
    tokio::time::sleep(Duration::from_secs(2)).await;
    let resp = reqwest::get(&format!("{}/api/nodes", cluster.control_url()))
        .await
        .unwrap()
        .json::<ListNodesResponse>()
        .await
        .unwrap();
    assert_eq!(resp.nodes[0].status, "up");

    // Send SIGTERM to agent (graceful shutdown)
    #[cfg(unix)]
    {
        let pid = cluster.agent_mut(0).id();
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
    }

    #[cfg(not(unix))]
    {
        let _ = cluster.agent_mut(0).kill();
    }

    // Wait for agent process to actually exit (indicates shutdown completed)
    let agent = cluster.agent_mut(0);
    for _ in 0..30 {
        if agent.try_wait().unwrap().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    // Give control plane a moment to process the deregister
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Node should be "down" immediately (not waiting for heartbeat timeout)
    let resp = reqwest::get(&format!("{}/api/nodes", cluster.control_url()))
        .await
        .unwrap()
        .json::<ListNodesResponse>()
        .await
        .unwrap();

    assert_eq!(
        resp.nodes[0].status, "down",
        "Node should be 'down' after graceful shutdown, got '{}'",
        resp.nodes[0].status
    );
}
