//! Evidence substrate: unit registry, conversion, and normalization.

mod common;
use common::connect;

#[tokio::test]
async fn unit_conversion_same_dimension() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();

    // 700 attoseconds = 0.7 femtoseconds
    let result: f64 = c
        .query_one(
            "select donto_convert_unit(700, 'unit:attosecond', 'unit:femtosecond')",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!((result - 0.7).abs() < 1e-10);

    // 1 hour = 3600 seconds
    let result: f64 = c
        .query_one(
            "select donto_convert_unit(1, 'unit:hour', 'unit:second')",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!((result - 3600.0).abs() < 0.1);

    // 500 milligrams = 0.0005 kilograms
    let result: f64 = c
        .query_one(
            "select donto_convert_unit(500, 'unit:milligram', 'unit:kilogram')",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!((result - 0.0005).abs() < 1e-10);
}

#[tokio::test]
async fn unit_conversion_cross_dimension_is_null() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();

    let result: Option<f64> = c
        .query_one(
            "select donto_convert_unit(1, 'unit:second', 'unit:meter')",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(
        result.is_none(),
        "cross-dimension conversion must return null"
    );
}

#[tokio::test]
async fn unit_conversion_unknown_unit_is_null() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();

    let result: Option<f64> = c
        .query_one(
            "select donto_convert_unit(1, 'unit:nonexistent', 'unit:second')",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!(result.is_none());
}

#[tokio::test]
async fn unit_identity_conversion() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();

    let result: f64 = c
        .query_one(
            "select donto_convert_unit(42.0, 'unit:second', 'unit:second')",
            &[],
        )
        .await
        .unwrap()
        .get(0);
    assert!((result - 42.0).abs() < 1e-10);
}

#[tokio::test]
async fn normalize_percent() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();

    // "60.1%" → 0.601
    let r: f64 = c
        .query_one("select donto_normalize_percent('60.1%')", &[])
        .await
        .unwrap()
        .get(0);
    assert!((r - 0.601).abs() < 1e-6);

    // "0.601" → 0.601
    let r: f64 = c
        .query_one("select donto_normalize_percent('0.601')", &[])
        .await
        .unwrap()
        .get(0);
    assert!((r - 0.601).abs() < 1e-6);

    // "100%" → 1.0
    let r: f64 = c
        .query_one("select donto_normalize_percent('100%')", &[])
        .await
        .unwrap()
        .get(0);
    assert!((r - 1.0).abs() < 1e-6);

    // "0%" → 0.0
    let r: f64 = c
        .query_one("select donto_normalize_percent('0%')", &[])
        .await
        .unwrap()
        .get(0);
    assert!(r.abs() < 1e-6);
}

#[tokio::test]
async fn seeded_units_exist() {
    let client = pg_or_skip!(connect().await);
    let pool = client.pool();
    let c = pool.get().await.unwrap();

    let count: i64 = c
        .query_one("select count(*) from donto_unit", &[])
        .await
        .unwrap()
        .get(0);
    assert!(
        count >= 26,
        "expected at least 26 seeded units, got {count}"
    );

    // Spot check a few
    for iri in [
        "unit:second",
        "unit:attosecond",
        "unit:percent",
        "unit:usd",
        "unit:kilogram",
    ] {
        let exists: bool = c
            .query_one(
                "select exists(select 1 from donto_unit where iri = $1)",
                &[&iri],
            )
            .await
            .unwrap()
            .get(0);
        assert!(exists, "unit {iri} must exist");
    }
}
