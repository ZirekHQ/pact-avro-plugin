use clap::Parser;
use pact_avro_plugin::pact_plugin::pact_plugin_server::PactPluginServer;
use pact_avro_plugin::service::PactAvroPluginService;
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Notify;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::codec::CompressionEncoding;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 600;

/// Bounds how long `main` waits for in-flight requests to finish once any
/// shutdown trigger (idle timeout, Ctrl-C, SIGTERM) fires. Without this,
/// `serve_with_incoming_shutdown` can wait indefinitely for a connection
/// that never closes.
const SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_secs(5);

fn version_requested() -> bool {
    std::env::args().nth(1).as_deref() == Some("--version")
}

/// Command-line arguments accepted by the plugin binary. `--version` is
/// handled separately by [`version_requested`] before this ever parses, so
/// clap's own version flag stays off to avoid a conflicting output format.
#[derive(Parser)]
#[command(disable_version_flag = true)]
struct Cli {
    /// Seconds of gRPC inactivity before the plugin shuts itself down.
    /// `0` disables the idle watchdog.
    #[arg(long, default_value_t = DEFAULT_IDLE_TIMEOUT_SECS)]
    timeout: u64,

    /// Address to bind the gRPC listener to; must resolve to loopback or
    /// unspecified, since the Pact driver always dials 127.0.0.1.
    #[arg(long, default_value = DEFAULT_HOST)]
    host: String,
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

    let cli = Cli::parse();
    let (host, idle_timeout) = (cli.host, cli.timeout);

    let (listener, port) = bind_ephemeral_port(&host).await?;

    let server_key = Uuid::new_v4();
    print_pact_handshake(port, server_key)?;

    let start = Instant::now();
    let last_access_millis = Arc::new(AtomicU64::new(0));
    let authenticate_and_touch = {
        let last_access_millis = last_access_millis.clone();
        let authenticate = authenticate(server_key);
        move |request: tonic::Request<()>| {
            let request = authenticate(request)?;
            record_access(&last_access_millis, start.elapsed());
            Ok(request)
        }
    };
    let service = tonic::service::interceptor::InterceptedService::new(
        PactPluginServer::new(PactAvroPluginService)
            .accept_compressed(CompressionEncoding::Gzip)
            .send_compressed(CompressionEncoding::Gzip),
        authenticate_and_touch,
    );

    let idle_shutdown = idle_watchdog(last_access_millis, start, idle_timeout);

    // Fires once a shutdown trigger is selected, so the grace period below
    // starts counting from there rather than from process start.
    let shutdown_triggered = Arc::new(Notify::new());
    let shutdown = {
        let shutdown_triggered = shutdown_triggered.clone();
        async move {
            shutdown_signal(idle_shutdown).await;
            shutdown_triggered.notify_one();
        }
    };

    let serve = tonic::transport::Server::builder()
        .layer(TraceLayer::new_for_grpc())
        .add_service(service)
        .serve_with_incoming_shutdown(TcpListenerStream::new(listener), shutdown);
    tokio::pin!(serve);

    tokio::select! {
        result = &mut serve => result?,
        _ = shutdown_triggered.notified() => {
            if tokio::time::timeout(SHUTDOWN_GRACE_PERIOD, &mut serve).await.is_err() {
                tracing::warn!(
                    "shutdown grace period ({}s) elapsed with a request still in flight; forcing exit",
                    SHUTDOWN_GRACE_PERIOD.as_secs()
                );
            }
        }
    }

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

/// Builds the gRPC interceptor that rejects any call whose `authorization`
/// metadata doesn't match the `server_key` printed at handshake, mirroring
/// the reference protobuf plugin's `AuthInterceptor`.
fn authenticate(
    server_key: Uuid,
) -> impl Fn(tonic::Request<()>) -> Result<tonic::Request<()>, tonic::Status> + Clone {
    // A UUID's hyphenated string form is always valid ASCII metadata.
    let expected = tonic::metadata::MetadataValue::try_from(server_key.to_string())
        .expect("UUID string is always a valid metadata value");
    move |request: tonic::Request<()>| match request.metadata().get("authorization") {
        Some(value) if *value == expected => Ok(request),
        _ => Err(tonic::Status::unauthenticated(
            "missing or invalid authorization header",
        )),
    }
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
        _ = idle => tracing::info!("idle timeout elapsed, shutting down"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_timeout_defaults_when_absent() {
        assert_eq!(
            Cli::try_parse_from(["pact-avro-plugin"]).unwrap().timeout,
            DEFAULT_IDLE_TIMEOUT_SECS
        );
    }

    #[test]
    fn idle_timeout_parses_the_flag_value() {
        let cli = Cli::try_parse_from(["pact-avro-plugin", "--timeout", "42"]).unwrap();
        assert_eq!(cli.timeout, 42);
    }

    #[test]
    fn idle_timeout_zero_is_accepted_as_disabled() {
        let cli = Cli::try_parse_from(["pact-avro-plugin", "--timeout", "0"]).unwrap();
        assert_eq!(cli.timeout, 0);
    }

    #[test]
    fn idle_timeout_rejects_a_missing_value() {
        assert!(Cli::try_parse_from(["pact-avro-plugin", "--timeout"]).is_err());
    }

    #[test]
    fn idle_timeout_rejects_a_non_numeric_value() {
        assert!(Cli::try_parse_from(["pact-avro-plugin", "--timeout", "soon"]).is_err());
    }

    #[test]
    fn host_defaults_when_absent() {
        assert_eq!(
            Cli::try_parse_from(["pact-avro-plugin"]).unwrap().host,
            DEFAULT_HOST
        );
    }

    #[test]
    fn host_parses_the_flag_value() {
        let cli = Cli::try_parse_from(["pact-avro-plugin", "--host", "0.0.0.0"]).unwrap();
        assert_eq!(cli.host, "0.0.0.0");
    }

    #[test]
    fn host_rejects_a_missing_value() {
        assert!(Cli::try_parse_from(["pact-avro-plugin", "--host"]).is_err());
    }

    #[test]
    fn host_rejects_a_flag_as_the_value() {
        assert!(Cli::try_parse_from(["pact-avro-plugin", "--host", "--version"]).is_err());
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

    fn request_with_authorization(value: &str) -> tonic::Request<()> {
        let mut request = tonic::Request::new(());
        request.metadata_mut().insert(
            "authorization",
            tonic::metadata::MetadataValue::try_from(value).unwrap(),
        );
        request
    }

    #[test]
    fn accepts_matching_authorization_header() {
        let server_key = Uuid::new_v4();
        let intercept = authenticate(server_key);

        assert!(intercept(request_with_authorization(&server_key.to_string())).is_ok());
    }

    #[test]
    fn rejects_missing_authorization_header() {
        let intercept = authenticate(Uuid::new_v4());

        assert_eq!(
            intercept(tonic::Request::new(())).unwrap_err().code(),
            tonic::Code::Unauthenticated
        );
    }

    #[test]
    fn rejects_mismatched_authorization_header() {
        let intercept = authenticate(Uuid::new_v4());

        assert_eq!(
            intercept(request_with_authorization(&Uuid::new_v4().to_string()))
                .unwrap_err()
                .code(),
            tonic::Code::Unauthenticated
        );
    }
}
