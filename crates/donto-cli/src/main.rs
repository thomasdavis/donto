//! `donto` CLI — end-user command surface.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use donto_client::{ContextScope, DontoClient, Polarity};
use std::path::PathBuf;
use uuid::Uuid;

mod nquads;

#[derive(Parser, Debug)]
#[command(version, about = "donto command-line interface")]
struct Cli {
    #[arg(long, env = "DONTO_DSN",
          default_value = "postgres://donto:donto@127.0.0.1:55432/donto")]
    dsn: String,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    Migrate,
    Ingest {
        file: PathBuf,
        #[arg(long, value_enum, default_value_t = Format::NQuads)]
        format: Format,
        #[arg(long)]
        default_context: Option<String>,
        #[arg(long, default_value_t = 1000)]
        batch: usize,
    },
    Match {
        #[arg(long)] subject: Option<String>,
        #[arg(long)] predicate: Option<String>,
        #[arg(long)] object_iri: Option<String>,
        #[arg(long)] context: Option<String>,
        #[arg(long, default_value = "asserted")] polarity: String,
        #[arg(long, default_value_t = 0)] min_maturity: u8,
    },
    Query {
        query: String,
        #[arg(long)] preset: Option<String>,
    },
    Retract { id: Uuid },
    /// Run builtin performance benchmarks (PRD §25 H1-H10 smoke subset).
    Bench {
        #[arg(long, default_value_t = 10_000)] insert_count: u64,
    },
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum Format { NQuads, Turtle, Trig, RdfXml, JsonLd, Jsonl, PropertyGraph, Csv }

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")))
        .init();
    let cli = Cli::parse();
    let client = DontoClient::from_dsn(&cli.dsn).with_context(|| format!("connecting to {}", cli.dsn))?;

    match cli.cmd {
        Cmd::Migrate => { client.migrate().await?; println!("migrations applied"); }
        Cmd::Ingest { file, format, default_context, batch } => {
            let ctx = default_context.as_deref().unwrap_or("donto:anonymous");
            use donto_ingest::*;
            let stmts = match format {
                Format::NQuads        => nquads::parse_path(&file, ctx)?,
                Format::Turtle        => turtle::parse_turtle_path(&file, ctx)?,
                Format::Trig          => turtle::parse_trig_path(&file, ctx)?,
                Format::RdfXml        => rdfxml::parse_path(&file, ctx)?,
                Format::JsonLd        => jsonld::parse_path(&file, ctx)?,
                Format::Jsonl         => jsonl::parse_path(&file, ctx)?,
                Format::PropertyGraph => property_graph::parse_path(&file, ctx, "ex:")?,
                Format::Csv           => return Err(anyhow::anyhow!("csv requires --mapping (future work)")),
            };
            let report = Pipeline::new(&client, ctx).batch_size(batch)
                .run(&file.display().to_string(), &format!("{format:?}"), stmts).await?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Cmd::Match { subject, predicate, object_iri, context, polarity, min_maturity } => {
            let scope = context.as_deref().map(ContextScope::just);
            let pol = if polarity == "any" { None }
                      else { Some(Polarity::parse(&polarity).ok_or_else(|| anyhow::anyhow!("bad polarity {polarity}"))?) };
            let stmts = client.match_pattern(
                subject.as_deref(), predicate.as_deref(), object_iri.as_deref(),
                scope.as_ref(), pol, min_maturity, None, None,
            ).await?;
            for s in stmts {
                println!("{}", serde_json::json!({
                    "id": s.statement_id, "subject": s.subject, "predicate": s.predicate,
                    "object": s.object, "context": s.context, "polarity": s.polarity.as_str(),
                    "maturity": s.maturity,
                    "valid_lo": s.valid_lo, "valid_hi": s.valid_hi,
                    "tx_lo": s.tx_lo, "tx_hi": s.tx_hi,
                }));
            }
        }
        Cmd::Query { query, preset } => {
            let mut q = if query.trim_start().to_ascii_uppercase().starts_with("SELECT")
                           || query.trim_start().to_ascii_uppercase().starts_with("PREFIX") {
                donto_query::parse_sparql(&query).map_err(|e| anyhow::anyhow!("{e}"))?
            } else {
                donto_query::parse_dontoql(&query).map_err(|e| anyhow::anyhow!("{e}"))?
            };
            if let Some(p) = preset { q.scope_preset = Some(p); }
            let rows = donto_query::evaluate(&client, &q).await?;
            for row in rows { println!("{}", serde_json::to_string(&row)?); }
        }
        Cmd::Retract { id } => {
            println!("{}", if client.retract(id).await? { "retracted" } else { "no open statement" });
        }
        Cmd::Bench { insert_count } => {
            let report = bench::run(&client, insert_count).await?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
    }

    Ok(())
}

mod bench {
    use super::*;
    use donto_client::{Object, StatementInput};
    use serde::Serialize;
    use std::time::Instant;

    #[derive(Debug, Serialize)]
    pub struct BenchReport {
        pub inserts: u64,
        pub insert_elapsed_ms: u64,
        pub inserts_per_sec: f64,
        pub point_query_elapsed_us: u64,
        pub batch_query_rows: usize,
        pub batch_query_elapsed_ms: u64,
    }

    pub async fn run(client: &DontoClient, n: u64) -> anyhow::Result<BenchReport> {
        let prefix = format!("bench:{}", uuid::Uuid::new_v4().simple());
        let ctx = format!("{prefix}/ctx");
        client.ensure_context(&ctx, "custom", "permissive", None).await?;

        let start = Instant::now();
        let mut batch = Vec::with_capacity(2000);
        for i in 0..n {
            batch.push(StatementInput::new(format!("ex:s/{i}"), "ex:p",
                Object::iri(format!("ex:o/{i}"))).with_context(&ctx));
            if batch.len() == 2000 { client.assert_batch(&batch).await?; batch.clear(); }
        }
        if !batch.is_empty() { client.assert_batch(&batch).await?; }
        let insert_elapsed = start.elapsed();

        // Point query.
        let t = Instant::now();
        let rows = client.match_pattern(
            Some("ex:s/42"), Some("ex:p"), None,
            Some(&ContextScope::just(&ctx)), Some(Polarity::Asserted), 0, None, None,
        ).await?;
        let point_us = t.elapsed().as_micros() as u64;
        assert!(!rows.is_empty());

        // Batch query.
        let t = Instant::now();
        let all = client.match_pattern(
            None, Some("ex:p"), None,
            Some(&ContextScope::just(&ctx)), Some(Polarity::Asserted), 0, None, None,
        ).await?;
        let batch_elapsed = t.elapsed();

        Ok(BenchReport {
            inserts: n,
            insert_elapsed_ms: insert_elapsed.as_millis() as u64,
            inserts_per_sec: (n as f64) / insert_elapsed.as_secs_f64().max(1e-9),
            point_query_elapsed_us: point_us,
            batch_query_rows: all.len(),
            batch_query_elapsed_ms: batch_elapsed.as_millis() as u64,
        })
    }
}
