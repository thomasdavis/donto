# M9 — Federation Research Memo

**Status:** Research, not implementation. Per PRD §18, M9 is a
spike that ends with a *product decision*: proceed, defer, or
reject federation. This memo is the input to that decision.

**Date:** 2026-05-15.
**Author:** session-handoff draft (Claude Opus 4.7).

## What "federated donto" would mean

A donto instance A wants to answer queries that include claims
stored on instance B, without B having to ship its raw rows. The
hard parts are: which claims to expose, what evidence to attach
to them, whose policy is enforced where, and how a cross-instance
release ("citation") proves it didn't leak restricted content.

Specifically, M9 acceptance asks:

> Two toy instances exchange policy-filtered release metadata.
> Cross-instance restricted content cannot leak through counts
> or errors. Product decision recorded: proceed, defer, or
> reject federation.

The non-goals are explicit: federation is **not** "let any
researcher query any other researcher's tree." That's the wrong
problem. Federation is "instance B can verify a release manifest
from instance A without re-ingesting A's source content."

## The five candidate stacks

### 1. W3C Verifiable Credentials (VC) + DID

**Shape.** Each claim or release manifest is a signed VC; the
signer is identified by a DID document. A consumer instance
verifies the signature and the DID resolution chain. Selective
disclosure (BBS+, SD-JWT) hides the underlying claim payload
when policy demands.

**Fit with donto.**
- Maps cleanly onto attestation: a `donto_attestation` row is
  essentially a VC about a claim or release.
- DID resolution is the right answer for the "who signed this"
  question; donto's `authority_refs` field on a policy already
  treats the issuer as data.
- Selective-disclosure schemes (BBS+) align with
  `WITH evidence = redacted_if_required` — the prover knows
  which fields to redact at proof time.

**Cost.**
- BBS+ requires pairing-friendly curves; not in the standard
  Postgres / Rust crypto stacks. Implementation cost is real
  (~weeks).
- DID method choice locks the federation: pick `did:web` for
  flexibility, `did:key` for simplicity, neither is wrong but
  changing later is painful.

**Verdict.** **Strongest fit.** If we proceed with federation,
this is the trust layer.

### 2. Solid Pods

**Shape.** Each user/institution has a Solid Pod — a personal
data-store with access-controlled HTTP resources. Apps query the
pod via Linked Data Platform conventions. Authentication is via
WebID + OIDC.

**Fit with donto.**
- The pod model assumes one principal per pod. donto already has
  multi-context, multi-authority semantics that the pod model
  doesn't represent natively.
- Solid is designed for personal data sovereignty (one person ↔
  one pod). The genealogy use case (and the broader
  evidence-store use case) routinely federates *between*
  institutions, not just between individuals.
- The protocol layer is mature; the access-control story (ACP /
  WAC) doesn't compose with donto's `donto_policy_capsule`
  semantics — it'd be a parallel system, not an integration.

**Verdict.** **Defer.** Useful as a *deployment shape* (each
researcher gets a pod hosting their donto instance) but not the
right protocol layer for cross-instance trust.

### 3. SPARQL Federation (`SERVICE` keyword)

**Shape.** A federated SPARQL query dispatches sub-patterns to
remote endpoints and joins the results locally. No content moves
unless the local query touches it.

**Fit with donto.**
- The native query language is DontoQL, not SPARQL. Adding a
  `SERVICE` clause to DontoQL is mechanical (one more clause)
  but the semantics are deep: cross-instance contexts,
  cross-instance policy enforcement, and bitemporal alignment
  across clocks are all open problems.
- The big risk is **information leakage through query shape and
  count**. SPARQL federation famously has timing channels and
  COUNT-based oracle attacks. Our PRD invariant ("cross-instance
  restricted content cannot leak through counts or errors")
  rules out the naive implementation.

**Verdict.** **Reject as the primary federation layer.** Worth
revisiting only as an interop adapter for external SPARQL
endpoints (no policy enforcement on our side; the remote endpoint
owns its access control).

### 4. DataCite-style citation metadata

**Shape.** Instance A produces a citable release artefact (DOI,
PURL, IPFS hash) with a metadata record. Instance B's user cites
that artefact in their work. The metadata record carries
provenance, license, and access information; the artefact itself
is fetched separately (if access is permitted).

**Fit with donto.**
- This is essentially what `donto_release_manifest` already is.
  The "federation" piece is just *publishing* the manifest to a
  registry (DataCite, OpenAIRE, Zenodo) and adopting their
  identifier scheme.
- Compatible with VC/DID for trust: the manifest is a VC; the
  publication is a registration of the VC's reference.
- No new substrate work in donto. Pure plumbing.

**Verdict.** **Proceed — this is the cheapest, highest-leverage
win.** It's not federation in the "live query" sense, but it
solves the "I cite a release that was produced under a known
policy" problem completely.

### 5. RO-Crate

**Shape.** A Research Object Crate is a directory + JSON-LD
manifest packaging research output: data, code, metadata,
provenance. Conformance profiles (BioSchemas, WorkflowHub)
define what fields are mandatory for a given domain.

**Fit with donto.**
- Already on the PRD §17 export list. RO-Crate is a *format*,
  not a *federation protocol*. It answers "how do I ship a
  release artefact" but not "how does instance B verify the
  release came from instance A."
- Naturally pairs with VC/DID (sign the crate's manifest) and
  DataCite (publish the signed crate's identifier).

**Verdict.** **Proceed independently of M9** as part of M7
release builder. Federation-relevant but not federation by
itself.

## Synthesis

The architecture that emerges:

- **Manifest format:** RO-Crate (M7 work).
- **Signing layer:** Verifiable Credentials over the manifest
  (M9 spike, picks DID method).
- **Publishing:** DataCite-style citation metadata pointing at
  the signed crate.
- **Live cross-instance query (`SERVICE`-style):** explicit
  non-goal for v1.

This pattern preserves donto's invariants:
- I2 (no source without policy): each crate-level claim retains
  its source's `policy_id`.
- I6 (governance propagates to derivatives): the signed crate
  *is* a derivative; its release-manifest carries the
  union/intersection of contributing policies.
- I10 (a release is a reproducible view): RO-Crate gives the
  reproducible filesystem; VC gives the auditable signature.

The "two toy instances exchange policy-filtered release metadata"
acceptance test becomes:

1. Instance A builds release R using `donto release` (M7).
2. The output is an RO-Crate signed by a VC issued under A's DID.
3. Instance B fetches the crate, verifies the VC, reads the
   release manifest. Its query-evaluator can answer "does this
   crate contain claims about entity X" *without instance B ever
   storing A's raw rows*. If a claim's `WITH evidence` is
   redacted at A's policy layer, B sees it redacted in the
   crate.

## Recommended decision

**Proceed with a narrow federation scope:** signed RO-Crate
releases + DataCite-style publication.

**Reject** live cross-instance SPARQL/DontoQL federation for v1.
Revisit when the count-channel mitigation work is funded.

**Defer** Solid Pod integration to a deployment-shape decision,
not a federation-protocol decision.

## What this memo doesn't decide

- Which DID method (`did:web` vs `did:key` vs `did:plc`).
- Which signature suite (Ed25519Signature2020 vs BBS+ vs
  Dilithium for post-quantum).
- Whether to register a `donto:` URI scheme with IANA (probably
  not — `https://` is fine for the next decade).
- The economic question: who pays to run the registry that maps
  release-manifest hashes to DOIs?

## Next concrete step (if the decision is "proceed")

A 200-line spike inside `donto-release`:

1. Define a `ReleaseEnvelope` struct: `{ manifest_id,
   manifest_sha256, issuer_did, signature, signature_suite }`.
2. Add a `donto release sign --did <did> --keyfile <path>`
   subcommand that produces a signed envelope.
3. Add a `donto release verify --envelope <path>` subcommand
   that validates the signature and resolves the DID.
4. Two-instance smoke test: `donto release sign` on instance A,
   `donto release verify` on instance B against the same crate.

That spike unblocks the M9 acceptance bullet without committing
to a full federation stack.
