//! Alexandria §3.8: time-binned aggregation over valid_time.

use chrono::NaiveDate;
use donto_client::{Object, StatementInput};

mod common;
use common::{cleanup_prefix, connect, ctx, tag};

fn lit_stmt(subject: &str, predicate: &str, object: &str, context: &str) -> StatementInput {
    StatementInput::new(
        subject,
        predicate,
        Object::Literal(donto_client::Literal::string(object)),
    )
    .with_context(context)
}

async fn assert_with_valid_from(
    client: &donto_client::DontoClient,
    prefix: &str,
    context: &str,
    predicate: &str,
    valid_from: NaiveDate,
) {
    let subject = format!("{prefix}/{}", valid_from.format("%Y%m%d"));
    let s = lit_stmt(&subject, predicate, &format!("v-{valid_from}"), context)
        .with_valid(Some(valid_from), None);
    client.assert(&s).await.unwrap();
}

#[tokio::test]
async fn yearly_buckets_aggregate_by_valid_time_from() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("bkt-yr");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "bkt-yr").await;

    // Seed five statements across two decades.
    let dates = [
        NaiveDate::from_ymd_opt(1955, 1, 1).unwrap(),
        NaiveDate::from_ymd_opt(1957, 6, 15).unwrap(),
        NaiveDate::from_ymd_opt(1965, 3, 1).unwrap(),
        NaiveDate::from_ymd_opt(2018, 11, 30).unwrap(),
        NaiveDate::from_ymd_opt(2019, 4, 1).unwrap(),
    ];
    for d in dates {
        assert_with_valid_from(&client, &prefix, &ctx, "ex:usage", d).await;
    }

    // 10-year buckets, epoch = 2000-01-01.
    let scope = donto_client::ContextScope::just(ctx.as_str());
    let mut buckets = client
        .valid_time_buckets(
            "10 years",
            NaiveDate::from_ymd_opt(2000, 1, 1).unwrap(),
            Some("ex:usage"),
            None,
            Some(&scope),
        )
        .await
        .unwrap();
    buckets.sort_by_key(|b| b.bucket_start);

    // Expect: 1950..1960 -> 2, 1960..1970 -> 1, 2010..2020 -> 2.
    let by_start: std::collections::BTreeMap<NaiveDate, u64> =
        buckets.iter().map(|b| (b.bucket_start, b.count)).collect();
    assert_eq!(
        by_start.get(&NaiveDate::from_ymd_opt(1950, 1, 1).unwrap()),
        Some(&2)
    );
    assert_eq!(
        by_start.get(&NaiveDate::from_ymd_opt(1960, 1, 1).unwrap()),
        Some(&1)
    );
    assert_eq!(
        by_start.get(&NaiveDate::from_ymd_opt(2010, 1, 1).unwrap()),
        Some(&2)
    );
    // bucket_end = bucket_start + 10y.
    for b in &buckets {
        assert_eq!(b.bucket_end, b.bucket_start + chrono::Months::new(120));
    }
}

#[tokio::test]
async fn day_buckets_align_to_epoch() {
    let client = pg_or_skip!(connect().await);
    let prefix = tag("bkt-day");
    cleanup_prefix(&client, &prefix).await;
    let ctx = ctx(&client, "bkt-day").await;

    // Three days in one 7-day bucket, two in the next.
    let dates = [
        (2020, 1, 6),
        (2020, 1, 7),
        (2020, 1, 12),
        (2020, 1, 13),
        (2020, 1, 14),
    ];
    for (y, m, d) in dates {
        assert_with_valid_from(
            &client,
            &prefix,
            &ctx,
            "ex:daily",
            NaiveDate::from_ymd_opt(y, m, d).unwrap(),
        )
        .await;
    }

    let scope = donto_client::ContextScope::just(ctx.as_str());
    let mut buckets = client
        .valid_time_buckets(
            "7 days",
            // Monday anchor.
            NaiveDate::from_ymd_opt(2020, 1, 6).unwrap(),
            Some("ex:daily"),
            None,
            Some(&scope),
        )
        .await
        .unwrap();
    buckets.sort_by_key(|b| b.bucket_start);
    assert_eq!(buckets.len(), 2);
    assert_eq!(
        buckets[0].bucket_start,
        NaiveDate::from_ymd_opt(2020, 1, 6).unwrap()
    );
    assert_eq!(buckets[0].count, 3);
    assert_eq!(
        buckets[1].bucket_start,
        NaiveDate::from_ymd_opt(2020, 1, 13).unwrap()
    );
    assert_eq!(buckets[1].count, 2);
}
