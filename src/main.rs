use pact_avro_plugin::pact_plugin::pact_plugin_server::PactPluginServer;
use pact_avro_plugin::service::PactAvroPluginService;
use std::io::Write;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::codec::CompressionEncoding;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

const DEFAULT_HOST: &str = "127.0.0.1";

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

    // Bind before announcing the port, and keep this listener for serving:
    // reserving the OS-assigned port here closes the window in which another
    // process could claim it between lookup and bind.
    let listener = tokio::net::TcpListener::bind((host.as_str(), 0)).await?;
    let port = listener.local_addr()?.port();

    let server_key = Uuid::new_v4();
    // Pact core reads this exact line from stdout to discover how to reach
    // the plugin. Must stay valid, single-line JSON with these two keys.
    let handshake = serde_json::json!({ "port": port, "serverKey": server_key.to_string() });
    println!("{handshake}");
    std::io::stdout().flush()?;

    let service = PactPluginServer::new(PactAvroPluginService)
        .accept_compressed(CompressionEncoding::Gzip)
        .send_compressed(CompressionEncoding::Gzip);

    tonic::transport::Server::builder()
        .layer(TraceLayer::new_for_grpc())
        .add_service(service)
        .serve_with_incoming_shutdown(TcpListenerStream::new(listener), shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        // Signal-handler registration failing at startup is unrecoverable —
        // without it the process can't shut down cleanly.
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl-C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        // Signal-handler registration failing at startup is unrecoverable —
        // without it the process can't shut down cleanly.
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => tracing::info!("received Ctrl-C, shutting down"),
        _ = terminate => tracing::info!("received SIGTERM, shutting down"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
