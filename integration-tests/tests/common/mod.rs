use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::Duration;
use tempfile::TempDir;

/// Find a free TCP port by binding to port 0
pub fn find_free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind to port 0");
    listener.local_addr().unwrap().port()
}

/// Wait for a TCP port to accept connections
pub async fn wait_for_port(port: u16, timeout: Duration) {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if tokio::time::Instant::now() > deadline {
            panic!("Timed out waiting for port {} to be ready", port);
        }
        if tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port))
            .await
            .is_ok()
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Get the path to a compiled binary in the target directory
fn cargo_bin(name: &str) -> PathBuf {
    // Look for the binary in target/debug (standard cargo test location)
    let mut path = std::env::current_exe()
        .expect("Failed to get current exe")
        .parent()
        .expect("No parent")
        .parent()
        .expect("No grandparent")
        .to_path_buf();
    path.push(name);
    if path.exists() {
        return path;
    }

    // Fallback: try target/debug directly
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop(); // integration-tests -> workspace root
    path.push("target");
    path.push("debug");
    path.push(name);
    if path.exists() {
        return path;
    }

    panic!(
        "Binary '{}' not found. Run `cargo build --workspace --features dev-stubs` first.",
        name
    );
}

/// A test cluster with control plane, runtime(s), and agent(s)
pub struct TestCluster {
    pub control_port: u16,
    control: Child,
    runtimes: Vec<Child>,
    agents: Vec<Child>,
    _temp_dir: TempDir,
    db_path: PathBuf,
}

impl TestCluster {
    /// Start a control plane on an ephemeral port
    pub async fn new() -> Self {
        let port = find_free_port();
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let db_path = temp_dir.path().join("control.db");

        let control = Command::new(cargo_bin("quilt-mesh-control"))
            .args(["--bind", &format!("127.0.0.1:{}", port)])
            .args(["--db-path", db_path.to_str().unwrap()])
            .args(["--log-level", "debug"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("Failed to start control plane");

        wait_for_port(port, Duration::from_secs(10)).await;

        Self {
            control_port: port,
            control,
            runtimes: Vec::new(),
            agents: Vec::new(),
            db_path,
            _temp_dir: temp_dir,
        }
    }

    /// Add a node (runtime + agent pair) with the given host IP
    pub async fn add_node(&mut self, host_ip: &str) -> u16 {
        let grpc_port = find_free_port();

        // Start runtime
        let runtime = Command::new(cargo_bin("quilt-runtime"))
            .args(["--grpc-addr", &format!("127.0.0.1:{}", grpc_port)])
            .args(["--log-level", "debug"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("Failed to start runtime");

        wait_for_port(grpc_port, Duration::from_secs(10)).await;

        // Start agent
        let agent = Command::new(cargo_bin("quilt-mesh-agent"))
            .args([
                "--control-plane",
                &format!("http://127.0.0.1:{}", self.control_port),
            ])
            .args([
                "--quilt-runtime",
                &format!("http://127.0.0.1:{}", grpc_port),
            ])
            .args(["--host-ip", host_ip])
            .args(["--log-level", "debug"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("Failed to start agent");

        // Give agent time to register
        tokio::time::sleep(Duration::from_secs(1)).await;

        self.runtimes.push(runtime);
        self.agents.push(agent);

        grpc_port
    }

    /// Get the control plane API URL
    pub fn control_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.control_port)
    }

    /// Get a mutable reference to an agent process
    pub fn agent_mut(&mut self, index: usize) -> &mut Child {
        &mut self.agents[index]
    }

    /// Restart the control plane (same DB path, new port)
    pub async fn restart_control(&mut self) -> u16 {
        let _ = self.control.kill();
        let _ = self.control.wait();

        let port = find_free_port();

        self.control = Command::new(cargo_bin("quilt-mesh-control"))
            .args(["--bind", &format!("127.0.0.1:{}", port)])
            .args(["--db-path", self.db_path.to_str().unwrap()])
            .args(["--log-level", "debug"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("Failed to restart control plane");

        wait_for_port(port, Duration::from_secs(10)).await;

        self.control_port = port;
        port
    }
}

impl Drop for TestCluster {
    fn drop(&mut self) {
        for agent in &mut self.agents {
            let _ = agent.kill();
            let _ = agent.wait();
        }
        for runtime in &mut self.runtimes {
            let _ = runtime.kill();
            let _ = runtime.wait();
        }
        let _ = self.control.kill();
        let _ = self.control.wait();
    }
}

/// Response types for deserialization
#[derive(Debug, serde::Deserialize)]
pub struct Node {
    pub node_id: String,
    pub hostname: String,
    pub host_ip: String,
    pub subnet: String,
    pub status: String,
}

#[derive(Debug, serde::Deserialize)]
pub struct ListNodesResponse {
    pub nodes: Vec<Node>,
}

#[derive(Debug, serde::Deserialize)]
pub struct RegisterNodeResponse {
    pub node_id: String,
    pub subnet: String,
}
