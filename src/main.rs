use pact_avro_plugin::pact_plugin::pact_plugin_server::PactPluginServer;
use pact_avro_plugin::service::PactAvroPluginService;
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::codec::CompressionEncoding;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 600;

fn version_requested() -> bool {
    std::env::args().nth(1).as_deref() == Some("--version")
}

/// Reads `--host <address>` from the CLI args, defaulting to
/// [`DEFAULT_HOST`] when absent. Returns an error if `--host` is given
/// without a value, or if the next token is itself a flag.
fn host_arg(args: impl Iterator<Item = String>) -> Result<String, String> {
    let mut args = args;
    while let Some(arg) = args.next() {
        if arg != "--host" {
            continue;
        }
        let value = args
            .next()
            .ok_or_else(|| "--host requires a value".to_string())?;
        return if value.starts_with("--") {
            Err(format!(
                "--host requires an address value, got flag `{value}`"
            ))
        } else {
            Ok(value)
        };
    }
    Ok(DEFAULT_HOST.to_string())
}

/// Returns the bound port, or an error if `addr` won't be reachable at
/// `127.0.0.1` — the address the Pact driver always dials after reading the
/// handshake (the handshake JSON carries no host field).
fn port_reachable_via_loopback(addr: SocketAddr) -> Result<u16, String> {
    match addr.ip() {
        IpAddr::V4(ip) if ip == Ipv4Addr::LOCALHOST || ip.is_unspecified() => Ok(addr.port()),
        ip => Err(format!(
            "--host must resolve to 127.0.0.1 or 0.0.0.0, got {ip}: the Pact driver always connects to 127.0.0.1"
        )),
    }
}

/// Reads `--timeout <seconds>` from the CLI args, defaulting to
/// [`DEFAULT_IDLE_TIMEOUT_SECS`] when absent. A value of `0` disables the
/// idle watchdog. Returns an error if `--timeout` is given without a value
/// or with a non-numeric value.
fn idle_timeout_secs(args: impl Iterator<Item = String>) -> Result<u64, String> {
    let mut args = args;
    while let Some(arg) = args.next() {
        if arg != "--timeout" {
            continue;
        }
        let value = args
            .next()
            .ok_or_else(|| "--timeout requires a value".to_string())?;
        return value
            .parse::<u64>()
            .map_err(|_| format!("--timeout value must be a non-negative integer, got '{value}'"));
    }
    Ok(DEFAULT_IDLE_TIMEOUT_SECS)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if version_requested() {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let host = host_arg(std::env::args().skip(1))?;
    let idle_timeout = idle_timeout_secs(std::env::args().skip(1))?;

    let (listener, port) = bind_ephemeral_port(&host).await?;

    let server_key = Uuid::new_v4();
    print_pact_handshake(port, server_key)?;

    let start = Instant::now();
    let last_access_millis = Arc::new(AtomicU64::new(0));
    let touch_last_access = {
        let last_access_millis = last_access_millis.clone();
        move |request: tonic::Request<()>| {
            record_access(&last_access_millis, start.elapsed());
            Ok(request)
        }
    };
    let service = tonic::service::interceptor::InterceptedService::new(
        PactPluginServer::new(PactAvroPluginService)
            .accept_compressed(CompressionEncoding::Gzip)
            .send_compressed(CompressionEncoding::Gzip),
        touch_last_access,
    );

    let idle_shutdown = idle_watchdog(last_access_millis, start, idle_timeout);

    tonic::transport::Server::builder()
        .layer(TraceLayer::new_for_grpc())
        .add_service(service)
        .serve_with_incoming_shutdown(
            TcpListenerStream::new(listener),
            shutdown_signal(idle_shutdown),
        )
        .await?;

    Ok(())
}

/// Binds an OS-assigned TCP port and returns the still-bound listener
/// alongside it, so the port advertised to Pact core is guaranteed reserved
/// for this process rather than a snapshot another process could then claim.
async fn bind_ephemeral_port(
    host: &str,
) -> Result<(tokio::net::TcpListener, u16), Box<dyn std::error::Error>> {
    let listener = tokio::net::TcpListener::bind((host, 0)).await?;
    let port = port_reachable_via_loopback(listener.local_addr()?)?;
    Ok((listener, port))
}

/// Emits the single-line JSON handshake that Pact core parses from stdout
/// to discover this plugin's port and server key.
fn print_pact_handshake(port: u16, server_key: Uuid) -> std::io::Result<()> {
    let handshake = serde_json::json!({ "port": port, "serverKey": server_key.to_string() });
    println!("{handshake}");
    std::io::stdout().flush()
}

/// Advances the last-access clock forward only, via `fetch_max`: a plain
/// store could race with a newer concurrent request and roll the recorded
/// time backward.
fn record_access(last_access_millis: &AtomicU64, elapsed: Duration) {
    last_access_millis.fetch_max(elapsed.as_millis() as u64, Ordering::Relaxed);
}

/// Resolves once `timeout_secs` seconds have elapsed since the last gRPC
/// request, or since startup if none has occurred yet. Never resolves when
/// `timeout_secs` is zero — the watchdog is then disabled.
fn idle_watchdog(
    last_access_millis: Arc<AtomicU64>,
    start: Instant,
    timeout_secs: u64,
) -> Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    if timeout_secs == 0 {
        return Box::pin(std::future::pending());
    }

    let timeout = Duration::from_secs(timeout_secs);
    Box::pin(async move {
        loop {
            let elapsed_millis = start.elapsed().as_millis() as u64;
            let idle_for = Duration::from_millis(
                elapsed_millis.saturating_sub(last_access_millis.load(Ordering::Relaxed)),
            );
            match timeout.checked_sub(idle_for) {
                Some(remaining) if !remaining.is_zero() => tokio::time::sleep(remaining).await,
                _ => {
                    tracing::warn!("no gRPC activity for {}s, shutting down", timeout.as_secs());
                    return;
                }
            }
        }
    })
}

async fn shutdown_signal(idle: impl std::future::Future<Output = ()>) {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.expect(
            "failed to install Ctrl-C handler; startup is unrecoverable without it, since the process could then never shut down cleanly",
        );
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect(
                "failed to install SIGTERM handler; startup is unrecoverable without it, since the process could then never shut down cleanly",
            )
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("received Ctrl-C, shutting down"),
        _ = terminate => tracing::info!("received SIGTERM, shutting down"),
        _ = idle => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_timeout_defaults_when_absent() {
        assert_eq!(
            idle_timeout_secs(std::iter::empty()).unwrap(),
            DEFAULT_IDLE_TIMEOUT_SECS
        );
    }

    #[test]
    fn idle_timeout_parses_the_flag_value() {
        let args = vec!["--timeout".to_string(), "42".to_string()].into_iter();
        assert_eq!(idle_timeout_secs(args).unwrap(), 42);
    }

    #[test]
    fn idle_timeout_zero_is_accepted_as_disabled() {
        let args = vec!["--timeout".to_string(), "0".to_string()].into_iter();
        assert_eq!(idle_timeout_secs(args).unwrap(), 0);
    }

    #[test]
    fn idle_timeout_rejects_a_missing_value() {
        let args = vec!["--timeout".to_string()].into_iter();
        assert!(idle_timeout_secs(args).is_err());
    }

    #[test]
    fn idle_timeout_rejects_a_non_numeric_value() {
        let args = vec!["--timeout".to_string(), "soon".to_string()].into_iter();
        assert!(idle_timeout_secs(args).is_err());
    }

    #[test]
    fn host_defaults_when_absent() {
        assert_eq!(host_arg(std::iter::empty()).unwrap(), DEFAULT_HOST);
    }

    #[test]
    fn host_parses_the_flag_value() {
        let args = vec!["--host".to_string(), "0.0.0.0".to_string()].into_iter();
        assert_eq!(host_arg(args).unwrap(), "0.0.0.0");
    }

    #[test]
    fn host_rejects_a_missing_value() {
        let args = vec!["--host".to_string()].into_iter();
        assert!(host_arg(args).is_err());
    }

    #[test]
    fn host_rejects_a_flag_as_the_value() {
        let args = vec!["--host".to_string(), "--version".to_string()].into_iter();
        assert!(host_arg(args).is_err());
    }

    #[test]
    fn port_reachable_via_loopback_accepts_localhost() {
        let addr: SocketAddr = "127.0.0.1:1234".parse().unwrap();
        assert_eq!(port_reachable_via_loopback(addr), Ok(1234));
    }

    #[test]
    fn port_reachable_via_loopback_accepts_unspecified() {
        let addr: SocketAddr = "0.0.0.0:1234".parse().unwrap();
        assert_eq!(port_reachable_via_loopback(addr), Ok(1234));
    }

    #[test]
    fn port_reachable_via_loopback_rejects_other_addresses() {
        let addr: SocketAddr = "192.168.1.5:1234".parse().unwrap();
        assert!(port_reachable_via_loopback(addr).is_err());
    }
}
