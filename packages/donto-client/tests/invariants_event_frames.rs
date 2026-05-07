//! Event frame decomposition invariants (migration 0054).
//!
//! n-ary relations are decomposed into a frame node plus role triples:
//! `<frame> rdf:type <frame_type>`, `<frame> <pred>/subject <s>`,
//! `<frame> <pred>/object <o>`, plus extra roles supplied as JSONB.

mod common;

use common::{connect, ctx, tag};

#[tokio::test]
async fn decompose_creates_frame_with_type_assertion() {
    let client = pg_or_skip!(connect().await);
    let ctx = ctx(&client, "ef-type").await;
    let prefix = tag("ef-type");

    let predicate = format!("{prefix}/worksAt");
    let frame_id = client
        .decompose_to_frame(
            &format!("{prefix}/alice"),
            &predicate,
            Some(&format!("{prefix}/sorbonne")),
            &ctx,
            Some("ex:EmploymentEvent"),
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    let c = client.pool().get().await.unwrap();
    let row = c
        .query_one(
            "select frame_iri, frame_type, source_predicate \
             from donto_event_frame where frame_id = $1",
            &[&frame_id],
        )
        .await
        .unwrap();
    let frame_iri: String = row.get("frame_iri");
    assert!(
        frame_iri.starts_with("donto:frame/"),
        "frame_iri must use donto:frame/ namespace, got {frame_iri}"
    );
    assert_eq!(row.get::<_, String>("frame_type"), "ex:EmploymentEvent");
    assert_eq!(row.get::<_, String>("source_predicate"), predicate);

    // rdf:type assertion must exist.
    let n_type: i64 = c
        .query_one(
            "select count(*) from donto_statement \
             where subject = $1 and predicate = 'rdf:type' \
               and object_iri = 'ex:EmploymentEvent' \
               and context = $2 and upper(tx_time) is null",
            &[&frame_iri, &ctx],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n_type, 1, "frame must carry an rdf:type assertion");
}

#[tokio::test]
async fn frame_has_subject_and_object_role_links() {
    let client = pg_or_skip!(connect().await);
    let ctx = ctx(&client, "ef-roles").await;
    let prefix = tag("ef-roles");

    let predicate = format!("{prefix}/worksAt");
    let subject = format!("{prefix}/alice");
    let object = format!("{prefix}/sorbonne");
    let frame_id = client
        .decompose_to_frame(
            &subject,
            &predicate,
            Some(&object),
            &ctx,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    let c = client.pool().get().await.unwrap();
    let frame_iri: String = c
        .query_one(
            "select frame_iri from donto_event_frame where frame_id = $1",
            &[&frame_id],
        )
        .await
        .unwrap()
        .get(0);

    let subject_role = format!("{predicate}/subject");
    let n_subj: i64 = c
        .query_one(
            "select count(*) from donto_statement \
             where subject = $1 and predicate = $2 and object_iri = $3 \
               and upper(tx_time) is null",
            &[&frame_iri, &subject_role, &subject],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n_subj, 1, "frame must carry a subject role link");

    let object_role = format!("{predicate}/object");
    let n_obj: i64 = c
        .query_one(
            "select count(*) from donto_statement \
             where subject = $1 and predicate = $2 and object_iri = $3 \
               and upper(tx_time) is null",
            &[&frame_iri, &object_role, &object],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n_obj, 1, "frame must carry an object role link");
}

#[tokio::test]
async fn extra_roles_from_jsonb_are_asserted() {
    let client = pg_or_skip!(connect().await);
    let ctx = ctx(&client, "ef-extra").await;
    let prefix = tag("ef-extra");

    let predicate = format!("{prefix}/worksAt");
    let role_iri_pred = format!("{prefix}/role");
    let role_lit_pred = format!("{prefix}/title");

    let extra = serde_json::json!({
        role_iri_pred.clone(): { "iri": format!("{prefix}/professor") },
        role_lit_pred.clone(): { "v": "Professor of Physics", "dt": "xsd:string" }
    });

    let frame_id = client
        .decompose_to_frame(
            &format!("{prefix}/alice"),
            &predicate,
            Some(&format!("{prefix}/sorbonne")),
            &ctx,
            None,
            Some(&extra),
            None,
            None,
            None,
        )
        .await
        .unwrap();

    let c = client.pool().get().await.unwrap();
    let frame_iri: String = c
        .query_one(
            "select frame_iri from donto_event_frame where frame_id = $1",
            &[&frame_id],
        )
        .await
        .unwrap()
        .get(0);

    // IRI-typed extra role.
    let n_iri: i64 = c
        .query_one(
            "select count(*) from donto_statement \
             where subject = $1 and predicate = $2 \
               and object_iri = $3 and upper(tx_time) is null",
            &[&frame_iri, &role_iri_pred, &format!("{prefix}/professor")],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n_iri, 1, "extra IRI role must be asserted");

    // Literal-typed extra role.
    let n_lit: i64 = c
        .query_one(
            "select count(*) from donto_statement \
             where subject = $1 and predicate = $2 \
               and object_lit is not null and upper(tx_time) is null",
            &[&frame_iri, &role_lit_pred],
        )
        .await
        .unwrap()
        .get(0);
    assert_eq!(n_lit, 1, "extra literal role must be asserted");
}
