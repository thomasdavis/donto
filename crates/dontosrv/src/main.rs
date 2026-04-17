use anyhow::Result;
use clap::Parser;
use donto_client::DontoClient;
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(version, about = "donto sidecar (dontosrv)")]
struct Args {
    #[arg(long, env = "DONTO_DSN", default_value = "postgres://donto:donto@127.0.0.1:55432/donto")]
    dsn: String,
    #[arg(long, default_value = "127.0.0.1:7878")]
    bind: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")))
        .init();
    let args = Args::parse();
    let client = DontoClient::from_dsn(&args.dsn)?;
    client.migrate().await?;
    let state = Arc::new(dontosrv::AppState { client });
    let app = dontosrv::router(state);

    let addr: SocketAddr = args.bind.parse()?;
    tracing::info!(%addr, "dontosrv listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
