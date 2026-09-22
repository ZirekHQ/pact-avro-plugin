use pact_avro_plugin::pact_plugin::pact_plugin_client::PactPluginClient;
use pact_avro_plugin::pact_plugin::Catalogue;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use tonic::codec::CompressionEncoding;

fn spawn_plugin(args: &[&str], rust_log: &str) -> Child {
    Command::new(env!("CARGO_BIN_EXE_pact-avro-plugin"))
        .args(args)
        .env("RUST_LOG", rust_log)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

/// Reads the handshake line on a background thread so a plugin that never
/// writes one (hangs, crashes without output) can't block the test past
/// `timeout` — `read_line` alone gives no such bound.
fn read_handshake_port(child: &mut Child, timeout: Duration) -> Option<u16> {
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = BufReader::new(stdout).read_line(&mut line);
        let _ = tx.send(line);
    });
    let line = rx.recv_timeout(timeout).ok()?;
    let handshake: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    handshake["port"].as_u64().map(|p| p as u16)
}

/// Reads all stderr lines emitted within `duration` onto a background
/// thread, returning them joined, so a slow or silent child can't block the
/// test.
fn read_stderr_for(child: &mut Child, duration: Duration) -> String {
    let stderr = child.stderr.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                return;
            }
        }
    });
    let deadline = Instant::now() + duration;
    let mut lines = Vec::new();
    while let Ok(remaining) = deadline.checked_duration_since(Instant::now()).ok_or(()) {
        match rx.recv_timeout(remaining) {
            Ok(line) => lines.push(line),
            Err(_) => break,
        }
    }
    lines.join("\n")
}

/// Ends the plugin the same way a real shutdown does, not with SIGKILL: the
/// child is built with the same coverage instrumentation as the test binary,
/// and that instrumentation only flushes its profile on a clean exit --
/// SIGKILL discards it, silently zeroing out coverage for every line the
/// process reached.
#[cfg(unix)]
fn graceful_kill(child: &mut Child, timeout: Duration) {
    // SAFETY: child.id() is a live PID this process owns for the duration of
    // this call (`child` is `&mut`, so no concurrent wait/kill is racing us).
    unsafe {
        libc::kill(child.id() as libc::pid_t, libc::SIGTERM);
    }
    let deadline = Instant::now() + timeout;
    loop {
        if child.try_wait().unwrap().is_some() {
            return;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(not(unix))]
fn graceful_kill(child: &mut Child, _timeout: Duration) {
    let _ = child.kill();
    let _ = child.wait();
}

async fn connect(port: u16) -> PactPluginClient<tonic::transport::Channel> {
    PactPluginClient::connect(format!("http://127.0.0.1:{port}"))
        .await
        .expect("failed to connect to the plugin")
}

/// A TEST-NET-3 address (RFC 5737): reserved for documentation, never
/// assigned to a local interface, so binding to it must fail wherever this
/// test runs.
const UNASSIGNED_TEST_HOST: &str = "203.0.113.1";

#[test]
fn rejects_an_unbindable_host() {
    let mut child = spawn_plugin(&["--host", UNASSIGNED_TEST_HOST], "info");

    assert!(
        read_handshake_port(&mut child, Duration::from_secs(3)).is_none(),
        "plugin printed a handshake despite an unbindable --host, meaning the flag was ignored"
    );

    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(
                !status.success(),
                "plugin should exit non-zero on bind failure"
            );
            return;
        }
        assert!(
            Instant::now() < deadline,
            "plugin did not exit after a bind failure"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn negotiates_gzip_compression() {
    let mut child = spawn_plugin(&[], "info");
    let port = read_handshake_port(&mut child, Duration::from_secs(5))
        .expect("plugin did not print a handshake in time");

    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let mut client = connect(port)
            .await
            .send_compressed(CompressionEncoding::Gzip)
            .accept_compressed(CompressionEncoding::Gzip);
        client
            .update_catalogue(Catalogue::default())
            .await
            .expect("gzip-compressed update_catalogue call failed");
    });

    graceful_kill(&mut child, Duration::from_secs(2));
}

#[test]
fn logs_grpc_requests_via_tracing() {
    let mut child = spawn_plugin(&[], "tower_http=debug");
    let port = read_handshake_port(&mut child, Duration::from_secs(5))
        .expect("plugin did not print a handshake in time");

    tokio::runtime::Runtime::new().unwrap().block_on(async {
        connect(port)
            .await
            .update_catalogue(Catalogue::default())
            .await
            .expect("update_catalogue call failed");
    });

    let stderr = read_stderr_for(&mut child, Duration::from_millis(500));
    graceful_kill(&mut child, Duration::from_secs(2));

    assert!(
        stderr.contains("started processing request"),
        "expected a tower-http request span in stderr, got:\n{stderr}"
    );
    assert!(
        stderr.contains("finished processing request"),
        "expected a tower-http response event in stderr, got:\n{stderr}"
    );
}
