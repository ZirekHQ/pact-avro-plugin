use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;
#[cfg(unix)]
use std::time::Instant;

pub fn spawn_plugin() -> Child {
    Command::new(env!("CARGO_BIN_EXE_pact-avro-plugin"))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

pub struct Handshake {
    pub port: u16,
    pub server_key: String,
}

/// Reads the handshake line on a background thread so a plugin that never
/// writes one (hangs, crashes without output) can't block the test past
/// `timeout` — `read_line` alone gives no such bound.
pub fn read_handshake(child: &mut Child, timeout: Duration) -> Handshake {
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
    Handshake {
        port: handshake["port"].as_u64().expect("handshake missing port") as u16,
        server_key: handshake["serverKey"]
            .as_str()
            .expect("handshake missing serverKey")
            .to_string(),
    }
}

/// Ends the plugin the same way a real shutdown does, not with SIGKILL: the
/// child is built with the same coverage instrumentation as the test binary,
/// and that instrumentation only flushes its profile on a clean exit --
/// SIGKILL discards it, silently zeroing out coverage for every line the
/// process reached.
#[cfg(unix)]
pub fn graceful_kill(child: &mut Child, timeout: Duration) {
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
pub fn graceful_kill(child: &mut Child, _timeout: Duration) {
    let _ = child.kill();
    let _ = child.wait();
}
