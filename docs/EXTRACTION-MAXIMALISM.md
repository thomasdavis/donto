# Extraction Maximalism — donto's stance on what comes out of a source

> Goal: a single small source can be deconstructed into a million facts.
> Phonological detail. Idiolectal fingerprinting. Presuppositions. Counterfactuals.
> The "if I had to find this person again from anything they touched" lens.
> Beyond the philosophical. Beyond the syntactic. Beyond the obvious.

This document states the position. It exists so an outside reviewer
(human or model) can push back on (a) what is too ambitious, (b) what
is missing, (c) which apertures are wrong, (d) where the yield ceiling
actually sits, and (e) what the cost/value curve looks like at scale.

## 1. Why "tiers" had to die

The previous extractor was a single LLM call with an 8-tier prompt:
"label each fact you find as Tier 1 (surface) through Tier 8
(intertextual)." This had three structural failures:

1. **It capped yield at the prompt's elicitation budget.** One pass.
   One context window. One temperature. The LLM could only return as
   many facts as it could fit into one response, and it was rewarded
   for *labelling* rather than *mining*.

2. **Tiers conflated epistemic kind with topic.** "Ajax was born in
   1990" (asserted) and "Ajax has hair" (conceivable) both got dumped
   under "Tier 1 surface fact" or rejected entirely. The truth status
   of a candidate was not first-class.

3. **No recursion.** Each entity the LLM mentioned was a dead end.
   "the donto project" appeared in five facts but was never re-mined
   as a subject in its own right.

The replacement (shipped today) uses **apertures** — independent
specialised passes that decompose the source through different lenses
— and **content-hash deduplication** across the union.

## 2. The current aperture set (six)

| Aperture | What it mines | Modality | Confidence band |
|---|---|---|---|
| **surface** | What the text explicitly states | asserted, anchored | 0.95–1.0 |
| **linguistic** | Clause-by-clause syntactic decomposition; every NP→entity, every VP→event, every modifier→property | asserted, anchored | 0.85–1.0 |
| **presupposition** | What the text takes for granted but does not assert | hypothesis_only, anchored to trigger | 0.7–0.95 |
| **inferential** | Claims that follow from stated facts via common knowledge | asserted, anchored to trigger | 0.4–0.7 |
| **conceivable** | Claims that *could* hold given entity types ("hairs on the head" lens) | hypothesis_only, no anchor | 1.0 (it is conceivable) |
| **recursive** | Re-runs surface against newly discovered entities | asserted, anchored | 0.85–1.0 |

Live yield against a 1376-character bio:

```
1-pass tier        :   95 facts at $0.0042 in 65s
6-pass aperture    :  341 facts at $0.0252 in 449s   (3.6× yield, 6× cost)
  surface          :   87
  linguistic       :  127
  presupposition   :   34
  inferential      :   12
  conceivable      :   54
  recursive        :   27
distinct predicates:  171
distinct subjects  :   70
anchor coverage    : 0.842
hypothesis density : 0.258
dedup collisions   :    6
```

This is the floor. The actual ambition is far past this number.

## 3. The "million facts from a small source" target

The user's framing: *"if i was trying to find somebody completely
annoying but given their internal physiology and idiosyncrasies and
writing style we could create relationships, but even further beyond
that simple problem, i don't care if just a small little source can
be deconstructed into a million facts."*

This is not hyperbole if you take seriously what's mineable from an
arbitrary text:

- **Phonological** — every transcribable sound the speaker would utter
  reading the text aloud. ~4000 phonemes per 1376 chars.
- **Morphological** — every morpheme, every inflection, every derivational
  affix. ~2000 morphemes.
- **Syntactic** — every dependency arc. ~600 arcs.
- **Lexical** — every token, every lemma, every sense-disambiguation. ~250 tokens.
- **Semantic** — every frame element, every role filler. ~500 frame instances.
- **Pragmatic** — every illocutionary act, every implicature, every face act.
- **Stylistic** — sentence-length distribution, punctuation rhythm, hapax
  count, type-token ratio, reading-grade level, lexical richness moments.
  These are *facts* about the writer.
- **Idiolectal** — the writer's preferred clause length, their hedging rate,
  their nominalisation tendency, their function-word fingerprint, their
  comma-vs-semicolon ratio. Stylometry-grade features.
- **Physiological-implicit** — vocal tract requirements to produce these
  sounds. Hand mechanics if handwritten. Reading-pace if performed.
- **Counterfactual** — for every claim *p* in the text, the negated
  alternative ¬*p* is a fact about the choice the writer made.
- **Citational** — every reference, allusion, intertextual echo, quoted
  pattern. Even unconscious echoes ("a million facts" alludes to scale-as-rhetoric).
- **Modal** — for every claim, the modal envelope: necessary? possible?
  contingent? counter-to-fact?
- **Temporal** — every implied time anchor, every aspect, every tense
  decision.
- **Authorial-stance** — what the writer endorses, doubts, mocks,
  resigns to, finds beautiful.
- **Interlocutor-modelled** — what the writer assumes the reader knows,
  doesn't know, would object to, would agree with.

A 1376-character source could plausibly yield 50,000–500,000 distinct
candidate claims if every one of these layers is mined exhaustively.
A million is achievable if you also enumerate the **conceivable** space
(every entity gets every type-appropriate property as a hypothesis).

## 4. What's missing to hit that target

The current six apertures cover roughly half of the layers above.
Specifically, what we ship today **does not** mine:

### 4a. Phonological / orthographic
A `PHONETIC` aperture that emits one fact per syllable boundary, one per
diphthong, one per voicing alternation. Anchored to character offsets.
Per-language phonotactic constraints encoded as predicates.

### 4b. Morphosyntactic
A `MORPHOSYNTACTIC` aperture that emits one fact per morpheme,
per inflection, per affix, per dependency arc, per coreference chain.
This is essentially CoNLL-U-as-claims. Per-token grammatical role,
per-clause finiteness, per-NP definiteness, per-VP aspect.

### 4c. Stylometric
A `STYLOMETRIC` aperture that fingerprints the *writer*, not the text:
type-token ratio, mean sentence length, function-word distribution,
hapax legomena ratio, average syllables per word, comma-to-period
ratio, em-dash usage, nominalisation rate, passive-voice rate. Each
metric becomes a fact about an `ex:author/<hash>` entity. Crucially,
these *travel*: facts about an author from one source can be matched
against facts from another source for authorship attribution.

### 4d. Idiolectal
An `IDIOLECT` aperture that mines the writer's specific tics:
preferred discourse markers ("anyway", "frankly"), hedge inventory,
intensifier preferences, signature collocations, pet metaphors, the
author's dispreferred lexicon (words they could have used and didn't).

### 4e. Counterfactual
A `COUNTERFACTUAL` aperture that for every claim *p* emits the negated
claim ¬*p* as `hypothesis_only` with predicate `couldHaveBeen`. For
every word choice, the unchosen alternative. For every assertion, the
hedged form. This is how you mine "what was rejected".

### 4f. Pragmatic
A `PRAGMATIC` aperture that mines speech acts, illocutionary force,
politeness markers, face-threatening acts, hedging strategy,
audience-modelling. "Ajax has stated publicly that…" emits facts
about Ajax's *speech acts*, not just their content.

### 4g. Frame-typed
A `FRAME` aperture that *types* facts against a registered frame
schema (BirthEvent, GraduationEvent, FoundingEvent, MaintainershipRelation,
LinguisticContribution). Unframed facts still flow but typed ones
become first-class with role-filler structure. This is the donto PRD
§5 frame work, currently latent.

### 4h. Bibliographic-graph
A `CITATIONAL` aperture that for every reference, allusion, or echo
emits a citation edge. Even unstated references — "the LP system of
Graham Priest" → cite arc to Priest's *In Contradiction*.

### 4i. Cross-document
A `INTERTEXTUAL` aperture that compares the source against a corpus
the system already has. Every shared n-gram becomes a fact.
Every paraphrase gets a similarity edge. Every contradicting claim
elsewhere becomes a `disagreesWith` edge.

### 4j. Reactive
A `REACTION` aperture that simulates what readers would *do* with
each claim — endorse, push back, ask for evidence, escalate, ignore.
Maps to donto's existing reactions table (PRD §11).

### 4k. Recursive at depth N
Currently `recursive` runs once. The next move is recursion to a
configurable depth, with cycle detection, where each newly discovered
entity becomes the seed for a fresh `surface + linguistic +
conceivable` triple.

## 5. The cost and the curve

Naïve linear scaling (one LLM call per aperture) puts a 15-aperture
exhaustive run at ~$0.06 per 1.4kB source. That's $43/MB or $43k per
GB of text. Not feasible at corpus scale.

What changes the curve:

1. **Smaller specialised models** — phonological/morphosyntactic
   apertures don't need a frontier LLM. A spaCy/UDPipe pipeline
   produces the same facts at 0.001× the cost. Frame typing can run
   off a fine-tuned 7B local model.
2. **Chunking** — current apertures run on the whole text. For
   sources >2k chars, chunk + per-chunk apertures + cross-chunk
   coreference resolution.
3. **Batched LLM calls** — OpenAI/OpenRouter batch APIs cut cost ~50%
   for non-latency-critical apertures (presupposition, conceivable).
4. **Speculative-then-verify** — run a cheap model first, send only
   the disputed candidates to a frontier model.
5. **Caching** — apertures whose output depends only on the text
   (linguistic, phonological, stylometric) are pure functions and
   should be cached by content hash.
6. **Differential extraction** — when a source updates, only re-mine
   the changed spans. donto's bitemporality already supports the
   `tx_time` anchoring required to make this safe.

A realistic target: **\$0.02-\$0.05 per 1kB** for the full 15-aperture
pass, **\$0.001/kB** for cached re-runs, **5,000-50,000 facts per kB**
depending on language and density.

A million facts from a 1.4kB source is ~700 facts per character — 3×
beyond the upper bound above. The realistic ceiling at full aperture
coverage is **20,000-30,000 facts** for a source of that size. To get
to a million you'd need either:

- a longer source (1MB → 5M-50M facts at the same density), or
- recursion-to-depth across an entity graph the source touches (each
  named entity expands to its own corpus of facts), or
- the corpus the source sits inside, mined *as a function of* this
  source's claims.

All three are donto-compatible and are listed in §6.

## 6. What I'd ask ChatGPT Pro Research

1. **Are these the right apertures?** Six are shipped, eleven more are
   sketched in §4. What's missing? Which is over-specified? Where do
   linguistic vs morphosyntactic vs phonological cleanly separate vs
   collapse together?

2. **What's the published yield ceiling?** UDPipe + AMR + FrameNet +
   PropBank gives N facts per token. Where does that N actually sit?
   What's the published number from RDF triplification papers (e.g.
   FRED, mAKR, KnowledgeNet)?

3. **Is content-hash dedup the right merge strategy across apertures?**
   Or should two apertures that mine the same triple from different
   epistemic angles produce *two* facts (one anchored asserted, one
   hypothesis-only) so the modality ambiguity survives?

4. **What does the cost curve look like at real scale?** 1MB sources.
   100MB corpora. 1GB.

5. **Authorship fingerprinting**: which stylometric feature set is
   most discriminative without being most expensive? Burrows' Delta?
   Stamatatos function-word vectors? Newer transformer-embedding
   approaches?

6. **Counterfactual mining**: is there published work on systematically
   enumerating ¬p for every p in a text? This feels under-explored.

7. **The conceivable aperture is currently flat enumeration**: every
   person has hair, blood-type, etc. Is there a principled type-system
   to drive this — Schema.org? Wikidata properties of P31? An
   ontologically-derived conceivable surface?

8. **Recursion termination**: how deep before yield drops below
   marginal cost? Is there a published donto-shaped paper on this
   (paraconsistent stores + LLM extraction)?

9. **Idiolect transfer**: facts mined from source A about author X
   should compose with facts from source B about author X. donto's
   existing alignment vocabulary handles entity identity, but does it
   handle *fingerprint similarity* as a first-class edge type?

10. **Quarantine vs hypothesis_only**: currently invalid candidates go
    to quarantine and unanchored ones go to hypothesis_only. Is this
    the right cut? Could there be an `obligation` channel for
    candidates that need verification (PRD §11) — facts that *claim*
    to be anchored but the anchor doesn't resolve?

## 7. Where this lives in the codebase

| Path | Role |
|---|---|
| `apps/donto-api/extraction/apertures.py` | Six aperture prompts |
| `apps/donto-api/extraction/exhaustive.py` | Multi-pass orchestrator |
| `apps/donto-api/extraction/dispatch.py` | M5 single-pass (still works for cheap surface-only runs) |
| `apps/donto-api/extraction/validation.py` | Hard-gate validator (anchor + hypothesis_only invariant) |
| `apps/donto-api/extraction/quarantine.py` | Quarantine sink |
| `apps/donto-api/extraction/policy_gate.py` | Trust Kernel policy probe before any external model call |
| `apps/donto-api/main.py::extract_exhaustive` | `POST /extract/exhaustive` |
| `apps/donto-api/helpers.py::compute_yield` | Yield metrics |
| `apps/donto-api/tests/test_exhaustive.py` | 11 unit tests |

PRD references that bind this work:

- **§3** Principles — paraconsistent + bitemporal + every-statement-has-context.
- **§5** Frames — the FRAME aperture (§4g) wires here.
- **§11** Reactions / Obligations — the REACTION aperture (§4j) wires here.
- **§13** Arguments — counterfactual mining (§4e) supplies the alternatives.
- **§15** Trust Kernel — the policy gate guards every aperture's external model call.
- **§17** Release builder — `donto-release` already filters by maturity; needs a new aperture-stratified release lens.

## 8. What I am not claiming

- I am not claiming the current six apertures match the published SOTA.
  They are the floor that ships today.
- I am not claiming a million-fact yield is achievable from 1.4kB at
  current model cost. The realistic ceiling is 20k–30k. The path to
  a million goes through recursion-into-the-corpus, not deeper-into-the-source.
- I am not claiming the conceivable aperture's outputs are *useful*
  by default. They flood the candidate space; downstream curation
  (Trust Kernel + maturity gate + reviewer reactions) decides what
  survives into a release.

The position is: **maximal extraction is a design stance, not a yield
target**. Mine everything. Quarantine the malformed. Flag the
hypothetical. Let the curation gate, not the extractor, decide what
counts.

— donto, 2026-05-08
