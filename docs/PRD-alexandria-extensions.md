# PRD: donto semantic extensions from Alexandria 2.0

Status: proposal. Author: Thomas Davis. Date: 2026-04-19.
Companion to: [`PRD.md`](../PRD.md). This PRD extends, never overrides.

## 0. One-paragraph summary

Alexandria 2.0 (the Open World Library concept, ca. 2020) is a product sketch full of graph-shaped ideas. This PRD pulls out the ones that are core-donto work — shapes, rules, overlays, registry mechanics, ingestion patterns — and excludes everything product-facing. No auth, no feeds, no recommendations, no moderation UI, no access control, no notifications, no embeddings, no media storage. Nine extensions below; each is a donto core change that makes the quad store strictly more expressive for real knowledge-graph workloads, and each is independently implementable within the phased plan.

## 1. Principles inherited from `PRD.md`

All design principles in `PRD.md` §3 apply unchanged. Every extension below is a statement, a shape, a rule, or a registry entry. Nothing in this document can delete from `donto_statement`, reject a contradiction, or put Lean in the ingest path. If a proposed extension here conflicts with `PRD.md`, `PRD.md` wins.

## 2. In scope / out of scope

**In scope.** Core-donto extensions:
1. Bitemporal canonicals (canonical IRIs that drift over time).
2. Reaction/endorsement meta-statement pattern.
3. Rule-derived aggregates (consensus, edge weight).
4. Retrofit ingest (new predicate over existing subjects, backdated `valid_time`).
5. Shape reports as first-class attached annotations.
6. Parallel-literal alignment (translations, paraphrases, dialect variants).
7. Environment/bias overlay on contexts.
8. Time-binned aggregation helpers in DontoQL.
9. Full-text search over literal values, Postgres-native.

**Out of scope — rabbit hole.** Anything that requires a product team:
- User accounts, sessions, anonymity UX.
- Feeds, notifications, pub/sub, streaming deltas.
- Recommendations, embeddings, pgvector, similarity search.
- Moderation workflows, ACLs, private/public toggles, row-level security recipes.
- Media / blob storage.
- UI, visualization, mind-map rendering, span-anchored comments on document bodies.

Span-anchored comments are borderline: they're graph-shaped, but pull in a document-body model and a range-offset primitive that doesn't earn its keep without a product. Deferred.

---

## 3. Extensions

### 3.1 Bitemporal canonicals

**Problem.** The word "lit" meant *bright* in 1950 and *excellent* in 2020. The predicate registry today maps alias → canonical globally. Under open-world predicates (PRD §3.4), the canonical itself needs a valid-time interval.

**Proposal.** Extend `donto_predicate_registry` (or equivalent) so each alias → canonical mapping carries a `valid_time` interval. Query-time resolution picks the canonical whose valid interval contains the statement's own `valid_time_from`. If no interval matches, fall back to the alias as-is (open-world).

**Invariant.** Re-ingesting the same alias under a different canonical at a different valid_time is a legal coexistence, not a correction.

**Non-goal.** Inferring the drift automatically. Alias edges are human-curated (or LLM-extracted at Level 0, same as statements).

**Maturity interaction.** A Level 1 statement whose predicate's canonical drifted since ingest stays Level 1; resolution is a read-time join, not a rewrite.

### 3.2 Reaction meta-statement pattern

**Problem.** "I agree with statement S", "I disagree with S, see article A for why", "I've changed my mind about S" recur across any knowledge graph that captures human input. Encoding each as ad-hoc predicates scatters the pattern.

**Proposal.** Register a small canonical vocabulary under `donto:` for reactions:
- `donto:endorses` (polarity asserted)
- `donto:rejects` (polarity negated)
- `donto:cites` (optional object: supporting statement/IRI)
- `donto:supersedes` (subject statement replaces object statement, both remain in tx-history)

These are ordinary predicates. Subjects and objects of reactions are *statement IRIs*, which requires RDF-star-style statement identity — already present in donto via the `stmt_id` column. Reactions themselves get contexts, so "who reacted" is provenance for free.

**Invariant.** A reaction does not change the reacted-to statement's polarity, confidence, or maturity. It is a sibling statement in the graph.

**Aggregate derivation.** "How many endorse S?" is a Level-3 rule (see §3.3).

### 3.3 Rule-derived aggregates

**Problem.** Edge weight from endorsement count, consensus score, tag popularity, "strength of connection thicker when more people back the tag" — all are aggregates over reaction statements. PRD's maturity Level 3 says rule-derived statements are legal. This subsection names the pattern.

**Proposal.** Standardize an aggregate rule kind in the Lean sidecar whose output is a statement of shape `(subject, donto:weight, literal_number, scope_context)` with `source_stmt` pointing at every input reaction. Re-running the rule over the same input context reproduces the aggregate.

**Invariant.** Aggregate statements are themselves subject to retract/correct. If the rule is re-run with a different input window, the old aggregate's `tx_time` closes; history preserved.

**DontoQL surface.** A `with weights(scope=ctx)` clause that projects the derived weight as a virtual column — no write required for ephemeral reads.

**Non-goal.** Ranking, ordering, trending. The aggregate is a number; how you sort by it is a query concern, not a donto concern (PRD §3.10).

### 3.4 Retrofit ingest

**Problem.** A new predicate (e.g. `openworld:flagged_as_biased`) needs to be applied to articles that were ingested years earlier. Setting `valid_time_from = now()` misrepresents when the tag became valid; setting it to the article's creation date needs to be explicit and auditable.

**Proposal.** An ingest mode `retrofit` that requires the caller to supply both:
- `valid_time_from` — explicitly backdated.
- `tx_time_from` — always `now()`, never backdated.

This is already physically possible today. The proposal is to (a) name it, (b) require the caller to pass a `retrofit_reason` literal into a dedicated context overlay so the intent is queryable, and (c) ensure ingestion adapters (JSON-LD, CSV, etc.) expose the mode safely without accidental backdating.

**Invariant.** `tx_time_from` is never retrofitted. Backdating transaction time breaks audit — the whole point of bitemporal.

### 3.5 Shape reports as attached annotations

**Problem.** PRD §3.5 says shapes are overlays, not schemas, and shape failures are reports. The storage shape for "report attached to statement S" isn't spelled out. "Flag this racist tag" and "this statement violates shape X" are the same mechanism; give it a form.

**Proposal.** A `donto_shape_report` table with `(stmt_id, shape_iri, verdict, context_id, tx_time, detail_literal)` where verdict ∈ {pass, warn, violate}. Shape reports are generated by the Lean sidecar (shapes and rules live in Lean, PRD §3.6) and attached asynchronously. Queries can filter by presence/absence of reports (`where exists shape_report(shape=X, verdict=violate)`).

**Invariant.** A shape report never removes or modifies the underlying statement. Reports are additive.

**Reuse.** User-submitted "flags" from a product layer land in the same table, with the flag author's context. No separate "flag" system.

### 3.6 Parallel-literal alignment

**Problem.** Bible translations side-by-side; paraphrases; dialect variants; "the same claim in French and in Scottish English". RDF's `@lang` tag is a property of the literal, but alignment across literals of the same meaning isn't modeled.

**Proposal.** A shape `donto:SameMeaning` whose subject and object are both statements (RDF-star). Passes when two statements are human- or rule-asserted to be translations/paraphrases. Reuses the reaction mechanism from §3.2 (a `SameMeaning` assertion *is* a reaction).

**Non-goal.** Automated translation, quality scoring, bilingual voting UI. Those are product. What donto guarantees is that once someone asserts alignment, the two statements are queryable as a cluster.

### 3.7 Environment / bias overlay

**Problem.** "California Cold" — the same predicate-object pair (`temperature = cold`) means different things from different speakers. donto already has confidence and modality as sparse overlays (PRD §3 truth model). Environment is a third overlay.

**Proposal.** A context-level overlay `donto_context_env` carrying structured qualifiers: `location`, `climate_band`, `speaker_demographic`, etc. Keys are open-world — the overlay is a bag of `(key, literal)` pairs attached to a context, not a fixed schema. Query-time filters can require/exclude environment keys.

**Invariant.** The overlay is advisory, not filter-enforcing. A query that ignores the overlay gets all statements; a query that filters on it gets a narrower slice.

**Why context-level not statement-level.** Bias is almost always a property of the speaker, not the atom. A user-scoped context already carries the speaker identity; the overlay carries their situational qualifiers.

### 3.8 Time-binned aggregation helpers

**Problem.** "How did the canonical for `lit` shift decade by decade?" is derivable from `valid_time` but awkward to write. This is a pure ergonomics gap.

**Proposal.** DontoQL syntax `group by valid_time bucket(interval)` producing `(bucket_start, bucket_end, count_or_agg)` rows. Buckets are half-open, aligned to a user-supplied epoch. Works over any bitemporal column.

**Invariant.** PRD §3.10 (no hidden ordering) still applies — bucket rows are unordered unless the query says `order by bucket_start`.

### 3.9 Full-text search over literal values

**Problem.** "Search the library for mentions of 'intersectionality'" lands on literal-valued statements (`(subject, rdfs:label, "...")`, `(subject, schema:description, "...")`). donto has no text index today.

**Proposal.** A generated `tsvector` column on literal-valued statement rows, with a GIN index. DontoQL exposes a `match_text('query')` predicate that compiles to the Postgres FTS operator.

**Scope.** English default, with per-statement language tag driving the text-search configuration. No synonym expansion, no stemming beyond Postgres defaults, no ranking beyond `ts_rank_cd` as a sortable expression. Ranking is the caller's problem.

**Non-goal.** Embeddings, semantic similarity, vector search. Those are the pgvector conversation, deferred.

---

## 4. Non-goals (explicit)

To close the loop on the rabbit hole: these are explicitly *not* part of donto and will be rejected as out-of-scope in review.

- User model, auth, sessions, anonymity toggles.
- Access control, privacy, visibility rules, ACLs, RLS recipes.
- Feeds, notifications, realtime subscriptions, CDC streams as a product surface.
- Recommendations, trending, "who else likes this", similarity search.
- Embeddings, pgvector integration.
- Moderation workflows, report queues, human-review UX.
- Media / blob storage, image handling, attachment signing.
- Mind-map / force-directed / any visualization (lives in `donto-faces` or an app).
- Span-anchored annotations on document bodies (deferred; requires a document model).
- Ordering / ranking as a first-class primitive (PRD §3.10 stands).

## 5. Phasing

Rough order; each is independently shippable.

- **Phase A** — §3.4 retrofit ingest, §3.5 shape reports, §3.8 time-binned aggregation. Low-risk, schema-shaped, unblock existing donto users.
- **Phase B** — §3.2 reactions, §3.3 aggregates, §3.9 full-text search. The "claim graph" layer; makes donto useful for any human-in-the-loop knowledge graph.
- **Phase C** — §3.1 bitemporal canonicals, §3.6 parallel-literal alignment, §3.7 environment overlay. The "semantic drift / multi-voice" layer; depends on B's reactions.

## 6. Invariants the extensions must preserve

- Paraconsistent. No extension rejects a contradiction.
- Bitemporal. No extension mutates `tx_time` retroactively.
- Open-world. No extension closes the predicate space.
- Lean certifies, doesn't gate. Shape reports and aggregates are async.
- Every statement has a context. Every reaction, aggregate, and shape report is itself a statement with its own context.
- No hidden ordering. Aggregates produce numbers, not orderings.
