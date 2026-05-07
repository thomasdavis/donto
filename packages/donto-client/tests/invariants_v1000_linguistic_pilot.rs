//! v1000 linguistic pilot: end-to-end exercises of the language-pilot
//! frame types (PRD §13.4) and supporting machinery.

use donto_client::{Object, StatementInput};
use serde_json::json;

mod common;
use common::{cleanup_prefix, connect, ctx, tag};

async fn make_frame(c: &deadpool_postgres::Object, frame_type: &str, ctx_iri: &str) -> uuid::Uuid {
    c.query_one(
        "select donto_create_claim_frame($1, $2)",
        &[&frame_type, &ctx_iri],
    )
    .await
    .unwrap()
    .get(0)
}

#[tokio::test]
async fn phoneme_inventory_frame_with_segments() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("ling-phon");
    let ctx_iri = ctx(&client, "ling-phon").await;
    let frame_id = make_frame(&c, "phoneme_inventory", &ctx_iri).await;

    c.execute(
        "select donto_add_frame_role($1, 'language_variety', 'entity', $2, null)",
        &[&frame_id, &format!("lang:{prefix}/x")],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_frame_role($1, 'phonemes', 'literal', null, $2)",
        &[&frame_id, &json!(["p", "t", "k", "m", "n", "ŋ"])],
    )
    .await
    .unwrap();

    let valid: bool = c
        .query_one(
            "select donto_validate_frame_roles('phoneme_inventory', \
                array['language_variety','phonemes']::text[])",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(valid);
}

#[tokio::test]
async fn allomorphy_rule_frame() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("ling-allo");
    let ctx_iri = ctx(&client, "ling-allo").await;
    let frame_id = make_frame(&c, "allomorphy_rule", &ctx_iri).await;

    let morpheme = format!("morph:{prefix}/LOC");
    c.execute(
        "select donto_add_frame_role($1, 'morpheme', 'entity', $2, null)",
        &[&frame_id, &morpheme],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_frame_role($1, 'allomorph', 'literal', null, $2)",
        &[&frame_id, &json!({"text": "-ngka"})],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_frame_role($1, 'environment', 'expression', null, null)",
        &[&frame_id],
    )
    .await
    .unwrap();

    let rows = c
        .query("select role from donto_frame_roles($1)", &[&frame_id])
        .await
        .unwrap();
    assert_eq!(rows.len(), 3);
}

#[tokio::test]
async fn paradigm_cell_frame() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let ctx_iri = ctx(&client, "ling-paradigm").await;
    let frame_id = make_frame(&c, "paradigm_cell", &ctx_iri).await;

    c.execute(
        "select donto_add_frame_role($1, 'lexeme', 'entity', 'lex:dormir', null)",
        &[&frame_id],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_frame_role($1, 'features', 'literal', null, $2)",
        &[
            &frame_id,
            &json!({"tense": "PST", "person": 3, "number": "SG"}),
        ],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_frame_role($1, 'form', 'literal', null, $2)",
        &[&frame_id, &json!({"text": "durmió"})],
    )
    .await
    .unwrap();

    let valid: bool = c
        .query_one(
            "select donto_validate_frame_roles('paradigm_cell', \
                array['lexeme','features','form']::text[])",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(valid);
}

#[tokio::test]
async fn interlinear_example_frame() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let ctx_iri = ctx(&client, "ling-igt").await;
    let frame_id = make_frame(&c, "interlinear_example", &ctx_iri).await;

    c.execute(
        "select donto_add_frame_role($1, 'vernacular', 'literal', null, $2)",
        &[&frame_id, &json!({"text": "wungar bama-nga"})],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_frame_role($1, 'gloss', 'literal', null, $2)",
        &[&frame_id, &json!({"text": "walk man-ABL"})],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_frame_role($1, 'translation', 'literal', null, $2)",
        &[&frame_id, &json!({"text": "(s)he walks from the man"})],
    )
    .await
    .unwrap();

    let rows = c
        .query("select role from donto_frame_roles($1)", &[&frame_id])
        .await
        .unwrap();
    let roles: Vec<String> = rows.iter().map(|r| r.get(0)).collect();
    assert!(roles.contains(&"vernacular".to_string()));
    assert!(roles.contains(&"gloss".to_string()));
    assert!(roles.contains(&"translation".to_string()));
}

#[tokio::test]
async fn valency_frame_with_arguments() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let ctx_iri = ctx(&client, "ling-valency").await;
    let frame_id = make_frame(&c, "valency_frame", &ctx_iri).await;

    c.execute(
        "select donto_add_frame_role($1, 'verb', 'entity', 'lex:see', null)",
        &[&frame_id],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_frame_role($1, 'arguments', 'literal', null, $2)",
        &[
            &frame_id,
            &json!([
                {"role": "agent", "case": "ERG"},
                {"role": "patient", "case": "ABS"}
            ]),
        ],
    )
    .await
    .unwrap();

    let valid: bool = c
        .query_one(
            "select donto_validate_frame_roles('valency_frame', \
                array['verb','arguments']::text[])",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(valid);
}

#[tokio::test]
async fn dialect_variant_frame() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let ctx_iri = ctx(&client, "ling-dialect").await;
    let frame_id = make_frame(&c, "dialect_variant", &ctx_iri).await;

    c.execute(
        "select donto_add_frame_role($1, 'language_variety', 'entity', 'lang:kuya/dialect/yalanji', null)",
        &[&frame_id],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_frame_role($1, 'feature', 'entity', 'feat:case-allomorphy', null)",
        &[&frame_id],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_frame_role($1, 'variant', 'literal', null, $2)",
        &[&frame_id, &json!({"realisation": "-ngka"})],
    )
    .await
    .unwrap();

    let rows = c
        .query("select role from donto_frame_roles($1)", &[&frame_id])
        .await
        .unwrap();
    assert_eq!(rows.len(), 3);
}

#[tokio::test]
async fn language_identity_hypothesis_frame() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let ctx_iri = ctx(&client, "ling-id-hyp").await;
    let frame_id = make_frame(&c, "language_identity_hypothesis", &ctx_iri).await;

    c.execute(
        "select donto_add_frame_role($1, 'candidate_a', 'entity', 'lang:abcd1234', null)",
        &[&frame_id],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_frame_role($1, 'candidate_b', 'entity', 'lang:efgh5678', null)",
        &[&frame_id],
    )
    .await
    .unwrap();

    let valid: bool = c
        .query_one(
            "select donto_validate_frame_roles('language_identity_hypothesis', \
                array['candidate_a','candidate_b']::text[])",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(valid);
}

#[tokio::test]
async fn language_variety_with_external_ids() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("ling-lang");
    let id: i64 = c
        .query_one(
            "select donto_ensure_symbol($1, 'language_variety', 'Test Language', null, null, null)",
            &[&format!("lang:{prefix}/test")],
        )
        .await
        .unwrap()
        .get(0);

    c.execute(
        "update donto_entity_symbol set entity_kind = 'language_variety', identity_status = 'provisional' where symbol_id = $1",
        &[&id],
    )
    .await
    .unwrap();

    let glot = format!("g-{}", uuid::Uuid::new_v4().simple());
    let iso = format!("i-{}", uuid::Uuid::new_v4().simple());
    c.execute(
        "select donto_add_external_id($1, 'glottolog', $2, 1.0)",
        &[&id, &glot],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_external_id($1, 'iso639-3', $2, 1.0)",
        &[&id, &iso],
    )
    .await
    .unwrap();

    let kind: String = c
        .query_one(
            "select entity_kind from donto_entity_symbol where symbol_id = $1",
            &[&id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(kind, "language_variety");

    let resolved: Option<i64> = c
        .query_one(
            "select donto_symbol_by_external_id('glottolog', $1)",
            &[&glot],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(resolved, Some(id));
}

#[tokio::test]
async fn provisional_variety_for_uncertain_id() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("ling-prov");
    let id: i64 = c
        .query_one(
            "select donto_ensure_symbol($1, 'language_variety', 'Provisional Variety', null, null, null)",
            &[&format!("lang:provisional/{prefix}")],
        )
        .await
        .unwrap()
        .get(0);
    c.execute(
        "update donto_entity_symbol set entity_kind = 'language_variety', identity_status = 'provisional' where symbol_id = $1",
        &[&id],
    )
    .await
    .unwrap();
    let status: String = c
        .query_one(
            "select identity_status from donto_entity_symbol where symbol_id = $1",
            &[&id],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(status, "provisional");
}

#[tokio::test]
async fn split_candidate_identity_proposal() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("ling-split");
    let refs = vec![
        format!("lang:{prefix}/before-split"),
        format!("lang:{prefix}/after-split-a"),
        format!("lang:{prefix}/after-split-b"),
    ];
    let id: uuid::Uuid = c
        .query_one(
            "select donto_register_identity_proposal('split_candidate', $1::text[])",
            &[&refs],
        )
        .await
        .unwrap()
        .get(0);
    let row = c
        .query_one(
            "select hypothesis_kind, cardinality(entity_refs) from donto_identity_proposal where proposal_id = $1",
            &[&id],
        )
        .await
        .unwrap();
    let (kind, n): (String, i32) = (row.get(0), row.get(1));
    assert_eq!(kind, "split_candidate");
    assert_eq!(n, 3);
}

#[tokio::test]
async fn cross_schema_alignment_with_value_mapping() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("ling-aln");
    let aln: uuid::Uuid = c
        .query_one(
            "select donto_register_alignment($1, $2, 'has_value_mapping', 0.9)",
            &[
                &format!("ex:{prefix}/wals_98"),
                &format!("ex:{prefix}/grambank_GBxxx"),
            ],
        )
        .await
        .unwrap()
        .get(0);
    c.execute(
        "select donto_register_value_mapping($1, '1', 'present', 1.0, null)",
        &[&aln],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_register_value_mapping($1, '0', 'absent', 1.0, null)",
        &[&aln],
    )
    .await
    .unwrap();
    let n: i64 = c
        .query_one(
            "select count(*) from donto_alignment_value_mapping where alignment_id = $1",
            &[&aln],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 2);
}

#[tokio::test]
async fn cross_source_disagreement_about_grammatical_feature() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("ling-disagree");
    cleanup_prefix(&client, &prefix).await;

    let ctx_a = ctx(&client, "ling-disagree-grammar-a").await;
    let ctx_b = ctx(&client, "ling-disagree-grammar-b").await;

    let lang = format!("lang:{prefix}/test");
    let pred = "ex:hasErgativeMarking";

    let s1 = client
        .assert(
            &StatementInput::new(lang.clone(), pred, Object::iri("ex:value/yes"))
                .with_context(&ctx_a),
        )
        .await
        .unwrap();
    let s2 = client
        .assert(
            &StatementInput::new(lang.clone(), pred, Object::iri("ex:value/no"))
                .with_context(&ctx_b),
        )
        .await
        .unwrap();

    c.execute("select donto_set_modality($1, 'descriptive')", &[&s1])
        .await
        .unwrap();
    c.execute("select donto_set_modality($1, 'descriptive')", &[&s2])
        .await
        .unwrap();

    // Both rows survive (paraconsistency).
    let n: i64 = c
        .query_one(
            "select count(*) from donto_statement \
             where subject = $1 and predicate = $2 and upper(tx_time) is null",
            &[&lang, &pred],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 2);
}

#[tokio::test]
async fn lexeme_with_external_concept_alignment() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("ling-lex");
    let lex: i64 = c
        .query_one(
            "select donto_ensure_symbol($1, 'lexeme', 'wungar', null, null, null)",
            &[&format!("lex:{prefix}/wungar")],
        )
        .await
        .unwrap()
        .get(0);
    c.execute(
        "update donto_entity_symbol set entity_kind = 'lexeme' where symbol_id = $1",
        &[&lex],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_entity_label($1, 'wungar', null, 'Latn', 'preferred')",
        &[&lex],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_entity_label($1, 'walk', 'en', 'Latn', 'alternate')",
        &[&lex],
    )
    .await
    .unwrap();

    let n: i64 = c
        .query_one(
            "select count(*) from donto_entity_label where symbol_id = $1",
            &[&lex],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n, 2);
}

#[tokio::test]
async fn frame_filter_by_role_value_finds_constructions() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("ling-rev");
    let ctx_iri = ctx(&client, "ling-rev").await;
    let lex_a = format!("lex:{prefix}/A");

    for _ in 0..3 {
        let frame_id: uuid::Uuid = make_frame(&c, "paradigm_cell", &ctx_iri).await;
        c.execute(
            "select donto_add_frame_role($1, 'lexeme', 'entity', $2, null)",
            &[&frame_id, &lex_a],
        )
        .await
        .unwrap();
    }
    let rows = c
        .query(
            "select frame_id from donto_frames_with_role_value('lexeme', $1, 100)",
            &[&lex_a],
        )
        .await
        .unwrap();
    assert_eq!(rows.len(), 3);
}

#[tokio::test]
async fn corpus_token_annotation_frame() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let ctx_iri = ctx(&client, "ling-corpus-tok").await;
    let frame_id = make_frame(&c, "corpus_token_annotation", &ctx_iri).await;

    c.execute(
        "select donto_add_frame_role($1, 'token_id', 'literal', null, $2)",
        &[&frame_id, &json!("tok-001")],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_frame_role($1, 'sentence_id', 'literal', null, $2)",
        &[&frame_id, &json!("sent-001")],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_frame_role($1, 'annotation', 'literal', null, $2)",
        &[
            &frame_id,
            &json!({"upos": "NOUN", "feats": {"Case": "Erg"}}),
        ],
    )
    .await
    .unwrap();

    let valid: bool = c
        .query_one(
            "select donto_validate_frame_roles('corpus_token_annotation', \
                array['token_id','sentence_id','annotation']::text[])",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(valid);
}

#[tokio::test]
async fn dependency_edge_frame() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let ctx_iri = ctx(&client, "ling-dep").await;
    let frame_id = make_frame(&c, "dependency_edge", &ctx_iri).await;

    c.execute(
        "select donto_add_frame_role($1, 'head', 'literal', null, $2)",
        &[&frame_id, &json!("tok-002")],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_frame_role($1, 'dependent', 'literal', null, $2)",
        &[&frame_id, &json!("tok-001")],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_frame_role($1, 'relation', 'literal', null, $2)",
        &[&frame_id, &json!("nsubj")],
    )
    .await
    .unwrap();

    let valid: bool = c
        .query_one(
            "select donto_validate_frame_roles('dependency_edge', \
                array['head','dependent','relation']::text[])",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(valid);
}

#[tokio::test]
async fn schema_mapping_cross_domain_frame() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let ctx_iri = ctx(&client, "ling-schema-map").await;
    let frame_id = make_frame(&c, "schema_mapping", &ctx_iri).await;

    c.execute(
        "select donto_add_frame_role($1, 'left', 'entity', 'wals:Feature98', null)",
        &[&frame_id],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_frame_role($1, 'right', 'entity', 'grambank:GBxxx', null)",
        &[&frame_id],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_frame_role($1, 'relation', 'literal', null, $2)",
        &[&frame_id, &json!("close_match")],
    )
    .await
    .unwrap();

    let valid: bool = c
        .query_one(
            "select donto_validate_frame_roles('schema_mapping', \
                array['left','right','relation']::text[])",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(valid);
}

#[tokio::test]
async fn end_to_end_linguistic_workflow() {
    let client = pg_or_skip!(connect().await);
    let c = client.pool().get().await.unwrap();
    let prefix = tag("ling-e2e");
    cleanup_prefix(&client, &prefix).await;

    // 1. Register the source (a grammar PDF).
    let src = format!("src:{prefix}/grammar.pdf");
    c.execute(
        "select donto_register_source_v1000($1, 'pdf', 'policy:default/public')",
        &[&src],
    )
    .await
    .unwrap();

    // 2. Register the language variety.
    let lang_id: i64 = c
        .query_one(
            "select donto_ensure_symbol($1, 'language_variety', 'Test Language', null, null, null)",
            &[&format!("lang:{prefix}/main")],
        )
        .await
        .unwrap()
        .get(0);
    c.execute(
        "update donto_entity_symbol set entity_kind = 'language_variety', identity_status = 'provisional' \
         where symbol_id = $1",
        &[&lang_id],
    )
    .await
    .unwrap();

    // 3. Build a phoneme inventory frame.
    let ctx_iri = ctx(&client, "ling-e2e-ling").await;
    let phon_frame: uuid::Uuid = c
        .query_one(
            "select donto_create_claim_frame('phoneme_inventory', $1)",
            &[&ctx_iri],
        )
        .await
        .unwrap()
        .get(0);
    c.execute(
        "select donto_add_frame_role($1, 'language_variety', 'entity', $2, null)",
        &[&phon_frame, &format!("lang:{prefix}/main")],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_frame_role($1, 'phonemes', 'literal', null, $2)",
        &[&phon_frame, &json!(["p", "t", "k", "m", "n"])],
    )
    .await
    .unwrap();

    // 4. Build an IGT example frame.
    let igt: uuid::Uuid = c
        .query_one(
            "select donto_create_claim_frame('interlinear_example', $1)",
            &[&ctx_iri],
        )
        .await
        .unwrap()
        .get(0);
    c.execute(
        "select donto_add_frame_role($1, 'vernacular', 'literal', null, $2)",
        &[&igt, &json!({"text": "wungar bama"})],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_frame_role($1, 'gloss', 'literal', null, $2)",
        &[&igt, &json!({"text": "walk man"})],
    )
    .await
    .unwrap();
    c.execute(
        "select donto_add_frame_role($1, 'translation', 'literal', null, $2)",
        &[&igt, &json!({"text": "the man walks"})],
    )
    .await
    .unwrap();

    // 5. Mint a linguistic predicate.
    let pred = format!("ex:{prefix}/hasMorpheme");
    c.query_one(
        "select donto_mint_predicate_candidate($1, 'hasMorpheme', \
            'Subject lexeme contains object morpheme.', \
            'Lexeme', 'Morpheme', 'linguistics', $2, $3, 'donto-native', \
            'tester', null, null)",
        &[
            &pred,
            &json!([{"subject": "lex:wungar", "object": "morph:walk"}]),
            &json!([]),
        ],
    )
    .await
    .unwrap();
    c.query_one("select donto_approve_predicate($1, 'reviewer:1')", &[&pred])
        .await
        .unwrap();

    // 6. Reviewer accepts the IGT frame.
    c.query_one(
        "select donto_record_review('frame', $1, 'accept', 'reviewer:1', \
            'gloss alignment correct', null, 0.95::double precision, null, null, '{}'::jsonb)",
        &[&igt.to_string()],
    )
    .await
    .unwrap();

    // 7. Build a release manifest including this language pilot data.
    let release_id: uuid::Uuid = c
        .query_one(
            "insert into donto_dataset_release (release_name, release_version, query_spec, output_formats) \
             values ($1, '0.1.0', $2, '{donto-jsonl,cldf}'::text[]) returning release_id",
            &[
                &format!("ling-pilot-{prefix}"),
                &json!({"context": ctx_iri, "frames": [phon_frame, igt]}),
            ],
        )
        .await
        .unwrap()
        .get(0);
    c.query_one(
        "select donto_seal_release($1, 'release-bot')",
        &[&release_id],
    )
    .await
    .unwrap();

    // 8. Verify all events landed.
    let n_events: i64 = c
        .query_one(
            "select count(*) from donto_event_log \
             where occurred_at > now() - interval '5 minutes' \
             and target_kind in ('frame', 'predicate_descriptor', 'review_decision', 'release', 'frame_role')",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(n_events >= 6);
}
