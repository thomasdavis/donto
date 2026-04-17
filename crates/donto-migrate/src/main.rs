//! donto-migrate — migrators from external stores into donto.
//!
//! Phase 9 ships:
//!   * `genealogy` — the genealogy research SQLite schema (PRD §24).

use anyhow::Result;
use clap::{Parser, Subcommand};
use donto_client::DontoClient;
use std::path::PathBuf;

mod genealogy;

#[derive(Parser, Debug)]
#[command(version, about = "donto migrators")]
struct Cli {
    #[arg(long, env = "DONTO_DSN", default_value = "postgres://donto:donto@127.0.0.1:55432/donto")]
    dsn: String,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Migrate a genealogy research.db (SQLite) into donto.
    Genealogy {
        sqlite: PathBuf,
        /// Root context IRI for the migration.
        #[arg(long, default_value = "ctx:genealogy/research-db")]
        root: String,
        /// Don't actually write; print the plan.
        #[arg(long)]
        dry_run: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().init();
    let cli = Cli::parse();
    let client = DontoClient::from_dsn(&cli.dsn)?;
    client.migrate().await?;

    match cli.cmd {
        Cmd::Genealogy { sqlite, root, dry_run } => {
            let report = genealogy::migrate(&client, &sqlite, &root, dry_run).await?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
    }
    Ok(())
}
