//! `donto` — command-line interface to a bitemporal paraconsistent quad store.
//!
//! The binary is a thin wrapper over `donto-client`, `donto-ingest`, and
//! `donto-query`. Every subcommand is self-documenting: run `donto <cmd>
//! --help`. For a renderable man page, see `donto man`.

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand};
use donto_client::{ContextScope, DontoClient, Polarity};
use std::path::PathBuf;
use uuid::Uuid;

/// donto command-line interface.
///
/// donto is a bitemporal, paraconsistent quad store with contexts. Every
/// fact is (subject, predicate, object, context) carrying polarity +
/// maturity + valid_time + tx_time. Retraction closes tx_time; nothing is
/// ever physically deleted.
///
/// Common flow:
///
///     donto migrate                          # apply embedded SQL migrations
///     donto ingest file.nq --format n-quads  # load a data source
///     donto match --subject ex:alice         # pattern-match
///     donto query 'MATCH ?s ?p ?o LIMIT 5'   # DontoQL / SPARQL subset
///     donto retract <uuid>                   # close a statement's tx_time
///
/// Connection config:
///   --dsn         overrides DONTO_DSN / default.
///   env DONTO_DSN default: postgres://donto:donto@127.0.0.1:55432/donto
///
/// See also: `donto man` (roff man page), `donto completions <shell>`.
#[derive(Parser, Debug)]
#[command(
    name = "donto",
    version,
    about = "donto — bitemporal paraconsistent quad store CLI",
    long_about = None,
    arg_required_else_help = true,
)]
struct Cli {
    /// Postgres DSN. Overrides $DONTO_DSN.
    #[arg(
        long,
        global = true,
        env = "DONTO_DSN",
        default_value = "postgres://donto:donto@127.0.0.1:55432/donto",
        value_name = "URI"
    )]
    dsn: String,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Apply embedded SQL migrations to the target Postgres. Idempotent;
    /// safe to re-run. Required once before any other command succeeds.
    Migrate,

    /// Ingest a file into donto. Auto-batches and reports inserted count.
    ///
    /// Supported formats (--format):
    ///   n-quads        RDF N-Quads (default). Named graph → context.
    ///   turtle         Turtle. Uses --default-context for every statement.
    ///   trig           TriG. Named graphs → contexts.
    ///   rdf-xml        RDF/XML.
    ///   json-ld        JSON-LD subset (top-level @context prefix map,
    ///                  @graph, @id, @type, scalar property values).
    ///   jsonl          one JSON statement per line (LLM extractor
    ///                  friendly). Schema:
    ///                  {"s":"...","p":"...","o":{"iri":"..."}|{"v":...,"dt":"...","lang":null},
    ///                   "c":"ctx:...","pol":"asserted","maturity":0,
    ///                   "valid_lo":"YYYY-MM-DD","valid_hi":"YYYY-MM-DD"}
    ///   property-graph Neo4j/AGE export: nodes[] + edges[]. Edges are
    ///                  reified as event-nodes with IRI ex:edge/<id>.
    ///   csv            Requires a mapping file (not yet exposed via this
    ///                  subcommand — use donto-ingest directly).
    #[command(verbatim_doc_comment)]
    Ingest {
        /// Path to the source file.
        #[arg(value_name = "PATH")]
        file: PathBuf,
        /// Input format.
        #[arg(long, value_enum, default_value_t = Format::NQuads, value_name = "FORMAT")]
        format: Format,
        /// Context IRI to assign to statements that carry none in the
        /// source (e.g. Turtle, which has no graph). Defaults to
        /// `donto:anonymous`.
        #[arg(long, value_name = "IRI")]
        default_context: Option<String>,
        /// Statements per server-side insert batch.
        #[arg(long, default_value_t = 1000, value_name = "N")]
        batch: usize,
    },

    /// Pattern-match against the store. Every filter is optional; omit
    /// one to leave it unbound.
    ///
    /// Examples:
    ///   donto match --subject ex:alice
    ///   donto match --predicate ex:knows --polarity any
    ///   donto match --context ex:trusted --min-maturity 3
    ///
    /// Output is newline-delimited JSON — one statement per line —
    /// stable enough to pipe through jq.
    #[command(verbatim_doc_comment)]
    Match {
        /// Subject IRI.
        #[arg(long, value_name = "IRI")]
        subject: Option<String>,
        /// Predicate IRI.
        #[arg(long, value_name = "IRI")]
        predicate: Option<String>,
        /// Object IRI. Literal-object matching is not exposed on the CLI.
        #[arg(long, value_name = "IRI")]
        object_iri: Option<String>,
        /// Anchor context for a single-context scope (with descendants).
        /// Omit for no context filter (equivalent to `anywhere`).
        #[arg(long, value_name = "IRI")]
        context: Option<String>,
        /// Polarity filter. `any` disables the filter.
        #[arg(
            long,
            default_value = "asserted",
            value_name = "asserted|negated|absent|unknown|any"
        )]
        polarity: String,
        /// Maturity floor (0..=4). Rows below this are hidden.
        #[arg(long, default_value_t = 0, value_name = "N")]
        min_maturity: u8,
    },

    /// Run a DontoQL or SPARQL (subset) query. The dispatcher looks at
    /// the first non-whitespace keyword:
    ///
    ///   starts with SELECT / PREFIX → SPARQL subset
    ///   anything else              → DontoQL
    ///
    /// Output is newline-delimited JSON (one row per line).
    ///
    /// Examples:
    ///   donto query 'MATCH ?s ?p ?o LIMIT 10'
    ///   donto query 'PREFIX ex: <http://ex/> SELECT ?x WHERE {?x ex:p ?o .}'
    #[command(verbatim_doc_comment)]
    Query {
        /// Query text. Quote the entire query on the shell.
        #[arg(value_name = "QUERY")]
        query: String,
        /// Named scope preset (see PRD §7; presets are registered in
        /// `donto_preset_scope`).
        #[arg(long, value_name = "NAME")]
        preset: Option<String>,
    },

    /// Close an open statement's tx_time. The physical row remains —
    /// an as-of query before the retraction still returns it. Retracting
    /// an already-closed row is a no-op and prints "no open statement".
    Retract {
        /// UUID of the statement to retract (from `donto match` output).
        #[arg(value_name = "UUID")]
        id: Uuid,
    },

    /// Run builtin performance benchmarks (PRD §25 H1-H10 smoke subset).
    /// Writes N synthetic rows under a throwaway context, then times a
    /// point query and a batch query. JSON report on stdout.
    Bench {
        /// Number of rows to insert.
        #[arg(long, default_value_t = 10_000, value_name = "N")]
        insert_count: u64,
    },

    /// Emit the roff-formatted man page on stdout. Pipe to `man -l -` or
    /// redirect to a file under `~/.local/share/man/man1/donto.1`.
    Man,

    /// Emit shell completions on stdout. Supported shells: bash, zsh,
    /// fish, powershell, elvish. Example:
    ///   donto completions bash > /etc/bash_completion.d/donto
    #[command(verbatim_doc_comment)]
    Completions {
        /// Target shell.
        #[arg(value_enum, value_name = "SHELL")]
        shell: clap_complete::Shell,
    },
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum Format {
    /// RDF N-Quads (default).
    NQuads,
    /// Turtle (no graph block).
    Turtle,
    /// TriG (Turtle with named graphs).
    Trig,
    /// RDF/XML.
    RdfXml,
    /// JSON-LD subset.
    JsonLd,
    /// One JSON statement per line.
    Jsonl,
    /// Neo4j / Apache-AGE property-graph export.
    PropertyGraph,
    /// CSV (requires a mapping, not yet exposed here).
    Csv,
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

    // Man / completions don't need a database connection.
    if let Cmd::Man = cli.cmd {
        let mut cmd = Cli::command();
        let man = clap_mangen::Man::new(cmd.clone())
            .title("DONTO")
            .section("1");
        let mut buf = Vec::new();
        man.render(&mut buf)?;
        // Include subcommand sections too so every subcommand has its own
        // usage block in the rendered page.
        for sub in cmd
            .get_subcommands_mut()
            .map(|s| s.clone())
            .collect::<Vec<_>>()
        {
            let sub_name = format!("donto-{}", sub.get_name());
            let m = clap_mangen::Man::new(sub.clone())
                .title(sub_name.to_ascii_uppercase())
                .section("1");
            buf.extend(b"\n");
            m.render(&mut buf)?;
        }
        std::io::Write::write_all(&mut std::io::stdout(), &buf)?;
        return Ok(());
    }
    if let Cmd::Completions { shell } = cli.cmd {
        let mut cmd = Cli::command();
        let bin = cmd.get_name().to_string();
        clap_complete::generate(shell, &mut cmd, bin, &mut std::io::stdout());
        return Ok(());
    }

    let client =
        DontoClient::from_dsn(&cli.dsn).with_context(|| format!("connecting to {}", cli.dsn))?;

    match cli.cmd {
        Cmd::Man | Cmd::Completions { .. } => unreachable!("handled above"),
        Cmd::Migrate => {
            client.migrate().await?;
            println!("migrations applied");
        }
        Cmd::Ingest {
            file,
            format,
            default_context,
            batch,
        } => {
            let ctx = default_context.as_deref().unwrap_or("donto:anonymous");
            use donto_ingest::*;
            let stmts = match format {
                Format::NQuads => nquads::parse_path(&file, ctx)?,
                Format::Turtle => turtle::parse_turtle_path(&file, ctx)?,
                Format::Trig => turtle::parse_trig_path(&file, ctx)?,
                Format::RdfXml => rdfxml::parse_path(&file, ctx)?,
                Format::JsonLd => jsonld::parse_path(&file, ctx)?,
                Format::Jsonl => jsonl::parse_path(&file, ctx)?,
                Format::PropertyGraph => property_graph::parse_path(&file, ctx, "ex:")?,
                Format::Csv => return Err(anyhow::anyhow!("csv requires --mapping (future work)")),
            };
            let report = Pipeline::new(&client, ctx)
                .batch_size(batch)
                .run(&file.display().to_string(), &format!("{format:?}"), stmts)
                .await?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Cmd::Match {
            subject,
            predicate,
            object_iri,
            context,
            polarity,
            min_maturity,
        } => {
            let scope = context.as_deref().map(ContextScope::just);
            let pol = if polarity == "any" {
                None
            } else {
                Some(
                    Polarity::parse(&polarity)
                        .ok_or_else(|| anyhow::anyhow!("bad polarity {polarity}"))?,
                )
            };
            let stmts = client
                .match_pattern(
                    subject.as_deref(),
                    predicate.as_deref(),
                    object_iri.as_deref(),
                    scope.as_ref(),
                    pol,
                    min_maturity,
                    None,
                    None,
                )
                .await?;
            for s in stmts {
                println!(
                    "{}",
                    serde_json::json!({
                        "id": s.statement_id, "subject": s.subject, "predicate": s.predicate,
                        "object": s.object, "context": s.context, "polarity": s.polarity.as_str(),
                        "maturity": s.maturity,
                        "valid_lo": s.valid_lo, "valid_hi": s.valid_hi,
                        "tx_lo": s.tx_lo, "tx_hi": s.tx_hi,
                    })
                );
            }
        }
        Cmd::Query { query, preset } => {
            let mut q = if query
                .trim_start()
                .to_ascii_uppercase()
                .starts_with("SELECT")
                || query
                    .trim_start()
                    .to_ascii_uppercase()
                    .starts_with("PREFIX")
            {
                donto_query::parse_sparql(&query).map_err(|e| anyhow::anyhow!("{e}"))?
            } else {
                donto_query::parse_dontoql(&query).map_err(|e| anyhow::anyhow!("{e}"))?
            };
            if let Some(p) = preset {
                q.scope_preset = Some(p);
            }
            let rows = donto_query::evaluate(&client, &q).await?;
            for row in rows {
                println!("{}", serde_json::to_string(&row)?);
            }
        }
        Cmd::Retract { id } => {
            println!(
                "{}",
                if client.retract(id).await? {
                    "retracted"
                } else {
                    "no open statement"
                }
            );
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
        client
            .ensure_context(&ctx, "custom", "permissive", None)
            .await?;

        let start = Instant::now();
        let mut batch = Vec::with_capacity(2000);
        for i in 0..n {
            batch.push(
                StatementInput::new(
                    format!("ex:s/{i}"),
                    "ex:p",
                    Object::iri(format!("ex:o/{i}")),
                )
                .with_context(&ctx),
            );
            if batch.len() == 2000 {
                client.assert_batch(&batch).await?;
                batch.clear();
            }
        }
        if !batch.is_empty() {
            client.assert_batch(&batch).await?;
        }
        let insert_elapsed = start.elapsed();

        // Point query.
        let t = Instant::now();
        let rows = client
            .match_pattern(
                Some("ex:s/42"),
                Some("ex:p"),
                None,
                Some(&ContextScope::just(&ctx)),
                Some(Polarity::Asserted),
                0,
                None,
                None,
            )
            .await?;
        let point_us = t.elapsed().as_micros() as u64;
        assert!(!rows.is_empty());

        // Batch query.
        let t = Instant::now();
        let all = client
            .match_pattern(
                None,
                Some("ex:p"),
                None,
                Some(&ContextScope::just(&ctx)),
                Some(Polarity::Asserted),
                0,
                None,
                None,
            )
            .await?;
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
