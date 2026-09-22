use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn shuts_down_after_the_configured_idle_timeout() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_pact-avro-plugin"))
        .args(["--timeout", "1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    // Wait for the handshake line so the watchdog's clock and this test's
    // clock start from roughly the same point.
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut handshake = String::new();
    stdout.read_line(&mut handshake).unwrap();
    assert!(handshake.contains("\"port\""));

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
