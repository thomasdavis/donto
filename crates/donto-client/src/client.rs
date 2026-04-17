//! High-level client for the donto Phase 0 SQL surface.
//!
//! Backed by `deadpool_postgres` so callers can share one pool across tasks.

use crate::error::{Error, Result};
use crate::model::{Literal, Object, Polarity, Statement, StatementInput};
use crate::scope::ContextScope;

use chrono::{DateTime, NaiveDate, Utc};
use deadpool_postgres::{Config, ManagerConfig, Pool, RecyclingMethod, Runtime};
use serde_json::Value as Json;
use std::str::FromStr;
use tokio_postgres::types::ToSql;
use tokio_postgres::{NoTls, Row};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct DontoClient {
    pool: Pool,
}

impl DontoClient {
    /// Build a client from a libpq-style DSN (e.g.
    /// `postgres://donto:donto@127.0.0.1:55432/donto`).
    pub fn from_dsn(dsn: &str) -> Result<Self> {
        let pg_cfg = tokio_postgres::Config::from_str(dsn)?;

        let mut cfg = Config::new();
        cfg.host = pg_cfg.get_hosts().iter().find_map(|h| match h {
            tokio_postgres::config::Host::Tcp(s) => Some(s.clone()),
            #[allow(unreachable_patterns)]
            _ => None,
        });
        cfg.port = pg_cfg.get_ports().first().copied();
        cfg.user = pg_cfg.get_user().map(str::to_owned);
        cfg.password = pg_cfg
            .get_password()
            .map(|p| String::from_utf8_lossy(p).into_owned());
        cfg.dbname = pg_cfg.get_dbname().map(str::to_owned);
        cfg.manager = Some(ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        });

        let pool = cfg.create_pool(Some(Runtime::Tokio1), NoTls)?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &Pool {
        &self.pool
    }

    /// Apply all embedded migrations (idempotent).
    pub async fn migrate(&self) -> Result<()> {
        crate::migrations::apply_migrations(&self.pool).await
    }

    /// Ensure a context exists (idempotent).
    pub async fn ensure_context(
        &self,
        iri: &str,
        kind: &str,
        mode: &str,
        parent: Option<&str>,
    ) -> Result<()> {
        let c = self.pool.get().await?;
        c.execute(
            "select donto_ensure_context($1, $2, $3, $4)",
            &[&iri, &kind, &mode, &parent],
        )
        .await?;
        Ok(())
    }

    /// Insert a single statement and return its UUID. Idempotent: re-asserting
    /// the same content returns the existing id.
    pub async fn assert(&self, s: &StatementInput) -> Result<Uuid> {
        let (object_iri, object_lit): (Option<&str>, Option<Json>) = match &s.object {
            Object::Iri(i) => (Some(i.as_str()), None),
            Object::Literal(l) => (None, Some(serde_json::to_value(l)?)),
        };

        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "select donto_assert($1, $2, $3, $4, $5, $6, $7, $8, $9, null)",
                &[
                    &s.subject,
                    &s.predicate,
                    &object_iri,
                    &object_lit,
                    &s.context,
                    &s.polarity.as_str(),
                    &(s.maturity as i32),
                    &s.valid_lo,
                    &s.valid_hi,
                ],
            )
            .await?;
        Ok(row.get::<_, Uuid>(0))
    }

    /// Insert many statements in one server-side call. Returns count inserted.
    pub async fn assert_batch(&self, stmts: &[StatementInput]) -> Result<usize> {
        let payload: Vec<Json> = stmts.iter().map(stmt_to_json).collect();
        let arr = Json::Array(payload);
        let c = self.pool.get().await?;
        let row = c
            .query_one("select donto_assert_batch($1::jsonb, null)", &[&arr])
            .await?;
        Ok(row.get::<_, i32>(0) as usize)
    }

    /// Close the transaction-time of an open statement. Returns true if a row
    /// was actually closed.
    pub async fn retract(&self, statement_id: Uuid) -> Result<bool> {
        let c = self.pool.get().await?;
        let row = c
            .query_one("select donto_retract($1, null)", &[&statement_id])
            .await?;
        Ok(row.get::<_, bool>(0))
    }

    /// Correct a statement: retract the prior open row and assert a new one
    /// with the requested overrides. Returns the new statement_id.
    pub async fn correct(
        &self,
        statement_id: Uuid,
        new_subject: Option<&str>,
        new_predicate: Option<&str>,
        new_object: Option<&Object>,
        new_polarity: Option<Polarity>,
    ) -> Result<Uuid> {
        let (new_iri, new_lit): (Option<&str>, Option<Json>) = match new_object {
            None => (None, None),
            Some(Object::Iri(i)) => (Some(i.as_str()), None),
            Some(Object::Literal(l)) => (None, Some(serde_json::to_value(l)?)),
        };
        let polarity_str: Option<&str> = new_polarity.map(Polarity::as_str);

        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "select donto_correct($1, $2, $3, $4, $5, $6, null)",
                &[
                    &statement_id,
                    &new_subject,
                    &new_predicate,
                    &new_iri,
                    &new_lit,
                    &polarity_str,
                ],
            )
            .await?;
        Ok(row.get::<_, Uuid>(0))
    }

    /// Pattern match.
    #[allow(clippy::too_many_arguments)]
    pub async fn match_pattern(
        &self,
        subject: Option<&str>,
        predicate: Option<&str>,
        object_iri: Option<&str>,
        scope: Option<&ContextScope>,
        polarity: Option<Polarity>,
        min_maturity: u8,
        as_of_tx: Option<DateTime<Utc>>,
        as_of_valid: Option<NaiveDate>,
    ) -> Result<Vec<Statement>> {
        let scope_json: Option<Json> = scope.map(|s| s.to_json());
        let polarity_str: Option<&str> = polarity.map(Polarity::as_str);

        let c = self.pool.get().await?;
        let params: [&(dyn ToSql + Sync); 9] = [
            &subject,
            &predicate,
            &object_iri,
            &Option::<Json>::None, // object_lit pattern not exposed in Phase 0
            &scope_json,
            &polarity_str,
            &(min_maturity as i32),
            &as_of_tx,
            &as_of_valid,
        ];
        let rows = c
            .query(
                "select * from donto_match($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                &params,
            )
            .await?;
        rows.into_iter().map(row_to_statement).collect()
    }

    /// Return the unique contexts visible under a scope. Useful for sanity tests.
    pub async fn resolve_scope(&self, scope: &ContextScope) -> Result<Vec<String>> {
        let c = self.pool.get().await?;
        let scope_json = scope.to_json();
        let rows = c
            .query(
                "select context_iri from donto_resolve_scope($1::jsonb)",
                &[&scope_json],
            )
            .await?;
        Ok(rows.into_iter().map(|r| r.get::<_, String>(0)).collect())
    }
}

fn stmt_to_json(s: &StatementInput) -> Json {
    let (object_iri, object_lit): (Option<String>, Option<Json>) = match &s.object {
        Object::Iri(i) => (Some(i.clone()), None),
        Object::Literal(l) => (
            None,
            Some(serde_json::to_value(l).expect("literal serialization")),
        ),
    };
    let mut obj = serde_json::Map::new();
    obj.insert("subject".into(), Json::String(s.subject.clone()));
    obj.insert("predicate".into(), Json::String(s.predicate.clone()));
    if let Some(i) = object_iri {
        obj.insert("object_iri".into(), Json::String(i));
    }
    if let Some(l) = object_lit {
        obj.insert("object_lit".into(), l);
    }
    obj.insert("context".into(), Json::String(s.context.clone()));
    obj.insert("polarity".into(), Json::String(s.polarity.as_str().into()));
    obj.insert("maturity".into(), Json::Number((s.maturity as i64).into()));
    if let Some(lo) = s.valid_lo {
        obj.insert(
            "valid_lo".into(),
            Json::String(lo.format("%Y-%m-%d").to_string()),
        );
    }
    if let Some(hi) = s.valid_hi {
        obj.insert(
            "valid_hi".into(),
            Json::String(hi.format("%Y-%m-%d").to_string()),
        );
    }
    Json::Object(obj)
}

fn row_to_statement(row: Row) -> Result<Statement> {
    let object_iri: Option<String> = row.try_get("object_iri")?;
    let object_lit: Option<Json> = row.try_get("object_lit")?;
    let object = match (object_iri, object_lit) {
        (Some(i), None) => Object::Iri(i),
        (None, Some(l)) => Object::Literal(serde_json::from_value::<Literal>(l)?),
        _ => {
            return Err(Error::Invalid(
                "statement row has neither/both object kinds".into(),
            ))
        }
    };
    let polarity_str: String = row.try_get("polarity")?;
    let polarity = Polarity::parse(&polarity_str)
        .ok_or_else(|| Error::Invalid(format!("unknown polarity {polarity_str}")))?;
    let maturity: i32 = row.try_get("maturity")?;

    Ok(Statement {
        statement_id: row.try_get("statement_id")?,
        subject: row.try_get("subject")?,
        predicate: row.try_get("predicate")?,
        object,
        context: row.try_get("context")?,
        polarity,
        maturity: maturity as u8,
        valid_lo: row.try_get("valid_lo")?,
        valid_hi: row.try_get("valid_hi")?,
        tx_lo: row.try_get("tx_lo")?,
        tx_hi: row.try_get("tx_hi")?,
    })
}
