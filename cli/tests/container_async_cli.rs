use assert_cmd::Command;
use httpmock::prelude::*;
use predicates::str::contains;

fn quiltc_cmd(server: &MockServer) -> Command {
    let mut cmd = Command::cargo_bin("quiltc").expect("quiltc binary");
    cmd.arg("--base-url")
        .arg(server.base_url())
        .arg("--api-key")
        .arg("test-key");
    cmd
}

#[test]
fn create_supports_async_mode_override() {
    let server = MockServer::start();
    let create = server.mock(|when, then| {
        when.method(POST)
            .path("/api/containers")
            .body_contains("\"async_mode\":false");
        then.status(202).json_body_obj(&serde_json::json!({
            "success": true,
            "operation_id": "op-create",
            "status_url": "/api/operations/op-create"
        }));
    });

    quiltc_cmd(&server)
        .args([
            "containers",
            "create",
            r#"{"name":"c1","image":"alpine"}"#,
            "--async-mode",
            "false",
        ])
        .assert()
        .success()
        .stdout(contains("\"operation_id\": \"op-create\""));

    create.assert();
}

#[test]
fn batch_create_defaults_to_async_true() {
    let server = MockServer::start();
    let batch = server.mock(|when, then| {
        when.method(POST)
            .path("/api/containers/batch")
            .body_contains("\"async_mode\":true")
            .body_contains("\"items\":[");
        then.status(202).json_body_obj(&serde_json::json!({
            "success": true,
            "operation_id": "op-batch",
            "status_url": "/api/operations/op-batch"
        }));
    });

    quiltc_cmd(&server)
        .args([
            "containers",
            "batch-create",
            r#"[{"name":"a","image":"alpine"}]"#,
        ])
        .assert()
        .success()
        .stdout(contains("\"operation_id\": \"op-batch\""));

    batch.assert();
}

#[test]
fn stop_delete_and_start_support_async_mode_bodies() {
    let server = MockServer::start();
    let stop = server.mock(|when, then| {
        when.method(POST)
            .path("/api/containers/c1/stop")
            .body_contains("\"async_mode\":true");
        then.status(202).json_body_obj(&serde_json::json!({
            "success": true,
            "operation_id": "op-stop",
            "status_url": "/api/operations/op-stop"
        }));
    });
    let delete = server.mock(|when, then| {
        when.method(DELETE)
            .path("/api/containers/c1")
            .body_contains("\"async_mode\":false");
        then.status(200).json_body_obj(&serde_json::json!({
            "success": true,
            "message": "Container removed successfully"
        }));
    });
    let start = server.mock(|when, then| {
        when.method(POST)
            .path("/api/containers/c1/start")
            .body_contains("\"async_mode\":true");
        then.status(200).json_body_obj(&serde_json::json!({
            "success": true,
            "message": "Container start initiated"
        }));
    });

    quiltc_cmd(&server)
        .args(["containers", "stop", "c1", "--async-mode", "true"])
        .assert()
        .success();
    quiltc_cmd(&server)
        .args(["containers", "delete", "c1", "--async-mode", "false"])
        .assert()
        .success()
        .stdout(contains("Container removed successfully"));
    quiltc_cmd(&server)
        .args(["containers", "start", "c1", "--async-mode", "true"])
        .assert()
        .success()
        .stdout(contains("Container start initiated"));

    stop.assert();
    delete.assert();
    start.assert();
}

#[test]
fn fork_clone_and_resume_accept_async_mode() {
    let server = MockServer::start();
    let fork = server.mock(|when, then| {
        when.method(POST)
            .path("/api/containers/c1/fork")
            .header_exists("idempotency-key")
            .body_contains("\"async_mode\":true");
        then.status(202).json_body_obj(&serde_json::json!({
            "success": true,
            "operation_id": "op-fork",
            "status_url": "/api/operations/op-fork"
        }));
    });
    let clone = server.mock(|when, then| {
        when.method(POST)
            .path("/api/snapshots/s1/clone")
            .header_exists("idempotency-key")
            .body_contains("\"async_mode\":false");
        then.status(200).json_body_obj(&serde_json::json!({
            "success": true,
            "result": {"container_id":"c2"}
        }));
    });
    let resume = server.mock(|when, then| {
        when.method(POST)
            .path("/api/containers/c1/resume")
            .header_exists("idempotency-key")
            .body_contains("\"async_mode\":true");
        then.status(202).json_body_obj(&serde_json::json!({
            "success": true,
            "operation_id": "op-resume",
            "status_url": "/api/operations/op-resume"
        }));
    });

    quiltc_cmd(&server)
        .args(["containers", "fork", "c1", "--async-mode", "true"])
        .assert()
        .success()
        .stdout(contains("\"operation_id\": \"op-fork\""));
    quiltc_cmd(&server)
        .args(["snapshots", "clone", "s1", "--async-mode", "false"])
        .assert()
        .success()
        .stdout(contains("\"success\": true"));
    quiltc_cmd(&server)
        .args(["containers", "resume", "c1", "--async-mode", "true"])
        .assert()
        .success()
        .stdout(contains("\"operation_id\": \"op-resume\""));

    fork.assert();
    clone.assert();
    resume.assert();
}

#[test]
fn operations_watch_treats_timed_out_as_terminal_failure() {
    let server = MockServer::start();
    let get = server.mock(|when, then| {
        when.method(GET).path("/api/operations/op-timeout");
        then.status(200).json_body_obj(&serde_json::json!({
            "operation_id":"op-timeout",
            "status":"timed_out",
            "target_resource_type":"container",
            "target_resource_id":"c1",
            "items":[]
        }));
    });

    quiltc_cmd(&server)
        .args(["operations", "watch", "op-timeout", "--timeout-secs", "1"])
        .assert()
        .failure()
        .stderr(contains(
            "operation op-timeout terminated with status=timed_out",
        ));

    get.assert();
}
