//! `donto` — command-line interface to a bitemporal paraconsistent quad store.
//!
//! The binary is a thin wrapper over `donto-client`, `donto-ingest`, and
//! `donto-query`. Every subcommand is self-documenting: run `donto <cmd>
//! --help`. For a renderable man page, see `donto man`.

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand};
use donto_client::{AlignmentRelation, ContextScope, DontoClient, Polarity};
use std::path::PathBuf;
use uuid::Uuid;

mod analyze;

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

    /// Release-signing spike (M9 federation).
    #[command(subcommand)]
    Release(ReleaseCmd),

    /// Import linguistic-format datasets into donto (M6).
    ///
    /// One subcommand per supported format. Each parses the
    /// source, maps to quads, ingests into the given context, and
    /// prints a JSON Report with counts + loss list.
    #[command(subcommand)]
    Ling(LingCmd),

    /// One-shot JSON summary of the database. Use this first when
    /// you (or an agent) need to orient on the live deployment:
    /// what's in it, which features are active, how big each table
    /// is, distribution across maturity / polarity / modality, etc.
    Status,

    /// Read the donto canonical model: registered frame types,
    /// alignment relations, default policies, allowed modality
    /// values, extraction levels, predicate-clause vocabulary, etc.
    /// Use this when you (or an agent) need to know "what are the
    /// valid values here?" without grepping the migrations.
    Schema,

    /// Set / read sparse overlays on a statement.
    #[command(subcommand)]
    Modality(ModalityCmd),

    /// Set / read the extraction-level overlay on a statement.
    #[command(subcommand)]
    ExtractionLevel(ExtractionLevelCmd),

    /// Trust Kernel CRUD: register a policy capsule, assign a
    /// policy to a target, check whether an action is allowed.
    #[command(subcommand)]
    Policy(PolicyCmd),

    /// Extract knowledge from unstructured text using an LLM, then ingest
    /// the resulting facts into donto. Uses OpenRouter (Grok 4.1 Fast by
    /// default) to extract 8-tier predicates from articles, transcripts,
    /// essays, or any text.
    ///
    /// Model shortcuts: grok (default), sonnet (premium), mistral (fallback),
    /// or any OpenRouter model ID.
    ///
    /// Examples:
    ///   donto extract article.md
    ///   donto extract article.md --context ctx:research/cooktown
    ///   donto extract article.md --model sonnet
    ///   donto extract article.md --dry-run   # preview without ingesting
    ///
    /// Requires $OPENROUTER_API_KEY.
    #[command(verbatim_doc_comment)]
    Extract {
        /// Path to the source text file.
        #[arg(value_name = "PATH")]
        file: PathBuf,
        /// Context IRI for ingested statements. Defaults to
        /// ctx:extract/<filename>/<model>.
        #[arg(long, value_name = "IRI")]
        context: Option<String>,
        /// Model to use. Shortcuts: grok (default), sonnet, mistral.
        /// Or pass a full OpenRouter model ID.
        #[arg(long, default_value = "grok", value_name = "MODEL")]
        model: String,
        /// Statements per insert batch.
        #[arg(long, default_value_t = 1000, value_name = "N")]
        batch: usize,
        /// Print extracted facts as JSON without ingesting.
        #[arg(long)]
        dry_run: bool,
        /// Pre-flight policy check: refuse to call the external
        /// model if the source's policy denies derive_claims. The
        /// source is identified by `--source <iri>` (or by the
        /// filename if --source is omitted). With no registered
        /// policy, the check passes (no policy, no refusal).
        ///
        /// PRD M5 acceptance: "Policy blocks external calls for
        /// restricted sources."
        #[arg(long)]
        policy_check: bool,
        /// Source IRI for the policy-check. If omitted, the file
        /// path (canonicalised) is used as the source identifier.
        #[arg(long, value_name = "IRI")]
        source: Option<String>,
    },

    /// Manage predicate alignments: register, suggest, list, retract, and
    /// rebuild the predicate closure index.
    ///
    /// Predicate alignment maps between equivalent, inverse, sub-property,
    /// close-match, decomposition, and not-equivalent predicate pairs.
    /// The closure index powers alignment-aware queries.
    ///
    /// Examples:
    ///   donto align register ex:knows ex:acquaintedWith --relation exact_equivalent --confidence 0.95
    ///   donto align suggest ex:knows --threshold 0.5 --limit 10
    ///   donto align list ex:knows
    ///   donto align retract <uuid>
    ///   donto align rebuild
    #[command(verbatim_doc_comment)]
    Align {
        #[command(subcommand)]
        action: AlignCmd,
    },

    /// List predicates with statement counts.
    ///
    /// Queries the predicate registry and joins against current statements
    /// to show how many open statements use each predicate. Output is
    /// newline-delimited JSON, one predicate per line.
    ///
    /// Examples:
    ///   donto predicates
    ///   donto predicates --limit 20
    #[command(verbatim_doc_comment)]
    Predicates {
        /// Maximum number of predicates to return.
        #[arg(long, default_value_t = 100, value_name = "N")]
        limit: i32,
    },

    /// Query the canonical shadow view (alignment-aware match).
    ///
    /// Like `donto match` but expands predicates through the alignment
    /// closure, returning rows that match via equivalent, sub-property,
    /// or other aligned predicates. Each row carries `matched_via` and
    /// `alignment_confidence` fields.
    ///
    /// Examples:
    ///   donto shadow --predicate ex:knows
    ///   donto shadow --subject ex:alice --min-confidence 0.8
    ///   donto shadow --predicate ex:knows --no-expand
    #[command(verbatim_doc_comment)]
    Shadow {
        /// Subject IRI.
        #[arg(long, value_name = "IRI")]
        subject: Option<String>,
        /// Predicate IRI.
        #[arg(long, value_name = "IRI")]
        predicate: Option<String>,
        /// Object IRI.
        #[arg(long, value_name = "IRI")]
        object_iri: Option<String>,
        /// Anchor context.
        #[arg(long, value_name = "IRI")]
        context: Option<String>,
        /// Polarity filter. `any` disables the filter.
        #[arg(
            long,
            default_value = "asserted",
            value_name = "asserted|negated|absent|unknown|any"
        )]
        polarity: String,
        /// Maturity floor (0..=4).
        #[arg(long, default_value_t = 0, value_name = "N")]
        min_maturity: u8,
        /// Disable alignment expansion (strict match only).
        #[arg(long)]
        no_expand: bool,
        /// Minimum alignment confidence (0.0..=1.0).
        #[arg(long, default_value_t = 0.0, value_name = "F")]
        min_confidence: f64,
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

    /// Run telemetry analyzers and anomaly detectors.
    ///
    /// Subcommands:
    ///   rule-duration     Detect duration regressions in donto_derivation_report.
    ///   paraconsistency   Aggregate paraconsistency density and upsert results.
    ///
    /// Findings are written to donto_detector_finding. Run `donto migrate`
    /// first to create the required tables.
    #[command(verbatim_doc_comment)]
    Analyze {
        #[command(subcommand)]
        action: AnalyzeCmd,
    },
}

#[derive(Subcommand, Debug)]
enum ModalityCmd {
    /// Set the modality of a statement (overlay row, sparse).
    /// Valid values: descriptive, prescriptive, reconstructed,
    /// inferred, elicited, corpus_observed, typological_summary,
    /// experimental_result, clinical_observation, legal_holding,
    /// archival_metadata, oral_history, community_protocol,
    /// model_output, other.
    Set {
        #[arg(value_name = "STATEMENT_UUID")]
        statement: Uuid,
        #[arg(value_name = "MODALITY")]
        value: String,
        #[arg(long, default_value = "cli", value_name = "ACTOR")]
        set_by: String,
    },
    /// Read the modality (if any) of a statement.
    Get {
        #[arg(value_name = "STATEMENT_UUID")]
        statement: Uuid,
    },
    /// Distribution of modalities across the store.
    Stats,
}

#[derive(Subcommand, Debug)]
enum ExtractionLevelCmd {
    /// Set the extraction-level of a statement. Valid values:
    /// quoted, table_read, example_observed,
    /// source_generalization, cross_source_inference,
    /// model_hypothesis, human_hypothesis, manual_entry,
    /// registry_import, adapter_import.
    Set {
        #[arg(value_name = "STATEMENT_UUID")]
        statement: Uuid,
        #[arg(value_name = "LEVEL")]
        value: String,
        #[arg(long, default_value = "cli", value_name = "ACTOR")]
        set_by: String,
    },
    Get {
        #[arg(value_name = "STATEMENT_UUID")]
        statement: Uuid,
    },
    Stats,
}

#[derive(Subcommand, Debug)]
enum PolicyCmd {
    /// Register a new policy capsule. Idempotent on policy_iri.
    Register {
        #[arg(value_name = "POLICY_IRI")]
        policy_iri: String,
        /// One of: public, open_metadata_restricted_content,
        /// community_restricted, embargoed, licensed, private,
        /// regulated, sealed, unknown_restricted.
        #[arg(long, value_name = "KIND")]
        kind: String,
        /// Comma-separated allow-list of actions, e.g.
        /// `read_metadata,read_content,quote`. Anything not listed
        /// is denied. Pass `all` to permit everything.
        #[arg(long, value_name = "ACTIONS", default_value = "")]
        allow: String,
        #[arg(long, value_name = "TEXT")]
        summary: Option<String>,
    },
    /// Assign an existing policy to a target (document, claim,
    /// context, …).
    Assign {
        #[arg(long, value_name = "KIND")]
        target_kind: String,
        #[arg(long, value_name = "ID")]
        target_id: String,
        #[arg(long, value_name = "POLICY_IRI")]
        policy_iri: String,
        #[arg(long, default_value = "cli", value_name = "ACTOR")]
        assigned_by: String,
    },
    /// Check whether an action is allowed against a target.
    /// Prints {allowed: bool, effective_actions: {...}}.
    Check {
        #[arg(long, value_name = "KIND")]
        target_kind: String,
        #[arg(long, value_name = "ID")]
        target_id: String,
        #[arg(long, value_name = "ACTION")]
        action: String,
    },
    /// List registered policies (joined with assignment count).
    List,
}

#[derive(Subcommand, Debug)]
enum LingCmd {
    /// Import a CLDF directory dataset (TSV tables + metadata.json).
    Cldf {
        #[arg(value_name = "DIR")]
        path: PathBuf,
        /// Target context IRI for the ingested statements.
        #[arg(long, value_name = "IRI")]
        context: String,
        /// Abort if the loss report is non-empty.
        #[arg(long)]
        strict: bool,
    },
    /// Import a CoNLL-U file (Universal Dependencies).
    Ud {
        #[arg(value_name = "PATH")]
        path: PathBuf,
        #[arg(long, value_name = "IRI")]
        context: String,
        #[arg(long)]
        strict: bool,
    },
    /// Import a UniMorph paradigm TSV file.
    Unimorph {
        #[arg(value_name = "PATH")]
        path: PathBuf,
        #[arg(long, value_name = "IRI")]
        context: String,
        /// Language code (ISO 639-3 or any) used as the IRI prefix.
        #[arg(long, default_value = "und", value_name = "CODE")]
        language: String,
        #[arg(long)]
        strict: bool,
    },
    /// Import a LIFT XML lexicon (SIL FieldWorks dictionaries).
    Lift {
        #[arg(value_name = "PATH")]
        path: PathBuf,
        #[arg(long, value_name = "IRI")]
        context: String,
        #[arg(long)]
        strict: bool,
    },
    /// Import an EAF (ELAN) time-aligned annotation document.
    Eaf {
        #[arg(value_name = "PATH")]
        path: PathBuf,
        #[arg(long, value_name = "IRI")]
        context: String,
        #[arg(long)]
        strict: bool,
    },
}

#[derive(Subcommand, Debug)]
enum ReleaseCmd {
    /// Generate a fresh Ed25519 keypair, print the did:key and the
    /// secret seed (hex). The seed is the only thing you need to
    /// keep — pass it to `sign --seed-hex` to reconstruct the key.
    Keygen,
    /// Sign a release manifest. Reads JSON from `--manifest`, hashes
    /// it canonically, and emits a `ReleaseEnvelope` to stdout.
    Sign {
        #[arg(long, value_name = "PATH")]
        manifest: PathBuf,
        /// 32-byte Ed25519 seed in hex. Use `keygen` to generate one.
        #[arg(long, value_name = "HEX")]
        seed_hex: String,
    },
    /// Verify a release envelope. With `--manifest`, also confirms
    /// the manifest hashes to the value the envelope was signed
    /// over.
    Verify {
        #[arg(long, value_name = "PATH")]
        envelope: PathBuf,
        #[arg(long, value_name = "PATH")]
        manifest: Option<PathBuf>,
    },
    /// Build a ReleaseManifest from a JSON ReleaseSpec on disk.
    /// Emits the manifest to stdout (or --out).
    Build {
        #[arg(long, value_name = "PATH")]
        spec: PathBuf,
        #[arg(long, value_name = "PATH")]
        out: Option<PathBuf>,
    },
    /// Drive the full release pipeline:
    ///   1. build_release(spec)            → ReleaseManifest
    ///   2. write_native_jsonl             → <out>/manifest.jsonl
    ///   3. envelope::sign (--seed-hex)    → <out>/envelope.json
    ///   4. write_ro_crate_metadata        → <out>/ro-crate-metadata.json
    ///   5. write_cldf_release (optional)  → <out>/<rel>-metadata.json + CSVs
    ///
    /// The result is a citable, signed release directory.
    Pipeline {
        #[arg(long, value_name = "PATH")]
        spec: PathBuf,
        /// Output directory. Created if missing.
        #[arg(long, value_name = "DIR")]
        out_dir: PathBuf,
        /// 32-byte Ed25519 seed in hex. Use `keygen` to generate one.
        #[arg(long, value_name = "HEX")]
        seed_hex: String,
        /// Also write a CLDF release export (only meaningful for
        /// linguistic datasets; lossy_count is reported and the
        /// export is skipped if > --max-cldf-loss).
        #[arg(long)]
        cldf: bool,
        /// Refuse to emit the CLDF export if lossy_count exceeds
        /// this. Default: 0 (strict).
        #[arg(long, default_value_t = 0, value_name = "N")]
        max_cldf_loss: u64,
    },
}

#[derive(Subcommand, Debug)]
enum AlignCmd {
    /// Register an alignment between two predicates.
    ///
    /// Relation types: exact_equivalent, inverse_equivalent, sub_property_of,
    /// close_match, decomposition, not_equivalent.
    #[command(verbatim_doc_comment)]
    Register {
        /// Source predicate IRI.
        #[arg(value_name = "SOURCE")]
        source: String,
        /// Target predicate IRI.
        #[arg(value_name = "TARGET")]
        target: String,
        /// Alignment relation type.
        #[arg(long, value_name = "TYPE")]
        relation: String,
        /// Confidence score (0.0..=1.0).
        #[arg(long, default_value_t = 1.0, value_name = "F")]
        confidence: f64,
    },

    /// Suggest alignments for a predicate using trigram lexical similarity.
    /// Finds predicates with similar names that aren't already aligned.
    ///
    /// Example:
    ///   donto align suggest bornIn           # → bornInPlace (0.82), birthYear (0.45)
    ///   donto align suggest marriedTo --threshold 0.3
    #[command(verbatim_doc_comment)]
    Suggest {
        /// Predicate IRI to find suggestions for.
        #[arg(value_name = "PREDICATE")]
        predicate: String,
        /// Minimum trigram similarity (0.0..=1.0).
        #[arg(long, default_value_t = 0.3, value_name = "F")]
        threshold: f64,
        /// Maximum number of suggestions.
        #[arg(long, default_value_t = 20, value_name = "N")]
        limit: i32,
    },

    /// Auto-align predicates using trigram similarity. Scans all active
    /// predicates (or a specific list), finds the best lexical match above
    /// the threshold, and registers close_match alignments. Rebuilds
    /// the closure afterward.
    ///
    /// Example:
    ///   donto align auto                    # align all predicates
    ///   donto align auto --threshold 0.7    # stricter matching
    #[command(verbatim_doc_comment)]
    Auto {
        /// Minimum similarity for auto-alignment.
        #[arg(long, default_value_t = 0.6, value_name = "F")]
        threshold: f64,
    },

    /// List current alignment edges for a predicate.
    List {
        /// Predicate IRI (matches as source or target).
        #[arg(value_name = "PREDICATE")]
        predicate: String,
    },

    /// Retract (close) an alignment edge by its UUID.
    Retract {
        /// Alignment UUID.
        #[arg(value_name = "UUID")]
        id: Uuid,
    },

    /// Rebuild the materialized predicate closure index.
    Rebuild,
}

#[derive(Subcommand, Debug)]
enum AnalyzeCmd {
    /// Detect duration regressions in rule evaluation reports.
    ///
    /// Reads donto_derivation_report and flags rules whose most-recent
    /// evaluations deviate from their 30-day rolling median by more than
    /// k MAD-scaled standard deviations. Also flags rules where the
    /// NULL rate of duration_ms exceeds 30% in the trailing 24 h (sidecar
    /// health signal). Findings are written to donto_detector_finding.
    ///
    /// Examples:
    ///   donto analyze rule-duration
    ///   donto analyze rule-duration --since '14 days' --k 3.0
    ///   donto analyze rule-duration --detector-iri donto:detector/rule-duration/v2
    #[command(verbatim_doc_comment)]
    RuleDuration {
        /// How far back to look for rule evaluations (e.g. "7 days", "48 hours").
        #[arg(long, default_value = "7 days", value_name = "INTERVAL")]
        since: String,
        /// MAD-z threshold for flagging a run as anomalous.
        #[arg(long, default_value_t = 5.0, value_name = "K")]
        k: f64,
        /// IRI identifying this detector in donto_detector_finding.
        #[arg(
            long,
            default_value = "donto:detector/rule-duration/v1",
            value_name = "IRI"
        )]
        detector_iri: String,
        /// Maximum recent runs to evaluate per rule.
        #[arg(long, default_value_t = 100, value_name = "N")]
        n_runs: usize,
        /// Alert sink spec. Emit findings above 'info' to this channel in
        /// addition to writing them to donto_detector_finding.
        /// Values: 'stdout', 'file:///path/findings.jsonl'.
        /// Also reads $DONTO_ALERT_SINK when flag is omitted.
        #[arg(long, env = "DONTO_ALERT_SINK", value_name = "SPEC")]
        alert_sink: Option<String>,
    },

    /// Aggregate paraconsistency density into donto_paraconsistency_density.
    ///
    /// Groups open statements by (subject, predicate) over the requested
    /// window, counts distinct polarities and contexts, computes normalised
    /// Shannon entropy as conflict_score, and upserts rows. The top-K views
    /// donto_v_top_contested_predicates and donto_v_top_contested_subjects
    /// then query the pre-aggregated table efficiently.
    ///
    /// Pairs whose conflict_score exceeds --min-emit-score also produce a
    /// finding in donto_detector_finding (target_kind='predicate_pair'), and a
    /// _self finding is always written so `donto analyze health` covers this
    /// detector.
    ///
    /// Examples:
    ///   donto analyze paraconsistency
    ///   donto analyze paraconsistency --window-hours 48
    ///   donto analyze paraconsistency --start 2026-01-01T00:00:00Z --end 2026-01-02T00:00:00Z
    ///   donto analyze paraconsistency --min-emit-score 0.7 --alert-sink stdout
    #[command(verbatim_doc_comment)]
    Paraconsistency {
        /// Trailing window size in hours (ending now). Ignored if --start/--end given.
        #[arg(long, default_value_t = 24, value_name = "HOURS")]
        window_hours: u64,
        /// Explicit window start (ISO 8601). Overrides --window-hours.
        #[arg(long, value_name = "ISO")]
        start: Option<String>,
        /// Explicit window end (ISO 8601). Defaults to now.
        #[arg(long, value_name = "ISO")]
        end: Option<String>,
        /// IRI identifying this detector in donto_detector_finding.
        #[arg(
            long,
            default_value = "donto:detector/paraconsistency/v1",
            value_name = "IRI"
        )]
        detector_iri: String,
        /// Conflict score threshold above which a (subject, predicate) pair
        /// also writes a finding to donto_detector_finding. Lower = noisier.
        #[arg(long, default_value_t = 0.6, value_name = "F")]
        min_emit_score: f64,
        /// Alert sink spec (same as rule-duration). Reads $DONTO_ALERT_SINK
        /// when omitted; pass an empty string to opt out.
        #[arg(long, env = "DONTO_ALERT_SINK", value_name = "SPEC")]
        alert_sink: Option<String>,
    },

    /// Bucket reviewer decisions and emit warnings on high reject rates.
    ///
    /// Aggregates donto_review_decision rows by
    /// (review_context, reviewer_id) over the window. Buckets with
    /// reject_rate >= --warn-reject-rate and >=5 decisions become a
    /// warning finding under target_kind='review_context'. Calibrates
    /// extractor confidence against human review per PRD M5.
    ///
    /// Examples:
    ///   donto analyze reviewer-acceptance
    ///   donto analyze reviewer-acceptance --window-hours 168
    ///   donto analyze reviewer-acceptance --warn-reject-rate 0.5 --alert-sink stdout
    #[command(verbatim_doc_comment)]
    ReviewerAcceptance {
        /// Trailing window size in hours (ending now). Ignored if --start/--end given.
        #[arg(long, default_value_t = 24, value_name = "HOURS")]
        window_hours: u64,
        #[arg(long, value_name = "ISO")]
        start: Option<String>,
        #[arg(long, value_name = "ISO")]
        end: Option<String>,
        #[arg(
            long,
            default_value = "donto:detector/reviewer-acceptance/v1",
            value_name = "IRI"
        )]
        detector_iri: String,
        /// Bucket warn threshold: reject_rate ≥ this AND total ≥ 5 → warning.
        #[arg(long, default_value_t = 0.4, value_name = "F")]
        warn_reject_rate: f64,
        #[arg(long, env = "DONTO_ALERT_SINK", value_name = "SPEC")]
        alert_sink: Option<String>,
    },

    /// Check that all known detectors have run recently.
    ///
    /// Reads the most-recent `_self` finding per detector_iri. Exits non-zero
    /// if any detector has not run within --max-age-hours or if its reported
    /// null_rate exceeds --max-null-rate (indicating sidecar health issues).
    ///
    /// Output is newline-delimited JSON, one object per detector.
    ///
    /// Examples:
    ///   donto analyze health
    ///   donto analyze health --max-age-hours 48
    ///   donto analyze health --max-null-rate 0.5
    #[command(verbatim_doc_comment)]
    Health {
        /// Maximum acceptable age of the last detector run (hours).
        #[arg(long, default_value_t = 24, value_name = "H")]
        max_age_hours: i64,
        /// Maximum acceptable null_rate_observed from the _self payload.
        #[arg(long, default_value_t = 0.3, value_name = "F")]
        max_null_rate: f64,
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
        Cmd::Extract {
            file,
            context,
            model,
            batch,
            dry_run,
            policy_check,
            source,
        } => {
            let source_iri = source.unwrap_or_else(|| {
                std::fs::canonicalize(&file)
                    .map(|p| format!("file://{}", p.display()))
                    .unwrap_or_else(|_| format!("file://{}", file.display()))
            });
            if policy_check {
                // Pre-flight: refuse if the source policy denies
                // derive_claims. No policy → no refusal. M5 PRD
                // acceptance: "Policy blocks external calls for
                // restricted sources."
                let conn = client.pool().get().await?;
                let allowed: bool = conn
                    .query_one(
                        "select donto_action_allowed('document', $1, 'derive_claims')",
                        &[&source_iri],
                    )
                    .await?
                    .get(0);
                if !allowed {
                    anyhow::bail!(
                        "policy refused derive_claims for source {source_iri}. \
                         Pass without --policy-check to override, or attach a policy that \
                         permits derive_claims via donto_register_source."
                    );
                }
                eprintln!("policy-check: derive_claims permitted for {source_iri}");
            }
            let api_key = std::env::var("OPENROUTER_API_KEY")
                .context("$OPENROUTER_API_KEY not set. Get one at https://openrouter.ai")?;
            let model = extract::resolve_model(&model);
            let source_stem = file
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".into());
            let ctx = context.unwrap_or_else(|| {
                let model_short = model.split('/').last().unwrap_or(model);
                format!("ctx:extract/{source_stem}/{model_short}")
            });
            let report =
                extract::run(&client, &file, &ctx, model, batch, &api_key, dry_run).await?;
            eprintln!();
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Cmd::Bench { insert_count } => {
            let report = bench::run(&client, insert_count).await?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Cmd::Ling(action) => match action {
            LingCmd::Cldf { path, context, strict } => {
                let importer = donto_ling_cldf::Importer::new(&client, &context);
                let report = importer
                    .import(
                        &path,
                        donto_ling_cldf::ImportOptions {
                            strict,
                            ..donto_ling_cldf::ImportOptions::default()
                        },
                    )
                    .await?;
                println!("{}", serde_json::to_string_pretty(&report)?);
            }
            LingCmd::Ud { path, context, strict } => {
                let importer = donto_ling_ud::Importer::new(&client, &context);
                let report = importer
                    .import(
                        &path,
                        donto_ling_ud::ImportOptions {
                            strict,
                            ..donto_ling_ud::ImportOptions::default()
                        },
                    )
                    .await?;
                println!("{}", serde_json::to_string_pretty(&report)?);
            }
            LingCmd::Unimorph { path, context, language, strict } => {
                let importer = donto_ling_unimorph::Importer::new(&client, &context);
                let report = importer
                    .import(
                        &path,
                        donto_ling_unimorph::ImportOptions {
                            strict,
                            language,
                            ..donto_ling_unimorph::ImportOptions::default()
                        },
                    )
                    .await?;
                println!("{}", serde_json::to_string_pretty(&report)?);
            }
            LingCmd::Lift { path, context, strict } => {
                let importer = donto_ling_lift::Importer::new(&client, &context);
                let report = importer
                    .import(
                        &path,
                        donto_ling_lift::ImportOptions {
                            strict,
                            ..donto_ling_lift::ImportOptions::default()
                        },
                    )
                    .await?;
                println!("{}", serde_json::to_string_pretty(&report)?);
            }
            LingCmd::Eaf { path, context, strict } => {
                let importer = donto_ling_eaf::Importer::new(&client, &context);
                let report = importer
                    .import(
                        &path,
                        donto_ling_eaf::ImportOptions {
                            strict,
                            ..donto_ling_eaf::ImportOptions::default()
                        },
                    )
                    .await?;
                println!("{}", serde_json::to_string_pretty(&report)?);
            }
        },
        Cmd::Status => {
            let conn = client.pool().get().await?;
            // Pull everything in one round-trip-light pass; the
            // queries are individually trivial.
            let q = |sql: &str| -> String { sql.to_string() };
            macro_rules! count {
                ($sql:expr) => {{
                    let r = conn.query_one($sql, &[]).await?;
                    r.get::<_, i64>(0)
                }};
            }
            let stmts = count!("select count(*)::bigint from donto_statement");
            let stmts_open = count!(
                "select count(*)::bigint from donto_statement where upper(tx_time) is null"
            );
            let retracted = stmts - stmts_open;
            let contexts = count!("select count(*)::bigint from donto_context");
            let predicates = count!(
                "select count(distinct predicate)::bigint from donto_statement where upper(tx_time) is null"
            );
            let docs = count!("select count(*)::bigint from donto_document");
            let evidence = count!("select count(*)::bigint from donto_evidence_link");
            let alignment = count!("select count(*)::bigint from donto_predicate_alignment");
            let arguments = count!("select count(*)::bigint from donto_argument");
            let modality = count!("select count(*)::bigint from donto_stmt_modality");
            let extraction = count!("select count(*)::bigint from donto_stmt_extraction_level");
            let policies = count!("select count(*)::bigint from donto_policy_capsule");
            let assignments = count!("select count(*)::bigint from donto_access_assignment");
            let attestations = count!("select count(*)::bigint from donto_attestation");
            let frames = count!("select count(*)::bigint from donto_frame_type");
            let claim_frames = count!("select count(*)::bigint from donto_claim_frame");
            let reviews = count!("select count(*)::bigint from donto_review_decision");
            let findings = count!("select count(*)::bigint from donto_detector_finding");
            let snapshots = count!("select count(*)::bigint from donto_snapshot");

            // Maturity distribution.
            let mat_rows = conn
                .query(
                    "select donto_maturity(flags)::int as m, count(*)::bigint \
                     from donto_statement where upper(tx_time) is null group by 1 order by 1",
                    &[],
                )
                .await?;
            let mut maturity = serde_json::Map::new();
            for r in mat_rows {
                let m: i32 = r.get(0);
                let n: i64 = r.get(1);
                maturity.insert(format!("E{m}"), serde_json::json!(n));
            }
            // Polarity distribution.
            let pol_rows = conn
                .query(
                    "select (flags::int & 3) as p, count(*)::bigint \
                     from donto_statement where upper(tx_time) is null group by 1 order by 1",
                    &[],
                )
                .await?;
            let mut polarity = serde_json::Map::new();
            let p_names = ["asserted", "negated", "absent", "unknown"];
            for r in pol_rows {
                let p: i32 = r.get(0);
                let n: i64 = r.get(1);
                polarity.insert(
                    p_names.get(p as usize).copied().unwrap_or("other").into(),
                    serde_json::json!(n),
                );
            }

            // Top contexts and predicates.
            let top_ctx_rows = conn
                .query(
                    "select context, count(*)::bigint \
                     from donto_statement where upper(tx_time) is null \
                     group by 1 order by 2 desc limit 5",
                    &[],
                )
                .await?;
            let top_ctx: Vec<_> = top_ctx_rows
                .iter()
                .map(|r| serde_json::json!({"context": r.get::<_, String>(0), "n": r.get::<_, i64>(1)}))
                .collect();
            let top_pred_rows = conn
                .query(
                    "select predicate, count(*)::bigint \
                     from donto_statement where upper(tx_time) is null \
                     group by 1 order by 2 desc limit 5",
                    &[],
                )
                .await?;
            let top_pred: Vec<_> = top_pred_rows
                .iter()
                .map(|r| serde_json::json!({"predicate": r.get::<_, String>(0), "n": r.get::<_, i64>(1)}))
                .collect();
            let _ = q;

            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "core": {
                        "statements_total": stmts,
                        "statements_open": stmts_open,
                        "statements_retracted": retracted,
                        "contexts": contexts,
                        "distinct_predicates": predicates,
                        "documents": docs,
                        "evidence_links": evidence,
                    },
                    "maturity": maturity,
                    "polarity": polarity,
                    "alignment": {
                        "edges": alignment,
                    },
                    "argument_graph": {"edges": arguments},
                    "overlays": {
                        "modality_rows": modality,
                        "extraction_level_rows": extraction,
                    },
                    "trust_kernel": {
                        "policies": policies,
                        "assignments": assignments,
                        "attestations": attestations,
                    },
                    "frames": {"types_registered": frames, "claim_frames": claim_frames},
                    "review_workbench": {"decisions": reviews},
                    "analytics": {"detector_findings": findings},
                    "releases": {"snapshots": snapshots},
                    "top_contexts": top_ctx,
                    "top_predicates": top_pred,
                }))?
            );
        }
        Cmd::Schema => {
            let conn = client.pool().get().await?;
            // Allowed modality + extraction-level values come from
            // the column CHECK constraints.
            let modality_values: Vec<String> = vec![
                "descriptive", "prescriptive", "reconstructed", "inferred",
                "elicited", "corpus_observed", "typological_summary",
                "experimental_result", "clinical_observation", "legal_holding",
                "archival_metadata", "oral_history", "community_protocol",
                "model_output", "other",
            ].into_iter().map(String::from).collect();
            let extraction_levels: Vec<String> = vec![
                "quoted", "table_read", "example_observed",
                "source_generalization", "cross_source_inference",
                "model_hypothesis", "human_hypothesis", "manual_entry",
                "registry_import", "adapter_import",
            ].into_iter().map(String::from).collect();
            let policy_kinds: Vec<String> = vec![
                "public", "open_metadata_restricted_content",
                "community_restricted", "embargoed", "licensed", "private",
                "regulated", "sealed", "unknown_restricted",
            ].into_iter().map(String::from).collect();
            let actions: Vec<String> = vec![
                "read_metadata", "read_content", "quote",
                "view_anchor_location", "derive_claims",
                "derive_embeddings", "translate", "summarize",
                "export_claims", "export_sources", "export_anchors",
                "train_model", "publish_release",
                "share_with_third_party", "federated_query",
            ].into_iter().map(String::from).collect();
            // Frame types from the registry.
            let frame_rows = conn
                .query(
                    "select frame_type, domain, array_length(required_roles, 1) as req_n, \
                            array_length(optional_roles, 1) as opt_n \
                     from donto_frame_type \
                     order by domain, frame_type",
                    &[],
                )
                .await?;
            let frames: Vec<_> = frame_rows
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "frame_type": r.get::<_, String>(0),
                        "domain": r.get::<_, String>(1),
                        "required_roles": r.get::<_, Option<i32>>(2).unwrap_or(0),
                        "optional_roles": r.get::<_, Option<i32>>(3).unwrap_or(0),
                    })
                })
                .collect();
            let alignment_relations: Vec<String> = vec![
                "exact_equivalent", "inverse_equivalent", "sub_property_of",
                "super_property_of", "close_match", "decomposition",
                "not_equivalent", "broader_match", "narrower_match",
                "related", "context_specific",
            ].into_iter().map(String::from).collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "maturity_ladder": ["E0", "E1", "E2", "E3", "E4", "E5"],
                    "polarity_values": ["asserted", "negated", "absent", "unknown"],
                    "modality_values": modality_values,
                    "extraction_levels": extraction_levels,
                    "policy_kinds": policy_kinds,
                    "policy_actions": actions,
                    "alignment_relations": alignment_relations,
                    "identity_modes": [
                        "default", "expand_clusters",
                        "expand_sameas_transitive", "strict"
                    ],
                    "predicate_expansion": ["EXPAND", "STRICT", "EXPAND_ABOVE <pct>"],
                    "frame_types": frames,
                }))?
            );
        }
        Cmd::Modality(action) => match action {
            ModalityCmd::Set { statement, value, set_by } => {
                let conn = client.pool().get().await?;
                conn.execute(
                    "select donto_set_modality($1, $2, $3)",
                    &[&statement, &value, &set_by],
                )
                .await?;
                println!(
                    "{}",
                    serde_json::json!({"statement_id": statement, "modality": value})
                );
            }
            ModalityCmd::Get { statement } => {
                let conn = client.pool().get().await?;
                let row = conn
                    .query_opt(
                        "select modality, set_at, set_by \
                         from donto_stmt_modality where statement_id = $1",
                        &[&statement],
                    )
                    .await?;
                match row {
                    Some(r) => {
                        let m: String = r.get(0);
                        let set_at: chrono::DateTime<chrono::Utc> = r.get(1);
                        let set_by: Option<String> = r.get(2);
                        println!(
                            "{}",
                            serde_json::json!({
                                "statement_id": statement,
                                "modality": m,
                                "set_at": set_at,
                                "set_by": set_by,
                            })
                        );
                    }
                    None => println!("{}", serde_json::json!({"statement_id": statement, "modality": null})),
                }
            }
            ModalityCmd::Stats => {
                let conn = client.pool().get().await?;
                let rows = conn
                    .query(
                        "select modality, count(*)::bigint from donto_stmt_modality \
                         group by 1 order by 2 desc",
                        &[],
                    )
                    .await?;
                let dist: Vec<_> = rows
                    .iter()
                    .map(|r| serde_json::json!({"modality": r.get::<_, String>(0), "n": r.get::<_, i64>(1)}))
                    .collect();
                println!("{}", serde_json::to_string_pretty(&dist)?);
            }
        },
        Cmd::ExtractionLevel(action) => match action {
            ExtractionLevelCmd::Set { statement, value, set_by } => {
                let conn = client.pool().get().await?;
                conn.execute(
                    "select donto_set_extraction_level($1, $2, $3)",
                    &[&statement, &value, &set_by],
                )
                .await?;
                println!(
                    "{}",
                    serde_json::json!({"statement_id": statement, "extraction_level": value})
                );
            }
            ExtractionLevelCmd::Get { statement } => {
                let conn = client.pool().get().await?;
                let row = conn
                    .query_opt(
                        "select level, set_at, set_by \
                         from donto_stmt_extraction_level where statement_id = $1",
                        &[&statement],
                    )
                    .await?;
                match row {
                    Some(r) => {
                        let l: String = r.get(0);
                        let set_at: chrono::DateTime<chrono::Utc> = r.get(1);
                        let set_by: Option<String> = r.get(2);
                        println!(
                            "{}",
                            serde_json::json!({
                                "statement_id": statement,
                                "extraction_level": l,
                                "set_at": set_at,
                                "set_by": set_by,
                            })
                        );
                    }
                    None => println!("{}", serde_json::json!({"statement_id": statement, "extraction_level": null})),
                }
            }
            ExtractionLevelCmd::Stats => {
                let conn = client.pool().get().await?;
                let rows = conn
                    .query(
                        "select level, count(*)::bigint from donto_stmt_extraction_level \
                         group by 1 order by 2 desc",
                        &[],
                    )
                    .await?;
                let dist: Vec<_> = rows
                    .iter()
                    .map(|r| serde_json::json!({"level": r.get::<_, String>(0), "n": r.get::<_, i64>(1)}))
                    .collect();
                println!("{}", serde_json::to_string_pretty(&dist)?);
            }
        },
        Cmd::Policy(action) => match action {
            PolicyCmd::Register {
                policy_iri,
                kind,
                allow,
                summary,
            } => {
                let conn = client.pool().get().await?;
                // Build allowed_actions JSONB. `all` = wildcard.
                let allow_actions: Vec<&str> = if allow == "all" {
                    vec![
                        "read_metadata", "read_content", "quote",
                        "view_anchor_location", "derive_claims",
                        "derive_embeddings", "translate", "summarize",
                        "export_claims", "export_sources", "export_anchors",
                        "train_model", "publish_release",
                        "share_with_third_party", "federated_query",
                    ]
                } else if allow.is_empty() {
                    vec![]
                } else {
                    allow.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect()
                };
                let mut allowed = serde_json::Map::new();
                for a in &allow_actions {
                    allowed.insert((*a).to_string(), serde_json::Value::Bool(true));
                }
                let allowed_json = serde_json::Value::Object(allowed);
                conn.execute(
                    "insert into donto_policy_capsule \
                        (policy_iri, policy_kind, allowed_actions, created_by, \
                         human_readable_summary) \
                     values ($1, $2, $3, 'cli', $4) \
                     on conflict (policy_iri) do update set \
                       policy_kind = excluded.policy_kind, \
                       allowed_actions = excluded.allowed_actions, \
                       human_readable_summary = excluded.human_readable_summary",
                    &[&policy_iri, &kind, &allowed_json, &summary],
                )
                .await?;
                println!(
                    "{}",
                    serde_json::json!({
                        "policy_iri": policy_iri,
                        "kind": kind,
                        "allow": allow_actions,
                    })
                );
            }
            PolicyCmd::Assign {
                target_kind,
                target_id,
                policy_iri,
                assigned_by,
            } => {
                let conn = client.pool().get().await?;
                conn.execute(
                    "insert into donto_access_assignment \
                        (target_kind, target_id, policy_iri, assigned_by) \
                     values ($1, $2, $3, $4)",
                    &[&target_kind, &target_id, &policy_iri, &assigned_by],
                )
                .await?;
                println!(
                    "{}",
                    serde_json::json!({
                        "target_kind": target_kind,
                        "target_id": target_id,
                        "policy_iri": policy_iri,
                    })
                );
            }
            PolicyCmd::Check {
                target_kind,
                target_id,
                action,
            } => {
                let conn = client.pool().get().await?;
                let allowed: bool = conn
                    .query_one(
                        "select donto_action_allowed($1, $2, $3)",
                        &[&target_kind, &target_id, &action],
                    )
                    .await?
                    .get(0);
                let effective: serde_json::Value = conn
                    .query_one(
                        "select donto_effective_actions($1, $2)",
                        &[&target_kind, &target_id],
                    )
                    .await?
                    .get(0);
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "target_kind": target_kind,
                        "target_id": target_id,
                        "action": action,
                        "allowed": allowed,
                        "effective_actions": effective,
                    }))?
                );
            }
            PolicyCmd::List => {
                let conn = client.pool().get().await?;
                let rows = conn
                    .query(
                        "select p.policy_iri, p.policy_kind, p.revocation_status, \
                                p.allowed_actions, \
                                count(a.*)::bigint as assignments \
                         from donto_policy_capsule p \
                         left join donto_access_assignment a on a.policy_iri = p.policy_iri \
                         group by p.policy_iri, p.policy_kind, p.revocation_status, \
                                  p.allowed_actions \
                         order by p.policy_iri",
                        &[],
                    )
                    .await?;
                let out: Vec<_> = rows
                    .iter()
                    .map(|r| {
                        serde_json::json!({
                            "policy_iri": r.get::<_, String>(0),
                            "policy_kind": r.get::<_, String>(1),
                            "revocation_status": r.get::<_, String>(2),
                            "allowed_actions": r.get::<_, serde_json::Value>(3),
                            "assignments": r.get::<_, i64>(4),
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&out)?);
            }
        },
        Cmd::Release(action) => match action {
            ReleaseCmd::Keygen => {
                let kp = donto_release::envelope::Keypair::generate();
                println!(
                    "{}",
                    serde_json::json!({
                        "did_key": kp.did_key(),
                        "seed_hex": hex::encode(kp.seed_bytes()),
                        "note": "keep the seed secret; the did_key is public",
                    })
                );
            }
            ReleaseCmd::Sign { manifest, seed_hex } => {
                let manifest_text = std::fs::read_to_string(&manifest)
                    .with_context(|| format!("reading manifest {manifest:?}"))?;
                let manifest_value: serde_json::Value = serde_json::from_str(&manifest_text)
                    .with_context(|| "manifest must be JSON")?;
                let seed_bytes = hex::decode(seed_hex.trim())
                    .with_context(|| "seed_hex must be a 64-char hex string")?;
                if seed_bytes.len() != 32 {
                    anyhow::bail!(
                        "expected 32-byte seed (64 hex chars), got {} bytes",
                        seed_bytes.len()
                    );
                }
                let mut seed = [0u8; 32];
                seed.copy_from_slice(&seed_bytes);
                let kp = donto_release::envelope::Keypair::from_seed(seed);
                let env = donto_release::envelope::sign(&manifest_value, &kp)
                    .with_context(|| "signing")?;
                println!("{}", serde_json::to_string_pretty(&env)?);
            }
            ReleaseCmd::Verify { envelope, manifest } => {
                let env_text = std::fs::read_to_string(&envelope)?;
                let env: donto_release::envelope::ReleaseEnvelope =
                    serde_json::from_str(&env_text).with_context(|| "envelope must be JSON")?;
                if let Some(m_path) = manifest {
                    let m_text = std::fs::read_to_string(&m_path)?;
                    let m_value: serde_json::Value = serde_json::from_str(&m_text)?;
                    donto_release::envelope::verify_against_manifest(&env, &m_value)
                        .with_context(|| "verify against manifest")?;
                    println!("ok: envelope verified and manifest hash matches");
                } else {
                    donto_release::envelope::verify(&env)
                        .with_context(|| "verify signature")?;
                    println!("ok: envelope signature verified (manifest hash not re-checked)");
                }
            }
            ReleaseCmd::Build { spec, out } => {
                let spec_text = std::fs::read_to_string(&spec)
                    .with_context(|| format!("reading spec {spec:?}"))?;
                let spec_value: donto_release::ReleaseSpec =
                    serde_json::from_str(&spec_text).with_context(|| "spec must be JSON")?;
                let manifest = donto_release::build_release(&client, &spec_value)
                    .await
                    .with_context(|| "build_release")?;
                let body = serde_json::to_string_pretty(&manifest)?;
                match out {
                    Some(path) => {
                        std::fs::write(&path, &body)?;
                        eprintln!("wrote manifest to {path:?}");
                    }
                    None => println!("{body}"),
                }
            }
            ReleaseCmd::Pipeline {
                spec,
                out_dir,
                seed_hex,
                cldf,
                max_cldf_loss,
            } => {
                let spec_text = std::fs::read_to_string(&spec)?;
                let spec_value: donto_release::ReleaseSpec = serde_json::from_str(&spec_text)?;
                std::fs::create_dir_all(&out_dir)?;

                // 1. Build manifest.
                let manifest = donto_release::build_release(&client, &spec_value).await?;
                eprintln!(
                    "[1/5] built manifest: {} statements, releasable={}",
                    manifest.statement_checksums.len(),
                    manifest.policy_report.releasable,
                );

                // 2. Native JSONL.
                let jsonl_path = out_dir.join("manifest.jsonl");
                donto_release::write_native_jsonl(&manifest, &jsonl_path)?;
                eprintln!("[2/5] wrote {jsonl_path:?}");

                // 3. Ed25519 envelope.
                let seed_bytes = hex::decode(seed_hex.trim())?;
                if seed_bytes.len() != 32 {
                    anyhow::bail!("seed_hex must decode to 32 bytes");
                }
                let mut seed = [0u8; 32];
                seed.copy_from_slice(&seed_bytes);
                let kp = donto_release::envelope::Keypair::from_seed(seed);
                let manifest_value = serde_json::to_value(&manifest)?;
                let env = donto_release::envelope::sign(&manifest_value, &kp)?;
                let env_path = out_dir.join("envelope.json");
                std::fs::write(&env_path, serde_json::to_vec_pretty(&env)?)?;
                eprintln!("[3/5] wrote {env_path:?} signed by {}", env.issuer_did);

                // 4. RO-Crate metadata.
                let mut extras: Vec<(&str, &str)> =
                    vec![("envelope.json", "application/json")];
                let mut cldf_summary: Option<donto_release::CldfExportSummary> = None;
                if cldf {
                    // 5. Optional CLDF export — needs the actual
                    // statements, so re-query.
                    let scope = donto_client::ContextScope::any_of(spec_value.contexts.clone());
                    let stmts = client
                        .match_pattern(
                            None,
                            None,
                            None,
                            Some(&scope),
                            Some(donto_client::Polarity::Asserted),
                            spec_value.min_maturity,
                            spec_value.as_of,
                            None,
                        )
                        .await?;
                    let summary =
                        donto_release::write_cldf_release(&manifest, &stmts, &out_dir)?;
                    if summary.lossy_count > max_cldf_loss {
                        anyhow::bail!(
                            "CLDF export refused: lossy_count {} > max_cldf_loss {}",
                            summary.lossy_count,
                            max_cldf_loss
                        );
                    }
                    extras.push(("languages.csv", "text/csv"));
                    extras.push(("parameters.csv", "text/csv"));
                    extras.push(("codes.csv", "text/csv"));
                    extras.push(("values.csv", "text/csv"));
                    cldf_summary = Some(summary);
                    eprintln!("[5/5] CLDF export OK (lossy_count={})", cldf_summary.as_ref().unwrap().lossy_count);
                } else {
                    eprintln!("[5/5] CLDF export skipped (--cldf not set)");
                }

                donto_release::write_ro_crate_metadata(&manifest, &out_dir, &extras)?;
                eprintln!(
                    "[4/5] wrote {:?}",
                    out_dir.join("ro-crate-metadata.json")
                );

                // Final summary line as JSON for tooling.
                println!(
                    "{}",
                    serde_json::json!({
                        "out_dir": out_dir,
                        "release_id": manifest.release_id,
                        "manifest_sha256": manifest.manifest_sha256,
                        "issuer_did": env.issuer_did,
                        "statements": manifest.statement_checksums.len(),
                        "cldf_summary": cldf_summary,
                    })
                );
            }
        },
        Cmd::Align { action } => match action {
            AlignCmd::Register {
                source,
                target,
                relation,
                confidence,
            } => {
                let rel = AlignmentRelation::parse(&relation)
                    .ok_or_else(|| anyhow::anyhow!("unknown relation type: {relation}"))?;
                let id = client
                    .register_alignment(
                        &source, &target, rel, confidence, None, None, None, None, None,
                    )
                    .await?;
                println!(
                    "{}",
                    serde_json::json!({
                        "alignment_id": id,
                        "source": source,
                        "target": target,
                        "relation": relation,
                        "confidence": confidence,
                    })
                );
            }
            AlignCmd::Suggest {
                predicate,
                threshold,
                limit,
            } => {
                let suggestions = client
                    .suggest_alignments(&predicate, threshold, limit)
                    .await?;
                if suggestions.is_empty() {
                    eprintln!("no suggestions above {threshold} similarity for {predicate}");
                } else {
                    for (target, sim, label) in &suggestions {
                        println!(
                            "{}",
                            serde_json::json!({
                                "source": predicate,
                                "target": target,
                                "similarity": sim,
                                "label": label,
                            })
                        );
                    }
                }
            }
            AlignCmd::List { predicate } => {
                let c = client.pool().get().await?;
                let rows = c
                    .query(
                        "select alignment_id, source_iri, target_iri, relation, confidence, \
                                run_id, provenance, registered_by, registered_at \
                         from donto_predicate_alignment \
                         where (source_iri = $1 or target_iri = $1) \
                           and upper(tx_time) is null \
                         order by registered_at desc",
                        &[&predicate],
                    )
                    .await?;
                for r in rows {
                    let aid: Uuid = r.get("alignment_id");
                    let src: String = r.get("source_iri");
                    let tgt: String = r.get("target_iri");
                    let rel: String = r.get("relation");
                    let conf: f64 = r.get("confidence");
                    let run: Option<Uuid> = r.get("run_id");
                    let prov: serde_json::Value = r.get("provenance");
                    let by: Option<String> = r.get("registered_by");
                    let at: chrono::DateTime<chrono::Utc> = r.get("registered_at");
                    println!(
                        "{}",
                        serde_json::json!({
                            "alignment_id": aid,
                            "source": src,
                            "target": tgt,
                            "relation": rel,
                            "confidence": conf,
                            "run_id": run,
                            "provenance": prov,
                            "registered_by": by,
                            "registered_at": at,
                        })
                    );
                }
            }
            AlignCmd::Retract { id } => {
                println!(
                    "{}",
                    if client.retract_alignment(id).await? {
                        "retracted"
                    } else {
                        "no current alignment"
                    }
                );
            }
            AlignCmd::Rebuild => {
                let count = client.rebuild_predicate_closure().await?;
                println!(
                    "{}",
                    serde_json::json!({
                        "closure_rows": count,
                    })
                );
            }
            AlignCmd::Auto { threshold } => {
                eprintln!("running lexical auto-alignment (threshold {threshold})...");
                let run_id = client
                    .lexical_auto_align(None, threshold, Some("donto-cli"))
                    .await?;
                eprintln!("rebuilding closure...");
                let count = client.rebuild_predicate_closure().await?;
                println!(
                    "{}",
                    serde_json::json!({
                        "run_id": run_id,
                        "closure_rows": count,
                    })
                );
            }
        },
        Cmd::Predicates { limit } => {
            let c = client.pool().get().await?;
            let rows = c
                .query(
                    "select p.iri, p.label, \
                            coalesce(cnt.n, 0) as stmt_count \
                     from donto_predicate p \
                     left join lateral ( \
                         select count(*) as n \
                         from donto_statement s \
                         where s.predicate = p.iri and upper(s.tx_time) is null \
                     ) cnt on true \
                     order by cnt.n desc nulls last, p.iri \
                     limit $1",
                    &[&(limit as i64)],
                )
                .await?;
            for r in rows {
                let iri: String = r.get("iri");
                let label: Option<String> = r.get("label");
                let count: i64 = r.get("stmt_count");
                println!(
                    "{}",
                    serde_json::json!({
                        "iri": iri,
                        "label": label,
                        "count": count,
                    })
                );
            }
        }
        Cmd::Analyze { action } => {
            analyze::run(&client, action).await?;
        }
        Cmd::Shadow {
            subject,
            predicate,
            object_iri,
            context,
            polarity,
            min_maturity,
            no_expand,
            min_confidence,
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
                .match_aligned(
                    subject.as_deref(),
                    predicate.as_deref(),
                    object_iri.as_deref(),
                    scope.as_ref(),
                    pol,
                    min_maturity,
                    None,
                    None,
                    !no_expand,
                    min_confidence,
                )
                .await?;
            for s in stmts {
                println!(
                    "{}",
                    serde_json::json!({
                        "id": s.statement.statement_id,
                        "subject": s.statement.subject,
                        "predicate": s.statement.predicate,
                        "object": s.statement.object,
                        "context": s.statement.context,
                        "polarity": s.statement.polarity.as_str(),
                        "maturity": s.statement.maturity,
                        "valid_lo": s.statement.valid_lo,
                        "valid_hi": s.statement.valid_hi,
                        "tx_lo": s.statement.tx_lo,
                        "tx_hi": s.statement.tx_hi,
                        "matched_via": s.matched_via,
                        "alignment_confidence": s.alignment_confidence,
                    })
                );
            }
        }
    }

    Ok(())
}

mod extract;

mod bench {
    use super::*;
    use donto_client::{Object, StatementInput};
    use serde::Serialize;
    use std::time::Instant;

    #[derive(Debug, Serialize)]
    pub struct BenchReport {
        // H1: point-query a known subject (existing).
        pub inserts: u64,
        pub insert_elapsed_ms: u64,
        pub inserts_per_sec: f64,
        pub point_query_elapsed_us: u64,
        // H4: full-context batch read (existing).
        pub batch_query_rows: usize,
        pub batch_query_elapsed_ms: u64,
        // H2: aligned point-match through the predicate-closure path.
        pub h2_aligned_point_query_elapsed_us: u64,
        pub h2_aligned_point_query_rows: usize,
        // H3: as-of-tx point query against a fresh-context window.
        pub h3_asof_point_query_elapsed_us: u64,
        pub h3_asof_point_query_rows: usize,
        // H5: contradiction-frontier call across the bench context.
        pub h5_contradiction_frontier_elapsed_ms: u64,
        pub h5_contradiction_frontier_rows: usize,
        // H7: sparse-overlay filter via DontoQL MODALITY clause.
        pub h7_modality_setup_elapsed_ms: u64,
        pub h7_modality_query_elapsed_ms: u64,
        pub h7_modality_query_rows: usize,
        // H6: multi-pattern join with two patterns over the same
        // subject (`?s ex:p ?o, ?s ex:r ?o`).
        pub h6_multi_pattern_join_elapsed_ms: u64,
        pub h6_multi_pattern_join_rows: usize,
        // H8: policy-aware retrieval via POLICY ALLOWS read_metadata.
        // Setup links every 100th statement to a fail-closed source
        // so the policy join has work to do.
        pub h8_policy_allows_setup_elapsed_ms: u64,
        pub h8_policy_allows_query_elapsed_ms: u64,
        pub h8_policy_allows_query_rows: usize,
        // H9: 4 concurrent batch-writers, 500 rows each into
        // separate side contexts.
        pub h9_concurrent_writers: u32,
        pub h9_rows_per_writer: u64,
        pub h9_total_rows: u64,
        pub h9_elapsed_ms: u64,
    }

    pub async fn run(client: &DontoClient, n: u64) -> anyhow::Result<BenchReport> {
        let prefix = format!("bench:{}", uuid::Uuid::new_v4().simple());
        let ctx = format!("{prefix}/ctx");
        client
            .ensure_context(&ctx, "custom", "permissive", None)
            .await?;

        // The time floor used by H3 to query "as of one second ago"
        // — set just before the first insert so the bench context
        // legitimately existed at that timestamp.
        let asof_floor = chrono::Utc::now();

        // ---------------------------------------------------------
        // H1 baseline: insert N rows.
        // ---------------------------------------------------------
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

        // H1: point query.
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

        // H4: batch query.
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

        // ---------------------------------------------------------
        // H2: aligned point-match (rides the predicate closure).
        // Register one alignment ex:alias → ex:p, then point-query
        // by ex:alias and require the closure to return rows.
        // ---------------------------------------------------------
        let alias_iri = format!("{prefix}/alias");
        client
            .pool()
            .get()
            .await?
            .execute(
                "insert into donto_predicate_alignment \
                    (source_iri, target_iri, relation, confidence, \
                     safe_for_query_expansion, review_status) \
                 values ($1, 'ex:p', 'exact_equivalent', 0.99, true, 'accepted')",
                &[&alias_iri],
            )
            .await?;
        // Rebuild the closure index so the new edge is visible.
        client
            .pool()
            .get()
            .await?
            .execute("select donto_rebuild_predicate_closure()", &[])
            .await
            .ok(); // function exists post-0050-ish; ignore if not.
        let t = Instant::now();
        let aligned = client
            .match_aligned(
                Some("ex:s/42"),
                Some(&alias_iri),
                None,
                Some(&ContextScope::just(&ctx)),
                Some(Polarity::Asserted),
                0,
                None,
                None,
                true,
                0.5,
            )
            .await?;
        let h2_us = t.elapsed().as_micros() as u64;

        // ---------------------------------------------------------
        // H3: AS-OF point query at `asof_floor + 1s`. The
        // benchmark context was just created (so it exists at the
        // floor); the rows were also inserted within the bench
        // window. AS_OF a few seconds in the future returns rows
        // (open tx_time).
        // ---------------------------------------------------------
        let asof_ts = asof_floor + chrono::Duration::seconds(1);
        let t = Instant::now();
        let asof_rows = client
            .match_pattern(
                Some("ex:s/42"),
                Some("ex:p"),
                None,
                Some(&ContextScope::just(&ctx)),
                Some(Polarity::Asserted),
                0,
                Some(asof_ts),
                None,
            )
            .await?;
        let h3_us = t.elapsed().as_micros() as u64;

        // ---------------------------------------------------------
        // H5: contradiction frontier across the bench context.
        // No donto_argument rows seeded, so the result is empty —
        // we're measuring the SQL function's overhead.
        // ---------------------------------------------------------
        let t = Instant::now();
        let frontier_rows = client
            .pool()
            .get()
            .await?
            .query(
                "select statement_id, attack_count, support_count \
                 from donto_contradiction_frontier($1)",
                &[&ctx],
            )
            .await?;
        let h5_ms = t.elapsed().as_millis() as u64;

        // ---------------------------------------------------------
        // ---------------------------------------------------------
        // H6: multi-pattern join — basic graph pattern with three
        // shared variables. Time the evaluator's nested-loop join
        // over the bench context.
        // ---------------------------------------------------------
        // Seed two extra predicates on the same subjects so a
        // 3-pattern join has something to chain.
        let mut extra = Vec::with_capacity(2000);
        for i in 0..n {
            extra.push(
                StatementInput::new(
                    format!("ex:s/{i}"),
                    "ex:r",
                    Object::iri(format!("ex:o/{i}")),
                )
                .with_context(&ctx),
            );
            if extra.len() == 2000 {
                client.assert_batch(&extra).await?;
                extra.clear();
            }
        }
        if !extra.is_empty() {
            client.assert_batch(&extra).await?;
        }
        // Pin the first pattern's subject so the join cost is
        // dominated by the planner machinery, not the linear-scan
        // cost (the Phase-4 evaluator does one SQL roundtrip per
        // intermediate binding; an unconstrained leading pattern at
        // N=1M would emit ~1M roundtrips. PRD §26 Phase 10 covers
        // the planner work).
        let h6_query = format!(
            "MATCH ex:s/42 ex:p ?o, ex:s/42 ex:r ?o \
             SCOPE include {ctx} \
             PREDICATES STRICT \
             LIMIT 5",
            ctx = ctx
        );
        let q = donto_query::parse_dontoql(&h6_query)?;
        let t = Instant::now();
        let h6 = donto_query::evaluate(client, &q).await?;
        let h6_ms = t.elapsed().as_millis() as u64;

        // ---------------------------------------------------------
        // H7: sparse-overlay filter — set modality on half the rows
        // and then query with MODALITY descriptive.
        // ---------------------------------------------------------
        let t = Instant::now();
        client
            .pool()
            .get()
            .await?
            .execute(
                "insert into donto_stmt_modality (statement_id, modality, set_by) \
                 select statement_id, 'descriptive', 'bench' \
                 from donto_statement \
                 where context = $1 and upper(tx_time) is null \
                 order by statement_id \
                 limit greatest($2::bigint / 2, 1) \
                 on conflict (statement_id) do nothing",
                &[&ctx, &(n as i64)],
            )
            .await?;
        let h7_setup_ms = t.elapsed().as_millis() as u64;
        let t = Instant::now();
        let h7_rows = client
            .pool()
            .get()
            .await?
            .query(
                "select s.statement_id \
                 from donto_statement s \
                 join donto_stmt_modality m on m.statement_id = s.statement_id \
                 where s.context = $1 \
                   and upper(s.tx_time) is null \
                   and m.modality = 'descriptive'",
                &[&ctx],
            )
            .await?;
        let h7_query_ms = t.elapsed().as_millis() as u64;

        // ---------------------------------------------------------
        // H8: policy-aware retrieval. Register one fail-closed
        // policy + one document + an evidence_link to every Nth
        // statement. Then run POLICY ALLOWS read_metadata and
        // expect the linked rows to be dropped.
        // ---------------------------------------------------------
        let policy_iri = format!("{prefix}/policy/private");
        let doc_iri = format!("{prefix}/doc/private");
        let conn = client.pool().get().await?;
        conn.execute(
            "insert into donto_policy_capsule \
                (policy_iri, policy_kind, allowed_actions, created_by) \
             values ($1, 'private', \
                     jsonb_build_object('read_metadata', false), \
                     'bench')",
            &[&policy_iri],
        )
        .await?;
        let doc_id: uuid::Uuid = conn
            .query_one(
                "insert into donto_document \
                    (iri, media_type, policy_id, status) \
                 values ($1, 'text/plain', $2, 'registered') \
                 returning document_id",
                &[&doc_iri, &policy_iri],
            )
            .await?
            .get(0);
        let t = Instant::now();
        // Link every 100th statement to the private document; this
        // creates a real evidence_link join workload without
        // requiring N inserts.
        conn.execute(
            "insert into donto_evidence_link \
                (statement_id, link_type, target_document_id) \
             select statement_id, 'extracted_from', $2 \
             from donto_statement \
             where context = $1 and upper(tx_time) is null \
               and (substring(subject from 'ex:s/(.*)$')::bigint) % 100 = 0",
            &[&ctx, &doc_id],
        )
        .await?;
        let h8_setup_ms = t.elapsed().as_millis() as u64;
        let q_h8 = donto_query::parse_dontoql(&format!(
            "MATCH ?s ex:p ?o SCOPE include {ctx} \
             PREDICATES STRICT \
             POLICY ALLOWS read_metadata \
             LIMIT 10000000",
            ctx = ctx
        ))?;
        let t = Instant::now();
        let h8 = donto_query::evaluate(client, &q_h8).await?;
        let h8_ms = t.elapsed().as_millis() as u64;

        // ---------------------------------------------------------
        // H9: concurrent writers. Four parallel batch-asserters
        // each insert M = 500 rows into a side-context. Measures
        // the substrate's behaviour under contention without
        // touching the main bench context.
        // ---------------------------------------------------------
        let m_per_writer: u64 = 500;
        let h9_ctx_base = format!("{prefix}/h9");
        let t = Instant::now();
        let mut joiners = Vec::with_capacity(4);
        for w in 0..4u64 {
            let client_clone = client.clone();
            let ctx_w = format!("{h9_ctx_base}/w{w}");
            joiners.push(tokio::spawn(async move {
                client_clone
                    .ensure_context(&ctx_w, "custom", "permissive", None)
                    .await?;
                let mut batch = Vec::with_capacity(m_per_writer as usize);
                for i in 0..m_per_writer {
                    batch.push(
                        StatementInput::new(
                            format!("ex:h9/w{w}/{i}"),
                            "ex:p",
                            Object::iri(format!("ex:o/{i}")),
                        )
                        .with_context(&ctx_w),
                    );
                }
                client_clone.assert_batch(&batch).await?;
                Ok::<(), anyhow::Error>(())
            }));
        }
        for j in joiners {
            j.await??;
        }
        let h9_ms = t.elapsed().as_millis() as u64;
        let h9_total = 4 * m_per_writer;

        Ok(BenchReport {
            inserts: n,
            insert_elapsed_ms: insert_elapsed.as_millis() as u64,
            inserts_per_sec: (n as f64) / insert_elapsed.as_secs_f64().max(1e-9),
            point_query_elapsed_us: point_us,
            batch_query_rows: all.len(),
            batch_query_elapsed_ms: batch_elapsed.as_millis() as u64,
            h2_aligned_point_query_elapsed_us: h2_us,
            h2_aligned_point_query_rows: aligned.len(),
            h3_asof_point_query_elapsed_us: h3_us,
            h3_asof_point_query_rows: asof_rows.len(),
            h5_contradiction_frontier_elapsed_ms: h5_ms,
            h5_contradiction_frontier_rows: frontier_rows.len(),
            h7_modality_setup_elapsed_ms: h7_setup_ms,
            h7_modality_query_elapsed_ms: h7_query_ms,
            h7_modality_query_rows: h7_rows.len(),
            h6_multi_pattern_join_elapsed_ms: h6_ms,
            h6_multi_pattern_join_rows: h6.len(),
            h8_policy_allows_setup_elapsed_ms: h8_setup_ms,
            h8_policy_allows_query_elapsed_ms: h8_ms,
            h8_policy_allows_query_rows: h8.len(),
            h9_concurrent_writers: 4,
            h9_rows_per_writer: m_per_writer,
            h9_total_rows: h9_total,
            h9_elapsed_ms: h9_ms,
        })
    }
}
