use pact_avro_plugin::pact_plugin::pact_plugin_server::PactPluginServer;
use pact_avro_plugin::port_finder::find_free_port;
use pact_avro_plugin::service::PactAvroPluginService;
use std::io::Write;
use std::net::SocketAddr;
use tokio_stream::wrappers::TcpListenerStream;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let port = find_free_port().unwrap_or(9090);
    let addr: SocketAddr = ([127, 0, 0, 1], port).into();

    // Bind before announcing the port: printing the handshake for a port we
    // haven't confirmed we can actually listen on would tell Pact core to
    // connect to a dead server.
    let listener = tokio::net::TcpListener::bind(addr).await?;

    let server_key = Uuid::new_v4();
    // Pact core reads this exact line from stdout to discover how to reach
    // the plugin. Must stay valid, single-line JSON with these two keys.
    let handshake = serde_json::json!({ "port": port, "serverKey": server_key.to_string() });
    println!("{handshake}");
    std::io::stdout().flush()?;

    tonic::transport::Server::builder()
        .add_service(PactPluginServer::new(PactAvroPluginService))
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
