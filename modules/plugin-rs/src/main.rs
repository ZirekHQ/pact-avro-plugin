use pact_avro_plugin::pact_plugin::pact_plugin_server::PactPluginServer;
use pact_avro_plugin::port_finder::find_free_port;
use pact_avro_plugin::service::PactAvroPluginService;
use std::net::SocketAddr;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter(
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
    ).init();

    let port = find_free_port().unwrap_or(9090);
    let server_key = Uuid::new_v4();
    let addr: SocketAddr = ([0, 0, 0, 0], port).into();

    // Pact core reads this exact line from stdout to discover how to reach
    // the plugin. Must stay valid, single-line JSON with these two keys.
    let handshake = serde_json::json!({ "port": port, "serverKey": server_key.to_string() });
    println!("{handshake}");
    use std::io::Write;
    std::io::stdout().flush()?;

    tonic::transport::Server::builder()
        .add_service(PactPluginServer::new(PactAvroPluginService))
        .serve_with_shutdown(addr, shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl-C handler");
    };

    #[cfg(unix)]
    let terminate = async {
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
