use pact_avro_plugin::pact_plugin::pact_plugin_client::PactPluginClient;
use pact_avro_plugin::pact_plugin::Catalogue;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tonic::metadata::MetadataValue;

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
fn read_handshake(child: &mut Child, timeout: Duration) -> (u16, String) {
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
    let port = handshake["port"].as_u64().expect("handshake missing port") as u16;
    let server_key = handshake["serverKey"]
        .as_str()
        .expect("handshake missing serverKey")
        .to_string();
    (port, server_key)
}

fn send_update_catalogue(port: u16, server_key: &str) {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let channel = tonic::transport::Channel::from_shared(format!("http://127.0.0.1:{port}"))
            .expect("invalid plugin URI")
            .connect()
            .await
            .expect("failed to connect to the plugin");
        let authorization =
            MetadataValue::try_from(server_key).expect("serverKey is not valid metadata");
        let mut client =
            PactPluginClient::with_interceptor(channel, move |mut request: tonic::Request<()>| {
                request
                    .metadata_mut()
                    .insert("authorization", authorization.clone());
                Ok(request)
            });
        client
            .update_catalogue(Catalogue::default())
            .await
            .expect("update_catalogue call failed");
    });
}

#[test]
fn shuts_down_after_the_configured_idle_timeout() {
    let mut child = spawn_plugin("1");
    read_handshake(&mut child, Duration::from_secs(5));

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
    // timeout=4s, reset triggered at ~2s: the original (un-reset) deadline is
    // ~4s and the reset deadline is ~6s. Checkpoints are anchored to
    // `started_at` rather than accumulated sleeps, so RPC/connect latency
    // can't eat into the margin on either side.
    let mut child = spawn_plugin("4");
    let (port, server_key) = read_handshake(&mut child, Duration::from_secs(5));
    let started_at = Instant::now();

    let request_at = started_at + Duration::from_secs(2);
    while Instant::now() < request_at {
        assert!(
            child.try_wait().unwrap().is_none(),
            "plugin shut down before the gRPC activity request"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    send_update_catalogue(port, &server_key);

    let check_at = started_at + Duration::from_secs(5);
    while Instant::now() < check_at {
        assert!(
            child.try_wait().unwrap().is_none(),
            "plugin shut down even though the gRPC request at ~2s should have reset its 4s idle \
             clock, keeping it alive past the original, un-reset deadline at ~4s"
        );
        std::thread::sleep(Duration::from_millis(25));
    }

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
