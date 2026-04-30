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

            let stmts: Vec<Statement> = match q.predicate_expansion {
                PredicateExpansion::Expand => client
                    .match_pattern(
                        subject.as_deref(),
                        predicate.as_deref(),
                        object_iri_filter.as_deref(),
                        scope.as_ref(),
                        polarity,
                        q.min_maturity,
                        None,
                        None,
                    )
                    .await?,
                PredicateExpansion::Strict => client
                    .match_strict(
                        subject.as_deref(),
                        predicate.as_deref(),
                        object_iri_filter.as_deref(),
                        scope.as_ref(),
                        polarity,
                        q.min_maturity,
                        None,
                        None,
                    )
                    .await?,
                PredicateExpansion::ExpandAbove(pct) => client
                    .match_aligned(
                        subject.as_deref(),
                        predicate.as_deref(),
                        object_iri_filter.as_deref(),
                        scope.as_ref(),
                        polarity,
                        q.min_maturity,
                        None,
                        None,
                        true,
                        pct as f64 / 100.0,
                    )
                    .await?
                    .into_iter()
                    .map(IntoStatement::into_statement)
                    .collect(),
            };

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
