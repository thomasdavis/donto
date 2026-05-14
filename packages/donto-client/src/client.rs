//! High-level client for the donto Phase 0 SQL surface.
//!
//! Backed by `deadpool_postgres` so callers can share one pool across tasks.

use crate::error::{Error, Result};
use crate::model::{
    AlignedStatement, AlignmentRelation, Literal, Object, Polarity, PredicateCandidate, Reaction,
    ReactionKind, ShapeVerdict, Statement, StatementInput, TextMatch, TimeBucket,
};
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

    /// Alexandria §3.4: retrofit ingest. Explicit backdated valid_time, a
    /// required reason, and tx_time pinned to now(). The reason lands on a
    /// side overlay so "why was this retrofitted?" is queryable.
    #[allow(clippy::too_many_arguments)]
    pub async fn assert_retrofit(
        &self,
        s: &StatementInput,
        reason: &str,
        actor: Option<&str>,
    ) -> Result<Uuid> {
        if s.valid_lo.is_none() && s.valid_hi.is_none() {
            return Err(Error::Invalid(
                "assert_retrofit: at least one of valid_lo/valid_hi is required".into(),
            ));
        }
        let (object_iri, object_lit): (Option<&str>, Option<Json>) = match &s.object {
            Object::Iri(i) => (Some(i.as_str()), None),
            Object::Literal(l) => (None, Some(serde_json::to_value(l)?)),
        };
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "select donto_assert_retrofit($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
                &[
                    &s.subject,
                    &s.predicate,
                    &object_iri,
                    &object_lit,
                    &s.valid_lo,
                    &s.valid_hi,
                    &reason,
                    &s.context,
                    &s.polarity.as_str(),
                    &(s.maturity as i32),
                    &actor,
                ],
            )
            .await?;
        Ok(row.get::<_, Uuid>(0))
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

    /// Alexandria §3.5: attach a shape-report annotation to a statement.
    /// Returns the annotation_id.
    pub async fn attach_shape_report(
        &self,
        statement_id: Uuid,
        shape_iri: &str,
        verdict: ShapeVerdict,
        context: &str,
        detail: Option<&Json>,
    ) -> Result<i64> {
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "select donto_attach_shape_report($1, $2, $3, $4, $5)",
                &[
                    &statement_id,
                    &shape_iri,
                    &verdict.as_str(),
                    &context,
                    &detail,
                ],
            )
            .await?;
        Ok(row.get::<_, i64>(0))
    }

    /// Alexandria §3.5: does a statement currently carry an annotation with
    /// the given verdict (and optionally a specific shape)?
    pub async fn has_shape_verdict(
        &self,
        statement_id: Uuid,
        verdict: ShapeVerdict,
        shape_iri: Option<&str>,
    ) -> Result<bool> {
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "select donto_has_shape_verdict($1, $2, $3)",
                &[&statement_id, &verdict.as_str(), &shape_iri],
            )
            .await?;
        Ok(row.get::<_, bool>(0))
    }

    /// Alexandria §3.8: time-binned aggregation over valid_time. The
    /// bucket interval must be pure months/years OR pure days.
    pub async fn valid_time_buckets(
        &self,
        bucket_pg_interval: &str,
        epoch: NaiveDate,
        predicate: Option<&str>,
        subject: Option<&str>,
        scope: Option<&ContextScope>,
    ) -> Result<Vec<TimeBucket>> {
        let scope_json: Option<Json> = scope.map(|s| s.to_json());
        let c = self.pool.get().await?;
        let sql = "select bucket_start, bucket_end, cnt \
             from donto_valid_time_buckets($1, $2, $3, $4, $5)";
        let rows = c
            .query(
                sql,
                &[
                    &bucket_pg_interval,
                    &epoch,
                    &predicate,
                    &subject,
                    &scope_json,
                ],
            )
            .await?;
        rows.into_iter()
            .map(|r| {
                Ok(TimeBucket {
                    bucket_start: r.try_get("bucket_start")?,
                    bucket_end: r.try_get("bucket_end")?,
                    count: r.try_get::<_, i64>("cnt")? as u64,
                })
            })
            .collect()
    }

    /// Alexandria §3.2: attach a reaction to a statement. Returns the
    /// reaction's own statement_id.
    pub async fn react(
        &self,
        source: Uuid,
        kind: ReactionKind,
        object_iri: Option<&str>,
        context: &str,
        actor: Option<&str>,
    ) -> Result<Uuid> {
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "select donto_react($1, $2, $3, $4, $5)",
                &[&source, &kind.as_str(), &object_iri, &context, &actor],
            )
            .await?;
        Ok(row.get::<_, Uuid>(0))
    }

    /// Alexandria §3.2: enumerate current reactions to a statement.
    pub async fn reactions_for(&self, statement_id: Uuid) -> Result<Vec<Reaction>> {
        let c = self.pool.get().await?;
        let rows = c
            .query(
                "select reaction_id, kind, object_iri, context, polarity \
                 from donto_reactions_for($1)",
                &[&statement_id],
            )
            .await?;
        rows.into_iter()
            .map(|r| {
                let kind_str: String = r.try_get("kind")?;
                let kind = ReactionKind::parse(&kind_str)
                    .ok_or_else(|| Error::Invalid(format!("unknown reaction kind {kind_str}")))?;
                let polarity_str: String = r.try_get("polarity")?;
                let polarity = Polarity::parse(&polarity_str)
                    .ok_or_else(|| Error::Invalid(format!("unknown polarity {polarity_str}")))?;
                Ok(Reaction {
                    reaction_id: r.try_get("reaction_id")?,
                    kind,
                    object_iri: r.try_get("object_iri")?,
                    context: r.try_get("context")?,
                    polarity,
                })
            })
            .collect()
    }

    /// Alexandria §3.3: compute endorsement weights over a scope and write
    /// them into `into_ctx` as Level-3 derived statements. Returns the
    /// number of weight rows emitted.
    pub async fn compute_endorsement_weights(
        &self,
        scope: Option<&ContextScope>,
        into_ctx: &str,
        actor: Option<&str>,
    ) -> Result<u64> {
        let scope_json: Option<Json> = scope.map(|s| s.to_json());
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "select donto_compute_endorsement_weights($1, $2, $3)",
                &[&scope_json, &into_ctx, &actor],
            )
            .await?;
        Ok(row.get::<_, i64>(0) as u64)
    }

    /// Alexandria §3.3: ephemeral weight read — DontoQL `with weights(scope=ctx)`.
    pub async fn weight_of(&self, statement_id: Uuid, scope: Option<&ContextScope>) -> Result<i64> {
        let scope_json: Option<Json> = scope.map(|s| s.to_json());
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "select donto_weight_of($1, $2)",
                &[&statement_id, &scope_json],
            )
            .await?;
        Ok(row.get::<_, i64>(0))
    }

    /// Alexandria §3.9: websearch-style full-text search over literal
    /// values. `query_lang` is the BCP-47 primary subtag (default "en").
    #[allow(clippy::too_many_arguments)]
    pub async fn match_text(
        &self,
        query: &str,
        query_lang: Option<&str>,
        scope: Option<&ContextScope>,
        predicate: Option<&str>,
        polarity: Option<Polarity>,
        min_maturity: u8,
    ) -> Result<Vec<TextMatch>> {
        let scope_json: Option<Json> = scope.map(|s| s.to_json());
        let polarity_str: Option<&str> = polarity.map(Polarity::as_str);
        let lang = query_lang.unwrap_or("en");
        let c = self.pool.get().await?;
        let rows = c
            .query(
                "select statement_id, subject, predicate, object_lit, context, \
                        polarity, maturity, score \
                 from donto_match_text($1, $2, $3, $4, $5, $6)",
                &[
                    &query,
                    &lang,
                    &scope_json,
                    &predicate,
                    &polarity_str,
                    &(min_maturity as i32),
                ],
            )
            .await?;
        rows.into_iter()
            .map(|r| {
                let object_lit_json: Json = r.try_get("object_lit")?;
                let object_lit: Literal = parse_literal_lenient(object_lit_json)?;
                let polarity_s: String = r.try_get("polarity")?;
                let polarity = Polarity::parse(&polarity_s)
                    .ok_or_else(|| Error::Invalid(format!("unknown polarity {polarity_s}")))?;
                let maturity: i32 = r.try_get("maturity")?;
                Ok(TextMatch {
                    statement_id: r.try_get("statement_id")?,
                    subject: r.try_get("subject")?,
                    predicate: r.try_get("predicate")?,
                    object_lit,
                    context: r.try_get("context")?,
                    polarity,
                    maturity: maturity as u8,
                    score: r.try_get("score")?,
                })
            })
            .collect()
    }

    /// Alexandria §3.6: assert that two statements share the same meaning.
    /// Emits both directions (the predicate is symmetric).
    pub async fn align_meaning(
        &self,
        stmt_a: Uuid,
        stmt_b: Uuid,
        context: &str,
        actor: Option<&str>,
    ) -> Result<()> {
        let c = self.pool.get().await?;
        c.execute(
            "select donto_align_meaning($1, $2, $3, $4)",
            &[&stmt_a, &stmt_b, &context, &actor],
        )
        .await?;
        Ok(())
    }

    /// Alexandria §3.6: transitive closure of SameMeaning edges from a
    /// statement. Includes the starting statement itself.
    pub async fn meaning_cluster(
        &self,
        stmt_id: Uuid,
        scope: Option<&ContextScope>,
    ) -> Result<Vec<Uuid>> {
        let scope_json: Option<Json> = scope.map(|s| s.to_json());
        let c = self.pool.get().await?;
        let rows = c
            .query(
                "select statement_id from donto_meaning_cluster($1, $2)",
                &[&stmt_id, &scope_json],
            )
            .await?;
        Ok(rows.into_iter().map(|r| r.get::<_, Uuid>(0)).collect())
    }

    /// Alexandria §3.7: set an environment key on a context.
    pub async fn context_env_set(
        &self,
        context: &str,
        key: &str,
        value: &Json,
        actor: Option<&str>,
    ) -> Result<()> {
        let c = self.pool.get().await?;
        c.execute(
            "select donto_context_env_set($1, $2, $3, $4)",
            &[&context, &key, &value, &actor],
        )
        .await?;
        Ok(())
    }

    /// Alexandria §3.7: read an environment key on a context (None if absent).
    pub async fn context_env_get(&self, context: &str, key: &str) -> Result<Option<Json>> {
        let c = self.pool.get().await?;
        let row = c
            .query_one("select donto_context_env_get($1, $2)", &[&context, &key])
            .await?;
        Ok(row.get::<_, Option<Json>>(0))
    }

    /// Alexandria §3.7: contexts whose env overlay matches every required pair.
    pub async fn contexts_with_env(&self, required: &Json) -> Result<Vec<String>> {
        let c = self.pool.get().await?;
        let rows = c
            .query(
                "select context_iri from donto_contexts_with_env($1)",
                &[&required],
            )
            .await?;
        Ok(rows.into_iter().map(|r| r.get::<_, String>(0)).collect())
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

    // --- Evidence substrate: documents ---

    pub async fn ensure_document(
        &self,
        iri: &str,
        media_type: &str,
        label: Option<&str>,
        source_url: Option<&str>,
        language: Option<&str>,
    ) -> Result<Uuid> {
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "select donto_ensure_document($1, $2, $3, $4, $5)",
                &[&iri, &media_type, &label, &source_url, &language],
            )
            .await?;
        Ok(row.get::<_, Uuid>(0))
    }

    pub async fn add_revision(
        &self,
        document_id: Uuid,
        body: Option<&str>,
        body_bytes: Option<&[u8]>,
        parser_version: Option<&str>,
    ) -> Result<Uuid> {
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "select donto_add_revision($1, $2, $3, $4)",
                &[&document_id, &body, &body_bytes, &parser_version],
            )
            .await?;
        Ok(row.get::<_, Uuid>(0))
    }

    // --- Evidence substrate: spans ---

    pub async fn create_char_span(
        &self,
        revision_id: Uuid,
        start: i32,
        end: i32,
        surface_text: Option<&str>,
    ) -> Result<Uuid> {
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "select donto_create_char_span($1, $2, $3, $4)",
                &[&revision_id, &start, &end, &surface_text],
            )
            .await?;
        Ok(row.get::<_, Uuid>(0))
    }

    // --- Evidence substrate: extraction runs ---

    pub async fn start_extraction(
        &self,
        model_id: Option<&str>,
        model_version: Option<&str>,
        source_revision_id: Option<Uuid>,
        context: Option<&str>,
    ) -> Result<Uuid> {
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "select donto_start_extraction($1, $2, $3, $4)",
                &[&model_id, &model_version, &source_revision_id, &context],
            )
            .await?;
        Ok(row.get::<_, Uuid>(0))
    }

    pub async fn complete_extraction(
        &self,
        run_id: Uuid,
        status: &str,
        statements_emitted: Option<i64>,
        annotations_emitted: Option<i64>,
    ) -> Result<()> {
        let c = self.pool.get().await?;
        c.execute(
            "select donto_complete_extraction($1, $2, $3, $4)",
            &[&run_id, &status, &statements_emitted, &annotations_emitted],
        )
        .await?;
        Ok(())
    }

    // --- Evidence substrate: evidence links ---

    pub async fn link_evidence_span(
        &self,
        statement_id: Uuid,
        span_id: Uuid,
        link_type: &str,
        confidence: Option<f64>,
        context: Option<&str>,
    ) -> Result<Uuid> {
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "select donto_link_evidence_span($1, $2, $3, $4, $5)",
                &[&statement_id, &span_id, &link_type, &confidence, &context],
            )
            .await?;
        Ok(row.get::<_, Uuid>(0))
    }

    pub async fn link_evidence_run(
        &self,
        statement_id: Uuid,
        run_id: Uuid,
        link_type: &str,
        context: Option<&str>,
    ) -> Result<Uuid> {
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "select donto_link_evidence_run($1, $2, $3, $4)",
                &[&statement_id, &run_id, &link_type, &context],
            )
            .await?;
        Ok(row.get::<_, Uuid>(0))
    }

    // --- Evidence substrate: agents ---

    pub async fn ensure_agent(
        &self,
        iri: &str,
        agent_type: &str,
        label: Option<&str>,
        model_id: Option<&str>,
    ) -> Result<Uuid> {
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "select donto_ensure_agent($1, $2, $3, $4)",
                &[&iri, &agent_type, &label, &model_id],
            )
            .await?;
        Ok(row.get::<_, Uuid>(0))
    }

    pub async fn bind_agent_context(
        &self,
        agent_id: Uuid,
        context: &str,
        role: &str,
    ) -> Result<()> {
        let c = self.pool.get().await?;
        c.execute(
            "select donto_bind_agent_context($1, $2, $3)",
            &[&agent_id, &context, &role],
        )
        .await?;
        Ok(())
    }

    // --- Evidence substrate: arguments ---

    pub async fn assert_argument(
        &self,
        source: Uuid,
        target: Uuid,
        relation: &str,
        context: &str,
        strength: Option<f64>,
        agent_id: Option<Uuid>,
        evidence: Option<&Json>,
    ) -> Result<Uuid> {
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "select donto_assert_argument($1, $2, $3, $4, $5, $6, $7)",
                &[
                    &source, &target, &relation, &context, &strength, &agent_id, &evidence,
                ],
            )
            .await?;
        Ok(row.get::<_, Uuid>(0))
    }

    // --- Evidence substrate: proof obligations ---

    pub async fn emit_obligation(
        &self,
        statement_id: Uuid,
        obligation_type: &str,
        context: &str,
        priority: i16,
        detail: Option<&Json>,
        assigned_agent: Option<Uuid>,
    ) -> Result<Uuid> {
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "select donto_emit_obligation($1, $2, $3, $4, $5, $6)",
                &[
                    &statement_id,
                    &obligation_type,
                    &context,
                    &priority,
                    &detail,
                    &assigned_agent,
                ],
            )
            .await?;
        Ok(row.get::<_, Uuid>(0))
    }

    pub async fn resolve_obligation(
        &self,
        obligation_id: Uuid,
        resolved_by: Option<Uuid>,
        status: &str,
    ) -> Result<bool> {
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "select donto_resolve_obligation($1, $2, $3)",
                &[&obligation_id, &resolved_by, &status],
            )
            .await?;
        Ok(row.get::<_, bool>(0))
    }

    // --- Evidence substrate: vectors ---

    pub async fn store_vector(
        &self,
        subject_type: &str,
        subject_id: Uuid,
        model_id: &str,
        model_version: Option<&str>,
        embedding: &[f32],
    ) -> Result<Uuid> {
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "select donto_store_vector($1, $2, $3, $4, $5)",
                &[
                    &subject_type,
                    &subject_id,
                    &model_id,
                    &model_version,
                    &embedding,
                ],
            )
            .await?;
        Ok(row.get::<_, Uuid>(0))
    }

    // --- Predicate alignment layer ---

    /// Register an alignment edge between two predicates (migration 0048).
    #[allow(clippy::too_many_arguments)]
    pub async fn register_alignment(
        &self,
        source: &str,
        target: &str,
        relation: AlignmentRelation,
        confidence: f64,
        valid_lo: Option<NaiveDate>,
        valid_hi: Option<NaiveDate>,
        run_id: Option<Uuid>,
        provenance: Option<&Json>,
        actor: Option<&str>,
    ) -> Result<Uuid> {
        let c = self.pool.get().await?;
        let prov_default = Json::Object(serde_json::Map::new());
        let prov: &Json = provenance.unwrap_or(&prov_default);
        let row = c
            .query_one(
                "select donto_register_alignment($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                &[
                    &source,
                    &target,
                    &relation.as_str(),
                    &confidence,
                    &valid_lo,
                    &valid_hi,
                    &run_id,
                    &prov,
                    &actor,
                ],
            )
            .await?;
        Ok(row.get::<_, Uuid>(0))
    }

    /// Close transaction-time on an alignment edge (migration 0048). Returns
    /// true if a current row was actually retracted.
    pub async fn retract_alignment(&self, alignment_id: Uuid) -> Result<bool> {
        let c = self.pool.get().await?;
        let row = c
            .query_one("select donto_retract_alignment($1)", &[&alignment_id])
            .await?;
        Ok(row.get::<_, bool>(0))
    }

    /// Rebuild the materialized predicate closure index (migration 0051).
    /// Returns the number of rows in the closure after rebuild.
    pub async fn rebuild_predicate_closure(&self) -> Result<i32> {
        let c = self.pool.get().await?;
        let row = c
            .query_one("select donto_rebuild_predicate_closure()", &[])
            .await?;
        Ok(row.get::<_, i32>(0))
    }

    /// Upsert a predicate descriptor (migration 0049). Returns the IRI.
    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_descriptor(
        &self,
        iri: &str,
        label: &str,
        gloss: Option<&str>,
        subject_type: Option<&str>,
        object_type: Option<&str>,
        domain: Option<&str>,
        embedding_model: Option<&str>,
        embedding: Option<&[f32]>,
    ) -> Result<String> {
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "select donto_upsert_descriptor($1, $2, $3, $4, $5, $6, \
                                                 null, null, null, null, $7, $8)",
                &[
                    &iri,
                    &label,
                    &gloss,
                    &subject_type,
                    &object_type,
                    &domain,
                    &embedding_model,
                    &embedding,
                ],
            )
            .await?;
        Ok(row.get::<_, String>(0))
    }

    /// Find predicates whose descriptor embedding is closest to a query
    /// embedding (migration 0049).
    pub async fn nearest_predicates(
        &self,
        query_embedding: &[f32],
        model_id: &str,
        domain: Option<&str>,
        limit: i32,
    ) -> Result<Vec<PredicateCandidate>> {
        let c = self.pool.get().await?;
        let rows = c
            .query(
                "select iri, label, gloss, similarity \
                 from donto_nearest_predicates($1, $2, $3, null, null, $4)",
                &[&query_embedding, &model_id, &domain, &limit],
            )
            .await?;
        rows.into_iter()
            .map(|r| {
                Ok(PredicateCandidate {
                    iri: r.try_get("iri")?,
                    label: r.try_get("label")?,
                    gloss: r.try_get("gloss")?,
                    subject_type: None,
                    object_type: None,
                    similarity: r.try_get("similarity")?,
                })
            })
            .collect()
    }

    /// Open a new alignment run (migration 0050). Returns the run_id.
    pub async fn start_alignment_run(
        &self,
        run_type: &str,
        model_id: Option<&str>,
        config: Option<&Json>,
        source_predicates: Option<&[String]>,
    ) -> Result<Uuid> {
        let c = self.pool.get().await?;
        let cfg_default = Json::Object(serde_json::Map::new());
        let cfg: &Json = config.unwrap_or(&cfg_default);
        let row = c
            .query_one(
                "select donto_start_alignment_run($1, $2, null, $3, $4, '{}'::jsonb)",
                &[&run_type, &model_id, &cfg, &source_predicates],
            )
            .await?;
        Ok(row.get::<_, Uuid>(0))
    }

    /// Close an alignment run with status and counters (migration 0050).
    pub async fn complete_alignment_run(
        &self,
        run_id: Uuid,
        status: &str,
        proposed: Option<i32>,
        accepted: Option<i32>,
        rejected: Option<i32>,
    ) -> Result<()> {
        let c = self.pool.get().await?;
        c.execute(
            "select donto_complete_alignment_run($1, $2, $3, $4, $5)",
            &[&run_id, &status, &proposed, &accepted, &rejected],
        )
        .await?;
        Ok(())
    }

    /// Alignment-aware match: like [`match_pattern`] but expanding via the
    /// predicate closure (migration 0052). Each row carries `matched_via` and
    /// `alignment_confidence`.
    #[allow(clippy::too_many_arguments)]
    pub async fn match_aligned(
        &self,
        subject: Option<&str>,
        predicate: Option<&str>,
        object_iri: Option<&str>,
        scope: Option<&ContextScope>,
        polarity: Option<Polarity>,
        min_maturity: u8,
        as_of_tx: Option<DateTime<Utc>>,
        as_of_valid: Option<NaiveDate>,
        expand: bool,
        min_confidence: f64,
    ) -> Result<Vec<AlignedStatement>> {
        let scope_json: Option<Json> = scope.map(|s| s.to_json());
        let polarity_str: Option<&str> = polarity.map(Polarity::as_str);
        let c = self.pool.get().await?;
        let params: [&(dyn ToSql + Sync); 11] = [
            &subject,
            &predicate,
            &object_iri,
            &Option::<Json>::None,
            &scope_json,
            &polarity_str,
            &(min_maturity as i32),
            &as_of_tx,
            &as_of_valid,
            &expand,
            &min_confidence,
        ];
        let rows = c
            .query(
                "select * from donto_match_aligned($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
                &params,
            )
            .await?;
        rows.into_iter().map(row_to_aligned_statement).collect()
    }

    /// Strict (un-expanded) match (migration 0055).
    #[allow(clippy::too_many_arguments)]
    pub async fn match_strict(
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
            &Option::<Json>::None,
            &scope_json,
            &polarity_str,
            &(min_maturity as i32),
            &as_of_tx,
            &as_of_valid,
        ];
        let rows = c
            .query(
                "select * from donto_match_strict($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                &params,
            )
            .await?;
        rows.into_iter().map(row_to_statement).collect()
    }

    /// Materialize (or re-materialize) the canonical shadow for one statement
    /// (migration 0053). Returns the new shadow_id, or None if the statement
    /// is not currently open.
    pub async fn materialize_shadow(&self, statement_id: Uuid) -> Result<Option<Uuid>> {
        let c = self.pool.get().await?;
        let row = c
            .query_one("select donto_materialize_shadow($1)", &[&statement_id])
            .await?;
        Ok(row.get::<_, Option<Uuid>>(0))
    }

    /// Batch rebuild canonical shadows for a context (or all current
    /// statements when context is None). Returns the count rebuilt.
    pub async fn rebuild_shadows(&self, context: Option<&str>, limit: Option<i32>) -> Result<i32> {
        let c = self.pool.get().await?;
        let row = c
            .query_one("select donto_rebuild_shadows($1, $2)", &[&context, &limit])
            .await?;
        Ok(row.get::<_, i32>(0))
    }

    /// Decompose a statement into an event frame plus role triples
    /// (migration 0054). Returns the new frame_id.
    #[allow(clippy::too_many_arguments)]
    pub async fn decompose_to_frame(
        &self,
        subject: &str,
        predicate: &str,
        object_iri: Option<&str>,
        context: &str,
        frame_type: Option<&str>,
        extra_roles: Option<&Json>,
        valid_lo: Option<NaiveDate>,
        valid_hi: Option<NaiveDate>,
        actor: Option<&str>,
    ) -> Result<Uuid> {
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "select donto_decompose_to_frame($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                &[
                    &subject,
                    &predicate,
                    &object_iri,
                    &context,
                    &frame_type,
                    &extra_roles,
                    &valid_lo,
                    &valid_hi,
                    &actor,
                ],
            )
            .await?;
        Ok(row.get::<_, Uuid>(0))
    }

    /// Embedding-based extraction-time predicate candidates (migration 0056).
    pub async fn extraction_predicate_candidates(
        &self,
        embedding: &[f32],
        model_id: &str,
        domain: Option<&str>,
        subject_type: Option<&str>,
        limit: i32,
    ) -> Result<Vec<PredicateCandidate>> {
        let c = self.pool.get().await?;
        let rows = c
            .query(
                "select iri, label, gloss, subject_type, object_type, similarity \
                 from donto_extraction_predicate_candidates($1, $2, $3, $4, null, $5)",
                &[&embedding, &model_id, &domain, &subject_type, &limit],
            )
            .await?;
        rows.into_iter()
            .map(|r| {
                Ok(PredicateCandidate {
                    iri: r.try_get("iri")?,
                    label: r.try_get("label")?,
                    gloss: r.try_get("gloss")?,
                    subject_type: r.try_get("subject_type")?,
                    object_type: r.try_get("object_type")?,
                    similarity: r.try_get("similarity")?,
                })
            })
            .collect()
    }

    /// Suggest predicate alignments using trigram lexical similarity (migration 0056).
    pub async fn suggest_alignments(
        &self,
        source: &str,
        min_similarity: f64,
        limit: i32,
    ) -> Result<Vec<(String, f64, Option<String>)>> {
        let c = self.pool.get().await?;
        let rows = c
            .query(
                "select target_iri, similarity, target_label \
                 from donto_suggest_alignments($1, $2, $3)",
                &[&source, &min_similarity, &limit],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                (
                    r.get::<_, String>("target_iri"),
                    r.get::<_, f64>("similarity"),
                    r.get::<_, Option<String>>("target_label"),
                )
            })
            .collect())
    }

    /// Run batch lexical auto-alignment (migration 0056).
    /// Returns the alignment_run UUID.
    pub async fn lexical_auto_align(
        &self,
        sources: Option<&[&str]>,
        min_similarity: f64,
        actor: Option<&str>,
    ) -> Result<Uuid> {
        let c = self.pool.get().await?;
        let sources_arr: Option<Vec<String>> =
            sources.map(|s| s.iter().map(|x| x.to_string()).collect());
        let row = c
            .query_one(
                "select donto_auto_align_batch($1::text[], $2, $3)",
                &[&sources_arr, &min_similarity, &actor],
            )
            .await?;
        Ok(row.get::<_, Uuid>(0))
    }

    // ---------- Trust Kernel client wrappers ----------

    /// Top-level access check (PRD I2/I6). Returns true iff the caller
    /// has permission to perform the action against the target.
    /// Combines effective-policy AND with attestation-OR semantics.
    pub async fn authorise(
        &self,
        holder: &str,
        target_kind: &str,
        target_id: &str,
        action: &str,
    ) -> Result<bool> {
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "select donto_authorise($1, $2, $3, $4)",
                &[&holder, &target_kind, &target_id, &action],
            )
            .await?;
        Ok(row.get::<_, bool>(0))
    }

    /// Effective allowed_actions for a target (with no caller). Useful
    /// for reporting "what does this source permit anyone to do".
    pub async fn effective_actions(
        &self,
        target_kind: &str,
        target_id: &str,
    ) -> Result<serde_json::Value> {
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "select donto_effective_actions($1, $2)",
                &[&target_kind, &target_id],
            )
            .await?;
        Ok(row.get::<_, serde_json::Value>(0))
    }

    /// Quick allow/deny without caller (no attestation lookup).
    pub async fn action_allowed(
        &self,
        target_kind: &str,
        target_id: &str,
        action: &str,
    ) -> Result<bool> {
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "select donto_action_allowed($1, $2, $3)",
                &[&target_kind, &target_id, &action],
            )
            .await?;
        Ok(row.get::<_, bool>(0))
    }

    /// Assign a policy to a target (PRD §15).
    pub async fn assign_policy(
        &self,
        target_kind: &str,
        target_id: &str,
        policy_iri: &str,
        assigned_by: &str,
    ) -> Result<Uuid> {
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "select donto_assign_policy($1, $2, $3, $4, null)",
                &[&target_kind, &target_id, &policy_iri, &assigned_by],
            )
            .await?;
        Ok(row.get::<_, Uuid>(0))
    }

    /// Issue an attestation for a holder under a specific policy.
    /// Rationale is required (audit constraint).
    pub async fn issue_attestation(
        &self,
        holder: &str,
        issuer: &str,
        policy_iri: &str,
        actions: &[&str],
        purpose: &str,
        rationale: &str,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Uuid> {
        let c = self.pool.get().await?;
        let actions_owned: Vec<String> = actions.iter().map(|s| s.to_string()).collect();
        let row = c
            .query_one(
                "select donto_issue_attestation($1, $2, $3, $4::text[], $5, $6, $7, null)",
                &[
                    &holder,
                    &issuer,
                    &policy_iri,
                    &actions_owned,
                    &purpose,
                    &rationale,
                    &expires_at,
                ],
            )
            .await?;
        Ok(row.get::<_, Uuid>(0))
    }

    /// Revoke an attestation. Effect is immediate for new authorisation
    /// checks; in-flight reads in the same transaction may still proceed.
    pub async fn revoke_attestation(
        &self,
        attestation_id: Uuid,
        revoked_by: &str,
        reason: Option<&str>,
    ) -> Result<bool> {
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "select donto_revoke_attestation($1, $2, $3)",
                &[&attestation_id, &revoked_by, &reason],
            )
            .await?;
        Ok(row.get::<_, bool>(0))
    }

    /// Register a source with required policy_id on insert (PRD I2).
    /// Use this instead of `ensure_document` for new code paths.
    #[allow(clippy::too_many_arguments)]
    pub async fn register_source(
        &self,
        iri: &str,
        source_kind: &str,
        policy_iri: &str,
        media_type: Option<&str>,
        label: Option<&str>,
        source_url: Option<&str>,
        language: Option<&str>,
    ) -> Result<Uuid> {
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "select donto_register_source($1, $2, $3, coalesce($4, 'text/plain'), $5, $6, $7, '[]'::jsonb, null, null, null, null, null, '{}'::jsonb)",
                &[
                    &iri,
                    &source_kind,
                    &policy_iri,
                    &media_type,
                    &label,
                    &source_url,
                    &language,
                ],
            )
            .await?;
        Ok(row.get::<_, Uuid>(0))
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
    statement_from_row(&row)
}

fn statement_from_row(row: &Row) -> Result<Statement> {
    let object_iri: Option<String> = row.try_get("object_iri")?;
    let object_lit: Option<Json> = row.try_get("object_lit")?;
    let object = match (object_iri, object_lit) {
        (Some(i), None) => Object::Iri(i),
        (None, Some(l)) => Object::Literal(parse_literal_lenient(l)?),
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

/// Decode a `donto_statement.object_lit` value into a `Literal`,
/// tolerating one level of accidental string-wrapping. Some old
/// ingestion paths inserted `'{"v":..., "dt":...}'::jsonb` — that is,
/// a JSON *string* containing JSON — instead of the parsed object.
/// We treat those as recoverable: if the direct decode fails and the
/// payload is a string, parse that string as JSON and try again.
fn parse_literal_lenient(j: Json) -> Result<Literal> {
    match serde_json::from_value::<Literal>(j.clone()) {
        Ok(l) => Ok(l),
        Err(direct_err) => {
            if let serde_json::Value::String(s) = j {
                let inner: serde_json::Value = serde_json::from_str(&s).map_err(|_| {
                    Error::Invalid(format!(
                        "object_lit is a string but not valid JSON: {direct_err}"
                    ))
                })?;
                serde_json::from_value::<Literal>(inner).map_err(|e| {
                    Error::Invalid(format!("object_lit unwrap+decode failed: {e}"))
                })
            } else {
                Err(direct_err.into())
            }
        }
    }
}

fn row_to_aligned_statement(row: Row) -> Result<AlignedStatement> {
    let statement = statement_from_row(&row)?;
    Ok(AlignedStatement {
        statement,
        matched_via: row.try_get("matched_via")?,
        alignment_confidence: row.try_get("alignment_confidence")?,
    })
}

#[cfg(test)]
mod parse_literal_tests {
    use super::*;

    #[test]
    fn parses_direct_object_form() {
        let j = serde_json::json!({"v": 30, "dt": "xsd:integer"});
        let l = parse_literal_lenient(j).unwrap();
        assert_eq!(l.dt, "xsd:integer");
        assert_eq!(l.v, serde_json::json!(30));
    }

    #[test]
    fn parses_double_encoded_string_form() {
        // Some old ingestion paths inserted the JSON object as a
        // JSON-encoded string. Make sure we recover.
        let j = serde_json::Value::String(
            r#"{"v": 21, "dt": "xsd:integer"}"#.to_string(),
        );
        let l = parse_literal_lenient(j).unwrap();
        assert_eq!(l.dt, "xsd:integer");
        assert_eq!(l.v, serde_json::json!(21));
    }

    #[test]
    fn parses_double_encoded_with_lang() {
        let j = serde_json::Value::String(
            r#"{"v": "hello", "dt": "rdf:langString", "lang": "en"}"#.to_string(),
        );
        let l = parse_literal_lenient(j).unwrap();
        assert_eq!(l.lang.as_deref(), Some("en"));
        assert_eq!(l.v, serde_json::json!("hello"));
    }

    #[test]
    fn rejects_garbage_string() {
        let j = serde_json::Value::String("not json at all".to_string());
        assert!(parse_literal_lenient(j).is_err());
    }

    #[test]
    fn rejects_non_object_non_string_value() {
        let j = serde_json::Value::Number(serde_json::Number::from(42));
        assert!(parse_literal_lenient(j).is_err());
    }
}
