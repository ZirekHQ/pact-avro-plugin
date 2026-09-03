use std::net::TcpListener;

/// Mirrors PortFinder.scala: binds to port 0 (OS-assigned), reads back the
/// port that was actually bound, then releases it. `None` on any bind or
/// lookup failure, matching the Scala version's Option-based signature.
pub fn find_free_port() -> Option<u16> {
    TcpListener::bind(("127.0.0.1", 0))
        .and_then(|listener| listener.local_addr())
        .map(|addr| addr.port())
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_a_usable_port() {
        let port = find_free_port().expect("a free port must be found");
        assert!(port > 0, "port must be non-zero, got {port}");
    }

    #[test]
    fn successive_calls_can_bind_the_returned_ports() {
        let a = find_free_port().expect("first port");
        let b = find_free_port().expect("second port");
        // Not asserting a != b (the OS may legitimately hand back the same
        // port once each listener is dropped) — asserting both are usable.
        assert!(std::net::TcpListener::bind(("0.0.0.0", a)).is_ok());
        assert!(std::net::TcpListener::bind(("0.0.0.0", b)).is_ok());
    }
}
