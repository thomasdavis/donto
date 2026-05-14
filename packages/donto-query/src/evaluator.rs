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
use std::collections::BTreeMap;

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

    // DontoQL v2 clauses that have parsed shape but no executable
    // kernel yet. Declare the gap honestly rather than silently
    // returning misleading results. Tracking: PRD §11 v2 verdicts.
    if q.policy_allows.is_some() {
        return Err(EvalError::Unsupported(
            "POLICY ALLOWS evaluation requires the Trust Kernel \
             policy-join (PRD M0); statement→source→policy lookup is \
             not yet wired into donto_match. Statement parses but does \
             not filter."
                .into(),
        ));
    }
    if q.schema_lens.is_some() {
        return Err(EvalError::Unsupported(
            "SCHEMA_LENS evaluation requires the schema-lens registry \
             (PRD §6.x); lens-aware predicate expansion is not yet \
             implemented. Statement parses but does not filter."
                .into(),
        ));
    }
    if q.expands_from.is_some() {
        return Err(EvalError::Unsupported(
            "EXPANDS_FROM concept(..) USING schema_lens(..) requires \
             the schema-lens registry + concept resolver (PRD §11.2 \
             example 1). Statement parses but does not filter."
                .into(),
        ));
    }
    if matches!(
        q.order_by,
        OrderBy::ContradictionPressureDesc | OrderBy::ContradictionPressureAsc
    ) {
        return Err(EvalError::Unsupported(
            "ORDER BY contradiction_pressure needs the evaluator to \
             retain statement_ids per binding row so it can join \
             against donto_contradiction_frontier. Refactor pending."
                .into(),
        ));
    }
    // WITH evidence is recorded but the row shape today is Bindings
    // only — evidence is not yet attached. Parse, then carry on; this
    // is a future-shape directive, not a filter.
    let _ = q.evidence_shape;

    let polarity = q.polarity;
    let scope = q.scope.clone();

    let mut current: Vec<Bindings> = vec![Bindings::new()];
    for pat in &q.patterns {
        let mut next: Vec<Bindings> = Vec::new();
        for env in &current {
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

            for st in stmts {
                if let Some(env2) = unify(env, pat, &st) {
                    next.push(env2);
                }
            }
        }
        current = next;
        if current.is_empty() {
            break;
        }
    }

    // Apply FILTER expressions.
    current.retain(|env| q.filters.iter().all(|f| eval_filter(f, env)));

    // PROJECT.
    let rows: Vec<EvalRow> = current
        .into_iter()
        .map(|env| {
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
