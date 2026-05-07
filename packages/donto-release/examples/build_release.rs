//! One-shot release builder: read a JSON `ReleaseSpec` from
//! `argv[1]`, write the resulting `ReleaseManifest` to `argv[2]`.
//!
//! Used by integration scripts that already have the spec
//! materialised as JSON. Production code should use the
//! [`donto_release::build_release`] function directly.

use std::env;
use std::fs;
use std::path::PathBuf;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: build_release <spec.json> <out.manifest.json>");
        std::process::exit(2);
    }
    let spec_path = PathBuf::from(&args[1]);
    let out_path = PathBuf::from(&args[2]);

    let spec_bytes = fs::read(&spec_path)?;
    let spec: donto_release::ReleaseSpec = serde_json::from_slice(&spec_bytes)?;

    let dsn = env::var("DONTO_TEST_DSN")
        .unwrap_or_else(|_| "postgres://donto:donto@127.0.0.1:55432/donto".to_string());
    let client = donto_client::DontoClient::from_dsn(&dsn)?;
    client.migrate().await?;

    let manifest = donto_release::build_release(&client, &spec).await?;
    let json = serde_json::to_vec_pretty(&manifest)?;
    fs::write(&out_path, json)?;
    Ok(())
}
