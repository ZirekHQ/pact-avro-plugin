use pact_avro_plugin::pact_plugin::pact_plugin_client::PactPluginClient;
use pact_avro_plugin::pact_plugin::Catalogue;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn spawn_plugin(timeout_secs: &str) -> Child {
    Command::new(env!("CARGO_BIN_EXE_pact-avro-plugin"))
        .args(["--timeout", timeout_secs])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

/// Reads the handshake line on a background thread so a plugin that never
/// writes one (hangs, crashes without output) can't block the test past
/// `timeout` — `read_line` alone gives no such bound.
fn read_handshake_port(child: &mut Child, timeout: Duration) -> u16 {
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = BufReader::new(stdout).read_line(&mut line);
        let _ = tx.send(line);
    });
    let line = rx
        .recv_timeout(timeout)
        .expect("plugin did not print a handshake in time");
    let handshake: serde_json::Value =
        serde_json::from_str(line.trim()).expect("handshake line was not valid JSON");
    handshake["port"].as_u64().expect("handshake missing port") as u16
}

fn send_update_catalogue(port: u16) {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let mut client = PactPluginClient::connect(format!("http://127.0.0.1:{port}"))
            .await
            .expect("failed to connect to the plugin");
        client
            .update_catalogue(Catalogue::default())
            .await
            .expect("update_catalogue call failed");
    });
}

#[test]
fn shuts_down_after_the_configured_idle_timeout() {
    let mut child = spawn_plugin("1");
    read_handshake_port(&mut child, Duration::from_secs(5));

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            return;
        }
        assert!(
            Instant::now() < deadline,
            "plugin did not self-terminate within the idle timeout"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn grpc_activity_resets_the_idle_clock() {
    // timeout=3s, reset triggered at ~1s: the original (un-reset) deadline is
    // ~3s and the reset deadline is ~4s, leaving a 500ms margin on both sides
    // of the t=3.5s check below to absorb CI scheduling jitter.
    let mut child = spawn_plugin("3");
    let port = read_handshake_port(&mut child, Duration::from_secs(5));

    std::thread::sleep(Duration::from_millis(1000));
    send_update_catalogue(port);

    std::thread::sleep(Duration::from_millis(2500));
    assert!(
        child.try_wait().unwrap().is_none(),
        "plugin shut down even though the gRPC request at ~1s should have reset its 3s idle \
         clock, keeping it alive past the original, un-reset deadline at ~3s"
    );

    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            return;
        }
        assert!(
            Instant::now() < deadline,
            "plugin did not self-terminate within the reset idle timeout"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}
