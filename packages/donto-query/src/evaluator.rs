//! Algebra evaluator. Compiles a [`Query`] to a series of `donto_match`
//! calls, joined on shared variables, and projects the requested columns.
//!
//! Phase 4 evaluator is straightforward nested-loop: for each pattern,
//! call match_pattern with the bound terms substituted; cartesian-join
//! results on shared variable bindings; apply filters; project; limit.
//!
//! This is correct but not fast. The query planner is Phase 10 (see
//! PRD §26 Phase 10: "Query planner improvements").

use crate::algebra::*;
use donto_client::{ContextScope, DontoClient, Object, Statement};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use uuid::Uuid;

/// Pull a [`Statement`] out of either match path.
trait IntoStatement {
    fn into_statement(self) -> Statement;
}

impl IntoStatement for Statement {
    fn into_statement(self) -> Statement {
        self
    }
}

impl IntoStatement for donto_client::AlignedStatement {
    fn into_statement(self) -> Statement {
        self.statement
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EvalError {
    #[error("client error: {0}")]
    Client(#[from] donto_client::Error),
    #[error("unsupported algebra feature: {0}")]
    Unsupported(String),
    #[error("type error: {0}")]
    Type(String),
}

/// A single solution: variable name → bound value (term).
pub type Bindings = BTreeMap<String, Term>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalRow(pub Bindings);

pub async fn evaluate(client: &DontoClient, q: &Query) -> Result<Vec<EvalRow>, EvalError> {
    if q.patterns.is_empty() {
        return Ok(vec![]);
    }

    // PRESET resolution: translate the named scope preset into
    // adjustments to scope / maturity / as-of before pattern matching.
    let q_resolved = apply_preset(client, q).await?;
    let q = &q_resolved;

    // Up-front validation: fail fast on a malformed POLICY ALLOWS
    // action before any pattern matching, otherwise unconstrained
    // queries scan the whole store before the per-statement check
    // surfaces the error.
    if let Some(action) = q.policy_allows.as_deref() {
        validate_policy_action(action)?;
    }

    // EXPANDS_FROM concept(C) USING schema_lens(L) — resolve C to a
    // set of predicates via lens-scoped alignments, then filter
    // matched statements to those whose predicate is in the set.
    // The set is computed once per query.
    let expands_predicates: Option<std::collections::HashSet<String>> =
        if let Some(ef) = &q.expands_from {
            Some(load_concept_predicate_set(client, &ef.concept, &ef.schema_lens).await?)
        } else {
            None
        };

    // WITH evidence is recorded but the row shape today is Bindings
    // only — evidence is not yet attached. Parse, then carry on; this
    // is a future-shape directive, not a filter.
    let _ = q.evidence_shape;

    let polarity = q.polarity;
    let scope = q.scope.clone();

    let mut current: Vec<(Bindings, Option<Uuid>)> = vec![(Bindings::new(), None)];
    for pat in &q.patterns {
        let mut next: Vec<(Bindings, Option<Uuid>)> = Vec::new();
        for (env, _prev_id) in &current {
            let s_bound = substitute(&pat.subject, env);
            let p_bound = substitute(&pat.predicate, env);
            let o_bound = substitute(&pat.object, env);

            let subject = term_to_iri(&s_bound)?;
            let predicate = term_to_iri(&p_bound)?;
            let object_iri_filter = match &o_bound {
                Term::Iri(s) => Some(s.clone()),
                _ => None,
            };

            let mut stmts: Vec<Statement> = match q.predicate_expansion {
                PredicateExpansion::Expand => {
                    client
                        .match_pattern(
                            subject.as_deref(),
                            predicate.as_deref(),
                            object_iri_filter.as_deref(),
                            scope.as_ref(),
                            polarity,
                            q.min_maturity,
                            q.as_of_tx,
                            None,
                        )
                        .await?
                }
                PredicateExpansion::Strict => {
                    client
                        .match_strict(
                            subject.as_deref(),
                            predicate.as_deref(),
                            object_iri_filter.as_deref(),
                            scope.as_ref(),
                            polarity,
                            q.min_maturity,
                            q.as_of_tx,
                            None,
                        )
                        .await?
                }
                PredicateExpansion::ExpandAbove(pct) => client
                    .match_aligned(
                        subject.as_deref(),
                        predicate.as_deref(),
                        object_iri_filter.as_deref(),
                        scope.as_ref(),
                        polarity,
                        q.min_maturity,
                        q.as_of_tx,
                        None,
                        true,
                        pct as f64 / 100.0,
                    )
                    .await?
                    .into_iter()
                    .map(IntoStatement::into_statement)
                    .collect(),
            };

            // Sparse-overlay filters (MODALITY, EXTRACTION_LEVEL).
            // One round-trip per filter per pattern; drops matched
            // statements that lack the overlay row or don't satisfy
            // the requested value set. Both clauses default to
            // "any" when None, so the cost is zero in that case.
            if let Some(allowed) = &q.modality {
                stmts = retain_with_overlay(
                    client,
                    stmts,
                    "donto_stmt_modality",
                    "modality",
                    allowed,
                )
                .await?;
            }
            if let Some(allowed) = &q.extraction_level {
                stmts = retain_with_overlay(
                    client,
                    stmts,
                    "donto_stmt_extraction_level",
                    "level",
                    allowed,
                )
                .await?;
            }
            if let Some(action) = q.policy_allows.as_deref() {
                stmts = retain_policy_allows(client, stmts, action).await?;
            }
            if let Some(lens) = q.schema_lens.as_deref() {
                stmts = retain_in_schema_lens(client, stmts, lens).await?;
            }
            if let Some(allowed_predicates) = &expands_predicates {
                stmts.retain(|s| allowed_predicates.contains(&s.predicate));
            }

            for st in stmts {
                if let Some(env2) = unify(env, pat, &st) {
                    next.push((env2, Some(st.statement_id)));
                }
            }
        }
        current = next;
        if current.is_empty() {
            break;
        }
    }

    // Apply FILTER expressions.
    current.retain(|(env, _)| q.filters.iter().all(|f| eval_filter(f, env)));

    // ORDER BY contradiction_pressure — sort by net attack pressure of
    // the most recently matched statement_id. Computed in SQL via
    // donto_contradiction_frontier; rows without a frontier entry
    // (no rebuttals/undercuts against them) sort to pressure = 0.
    if matches!(
        q.order_by,
        OrderBy::ContradictionPressureDesc | OrderBy::ContradictionPressureAsc
    ) {
        let frontier = load_contradiction_pressure(client).await?;
        let desc = matches!(q.order_by, OrderBy::ContradictionPressureDesc);
        current.sort_by(|a, b| {
            // contradiction_pressure = attacks - supports = -net_pressure.
            // Higher pressure = more contradicted.
            let pa = pressure_for(&frontier, a.1);
            let pb = pressure_for(&frontier, b.1);
            if desc {
                pb.cmp(&pa)
            } else {
                pa.cmp(&pb)
            }
        });
    }

    // PROJECT.
    let rows: Vec<EvalRow> = current
        .into_iter()
        .map(|(env, _id)| {
            if q.project.is_empty() {
                EvalRow(env)
            } else {
                let mut out = Bindings::new();
                for v in &q.project {
                    if let Some(t) = env.get(v) {
                        out.insert(v.clone(), t.clone());
                    }
                }
                EvalRow(out)
            }
        })
        .collect();

    // OFFSET / LIMIT.
    let off = q.offset.unwrap_or(0) as usize;
    let lim = q.limit.unwrap_or(rows.len() as u64) as usize;
    Ok(rows.into_iter().skip(off).take(lim).collect())
}

/// Build a `statement_id -> contradiction_pressure` map by calling
/// `donto_contradiction_frontier(NULL)`. Contradiction pressure is
/// defined as `attack_count - support_count` (i.e. the negation of
/// `net_pressure`) so higher = more contradicted, matching the natural
/// reading of `ORDER BY contradiction_pressure DESC`.
async fn load_contradiction_pressure(
    client: &DontoClient,
) -> Result<HashMap<Uuid, i64>, EvalError> {
    let conn = client
        .pool()
        .get()
        .await
        .map_err(|e| EvalError::Client(donto_client::Error::Pool(e)))?;
    let rows = conn
        .query(
            "select statement_id, attack_count, support_count \
             from donto_contradiction_frontier(NULL)",
            &[],
        )
        .await
        .map_err(|e| EvalError::Client(donto_client::Error::Postgres(e)))?;
    let mut out = HashMap::with_capacity(rows.len());
    for r in rows {
        let id: Uuid = r.get(0);
        let attacks: i64 = r.get(1);
        let supports: i64 = r.get(2);
        out.insert(id, attacks - supports);
    }
    Ok(out)
}

fn pressure_for(map: &HashMap<Uuid, i64>, id: Option<Uuid>) -> i64 {
    id.and_then(|i| map.get(&i).copied()).unwrap_or(0)
}

/// SCHEMA_LENS: retain statements that have a secondary-context
/// membership with `role='schema_lens'` for the given lens IRI.
/// A statement's primary context is NOT considered a lens membership
/// — schema_lens is explicitly a *secondary* attachment (see
/// migration `0103_multi_context.sql`, role enum).
async fn retain_in_schema_lens(
    client: &DontoClient,
    stmts: Vec<Statement>,
    lens_iri: &str,
) -> Result<Vec<Statement>, EvalError> {
    if stmts.is_empty() {
        return Ok(stmts);
    }
    let ids: Vec<uuid::Uuid> = stmts.iter().map(|s| s.statement_id).collect();
    let conn = client
        .pool()
        .get()
        .await
        .map_err(|e| EvalError::Client(donto_client::Error::Pool(e)))?;
    let rows = conn
        .query(
            "select statement_id from donto_statement_context \
             where statement_id = any($1::uuid[]) \
               and context = $2 and role = 'schema_lens'",
            &[&ids, &lens_iri],
        )
        .await
        .map_err(|e| EvalError::Client(donto_client::Error::Postgres(e)))?;
    let keep: std::collections::HashSet<uuid::Uuid> =
        rows.iter().map(|r| r.get::<_, uuid::Uuid>(0)).collect();
    Ok(stmts
        .into_iter()
        .filter(|s| keep.contains(&s.statement_id))
        .collect())
}

/// EXPANDS_FROM concept(C) USING schema_lens(L): resolve C to its
/// expansion-set under the named lens. The set is every alignment
/// edge with `source_iri = C AND scope = L AND safe_for_query_expansion`,
/// plus C itself (the concept is its own predicate root).
async fn load_concept_predicate_set(
    client: &DontoClient,
    concept: &str,
    lens: &str,
) -> Result<std::collections::HashSet<String>, EvalError> {
    let conn = client
        .pool()
        .get()
        .await
        .map_err(|e| EvalError::Client(donto_client::Error::Pool(e)))?;
    let rows = conn
        .query(
            "select target_iri from donto_predicate_alignment \
             where source_iri = $1 and scope = $2 \
               and upper(tx_time) is null \
               and safe_for_query_expansion = true",
            &[&concept, &lens],
        )
        .await
        .map_err(|e| EvalError::Client(donto_client::Error::Postgres(e)))?;
    let mut set: std::collections::HashSet<String> = rows
        .iter()
        .map(|r| r.get::<_, String>(0))
        .collect();
    set.insert(concept.to_string());
    Ok(set)
}

/// Translate a `PRESET <name>` clause into concrete query adjustments.
/// Six presets are honoured:
///
/// * `latest` — current state (default; no adjustment)
/// * `raw` — include all maturities including E0 (clear min_maturity)
/// * `curated` — require maturity ≥ E2 (evidence-supported)
/// * `under_hypothesis` — restrict scope to hypothesis-kind contexts
/// * `as_of:<rfc3339-utc>` — bitemporal time-travel against tx_time
/// * `anywhere` — drop any scope restriction (override caller scope)
///
/// Unknown preset names return an `Unsupported` error rather than
/// silently doing nothing.
async fn apply_preset(client: &DontoClient, q: &Query) -> Result<Query, EvalError> {
    let mut out = q.clone();
    let Some(preset) = q.scope_preset.as_deref() else {
        return Ok(out);
    };
    let preset = preset.trim();
    let lower = preset.to_lowercase();
    match lower.as_str() {
        "" | "latest" => {
            // Default: open tx_time only — match_pattern's default.
            // No adjustment.
        }
        "raw" => {
            out.min_maturity = 0;
        }
        "curated" => {
            // Evidence-supported (E2 = stored value 2) is the curated floor.
            if out.min_maturity < 2 {
                out.min_maturity = 2;
            }
        }
        "anywhere" => {
            out.scope = None;
        }
        "under_hypothesis" => {
            let conn = client
                .pool()
                .get()
                .await
                .map_err(|e| EvalError::Client(donto_client::Error::Pool(e)))?;
            let rows = conn
                .query(
                    "select iri from donto_context where kind = 'hypothesis'",
                    &[],
                )
                .await
                .map_err(|e| EvalError::Client(donto_client::Error::Postgres(e)))?;
            if rows.is_empty() {
                // No hypothesis contexts → empty scope (returns no rows).
                out.scope = Some(ContextScope::default());
            } else {
                let iris: Vec<String> = rows.iter().map(|r| r.get::<_, String>(0)).collect();
                out.scope = Some(ContextScope::any_of(iris));
            }
        }
        _ if lower.starts_with("as_of:") || lower.starts_with("as_of ") => {
            // PRESET-level as_of would set Query.as_of_tx, but the
            // current Query doesn't carry a tx_at field — that's a
            // match_pattern parameter consumed by match calls. Wire
            // through a thin override on the query and have the loop
            // honour it.
            let ts_str = preset
                .split_once(|c: char| c == ':' || c.is_whitespace())
                .map(|x| x.1.trim())
                .unwrap_or("");
            if ts_str.is_empty() {
                return Err(EvalError::Unsupported(
                    "PRESET as_of requires a timestamp (as_of:<RFC3339>)".into(),
                ));
            }
            let ts = chrono::DateTime::parse_from_rfc3339(ts_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .map_err(|e| {
                    EvalError::Unsupported(format!(
                        "PRESET as_of: invalid RFC3339 timestamp `{ts_str}`: {e}"
                    ))
                })?;
            out.as_of_tx = Some(ts);
        }
        other => {
            return Err(EvalError::Unsupported(format!(
                "unknown PRESET `{other}` (valid: latest|raw|curated|under_hypothesis|as_of:<ts>|anywhere)"
            )));
        }
    }
    Ok(out)
}

fn substitute(t: &Term, env: &Bindings) -> Term {
    match t {
        Term::Var(n) => env.get(n).cloned().unwrap_or_else(|| t.clone()),
        _ => t.clone(),
    }
}

fn term_to_iri(t: &Term) -> Result<Option<String>, EvalError> {
    Ok(match t {
        Term::Iri(s) => Some(s.clone()),
        Term::Var(_) => None,
        Term::Literal { .. } => return Err(EvalError::Type("literal where IRI expected".into())),
    })
}

fn unify(env: &Bindings, pat: &Pattern, st: &Statement) -> Option<Bindings> {
    let mut out = env.clone();
    bind(&mut out, &pat.subject, &Term::Iri(st.subject.clone()))?;
    bind(&mut out, &pat.predicate, &Term::Iri(st.predicate.clone()))?;
    let obj_term = match &st.object {
        Object::Iri(i) => Term::Iri(i.clone()),
        Object::Literal(l) => Term::Literal {
            v: l.v.clone(),
            dt: l.dt.clone(),
            lang: l.lang.clone(),
        },
    };
    bind(&mut out, &pat.object, &obj_term)?;
    Some(out)
}

fn bind(env: &mut Bindings, pat: &Term, val: &Term) -> Option<()> {
    match pat {
        Term::Var(name) => {
            if let Some(existing) = env.get(name) {
                if existing == val {
                    Some(())
                } else {
                    None
                }
            } else {
                env.insert(name.clone(), val.clone());
                Some(())
            }
        }
        _ => {
            if pat == val {
                Some(())
            } else {
                None
            }
        }
    }
}

fn eval_filter(f: &Filter, env: &Bindings) -> bool {
    let resolve = |t: &Term| -> Option<Term> {
        match t {
            Term::Var(n) => env.get(n).cloned(),
            other => Some(other.clone()),
        }
    };
    match f {
        Filter::Eq(a, b) => resolve(a) == resolve(b),
        Filter::Neq(a, b) => resolve(a) != resolve(b),
        Filter::Bound(n) => env.contains_key(n),
        Filter::Lt(a, b) | Filter::Le(a, b) | Filter::Gt(a, b) | Filter::Ge(a, b) => {
            match (resolve(a), resolve(b)) {
                (Some(av), Some(bv)) => match (literal_num(&av), literal_num(&bv)) {
                    (Some(x), Some(y)) => match f {
                        Filter::Lt(..) => x < y,
                        Filter::Le(..) => x <= y,
                        Filter::Gt(..) => x > y,
                        Filter::Ge(..) => x >= y,
                        _ => false,
                    },
                    _ => false,
                },
                _ => false,
            }
        }
    }
}

fn literal_num(t: &Term) -> Option<f64> {
    match t {
        Term::Literal { v, .. } => v.as_f64().or_else(|| v.as_i64().map(|i| i as f64)),
        _ => None,
    }
}

#[allow(dead_code)]
fn _unused(_: ContextScope) {}

/// Drop statements whose `statement_id` lacks an overlay row in
/// `<table>` with `<col> in (allowed)`. Used by the MODALITY and
/// EXTRACTION_LEVEL clauses to filter through sparse overlays.
///
/// Returns the input unchanged if `allowed` is empty (no filter).
/// One SQL round-trip per call regardless of input size.
async fn retain_with_overlay(
    client: &DontoClient,
    stmts: Vec<Statement>,
    table: &str,
    col: &str,
    allowed: &[String],
) -> Result<Vec<Statement>, EvalError> {
    if allowed.is_empty() || stmts.is_empty() {
        return Ok(stmts);
    }
    // Table and column names are not user-supplied — callers pass
    // string literals — so direct interpolation is safe. (Defence:
    // pattern-match against an allow-list.)
    let (table, col) = match (table, col) {
        ("donto_stmt_modality", "modality") => ("donto_stmt_modality", "modality"),
        ("donto_stmt_extraction_level", "level") => ("donto_stmt_extraction_level", "level"),
        _ => {
            return Err(EvalError::Unsupported(format!(
                "retain_with_overlay: unknown (table, col) = ({table}, {col})"
            )));
        }
    };
    let ids: Vec<uuid::Uuid> = stmts.iter().map(|s| s.statement_id).collect();
    let allowed_vec: Vec<String> = allowed.iter().map(|s| s.to_string()).collect();
    let sql = format!(
        "select statement_id from {table} \
         where statement_id = any($1::uuid[]) and {col} = any($2::text[])"
    );
    let conn = client
        .pool()
        .get()
        .await
        .map_err(|e| EvalError::Client(donto_client::Error::Pool(e)))?;
    let rows = conn
        .query(sql.as_str(), &[&ids, &allowed_vec])
        .await
        .map_err(|e| EvalError::Client(donto_client::Error::Postgres(e)))?;
    let keep: std::collections::HashSet<uuid::Uuid> =
        rows.iter().map(|r| r.get::<_, uuid::Uuid>(0)).collect();
    Ok(stmts
        .into_iter()
        .filter(|s| keep.contains(&s.statement_id))
        .collect())
}

/// Drop statements whose policy explicitly disallows the named
/// action. Policy chain: statement → evidence_link → document →
/// policy_capsule. Action keys live in `allowed_actions` JSONB.
///
/// Permissive defaults:
///   * statement with **no** evidence link → kept (no policy claim)
///   * evidence link with no policy → kept (untyped source)
///   * policy with no entry for the action → dropped (closed-world;
///     unknown_restricted policy_kind defaults to deny across all
///     actions in the migration, so this matches that intent)
///   * `revocation_status != 'active'` → policy is ignored (treated
///     as no policy at all). Tests for revocation are M0 work.
/// Whitelist of POLICY ALLOWS action names against the policy
/// capsule's documented key set. Kept at module scope so the
/// pre-flight validator and the per-statement retainer share one
/// source of truth.
const KNOWN_POLICY_ACTIONS: &[&str] = &[
    "read_metadata",
    "read_content",
    "quote",
    "view_anchor_location",
    "derive_claims",
    "derive_embeddings",
    "translate",
    "summarize",
    "export_claims",
    "export_sources",
    "export_anchors",
    "train_model",
    "publish_release",
    "share_with_third_party",
    "federated_query",
];

fn validate_policy_action(action: &str) -> Result<(), EvalError> {
    if !KNOWN_POLICY_ACTIONS.iter().any(|a| *a == action) {
        return Err(EvalError::Unsupported(format!(
            "POLICY ALLOWS unknown action `{action}` (valid: {})",
            KNOWN_POLICY_ACTIONS.join(", ")
        )));
    }
    Ok(())
}

async fn retain_policy_allows(
    client: &DontoClient,
    stmts: Vec<Statement>,
    action: &str,
) -> Result<Vec<Statement>, EvalError> {
    if stmts.is_empty() {
        return Ok(stmts);
    }
    validate_policy_action(action)?;
    let ids: Vec<uuid::Uuid> = stmts.iter().map(|s| s.statement_id).collect();
    let conn = client
        .pool()
        .get()
        .await
        .map_err(|e| EvalError::Client(donto_client::Error::Pool(e)))?;
    // Keep statement if no evidence link with an active policy
    // explicitly denies the action.
    let sql = format!(
        "select s.statement_id \
         from unnest($1::uuid[]) as s(statement_id) \
         where not exists ( \
           select 1 from donto_evidence_link el \
           join donto_document d on d.document_id = el.target_document_id \
           join donto_policy_capsule p on p.policy_iri = d.policy_id \
           where el.statement_id = s.statement_id \
             and upper(el.tx_time) is null \
             and p.revocation_status = 'active' \
             and coalesce((p.allowed_actions->>'{action}')::boolean, false) = false \
         )"
    );
    let rows = conn
        .query(sql.as_str(), &[&ids])
        .await
        .map_err(|e| EvalError::Client(donto_client::Error::Postgres(e)))?;
    let keep: std::collections::HashSet<uuid::Uuid> =
        rows.iter().map(|r| r.get::<_, uuid::Uuid>(0)).collect();
    Ok(stmts
        .into_iter()
        .filter(|s| keep.contains(&s.statement_id))
        .collect())
}
