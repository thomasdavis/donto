//! Comprehensive examples exercising every donto capability.
//!
//! Scenario: a research team investigating historical claims about
//! Ada Lovelace, using donto as an evidence substrate.

use chrono::{Datelike, NaiveDate};
use donto_client::{Literal, Object, Polarity, ReactionKind, ShapeVerdict, StatementInput};
use serde_json::json;

mod common;
use common::{cleanup_prefix, connect, ctx, tag};

// ---------- 1. Assert statements ----------
#[tokio::test]
async fn ex_assert_statements() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("ex-assert");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "ex-assert").await;

    // IRI object
    let s1 = client
        .assert(
            &StatementInput::new(
                format!("{prefix}/ada"),
                "rdf:type",
                Object::iri("foaf:Person"),
            )
            .with_context(&ctx),
        )
        .await
        .unwrap();

    // Literal object
    let s2 = client
        .assert(
            &StatementInput::new(
                format!("{prefix}/ada"),
                "foaf:name",
                Object::Literal(Literal::string("Ada Lovelace")),
            )
            .with_context(&ctx),
        )
        .await
        .unwrap();

    assert_ne!(s1, s2);

    // Verify both exist
    let stmts = client
        .match_pattern(
            Some(&format!("{prefix}/ada")),
            None,
            None,
            None,
            None,
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert!(stmts.len() >= 2);
}

// ---------- 2. Paraconsistency: contradictions coexist ----------
#[tokio::test]
async fn ex_paraconsistency() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("ex-para");
    cleanup_prefix(&client, &prefix).await;

    // Two sources disagree about Ada's birth year
    let src_a = format!("{prefix}/source-a");
    let src_b = format!("{prefix}/source-b");
    client
        .ensure_context(&src_a, "source", "permissive", None)
        .await
        .unwrap();
    client
        .ensure_context(&src_b, "source", "permissive", None)
        .await
        .unwrap();

    client
        .assert(
            &StatementInput::new(
                format!("{prefix}/ada"),
                "ex:birthYear",
                Object::Literal(Literal::integer(1815)),
            )
            .with_context(&src_a),
        )
        .await
        .unwrap();

    client
        .assert(
            &StatementInput::new(
                format!("{prefix}/ada"),
                "ex:birthYear",
                Object::Literal(Literal::integer(1816)),
            )
            .with_context(&src_b),
        )
        .await
        .unwrap();

    // Both live; donto never rejects contradictions
    let all = client
        .match_pattern(
            Some(&format!("{prefix}/ada")),
            Some("ex:birthYear"),
            None,
            None,
            None,
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(all.len(), 2, "both contradictory birth years must coexist");
}

// ---------- 3. Bitemporal queries ----------
#[tokio::test]
async fn ex_bitemporal() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("ex-bitemp");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "ex-bitemp").await;

    // Assert with explicit valid_time
    let s = client
        .assert(
            &StatementInput::new(
                format!("{prefix}/ada"),
                "ex:title",
                Object::Literal(Literal::string("Countess of Lovelace")),
            )
            .with_context(&ctx)
            .with_valid(Some(NaiveDate::from_ymd_opt(1838, 1, 1).unwrap()), None),
        )
        .await
        .unwrap();

    // Query as-of a valid date within the range
    let found = client
        .match_pattern(
            Some(&format!("{prefix}/ada")),
            Some("ex:title"),
            None,
            None,
            None,
            0,
            None,
            Some(NaiveDate::from_ymd_opt(1850, 1, 1).unwrap()),
        )
        .await
        .unwrap();
    assert_eq!(found.len(), 1);

    // Query as-of a date before the valid range
    let not_found = client
        .match_pattern(
            Some(&format!("{prefix}/ada")),
            Some("ex:title"),
            None,
            None,
            None,
            0,
            None,
            Some(NaiveDate::from_ymd_opt(1830, 1, 1).unwrap()),
        )
        .await
        .unwrap();
    assert_eq!(not_found.len(), 0);

    // Retract — closes tx_time, never deletes
    client.retract(s).await.unwrap();
    let after = client
        .match_pattern(
            Some(&format!("{prefix}/ada")),
            Some("ex:title"),
            None,
            None,
            None,
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        after.len(),
        0,
        "retracted statement vanishes from current view"
    );

    // But the row is still there historically
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let exists: bool = c
        .query_one(
            "select exists(select 1 from donto_statement where statement_id = $1)",
            &[&s],
        )
        .await
        .unwrap()
        .get(0);
    assert!(exists, "retracted row must never be deleted");
}

// ---------- 4. Contexts ----------
#[tokio::test]
async fn ex_contexts() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("ex-ctx");
    cleanup_prefix(&client, &prefix).await;

    // Context hierarchy: research > team-a, team-b
    let root = format!("{prefix}/research");
    let team_a = format!("{prefix}/research/team-a");
    let team_b = format!("{prefix}/research/team-b");
    client
        .ensure_context(&root, "source", "permissive", None)
        .await
        .unwrap();
    client
        .ensure_context(&team_a, "user", "permissive", Some(&root))
        .await
        .unwrap();
    client
        .ensure_context(&team_b, "user", "permissive", Some(&root))
        .await
        .unwrap();

    client
        .assert(
            &StatementInput::new(
                format!("{prefix}/ada"),
                "ex:note",
                Object::Literal(Literal::string("Team A finding")),
            )
            .with_context(&team_a),
        )
        .await
        .unwrap();

    client
        .assert(
            &StatementInput::new(
                format!("{prefix}/ada"),
                "ex:note",
                Object::Literal(Literal::string("Team B finding")),
            )
            .with_context(&team_b),
        )
        .await
        .unwrap();

    // Scope on root with descendants includes both team contexts
    let scope = donto_client::ContextScope::just(&root);
    let found = client
        .match_pattern(
            Some(&format!("{prefix}/ada")),
            Some("ex:note"),
            None,
            Some(&scope),
            None,
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(found.len(), 2, "descendants visible from parent scope");

    // Scope on just team_a finds only one
    let scope_a = donto_client::ContextScope::just(&team_a);
    let found_a = client
        .match_pattern(
            Some(&format!("{prefix}/ada")),
            Some("ex:note"),
            None,
            Some(&scope_a),
            None,
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(found_a.len(), 1);
}

// ---------- 5. Scope presets ----------
#[tokio::test]
async fn ex_scope_presets() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();

    // The 'anywhere' preset exists and resolves to all contexts
    let preset: serde_json::Value = c
        .query_one(
            "select scope from donto_scope_preset where name = 'anywhere'",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(preset.get("include").is_some());

    // Define a custom preset
    let prefix = tag("ex-preset");
    let ctx_iri = format!("{prefix}/ctx");
    c.execute(
        "select donto_ensure_context($1, 'custom', 'permissive')",
        &[&ctx_iri],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_define_preset($1, $2, $3)",
        &[
            &format!("{prefix}/my-preset"),
            &json!({"include": [ctx_iri], "include_descendants": true}),
            &"Test preset",
        ],
    )
    .await
    .unwrap();

    let stored: serde_json::Value = c
        .query_one(
            "select scope from donto_scope_preset where name = $1",
            &[&format!("{prefix}/my-preset")],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(stored["include"][0], ctx_iri);
}

// ---------- 6. Polarity (asserted/negated/absent/unknown) ----------
#[tokio::test]
async fn ex_polarity() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("ex-polar");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "ex-polar").await;

    for pol in [
        Polarity::Asserted,
        Polarity::Negated,
        Polarity::Absent,
        Polarity::Unknown,
    ] {
        client
            .assert(
                &StatementInput::new(
                    format!("{prefix}/ada"),
                    "ex:claim",
                    Object::Literal(Literal::string(format!("pol={}", pol.as_str()))),
                )
                .with_context(&ctx)
                .with_polarity(pol),
            )
            .await
            .unwrap();
    }

    // Filter by negated
    let negated = client
        .match_pattern(
            Some(&format!("{prefix}/ada")),
            Some("ex:claim"),
            None,
            None,
            Some(Polarity::Negated),
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(negated.len(), 1);
    assert_eq!(negated[0].polarity, Polarity::Negated);
}

// ---------- 7. Maturity levels ----------
#[tokio::test]
async fn ex_maturity() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("ex-mat");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "ex-mat").await;

    // Raw (0), curated (1), shape-checked (2)
    for m in 0..3u8 {
        client
            .assert(
                &StatementInput::new(
                    format!("{prefix}/ada"),
                    "ex:fact",
                    Object::Literal(Literal::string(format!("maturity={m}"))),
                )
                .with_context(&ctx)
                .with_maturity(m),
            )
            .await
            .unwrap();
    }

    // min_maturity=2 only returns the shape-checked one
    let checked = client
        .match_pattern(
            Some(&format!("{prefix}/ada")),
            Some("ex:fact"),
            None,
            None,
            None,
            2,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(checked.len(), 1);
    assert_eq!(checked[0].maturity, 2);
}

// ---------- 8. Batch assert ----------
#[tokio::test]
async fn ex_batch_assert() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("ex-batch");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "ex-batch").await;

    let stmts: Vec<StatementInput> = (0..5)
        .map(|i| {
            StatementInput::new(
                format!("{prefix}/ada"),
                format!("ex:prop{i}"),
                Object::Literal(Literal::string(format!("value-{i}"))),
            )
            .with_context(&ctx)
        })
        .collect();

    let n = client.assert_batch(&stmts).await.unwrap();
    assert_eq!(n, 5);
}

// ---------- 9. Retract and correct ----------
#[tokio::test]
async fn ex_retract_and_correct() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("ex-rc");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "ex-rc").await;

    let s = client
        .assert(
            &StatementInput::new(
                format!("{prefix}/ada"),
                "ex:occupation",
                Object::Literal(Literal::string("Mathematician")),
            )
            .with_context(&ctx),
        )
        .await
        .unwrap();

    // Correct: change the object
    let s2 = client
        .correct(
            s,
            None,
            None,
            Some(&Object::Literal(Literal::string(
                "Mathematician and Writer",
            ))),
            None,
        )
        .await
        .unwrap();

    assert_ne!(s, s2);
    let found = client
        .match_pattern(
            Some(&format!("{prefix}/ada")),
            Some("ex:occupation"),
            None,
            None,
            None,
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].statement_id, s2);
}

// ---------- 10. Predicate registry ----------
#[tokio::test]
async fn ex_predicate_registry() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let prefix = tag("ex-pred");

    // Register a predicate with a label and description
    c.execute(
        "select donto_register_predicate($1, $2, $3)",
        &[
            &format!("{prefix}/knows"),
            &"knows",
            &"Person knows another person",
        ],
    )
    .await
    .unwrap();

    // Register an alias: iri, label, description, canonical_of
    c.execute(
        "select donto_register_predicate($1, $2, $3, $4)",
        &[
            &format!("{prefix}/acquaintedWith"),
            &"acquaintedWith",
            &"alias for knows",
            &format!("{prefix}/knows"),
        ],
    )
    .await
    .unwrap();

    // Canonical lookup
    let canonical: String = c
        .query_one(
            "select donto_canonical_predicate($1)",
            &[&format!("{prefix}/acquaintedWith")],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(canonical, format!("{prefix}/knows"));
}

// ---------- 11. FTS ----------
#[tokio::test]
async fn ex_full_text_search() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("ex-fts");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "ex-fts").await;

    client
        .assert(
            &StatementInput::new(
                format!("{prefix}/ada"),
                "ex:bio",
                Object::Literal(Literal::lang_string(
                    "Ada Lovelace wrote the first algorithm intended to be carried out by a machine",
                    "en",
                )),
            )
            .with_context(&ctx),
        )
        .await
        .unwrap();

    let results = client
        .match_text("algorithm machine", Some("en"), None, None, None, 0)
        .await
        .unwrap();
    assert!(!results.is_empty(), "FTS must find the statement");
}

// ---------- 12. Bitemporal canonicals ----------
#[tokio::test]
async fn ex_bitemporal_canonicals() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let prefix = tag("ex-btcan");

    // Register two predicates
    c.execute(
        "select donto_register_predicate($1, $2)",
        &[&format!("{prefix}/oldName"), &"old name"],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_register_predicate($1, $2)",
        &[&format!("{prefix}/newName"), &"new name"],
    )
    .await
    .unwrap();

    // Register a time-dependent alias
    c.execute(
        "select donto_register_alias_at($1, $2, $3, $4)",
        &[
            &format!("{prefix}/oldName"),
            &format!("{prefix}/newName"),
            &NaiveDate::from_ymd_opt(2000, 1, 1).unwrap(),
            &Option::<NaiveDate>::None,
        ],
    )
    .await
    .unwrap();

    // Before 2000: no alias -> self
    let r: String = c
        .query_one(
            "select donto_canonical_predicate_at($1, $2)",
            &[
                &format!("{prefix}/oldName"),
                &NaiveDate::from_ymd_opt(1990, 1, 1).unwrap(),
            ],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(r, format!("{prefix}/oldName"));

    // After 2000: alias -> newName
    let r: String = c
        .query_one(
            "select donto_canonical_predicate_at($1, $2)",
            &[
                &format!("{prefix}/oldName"),
                &NaiveDate::from_ymd_opt(2020, 1, 1).unwrap(),
            ],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(r, format!("{prefix}/newName"));
}

// ---------- 13. SameMeaning ----------
#[tokio::test]
async fn ex_same_meaning() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("ex-sm");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "ex-sm").await;

    let en = client
        .assert(
            &StatementInput::new(
                format!("{prefix}/ada"),
                "rdfs:label",
                Object::Literal(Literal::lang_string("Ada Lovelace", "en")),
            )
            .with_context(&ctx),
        )
        .await
        .unwrap();
    let fr = client
        .assert(
            &StatementInput::new(
                format!("{prefix}/ada"),
                "rdfs:label",
                Object::Literal(Literal::lang_string("Ada Lovelace", "fr")),
            )
            .with_context(&ctx),
        )
        .await
        .unwrap();

    client.align_meaning(en, fr, &ctx, None).await.unwrap();

    let cluster = client.meaning_cluster(en, None).await.unwrap();
    assert!(cluster.contains(&en));
    assert!(cluster.contains(&fr));
}

// ---------- 14. Reactions ----------
#[tokio::test]
async fn ex_reactions() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("ex-react");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "ex-react").await;

    let claim = client
        .assert(
            &StatementInput::new(
                format!("{prefix}/ada"),
                "ex:invented",
                Object::Literal(Literal::string("programming")),
            )
            .with_context(&ctx),
        )
        .await
        .unwrap();

    // Alice endorses, Bob rejects
    let alice_ctx = format!("{prefix}/alice");
    let bob_ctx = format!("{prefix}/bob");
    client
        .ensure_context(&alice_ctx, "user", "permissive", None)
        .await
        .unwrap();
    client
        .ensure_context(&bob_ctx, "user", "permissive", None)
        .await
        .unwrap();

    client
        .react(claim, ReactionKind::Endorses, None, &alice_ctx, None)
        .await
        .unwrap();
    client
        .react(claim, ReactionKind::Rejects, None, &bob_ctx, None)
        .await
        .unwrap();

    let reactions = client.reactions_for(claim).await.unwrap();
    assert_eq!(reactions.len(), 2);
    assert!(reactions.iter().any(|r| r.kind == ReactionKind::Endorses));
    assert!(reactions.iter().any(|r| r.kind == ReactionKind::Rejects));
}

// ---------- 15. Shape annotations ----------
#[tokio::test]
async fn ex_shape_annotations() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("ex-shape");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "ex-shape").await;

    let s = client
        .assert(
            &StatementInput::new(
                format!("{prefix}/ada"),
                "ex:age",
                Object::Literal(Literal::string("not-a-number")),
            )
            .with_context(&ctx),
        )
        .await
        .unwrap();

    // Attach a violation — additive, does not mutate the statement
    client
        .attach_shape_report(
            s,
            "builtin:datatype/ex:age/xsd:integer",
            ShapeVerdict::Violate,
            &ctx,
            None,
        )
        .await
        .unwrap();

    assert!(client
        .has_shape_verdict(
            s,
            ShapeVerdict::Violate,
            Some("builtin:datatype/ex:age/xsd:integer")
        )
        .await
        .unwrap());
    assert!(!client
        .has_shape_verdict(
            s,
            ShapeVerdict::Pass,
            Some("builtin:datatype/ex:age/xsd:integer")
        )
        .await
        .unwrap());
}

// ---------- 16. Retrofit ----------
#[tokio::test]
async fn ex_retrofit() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("ex-retro");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "ex-retro").await;

    let s = client
        .assert_retrofit(
            &StatementInput::new(
                format!("{prefix}/ada"),
                "ex:residence",
                Object::Literal(Literal::string("London")),
            )
            .with_context(&ctx)
            .with_valid(
                Some(NaiveDate::from_ymd_opt(1835, 1, 1).unwrap()),
                Some(NaiveDate::from_ymd_opt(1852, 1, 1).unwrap()),
            ),
            "Newly discovered letter confirms London residence",
            Some("archivist"),
        )
        .await
        .unwrap();

    // Verify the reason is stored
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let reason: String = c
        .query_one(
            "select retrofit_reason from donto_retrofit where statement_id = $1",
            &[&s],
        )
        .await
        .unwrap()
        .get(0);
    assert!(reason.contains("Newly discovered"));
}

// ---------- 17. Snapshots ----------
#[tokio::test]
async fn ex_snapshots() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let prefix = tag("ex-snap");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "ex-snap").await;

    let s = client
        .assert(
            &StatementInput::new(format!("{prefix}/ada"), "ex:p", Object::iri("ex:o"))
                .with_context(&ctx),
        )
        .await
        .unwrap();

    let snap_iri = format!("{prefix}/snapshot-v1");
    let scope = json!({"include": [ctx]});
    c.execute(
        "select donto_snapshot_create($1, $2::jsonb)",
        &[&snap_iri.as_str(), &scope],
    )
    .await
    .unwrap();

    // Retract the original statement
    client.retract(s).await.unwrap();

    // Snapshot still has it
    let in_snap: i64 = c
        .query_one(
            "select count(*) from donto_match_in_snapshot($1, null, null, null, 'asserted', 0)",
            &[&snap_iri.as_str()],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(in_snap, 1, "snapshot membership survives retraction");

    // Live view doesn't
    let live = client
        .match_pattern(
            Some(&format!("{prefix}/ada")),
            Some("ex:p"),
            None,
            None,
            None,
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(live.len(), 0);
}

// ---------- 18. Hypothesis contexts ----------
#[tokio::test]
async fn ex_hypothesis() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("ex-hypo");
    cleanup_prefix(&client, &prefix).await;

    let base_ctx = format!("{prefix}/base");
    let hypo_ctx = format!("{prefix}/hypothesis/what-if");
    client
        .ensure_context(&base_ctx, "source", "permissive", None)
        .await
        .unwrap();
    client
        .ensure_context(&hypo_ctx, "hypothesis", "permissive", Some(&base_ctx))
        .await
        .unwrap();

    // Base fact
    client
        .assert(
            &StatementInput::new(
                format!("{prefix}/ada"),
                "ex:child",
                Object::Literal(Literal::string("Byron King-Noel")),
            )
            .with_context(&base_ctx),
        )
        .await
        .unwrap();

    // Hypothetical counterfactual
    client
        .assert(
            &StatementInput::new(
                format!("{prefix}/ada"),
                "ex:child",
                Object::Literal(Literal::string("Hypothetical-Child")),
            )
            .with_context(&hypo_ctx),
        )
        .await
        .unwrap();

    // Scope under hypothesis sees both (base + hypothesis)
    let hypo_scope = donto_client::ContextScope::just(&hypo_ctx).with_ancestors();
    let hypo_results = client
        .match_pattern(
            Some(&format!("{prefix}/ada")),
            Some("ex:child"),
            None,
            Some(&hypo_scope),
            None,
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        hypo_results.len(),
        2,
        "hypothesis scope includes base + hypothesis"
    );

    // Base scope doesn't see the hypothesis
    let base_scope = donto_client::ContextScope::just(&base_ctx).without_descendants();
    let base_results = client
        .match_pattern(
            Some(&format!("{prefix}/ada")),
            Some("ex:child"),
            None,
            Some(&base_scope),
            None,
            0,
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(base_results.len(), 1);
}

// ---------- 19. Valid-time bucketing ----------
#[tokio::test]
async fn ex_valid_time_buckets() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("ex-vtb");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "ex-vtb").await;

    for year in [1835, 1836, 1836, 1840] {
        client
            .assert(
                &StatementInput::new(
                    format!("{prefix}/ada"),
                    "ex:event",
                    Object::Literal(Literal::string(format!(
                        "event-{year}-{}",
                        uuid::Uuid::new_v4().simple()
                    ))),
                )
                .with_context(&ctx)
                .with_valid(Some(NaiveDate::from_ymd_opt(year, 6, 1).unwrap()), None),
            )
            .await
            .unwrap();
    }

    let buckets = client
        .valid_time_buckets(
            "1 year",
            NaiveDate::from_ymd_opt(1835, 1, 1).unwrap(),
            Some("ex:event"),
            Some(&format!("{prefix}/ada")),
            None,
        )
        .await
        .unwrap();
    assert!(!buckets.is_empty());
    // 1836 has 2 events
    let b1836 = buckets.iter().find(|b| b.bucket_start.year() == 1836);
    assert!(b1836.is_some());
    assert_eq!(b1836.unwrap().count, 2);
}

// ---------- 20. Context environment overlays ----------
#[tokio::test]
async fn ex_context_env() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("ex-env");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "ex-env").await;

    client
        .context_env_set(&ctx, "location", &json!("London"), None)
        .await
        .unwrap();
    client
        .context_env_set(&ctx, "era", &json!("Victorian"), None)
        .await
        .unwrap();

    let loc = client.context_env_get(&ctx, "location").await.unwrap();
    assert_eq!(loc.unwrap(), json!("London"));

    // Find contexts with specific env
    let matched = client
        .contexts_with_env(&json!({"location": "London", "era": "Victorian"}))
        .await
        .unwrap();
    assert!(matched.contains(&ctx));
}

// ---------- 21. Endorsement weights ----------
#[tokio::test]
async fn ex_endorsement_weights() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("ex-weight");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "ex-weight").await;

    let claim = client
        .assert(
            &StatementInput::new(
                format!("{prefix}/ada"),
                "ex:claim",
                Object::iri("ex:wrote-first-program"),
            )
            .with_context(&ctx),
        )
        .await
        .unwrap();

    // 3 endorsements, 1 rejection -> weight = 2
    for i in 0..3 {
        let c = format!("{prefix}/endorser-{i}");
        client
            .ensure_context(&c, "user", "permissive", None)
            .await
            .unwrap();
        client
            .react(claim, ReactionKind::Endorses, None, &c, None)
            .await
            .unwrap();
    }
    let rej_ctx = format!("{prefix}/rejector");
    client
        .ensure_context(&rej_ctx, "user", "permissive", None)
        .await
        .unwrap();
    client
        .react(claim, ReactionKind::Rejects, None, &rej_ctx, None)
        .await
        .unwrap();

    let weight = client.weight_of(claim, None).await.unwrap();
    assert_eq!(weight, 2, "3 endorsements - 1 rejection = weight 2");
}

// ---------- 22-31. Evidence substrate (documents -> vectors) ----------

#[tokio::test]
async fn ex_documents_and_revisions() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("ex-doc");

    let doc_iri = format!("test:doc/{prefix}/lovelace-letter");
    let doc_id = client
        .ensure_document(
            &doc_iri,
            "text/plain",
            Some("Ada's 1843 letter to Babbage"),
            Some("https://example.com/letters/1843"),
            Some("en"),
        )
        .await
        .unwrap();

    // Idempotent
    let doc_id2 = client
        .ensure_document(&doc_iri, "text/plain", None, None, None)
        .await
        .unwrap();
    assert_eq!(doc_id, doc_id2);

    // Add revisions
    let r1 = client
        .add_revision(
            doc_id,
            Some("Dear Mr. Babbage, I have been working on the Engine notes..."),
            None,
            Some("manual-v1"),
        )
        .await
        .unwrap();
    let r2 = client
        .add_revision(
            doc_id,
            Some("Dear Mr. Babbage, I have been working on the Analytical Engine notes and have devised an algorithm..."),
            None,
            Some("ocr-tesseract-5.3"),
        )
        .await
        .unwrap();
    assert_ne!(r1, r2, "different content -> different revision");

    // Latest revision
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let latest: uuid::Uuid = c
        .query_one("select donto_latest_revision($1)", &[&doc_id])
        .await
        .unwrap()
        .get(0);
    assert_eq!(latest, r2);
}

#[tokio::test]
async fn ex_spans_and_annotations() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("ex-span");

    let doc_id = client
        .ensure_document(
            &format!("test:doc/{prefix}"),
            "text/plain",
            None,
            None,
            Some("en"),
        )
        .await
        .unwrap();
    let rev_id = client
        .add_revision(
            doc_id,
            Some("Ada Lovelace invented the first algorithm for the Analytical Engine in 1843."),
            None,
            None,
        )
        .await
        .unwrap();

    // Create spans for key entities
    let span_ada = client
        .create_char_span(rev_id, 0, 13, Some("Ada Lovelace"))
        .await
        .unwrap();
    let span_algo = client
        .create_char_span(rev_id, 28, 43, Some("first algorithm"))
        .await
        .unwrap();
    let span_engine = client
        .create_char_span(rev_id, 52, 71, Some("Analytical Engine"))
        .await
        .unwrap();
    let span_date = client
        .create_char_span(rev_id, 75, 79, Some("1843"))
        .await
        .unwrap();

    // Annotation space for NER
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let ner_space: uuid::Uuid = c
        .query_one(
            "select donto_ensure_annotation_space($1, $2, $3)",
            &[
                &format!("test:space/{prefix}/ner"),
                &"NER",
                &"standard-ner-v1",
            ],
        )
        .await
        .unwrap()
        .get(0);

    // Annotate spans with NER labels
    for (span, label) in [
        (span_ada, "PERSON"),
        (span_algo, "ARTIFACT"),
        (span_engine, "ARTIFACT"),
        (span_date, "DATE"),
    ] {
        c.execute(
            "select donto_annotate_span($1, $2, $3, $4, $5, $6)",
            &[
                &span,
                &ner_space,
                &"ner_label",
                &label,
                &Option::<serde_json::Value>::None,
                &0.95f64,
            ],
        )
        .await
        .unwrap();
    }

    // Query annotations
    let ann_count: i64 = c
        .query_one(
            "select count(*) from donto_annotations_for_span($1, $2, $3)",
            &[&span_ada, &ner_space, &"ner_label"],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(ann_count, 1);

    // Annotation edge: dependency arc from "Ada" to "algorithm"
    let ann_ada: uuid::Uuid = c
        .query_one(
            "select donto_annotate_span($1, $2, $3, $4)",
            &[&span_ada, &ner_space, &"role", &"agent"],
        )
        .await
        .unwrap()
        .get(0);
    let ann_algo: uuid::Uuid = c
        .query_one(
            "select donto_annotate_span($1, $2, $3, $4)",
            &[&span_algo, &ner_space, &"role", &"patient"],
        )
        .await
        .unwrap()
        .get(0);
    c.execute(
        "select donto_link_annotations($1, $2, $3, $4)",
        &[&ann_ada, &ann_algo, &ner_space, &"agent-of"],
    )
    .await
    .unwrap();

    let edges: i64 = c
        .query_one("select count(*) from donto_edges_from($1)", &[&ann_ada])
        .await
        .unwrap()
        .get(0);
    assert_eq!(edges, 1);
}

#[tokio::test]
async fn ex_extraction_run_with_evidence() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("ex-extract");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "ex-extract").await;

    // Document -> revision
    let doc_id = client
        .ensure_document(
            &format!("test:doc/{prefix}"),
            "text/plain",
            None,
            None,
            None,
        )
        .await
        .unwrap();
    let rev_id = client
        .add_revision(
            doc_id,
            Some("Ada Lovelace wrote the first algorithm."),
            None,
            None,
        )
        .await
        .unwrap();

    // Start extraction
    let run_id = client
        .start_extraction(
            Some("claude-sonnet-4-6"),
            Some("20250514"),
            Some(rev_id),
            Some(&ctx),
        )
        .await
        .unwrap();

    // Create a span and extract a statement from it
    let span_id = client
        .create_char_span(rev_id, 0, 13, Some("Ada Lovelace"))
        .await
        .unwrap();
    let stmt_id = client
        .assert(
            &StatementInput::new(
                format!("{prefix}/ada"),
                "rdf:type",
                Object::iri("foaf:Person"),
            )
            .with_context(&ctx),
        )
        .await
        .unwrap();

    // Link statement to evidence
    client
        .link_evidence_span(stmt_id, span_id, "extracted_from", Some(0.95), Some(&ctx))
        .await
        .unwrap();
    client
        .link_evidence_run(stmt_id, run_id, "produced_by", Some(&ctx))
        .await
        .unwrap();

    // Complete extraction
    client
        .complete_extraction(run_id, "completed", Some(1), Some(0))
        .await
        .unwrap();

    // Verify evidence chain
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let ev_count: i64 = c
        .query_one("select count(*) from donto_evidence_for($1)", &[&stmt_id])
        .await
        .unwrap()
        .get(0);
    assert_eq!(ev_count, 2, "statement has both span and run evidence");
}

#[tokio::test]
async fn ex_agents_and_workspaces() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("ex-agent");
    let ctx = ctx(&client, "ex-agent").await;

    // Register agents
    let claude = client
        .ensure_agent(
            &format!("{prefix}/claude"),
            "llm",
            Some("Claude"),
            Some("claude-sonnet-4-6"),
        )
        .await
        .unwrap();
    let alice = client
        .ensure_agent(&format!("{prefix}/alice"), "human", Some("Alice"), None)
        .await
        .unwrap();

    // Bind to workspace
    client
        .bind_agent_context(claude, &ctx, "contributor")
        .await
        .unwrap();
    client
        .bind_agent_context(alice, &ctx, "owner")
        .await
        .unwrap();

    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let agents: i64 = c
        .query_one("select count(*) from donto_context_agents($1)", &[&ctx])
        .await
        .unwrap()
        .get(0);
    assert_eq!(agents, 2);
}

#[tokio::test]
async fn ex_arguments_and_contradiction_frontier() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("ex-argue");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "ex-argue").await;

    // Claim: Ada wrote the first program
    let claim = client
        .assert(
            &StatementInput::new(
                format!("{prefix}/ada"),
                "ex:wrote",
                Object::iri("ex:first-program"),
            )
            .with_context(&ctx),
        )
        .await
        .unwrap();

    // Counter-claim: Babbage wrote it
    let counter = client
        .assert(
            &StatementInput::new(
                format!("{prefix}/babbage"),
                "ex:wrote",
                Object::iri("ex:first-program"),
            )
            .with_context(&ctx),
        )
        .await
        .unwrap();

    // Supporting evidence
    let support = client
        .assert(
            &StatementInput::new(
                format!("{prefix}/note-g"),
                "ex:contains",
                Object::iri("ex:bernoulli-algorithm"),
            )
            .with_context(&ctx),
        )
        .await
        .unwrap();

    // Wire up arguments
    client
        .assert_argument(counter, claim, "rebuts", &ctx, Some(0.7), None, None)
        .await
        .unwrap();
    client
        .assert_argument(support, claim, "supports", &ctx, Some(0.9), None, None)
        .await
        .unwrap();

    // Contradiction frontier
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let rows = c
        .query("select * from donto_contradiction_frontier($1)", &[&ctx])
        .await
        .unwrap();

    let claim_row = rows
        .iter()
        .find(|r| r.get::<_, uuid::Uuid>("statement_id") == claim);
    assert!(
        claim_row.is_some(),
        "claim under attack appears in frontier"
    );
    assert_eq!(claim_row.unwrap().get::<_, i64>("attack_count"), 1);
    assert_eq!(claim_row.unwrap().get::<_, i64>("support_count"), 1);
}

#[tokio::test]
async fn ex_proof_obligations() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("ex-oblig");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "ex-oblig").await;

    let stmt = client
        .assert(
            &StatementInput::new(
                format!("{prefix}/ada"),
                "ex:knows",
                Object::Literal(Literal::string("Charles Babbage")),
            )
            .with_context(&ctx),
        )
        .await
        .unwrap();

    // Extraction couldn't resolve the entity
    let obl = client
        .emit_obligation(stmt, "needs-entity-disambiguation", &ctx, 5, None, None)
        .await
        .unwrap();

    // Assign to an agent
    let agent = client
        .ensure_agent(&format!("{prefix}/resolver"), "llm", None, None)
        .await
        .unwrap();
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    c.execute("select donto_assign_obligation($1, $2)", &[&obl, &agent])
        .await
        .unwrap();

    // Check it's in_progress
    let status: String = c
        .query_one(
            "select status from donto_proof_obligation where obligation_id = $1",
            &[&obl],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(status, "in_progress");

    // Resolve it
    client
        .resolve_obligation(obl, Some(agent), "resolved")
        .await
        .unwrap();

    // Summary shows the resolution
    let rows = c
        .query(
            "select obligation_type, status, cnt from donto_obligation_summary($1)",
            &[&ctx],
        )
        .await
        .unwrap();
    let resolved = rows.iter().find(|r| {
        r.get::<_, String>("obligation_type") == "needs-entity-disambiguation"
            && r.get::<_, String>("status") == "resolved"
    });
    assert!(resolved.is_some());
}

#[tokio::test]
async fn ex_vectors_and_similarity() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("ex-vec");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "ex-vec").await;

    // Three statements with embeddings
    let texts_and_embeddings: Vec<(&str, Vec<f32>)> = vec![
        ("Ada was a mathematician", vec![0.9, 0.1, 0.0, 0.0]),
        ("Ada was a writer", vec![0.7, 0.3, 0.0, 0.0]),
        ("Babbage built engines", vec![0.1, 0.0, 0.9, 0.1]),
    ];

    let model_id = format!("test-embed-{}", uuid::Uuid::new_v4().simple());
    let mut ids = Vec::new();
    for (text, emb) in &texts_and_embeddings {
        let id = client
            .assert(
                &StatementInput::new(
                    format!("{prefix}/s"),
                    "ex:desc",
                    Object::Literal(Literal::string(*text)),
                )
                .with_context(&ctx),
            )
            .await
            .unwrap();
        client
            .store_vector("statement", id, &model_id, Some("v1"), emb)
            .await
            .unwrap();
        ids.push(id);
    }

    // Nearest to "mathematician" embedding
    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let query_vec: Vec<f32> = vec![0.9, 0.1, 0.0, 0.0];
    let rows = c
        .query(
            "select subject_id, similarity \
             from donto_nearest_vectors('statement', $2, $1::float4[], 3)",
            &[&query_vec, &model_id.as_str()],
        )
        .await
        .unwrap();

    assert!(rows.len() >= 2);
    // First result should be the exact match
    let first_id: uuid::Uuid = rows[0].get("subject_id");
    assert_eq!(first_id, ids[0]);
    let sim: f64 = rows[0].get("similarity");
    assert!((sim - 1.0).abs() < 1e-6, "exact match -> similarity 1.0");
}

// ---------- 32. Idempotent re-assert ----------
#[tokio::test]
async fn ex_idempotent_reassert() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("ex-idem");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "ex-idem").await;

    let input = StatementInput::new(
        format!("{prefix}/ada"),
        "ex:born",
        Object::Literal(Literal::integer(1815)),
    )
    .with_context(&ctx);

    let id1 = client.assert(&input).await.unwrap();
    let id2 = client.assert(&input).await.unwrap();
    assert_eq!(id1, id2, "re-asserting identical content returns same id");
}

// ---------- 33. Audit log ----------
#[tokio::test]
async fn ex_audit_log() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("ex-audit");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "ex-audit").await;

    let s = client
        .assert(
            &StatementInput::new(format!("{prefix}/ada"), "ex:p", Object::iri("ex:o"))
                .with_context(&ctx),
        )
        .await
        .unwrap();
    client.retract(s).await.unwrap();

    let pool = client.pool();
    let c = pool.get().await.unwrap();
    let actions: Vec<String> = c
        .query(
            "select action from donto_audit where statement_id = $1 order by at",
            &[&s],
        )
        .await
        .unwrap()
        .iter()
        .map(|r| r.get(0))
        .collect();
    assert!(actions.contains(&"assert".to_string()));
    assert!(actions.contains(&"retract".to_string()));
}

// ---------- 34. Curated context rejects unregistered predicates ----------
#[tokio::test]
async fn ex_curated_context() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("ex-curated");
    cleanup_prefix(&client, &prefix).await;

    let curated = common::curated_ctx(&client, "ex-curated").await;

    // Unregistered predicate is rejected in curated context
    let err = client
        .assert(
            &StatementInput::new(
                format!("{prefix}/ada"),
                "ex:unregistered-pred-12345",
                Object::iri("ex:o"),
            )
            .with_context(&curated),
        )
        .await
        .err()
        .expect("curated context must reject unregistered predicate");
    let msg = format!("{err:?}");
    assert!(msg.contains("not registered"), "error: {msg}");
}
