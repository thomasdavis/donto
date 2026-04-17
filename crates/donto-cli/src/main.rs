//! `donto` — Phase 0 CLI.
//!
//! Subcommands:
//!   migrate              apply Phase 0 migrations
//!   ingest <FILE>        ingest N-Quads (graph IRI → context)
//!   match                pattern query
//!   retract <UUID>       close tx_time on a statement

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use donto_client::{ContextScope, DontoClient, Polarity};
use std::path::PathBuf;
use uuid::Uuid;

mod nquads;

#[derive(Parser, Debug)]
#[command(version, about = "donto Phase 0 command-line interface", long_about = None)]
struct Cli {
    /// libpq DSN (overrides $DONTO_DSN).
    #[arg(long, env = "DONTO_DSN", default_value = "postgres://donto:donto@127.0.0.1:55432/donto")]
    dsn: String,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Apply embedded SQL migrations (idempotent).
    Migrate,

    /// Ingest N-Quads from a file.
    Ingest {
        /// N-Quads file path. The graph IRI on each quad becomes the donto
        /// context. Quads in the default graph land in `donto:anonymous`.
        file: PathBuf,
        /// Override context for default-graph triples.
        #[arg(long)]
        default_context: Option<String>,
        /// Batch size for server round-trips.
        #[arg(long, default_value_t = 1000)]
        batch: usize,
    },

    /// Pattern match. All filters optional. Prints JSON lines.
    Match {
        #[arg(long)] subject: Option<String>,
        #[arg(long)] predicate: Option<String>,
        #[arg(long)] object_iri: Option<String>,
        #[arg(long)] context: Option<String>,
        #[arg(long, default_value = "asserted")]
        polarity: String,
        #[arg(long, default_value_t = 0)]
        min_maturity: u8,
    },

    /// Close transaction-time on a statement.
    Retract { id: Uuid },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")))
        .init();

    let cli = Cli::parse();
    let client = DontoClient::from_dsn(&cli.dsn)
        .with_context(|| format!("connecting to {}", cli.dsn))?;

    match cli.cmd {
        Cmd::Migrate => {
            client.migrate().await?;
            println!("migrations applied");
        }
        Cmd::Ingest { file, default_context, batch } => {
            let ctx = default_context.as_deref().unwrap_or("donto:anonymous");
            let count = nquads::ingest_file(&client, &file, ctx, batch).await?;
            println!("ingested {count} statements");
        }
        Cmd::Match { subject, predicate, object_iri, context, polarity, min_maturity } => {
            let scope = context.as_deref().map(ContextScope::just);
            let pol = if polarity == "any" { None } else {
                Some(Polarity::parse(&polarity)
                    .ok_or_else(|| anyhow::anyhow!("bad polarity {polarity}"))?)
            };
            let stmts = client.match_pattern(
                subject.as_deref(),
                predicate.as_deref(),
                object_iri.as_deref(),
                scope.as_ref(),
                pol,
                min_maturity,
                None,
                None,
            ).await?;
            for s in stmts {
                println!("{}", serde_json::json!({
                    "id":        s.statement_id,
                    "subject":   s.subject,
                    "predicate": s.predicate,
                    "object":    s.object,
                    "context":   s.context,
                    "polarity":  s.polarity.as_str(),
                    "maturity":  s.maturity,
                    "valid_lo":  s.valid_lo,
                    "valid_hi":  s.valid_hi,
                    "tx_lo":     s.tx_lo,
                    "tx_hi":     s.tx_hi,
                }));
            }
        }
        Cmd::Retract { id } => {
            let closed = client.retract(id).await?;
            if closed { println!("retracted {id}"); }
            else      { println!("no open statement {id}"); }
        }
    }

    Ok(())
}
