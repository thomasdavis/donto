# PRD: donto Faces

**donto Faces** is a small public-facing web application that exposes
donto's distinctive data model through three single-page interfaces:
*Séance*, *Two Oracles*, *Notary*. It is a sibling project to
[`donto`](PRD.md), not a rewrite. Faces talks to a running `dontosrv`
over HTTP. donto remains usable without it.

This document is the source of truth for Faces. The engine PRD
(`PRD.md`) governs everything below the HTTP layer.

Working name: **donto-faces** (or *Faces* in prose).
Author: Thomas Davis. Date: 2026-04-18.

---

## 0. One-paragraph summary

donto's thesis — that contradictions are evidence, that retraction
preserves history, that meaning is a Lean term — is invisible from a
SQL prompt. A senior engineer who reads `PRD.md` understands. Everyone
else just sees a Postgres extension. Faces is the smallest application
surface that makes the thesis legible to a non-database audience: a
graveyard of retracted facts (*Séance*), a side-by-side renderer of
two contradictory worlds under different hypotheses (*Two Oracles*),
and a notary that re-checks Lean certificates in the user's own
browser (*Notary*). One Next.js app, three pages, no proprietary
backend. The audience target is the journalist, the researcher, the
investor reading the README — not the operator who already gets it.

---

## 1. Why this exists

donto is a database that does several things no other database does.
Most of them are invisible at the call-site:

- A retracted fact is still in the table. You only know that if you
  read `tx_time` and squint.
- Two contradictory facts coexist in the same store. You only see
  one at a time unless your scope happens to include both contexts.
- A derived fact carries a Lean certificate. You only know that if
  you read the SQL schema and trust the verifier.

A new visitor reading the README sees prose. Most don't read prose.
They want to *touch* the thing. Faces gives them three things to
touch, each chosen so that the donto-only behaviour is the user's
first sensation, not the third.

Out of scope: a CRUD admin UI, a query builder, a graph explorer.
Those exist for every database. None of them communicate the thesis.

---

## 2. The three faces

### 2.1 Séance — *what we used to think about you*

A single text input. The user types a subject IRI (or paste in a
person's name and Faces picks the right IRI). The page slowly
renders, one line at a time, every retracted statement about that
subject in chronological order. Each line says what the database
used to believe and the day it stopped.

```
ex:alice
   Born 1899 — believed until 14 March 2026
   Wife: Margaret — believed until 02 May 2024
   Address: 14 Kent St — believed until 11 November 2018
   …
```

Dark theme. Slow scroll. No "edit" button. The page does not let you
do anything; it just shows you what was once true and isn't anymore.

**donto features exercised:** `tx_time` upper-bound queries,
`donto_match` with `--as-of` semantics, the audit log.

**Why it matters:** every donto deployment has this graveyard
already. Nobody has ever shown it to anyone. The page makes
"retraction never deletes" emotional, not architectural.

### 2.2 Two Oracles — *Sliding Doors for facts*

A single subject. Two columns. Each column is rendered under a
distinct hypothesis context. The middle of the page has a small well
of unattached facts — claims you have but haven't decided what to do
with yet.

The user drags a fact from the well into the left column. The left
column reflows: derivations fire under that hypothesis, the family
tree extends three generations, kernel-verified badges tick green.
The right column hasn't changed; over there, this fact never
existed, so the cousins are strangers.

Drag the fact to the right column. The left collapses, the right
blooms. Same data, two parallel donto worlds, both logically
consistent on their own terms.

**donto features exercised:** hypothesis contexts, scope-as-of-name,
on-demand rule derivation into derivation contexts, lineage badges,
non-monotonic identity (`donto:sameAs` appearing in one column and
disappearing in the other).

**Why it matters:** paraconsistency-as-UI. The user is *playing* the
maturity ladder rather than reading about it. The page is the
strongest single argument for keeping contradictions instead of
resolving them.

### 2.3 Notary — *machine-checkable provenance, in your browser*

A storefront. Stone-grey serif type, an embossed seal, a single
input field labelled **Statement ID**. The user pastes any donto
statement_id that was published with a certificate.

Faces fetches the certificate, hands it to a Lean verifier compiled
to WebAssembly, and the user's own CPU re-derives the proof. Output
is one of two stamps:

```
   ✓  VERIFIED at 2026-04-18 14:33 UTC
      kind: transitive_closure
      inputs: 7 source statements, all present
```
```
   ✗  CANNOT VERIFY
      kind: substitution
      inputs missing: 2 of 5 source statements have been retracted.
```

No backend trust. No "we say so." The verifier runs in-page; the
network is used only to fetch the inputs.

**donto features exercised:** the certificate overlay, the seven
certificate kinds, the Lean verifier surface.

**Why it matters:** every other "trust the database" claim collapses
into reputation. This page makes the trust *portable*. A journalist
cites a fact in an article and prints the statement_id alongside it;
anyone can paste it into the Notary and watch their machine
re-derive the conclusion, weeks later, without the publisher.

---

## 3. Design principles

Non-negotiable. Everything in §4 onwards conforms to these.

1. **No resolution.** When two contradictory facts are on screen, neither
   is dimmed, struck through, or down-ranked. Both are equally
   present. The user resolves; the page does not.
2. **The second time axis is visible.** Whenever a date appears, it is
   labelled `valid` or `believed`. Never just "date." The user must be
   gently forced to internalise that there are two.
3. **The user's machine does the verifying.** Where Faces shows a
   `✓ proven` mark, the proof is re-checked in-browser. Faces never
   asserts truth on the server.
4. **Faces never writes.** It is read-only. All ingestion is via donto's
   normal channels. This keeps the trust model simple: anything the
   user sees they can independently re-fetch from the underlying
   donto.
5. **Cold-pickup-able.** A reader who lands on any Face for the first
   time, with no prior context, must be able to perceive donto's
   distinctive behaviour within ten seconds. If a page needs a tutorial,
   the page is wrong.
6. **The aesthetic carries weight.** Séance is somber; Two Oracles is
   theatrical; Notary is institutional. The visual register is part of
   the message; do not sand it down.

---

## 4. Architecture

```
                    Browser
       ┌─────────────────────────────────────────┐
       │  Next.js (Faces)                        │
       │  ├─ Séance     (server component)       │
       │  ├─ Oracles    (client; drag/drop)      │
       │  └─ Notary     (client; lean.wasm)      │
       └────────────┬───────────────────┬────────┘
                    │ HTTP/JSON         │ static .wasm
                    ▼                   │
            ┌──────────────┐    ┌───────────────┐
            │  dontosrv    │    │ lean-verifier │
            │  (Rust)      │    │ (Lean → WASM) │
            └──────┬───────┘    └───────────────┘
                   │
                   ▼
            ┌──────────────┐
            │ Postgres +   │
            │ pg_donto     │
            └──────────────┘
```

- **Faces** is a small Next.js 15 app. App router. RSC where possible
  (Séance loads server-side; Oracles is interactive; Notary is fully
  client-side once `.wasm` is loaded).
- It calls **dontosrv** for everything: `/dontoql`, `/shapes/validate`,
  `/rules/derive`, plus a new `/certificates/:id` endpoint Faces will
  drive.
- It fetches a single **Lean verifier WASM** bundle for Notary. The
  bundle exposes one function: `verify(certificate_json) → {ok, reason}`.
- It deploys to Vercel. dontosrv runs anywhere reachable. Postgres is
  the operator's choice.
- Auth: none in v1. v2 may add a per-deployment scope token to limit
  which contexts Faces will surface.

What Faces does **not** include:
- Its own database (it has no state of its own).
- Any feature behind a network call to a service Faces operates.
  Faces is a thin client.

---

## 5. donto features each face touches

| Face        | donto feature exercised                                                  |
| ----------- | ------------------------------------------------------------------------ |
| Séance      | `tx_time` close, audit log, `as_of_tx` queries                           |
| Séance      | retraction history, `donto_correct` chains                               |
| Two Oracles | hypothesis contexts (PRD §20)                                            |
| Two Oracles | non-monotonic `donto:sameAs` (PRD §10)                                   |
| Two Oracles | on-demand derivation into per-hypothesis contexts                        |
| Two Oracles | lineage badges (`donto_stmt_lineage`)                                    |
| Notary      | seven certificate kinds (PRD §18)                                        |
| Notary      | DIR codec (the same JSON Lean already speaks via dontosrv)               |
| Notary      | offline verifier compiled from `lean/Donto/Certificate.lean`             |

If a feature does not appear in this table it is out of scope for v1.

---

## 6. Phased plan

Faces ships in three small phases, in this order.

**Phase A — Skeleton (1 week).** Next.js app scaffold, deploy to
Vercel, talk to dontosrv `/dontoql`. A single page that pretty-prints
the result of one query. No theme yet. Proves the deployment loop.

**Phase B — Séance (1 week).** First real face. Subject lookup;
fetch all retracted statements via a new dontosrv endpoint
`/history/:subject`; render slow-scroll. Add the dark theme. Dogfood
on a real donto deployment with a non-trivial audit history.

**Phase C — Notary (2 weeks).** Compile the Lean certificate
verifier to WASM (the harder unknown is the build pipeline, not the
proof itself). Storefront UI; one-input flow; show the seven kinds
with their respective verifier outputs. Testable from a single
published statement_id.

**Phase D — Two Oracles (3 weeks).** The most ambitious page.
Drag/drop UI; live derivation requests to dontosrv; per-column
scope re-resolution; rendering of contradictory facts. Pilot on the
genealogy dataset where the contradictions are interesting.

Phases B/C/D are independently shippable. Phase A is the only
prerequisite for the others. There is no Phase E in v1.

---

## 7. Risks

- **Lean → WASM toolchain.** The Lean compiler can target WASM but
  the path is not paved. Mitigation: spend two days at the start of
  Phase C confirming the bundle size and verify-time. Fallback:
  ship a server-side verifier first, port to client when WASM
  works. Notary still demonstrates portability of trust either way.
- **Two Oracles is conceptually hard to render.** A page that shows
  contradictory worlds side by side risks looking like a diff view
  most users have already seen. Mitigation: invest in a designer
  for Phase D, not just an engineer. The aesthetic carries the
  message.
- **Faces could leak data the operator didn't intend to expose.**
  v1 has no auth and surfaces every public context. Mitigation:
  Faces respects a `ctx:public` opt-in label; only contexts marked
  with that label appear in any face. Operators control exposure
  by labelling, not by configuring Faces.
- **The audience may be too narrow.** A page that wows database
  people may bore the rest. Mitigation: the Séance page is the
  least nerdy and ships first; user-test it on three non-engineers
  before greenlighting Phase C.
- **donto itself moves fast.** The schema and HTTP surface are not
  yet stable. Faces pins the dontosrv version it tested against and
  publishes a compatibility matrix.

---

## 8. Open questions

- **Should Notary include a way to *create* certificates, not just
  verify them?** Probably not; that pulls Faces into the write path
  and breaks the "Faces never writes" principle. Bookmark for v2.
- **Should Two Oracles support more than two columns?** The page
  scales to N hypothesis contexts in principle. Two is the smallest
  number that demonstrates the point. Three or more is a follow-on
  if users ask.
- **Should the Séance accept an IRI *or* a free-text name?** A name
  search requires a query that joins through `rdfs:label`; cheap
  to add, but it's a separate concern. v1 is IRI-only with a small
  picker for the obvious cases.
- **How do we host the Lean WASM?** Probably as a static asset on
  Vercel; the alternative is an npm package. The choice affects
  caching but not correctness.
- **Should Faces be a separate repository?** Likely yes — different
  release cadence, different toolchain (Node vs. Rust). Open
  question whether to ship in `donto/faces/` or a sibling repo.

---

## 9. What this PRD does not specify

By intent; resolved as Faces ships:

- Wireframes for any of the three pages.
- Exact API contracts for the new dontosrv endpoints (`/history/:subject`,
  `/certificates/:id`). They will be drafted in their respective phases.
- Telemetry and analytics policy.
- Internationalisation. v1 is English-only.
- Hosting strategy beyond "Vercel + a dontosrv somewhere."

---

## 10. Why this is worth building

donto's PRD is a 30,000-word document that earns the right to be read.
Most people will not read it. Faces is the version of donto a person
encounters in three seconds and remembers in three months. Each face
is the smallest possible artefact that makes one of donto's
distinctive behaviours physical: the pile of dead beliefs, the two
worlds that disagree without resolving, the proof you can re-check on
your own laptop. Build the three pages and donto stops being a
database in the abstract and starts being a *thing you have used*.

---

## 11. Change log

- 2026-04-18: initial draft. Three faces, four phases, single
  Next.js app. Author: Thomas Davis.

End of PRD.
