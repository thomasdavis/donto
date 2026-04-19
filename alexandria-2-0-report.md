# Alexandria 2.0 / Open World Library — donto fit analysis

Source: `https://megawatts.notion.site/Concepts-75967df4f4594361b0f6a8781bca895f`
Crawled: 2026-04-19.

## Pages crawled
- **Concepts** (root) — `Concepts-75967df4f4594361b0f6a8781bca895f`
- **Learning and Feedback** — `d50daedcc0e34f96968b4ee13262a511`

The wider Alexandria 2.0 hub has many sibling pages (Tech Stack, Roadmap, Competitive Analysis, Design System, Deck, etc.) not included in this scope.

## What Alexandria 2.0 is
A late-2020-era vision for an "Open World Library": a collaborative knowledge platform where users publish articles, tag anyone's content, endorse/disagree, change their minds, and watch ideas and tags evolve over time on a visual mind-map. Feedback loops (social, market, game-theoretic) are the engagement primitive.

## Feature inventory extracted
- **Tags**: folksonomy, anyone tags anyone's article, weighted by endorsement count (edge thickness), retrofitting old articles with new tags.
- **Comments**: sentence-anchored (Google-Docs-style), link to newer/correcting info.
- **Endorsements & voting**: agree / disagree / changed-vote, one-vote-per-unique-ID.
- **Never delete**: private/anonymous is allowed, deletion is not.
- **"Changed my mind" / "Failed project" / "Solution found"** markers preserve history and reason.
- **Search**: keyword, mind-map with value colors, intersectional filtering (Flint + anosmia → lead poisoning vs COVID), headlines / random feed.
- **Language**: jargon translator by industry/location, tracking word-meaning drift over time ("lit"), cross-cultural vocabulary (Dictionary of Obscure Sorrows style), parallel-translation alignment.
- **Bias**: environment-weighted bias scoring (California Cold), bias-of-the-system reporting.
- **Rating / N-of-1**: clinical-trial single-subject validation.
- **User pages**: multi-facet (farm + bake + make), bookmarks, "who else likes this".
- **Polls**: merge duplicates via auto-suggest, expand existing rather than create new.
- **Game-theoretic ranking** (from Learning and Feedback): chess-ladder / boxing-weight-class fairness for community engagement.

---

# How donto handles it

## Strong native fit (already in PRD)

| Alexandria feature | donto primitive |
|---|---|
| "Nothing ever deleted" | Bitemporal invariant — retract closes `tx_time`, never `DELETE` (PRD §3.3) |
| "Changed my mind" / vote-change | `donto_retract` + re-assert; prior value lives in tx-history |
| Anyone can tag/endorse anyone | Context-per-actor; no ownership gate on assertion |
| Endorse vs disagree | Polarity bits (asserted / negated / absent / unknown) |
| Contradictory tags coexist ("racist" vs "insightful") | Paraconsistency — both rows live forever (PRD §3.1) |
| Retrofitting old articles with new tags | `valid_time_from = article_created`, `tx_time_from = now` |
| Word meaning drifts ("lit") | Bitemporal alias: `lit` → `bright` canonical before ~1990, `lit` → `excellent` after ~2015 |
| Jargon translator by industry/location | Predicate registry aliases keyed by domain context |
| "Flag this" → sidebar, not removal | Shapes are overlays not schemas (PRD §3.5); a flag is a shape-report annotation |
| Edge thickness from endorsement count | Rule-derived statement (Level 3 on the maturity ladder) |
| "Certified" claims vs raw posts | Maturity ladder 0–4 maps 1:1 (raw user tag → registry canonical → shape-checked → rule-derived → certified) |
| N-of-1 trials | Per-subject context; trial data as scoped statements that don't leak into global views |
| Bias-of-the-system reporting | Meta-context: assertions about donto itself live in their own named context |
| Provenance "who endorses articles you like" | Every statement has context + source_stmt lineage |
| Poll dedup ("expand existing") | Predicate registry merge / alias workflow |

## What's clearly missing from donto

These would need new layers, not just new statements.

1. **Identity / auth / anonymity toggle.** donto models context authorship but has no user model, sessions, or anonymity primitive. "Creators can make things private" implies ACLs — donto's contexts are provenance, not authorization.
2. **Document bodies and sentence-range anchors.** A blog post is a blob; a sentence-comment is an offset range. PRD has no literal-size guidance, no full-text index (tsvector/GIN), no span-annotation model. Real gap for any "library" product.
3. **Ranking / feeds / "random headlines" / trending.** PRD §3.10 is explicit: *no hidden ordering*. "Headlines each day, refreshes random" and "trending topics" need a recommendation/ordering subsystem on top — donto deliberately refuses to pick one.
4. **Embeddings / semantic similarity.** Auto-suggest, clustering, "who else thinks like you" — no pgvector integration or vector-statement type in the PRD.
5. **Notifications / streaming / pub-sub.** donto is batch + bitemporal; no event stream for "new article posted" or "someone endorsed your comment".
6. **Access control.** Context ≠ ACL. Private/anonymous/mod-only content needs a separate policy layer (RLS or app-layer).
7. **Reputation / trust propagation.** "Unique IDs to be one vote", mod weighting, "who endorses things you like" — donto stores the raw edges; trust-graph propagation / Sybil resistance is out of scope.
8. **Media / attachments.** Images and files need a blob store; donto only holds IRIs.
9. **Parallel translation alignment.** RDF lang-tagged literals exist, but "show all Bible translations side-by-side, vetted by bilingual users" is a first-class alignment shape donto hasn't specified.
10. **UI / mind-map visualization.** Out of donto's scope, but the product needs a `faces`-style frontend. `donto-faces` exists for a different data shape.
11. **Moderation workflows.** donto can encode every flag and mod action as statements; workflow engine / queue / UI is not part of donto.
12. **Time-binned aggregation as a first-class surface.** "Ngram over time" and "language evolution" are derivable from valid_time, but no dedicated time-series query helper.

## Net read

The **philosophical core** of Alexandria 2.0 — open world, never delete, contradictions live, contexts per actor, words mean different things over time, tags earn weight through feedback, claims have certificates — is *exactly* what donto was built for. A huge fraction of the Alexandria data model lands natively on the quad store with zero schema work.

The **missing ~40%** is all the stuff around the graph: auth, full-text bodies, feeds, recommendations, vectors, moderation UI, notifications. None of it contradicts donto; most of it would live as sibling services that read/write donto. But a product team taking Alexandria 2.0 from vision → working site would spend more time on those adjacent systems than on the knowledge graph itself.
