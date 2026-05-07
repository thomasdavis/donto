//! Embedded SQL migrations.
//!
//! Each migration is an idempotent SQL script. We hash the script and record
//! it in `donto_migration` after a successful apply. Re-running skips
//! migrations whose hash matches a recorded entry.

use crate::Result;
use deadpool_postgres::Pool;
use sha2::{Digest, Sha256};

/// Embedded migration source. Order matters.
pub const MIGRATIONS: &[(&str, &str)] = &[
    (
        "0001_core",
        include_str!("../../sql/migrations/0001_core.sql"),
    ),
    (
        "0002_flags",
        include_str!("../../sql/migrations/0002_flags.sql"),
    ),
    (
        "0003_functions",
        include_str!("../../sql/migrations/0003_functions.sql"),
    ),
    (
        "0004_migrations",
        include_str!("../../sql/migrations/0004_migrations.sql"),
    ),
    (
        "0005_presets",
        include_str!("../../sql/migrations/0005_presets.sql"),
    ),
    (
        "0006_predicate",
        include_str!("../../sql/migrations/0006_predicate.sql"),
    ),
    (
        "0007_snapshot",
        include_str!("../../sql/migrations/0007_snapshot.sql"),
    ),
    (
        "0008_shape",
        include_str!("../../sql/migrations/0008_shape.sql"),
    ),
    (
        "0009_rule",
        include_str!("../../sql/migrations/0009_rule.sql"),
    ),
    (
        "0010_certificate",
        include_str!("../../sql/migrations/0010_certificate.sql"),
    ),
    (
        "0011_observability",
        include_str!("../../sql/migrations/0011_observability.sql"),
    ),
    (
        "0012_match_scope_fix",
        include_str!("../../sql/migrations/0012_match_scope_fix.sql"),
    ),
    (
        "0013_search_trgm",
        include_str!("../../sql/migrations/0013_search_trgm.sql"),
    ),
    (
        "0014_retrofit",
        include_str!("../../sql/migrations/0014_retrofit.sql"),
    ),
    (
        "0015_shape_annotations",
        include_str!("../../sql/migrations/0015_shape_annotations.sql"),
    ),
    (
        "0016_valid_time_buckets",
        include_str!("../../sql/migrations/0016_valid_time_buckets.sql"),
    ),
    (
        "0017_reactions",
        include_str!("../../sql/migrations/0017_reactions.sql"),
    ),
    (
        "0018_aggregates",
        include_str!("../../sql/migrations/0018_aggregates.sql"),
    ),
    (
        "0019_fts",
        include_str!("../../sql/migrations/0019_fts.sql"),
    ),
    (
        "0020_bitemporal_canonicals",
        include_str!("../../sql/migrations/0020_bitemporal_canonicals.sql"),
    ),
    (
        "0021_same_meaning",
        include_str!("../../sql/migrations/0021_same_meaning.sql"),
    ),
    (
        "0022_context_env",
        include_str!("../../sql/migrations/0022_context_env.sql"),
    ),
    (
        "0023_documents",
        include_str!("../../sql/migrations/0023_documents.sql"),
    ),
    (
        "0024_document_revisions",
        include_str!("../../sql/migrations/0024_document_revisions.sql"),
    ),
    (
        "0025_spans",
        include_str!("../../sql/migrations/0025_spans.sql"),
    ),
    (
        "0026_annotations",
        include_str!("../../sql/migrations/0026_annotations.sql"),
    ),
    (
        "0027_annotation_edges",
        include_str!("../../sql/migrations/0027_annotation_edges.sql"),
    ),
    (
        "0028_extraction_runs",
        include_str!("../../sql/migrations/0028_extraction_runs.sql"),
    ),
    (
        "0029_evidence_links",
        include_str!("../../sql/migrations/0029_evidence_links.sql"),
    ),
    (
        "0030_agents",
        include_str!("../../sql/migrations/0030_agents.sql"),
    ),
    (
        "0031_arguments",
        include_str!("../../sql/migrations/0031_arguments.sql"),
    ),
    (
        "0032_proof_obligations",
        include_str!("../../sql/migrations/0032_proof_obligations.sql"),
    ),
    (
        "0033_vectors",
        include_str!("../../sql/migrations/0033_vectors.sql"),
    ),
    (
        "0034_claim_card",
        include_str!("../../sql/migrations/0034_claim_card.sql"),
    ),
    (
        "0035_document_sections",
        include_str!("../../sql/migrations/0035_document_sections.sql"),
    ),
    (
        "0036_mentions",
        include_str!("../../sql/migrations/0036_mentions.sql"),
    ),
    (
        "0037_extraction_chunks",
        include_str!("../../sql/migrations/0037_extraction_chunks.sql"),
    ),
    (
        "0038_confidence",
        include_str!("../../sql/migrations/0038_confidence.sql"),
    ),
    (
        "0039_units",
        include_str!("../../sql/migrations/0039_units.sql"),
    ),
    (
        "0040_temporal_expressions",
        include_str!("../../sql/migrations/0040_temporal_expressions.sql"),
    ),
    (
        "0041_content_regions",
        include_str!("../../sql/migrations/0041_content_regions.sql"),
    ),
    (
        "0042_entity_aliases",
        include_str!("../../sql/migrations/0042_entity_aliases.sql"),
    ),
    (
        "0043_candidate_contexts",
        include_str!("../../sql/migrations/0043_candidate_contexts.sql"),
    ),
    (
        "0044_ontology_seeds",
        include_str!("../../sql/migrations/0044_ontology_seeds.sql"),
    ),
    (
        "0045_auto_shape_validation",
        include_str!("../../sql/migrations/0045_auto_shape_validation.sql"),
    ),
    (
        "0046_references",
        include_str!("../../sql/migrations/0046_references.sql"),
    ),
    (
        "0047_claim_lifecycle",
        include_str!("../../sql/migrations/0047_claim_lifecycle.sql"),
    ),
    (
        "0048_predicate_alignment",
        include_str!("../../sql/migrations/0048_predicate_alignment.sql"),
    ),
    (
        "0049_predicate_descriptor",
        include_str!("../../sql/migrations/0049_predicate_descriptor.sql"),
    ),
    (
        "0050_alignment_run",
        include_str!("../../sql/migrations/0050_alignment_run.sql"),
    ),
    (
        "0051_predicate_closure",
        include_str!("../../sql/migrations/0051_predicate_closure.sql"),
    ),
    (
        "0052_match_aligned",
        include_str!("../../sql/migrations/0052_match_aligned.sql"),
    ),
    (
        "0053_canonical_shadow",
        include_str!("../../sql/migrations/0053_canonical_shadow.sql"),
    ),
    (
        "0054_event_frames",
        include_str!("../../sql/migrations/0054_event_frames.sql"),
    ),
    (
        "0055_match_alignment_integration",
        include_str!("../../sql/migrations/0055_match_alignment_integration.sql"),
    ),
    (
        "0056_lexical_normalizer",
        include_str!("../../sql/migrations/0056_lexical_normalizer.sql"),
    ),
    (
        "0057_entity_symbol",
        include_str!("../../sql/migrations/0057_entity_symbol.sql"),
    ),
    (
        "0058_entity_mention",
        include_str!("../../sql/migrations/0058_entity_mention.sql"),
    ),
    (
        "0059_entity_signature",
        include_str!("../../sql/migrations/0059_entity_signature.sql"),
    ),
    (
        "0060_identity_edge",
        include_str!("../../sql/migrations/0060_identity_edge.sql"),
    ),
    (
        "0061_identity_hypothesis",
        include_str!("../../sql/migrations/0061_identity_hypothesis.sql"),
    ),
    (
        "0062_literal_canonical",
        include_str!("../../sql/migrations/0062_literal_canonical.sql"),
    ),
    (
        "0063_time_expression",
        include_str!("../../sql/migrations/0063_time_expression.sql"),
    ),
    (
        "0064_temporal_relation",
        include_str!("../../sql/migrations/0064_temporal_relation.sql"),
    ),
    (
        "0065_property_constraint",
        include_str!("../../sql/migrations/0065_property_constraint.sql"),
    ),
    (
        "0066_class_hierarchy",
        include_str!("../../sql/migrations/0066_class_hierarchy.sql"),
    ),
    (
        "0067_rule_engine",
        include_str!("../../sql/migrations/0067_rule_engine.sql"),
    ),
    // ---- Trust Kernel foundation ----
    (
        "0089_hypothesis_only_flag",
        include_str!("../../sql/migrations/0089_hypothesis_only_flag.sql"),
    ),
    (
        "0090_event_log",
        include_str!("../../sql/migrations/0090_event_log.sql"),
    ),
    (
        "0091_argument_relations_v2",
        include_str!("../../sql/migrations/0091_argument_relations_v2.sql"),
    ),
    (
        "0092_alignment_relations_v2",
        include_str!("../../sql/migrations/0092_alignment_relations_v2.sql"),
    ),
    (
        "0093_identity_hypothesis_kind",
        include_str!("../../sql/migrations/0093_identity_hypothesis_kind.sql"),
    ),
    (
        "0094_dataset_release",
        include_str!("../../sql/migrations/0094_dataset_release.sql"),
    ),
    (
        "0095_source_object_extension",
        include_str!("../../sql/migrations/0095_source_object_extension.sql"),
    ),
    (
        "0096_source_version_extension",
        include_str!("../../sql/migrations/0096_source_version_extension.sql"),
    ),
    (
        "0097_anchor_kind_registry",
        include_str!("../../sql/migrations/0097_anchor_kind_registry.sql"),
    ),
    (
        "0098_polarity_v2",
        include_str!("../../sql/migrations/0098_polarity_v2.sql"),
    ),
    (
        "0099_statement_modality",
        include_str!("../../sql/migrations/0099_statement_modality.sql"),
    ),
    (
        "0100_extraction_level",
        include_str!("../../sql/migrations/0100_extraction_level.sql"),
    ),
    (
        "0101_confidence_multivalue",
        include_str!("../../sql/migrations/0101_confidence_multivalue.sql"),
    ),
    (
        "0102_maturity_e_naming",
        include_str!("../../sql/migrations/0102_maturity_e_naming.sql"),
    ),
    (
        "0103_multi_context",
        include_str!("../../sql/migrations/0103_multi_context.sql"),
    ),
    (
        "0104_claim_kind",
        include_str!("../../sql/migrations/0104_claim_kind.sql"),
    ),
    (
        "0105_claim_frame",
        include_str!("../../sql/migrations/0105_claim_frame.sql"),
    ),
    (
        "0106_frame_role",
        include_str!("../../sql/migrations/0106_frame_role.sql"),
    ),
    (
        "0107_context_multi_parent",
        include_str!("../../sql/migrations/0107_context_multi_parent.sql"),
    ),
    (
        "0108_entity_extension",
        include_str!("../../sql/migrations/0108_entity_extension.sql"),
    ),
    (
        "0109_identity_hypothesis_v2",
        include_str!("../../sql/migrations/0109_identity_hypothesis_v2.sql"),
    ),
    (
        "0110_predicate_minting",
        include_str!("../../sql/migrations/0110_predicate_minting.sql"),
    ),
    (
        "0111_policy_capsule",
        include_str!("../../sql/migrations/0111_policy_capsule.sql"),
    ),
    (
        "0112_attestation",
        include_str!("../../sql/migrations/0112_attestation.sql"),
    ),
    (
        "0113_obligation_kinds_v2",
        include_str!("../../sql/migrations/0113_obligation_kinds_v2.sql"),
    ),
    (
        "0114_review_decision",
        include_str!("../../sql/migrations/0114_review_decision.sql"),
    ),
    (
        "0115_query_v2_metadata",
        include_str!("../../sql/migrations/0115_query_v2_metadata.sql"),
    ),
    (
        "0116_frame_type_registry",
        include_str!("../../sql/migrations/0116_frame_type_registry.sql"),
    ),
    (
        "0117_rename_drop_v1000_suffix",
        include_str!("../../sql/migrations/0117_rename_drop_v1000_suffix.sql"),
    ),
];

fn sha256_of(s: &str) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    h.finalize().to_vec()
}

pub async fn apply_migrations(pool: &Pool) -> Result<()> {
    let mut client = pool.get().await?;

    // Serialize concurrent migrate() callers (e.g., parallel cargo test
    // binaries). Without this, two callers can both see ledger_exists=false
    // on a fresh DB and race through the loop. Because some migrations use
    // `create or replace function` (notably 0006_predicate redefining
    // donto_assert), the LAST writer wins — and the loser may be running an
    // earlier migration whose `create or replace` clobbers a later one's
    // overlay. The advisory lock makes the apply path one-at-a-time.
    //
    // Lock id is a random constant for donto migrations; it's released
    // automatically when the connection is returned to the pool.
    client
        .execute("select pg_advisory_lock(8836428012345678901::bigint)", &[])
        .await?;

    // Detect first-time install: the ledger table only exists after 0004 has
    // run at least once. On first install we run every migration in order;
    // on subsequent runs we consult the ledger for *every* migration so that
    // later overrides (e.g. 0006_predicate redefining donto_assert) are not
    // clobbered by re-running an earlier migration.
    let ledger_exists: bool = client
        .query_one(
            "select to_regclass('public.donto_migration') is not null",
            &[],
        )
        .await?
        .get(0);

    for (name, sql) in MIGRATIONS.iter() {
        let hash = sha256_of(sql);

        if ledger_exists {
            let already = client
                .query_opt(
                    "select 1 from donto_migration where name = $1 and sha256 = $2",
                    &[&name, &hash],
                )
                .await?;
            if already.is_some() {
                tracing::debug!(name, "skipping migration (already applied)");
                continue;
            }
        }

        tracing::info!(name, "applying migration");
        let tx = client.transaction().await?;
        tx.batch_execute(sql).await?;

        // After 0004 has run, the ledger table exists and every subsequent
        // migration (and 0004 itself) is recorded. Migrations 0001..=0003 on
        // first install are recorded by the seed inside 0004 itself.
        let ledger_should_exist = ledger_exists
            || MIGRATIONS
                .iter()
                .position(|(n, _)| *n == *name)
                .is_some_and(|i| i >= 3); // 0004 is index 3
        if ledger_should_exist {
            tx.execute(
                "insert into donto_migration (name, sha256) values ($1, $2)
                 on conflict (name) do update set sha256 = excluded.sha256, applied_at = now()",
                &[&name, &hash],
            )
            .await?;
        }

        tx.commit().await?;
    }

    // Backfill real SHAs for the bootstrap migrations 0001/0002/0003.
    // Migration 0004 seeds them with sha=`decode('00','hex')` placeholder
    // because SQL has no access to the Rust-side hash. Without this fixup,
    // every subsequent migrate() call sees a sha mismatch on those three
    // and re-applies them — and the re-apply of 0003 silently overwrites
    // the donto_assert defined by 0006_predicate (which adds the
    // implicit-predicate-registration path). Update them in place.
    for (name, sql) in MIGRATIONS.iter() {
        if matches!(*name, "0001_core" | "0002_flags" | "0003_functions") {
            let hash = sha256_of(sql);
            client
                .execute(
                    "update donto_migration set sha256 = $2 where name = $1 and sha256 = decode('00','hex')",
                    &[&name, &hash],
                )
                .await?;
        }
    }

    // Explicit release; would also happen on connection return.
    client
        .execute(
            "select pg_advisory_unlock(8836428012345678901::bigint)",
            &[],
        )
        .await?;
    Ok(())
}
