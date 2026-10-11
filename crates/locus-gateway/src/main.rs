use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Parser;
use locus_gateway::{Config, router};
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "locus-gateway", version, about = "Host-side gateway for Locus")]
struct Args {
    /// Address to listen on.
    #[arg(long, default_value = "0.0.0.0:8090")]
    listen: SocketAddr,
    /// Base URL of the ship's Eyre.
    #[arg(long, default_value = "http://127.0.0.1:80")]
    eyre: String,
    /// Root of blob, temp and thumbnail storage.
    #[arg(long, default_value = "/var/lib/locus/files")]
    data_dir: PathBuf,
    /// Directory holding the compiled browser app.
    #[arg(long, default_value = "/var/lib/locus/web")]
    web_root: PathBuf,
    /// Maximum upload size in MiB.
    #[arg(long, default_value_t = 2048)]
    max_upload_mb: u64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();
    let max_upload_bytes = args
        .max_upload_mb
        .checked_mul(1024 * 1024)
        .ok_or("--max-upload-mb is too large")?;
    let config = Config {
        eyre: args.eyre,
        data_dir: args.data_dir,
        web_root: args.web_root,
        max_upload_bytes,
    };
    tracing::info!(data_dir = %config.data_dir.display(), web_root = %config.web_root.display(), eyre = %config.eyre, "starting");

    let app = router(config)?;
    let listener = tokio::net::TcpListener::bind(args.listen).await?;
    tracing::info!(listen = %args.listen, "listening");
    axum::serve(listener, app).await?;
    Ok(())
}
