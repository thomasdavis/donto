//! donto-synthetic — deterministic synthetic data generator for donto.
//!
//! Produces a dataset of synthetic statements used for detector evaluation.
//! Implements the full generator; anomalies.json is the ground-truth file
//! consumed by the ml-engineer's detector tests.
//!
//! Usage:
//!   cargo run -p donto-synthetic -- generate --seed 42 --dsn $DONTO_TEST_DSN
//!   cargo run -p donto-synthetic -- generate --seed 42 --scale 0.002 --dsn $DONTO_TEST_DSN
//!   cargo run -p donto-synthetic -- generate --seed 42 --reset --dsn $DONTO_TEST_DSN
//!
//! Scale factor: 1.0 = 500k statements. 0.002 = ~1k statements for CI.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

// Re-export generator module from the lib crate for the binary.
use donto_synthetic::generator;

/// donto-synthetic: deterministic synthetic dataset generator.
#[derive(Parser, Debug)]
#[command(
    name = "donto-synthetic",
    version,
    about,
    arg_required_else_help = true
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Generate synthetic statements and write anomalies.json ground truth.
    Generate {
        /// DSN for the target Postgres instance.
        #[arg(
            long,
            env = "DONTO_DSN",
            default_value = "postgres://donto:donto@127.0.0.1:55432/donto",
            value_name = "DSN"
        )]
        dsn: String,

        /// Random seed (deterministic per seed value). Same seed → identical rows.
        #[arg(long, value_name = "N")]
        seed: u64,

        /// Scale factor: 1.0 ≈ 500k statements, 0.002 ≈ 1k statements.
        #[arg(long, default_value_t = 1.0, value_name = "F")]
        scale: f64,

        /// Reset all synthetic data matching this run's IRI prefix before
        /// generating. Open donto_statement rows are closed via
        /// donto_retract (bitemporal contract — rows remain in the table
        /// with tx_time closed, actor 'agent:synthetic-reset'). Aux tables
        /// (donto_derivation_report, donto_shape_report,
        /// donto_detector_finding, donto_event_log, donto_context) are
        /// deleted outright. Use for clean reruns without violating the
        /// no-delete rule.
        #[arg(long)]
        reset: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    match cli.cmd {
        Cmd::Generate {
            dsn,
            seed,
            scale,
            reset,
        } => {
            let client =
                donto_client::DontoClient::from_dsn(&dsn).context("connecting to postgres")?;
            client.migrate().await.context("migrate")?;
            let report = generator::run(&client, seed, scale, reset)
                .await
                .context("synthetic generation failed")?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
    }

    Ok(())
}
