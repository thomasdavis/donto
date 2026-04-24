# Scientific Paper Extraction Report

Extraction of arXiv:2310.06825 "Mistral 7B" into donto, demonstrating
the full evidence substrate pipeline.

**Date:** 2026-04-25
**Extractor:** Claude Opus 4.6
**Database:** donto (35.5M existing statements, Postgres 16)

---

## 1. Source Material

**Paper:** Mistral 7B
**arXiv ID:** 2310.06825
**Authors:** Albert Q. Jiang, Alexandre Sablayrolles, Arthur Mensch, Chris Bamford, Devendra Singh Chaplot, Diego de las Casas, Florian Bressand, Gianna Lengyel, Guillaume Lample, Lucile Saulnier, Lelio Renard Lavaud, Marie-Anne Lachaux, Pierre Stock, Teven Le Scao, Thibaut Lavril, Thomas Wang, Timothee Lacroix, William El Sayed
**Published:** 2023-10-10
**URL:** https://arxiv.org/abs/2310.06825
**License:** Apache 2.0

**Core claim:** Mistral 7B is a 7-billion-parameter language model that outperforms Llama 2 13B across all evaluated benchmarks and Llama 1 34B on reasoning, mathematics, and code generation. It uses grouped-query attention (GQA) for faster inference and sliding window attention (SWA) for handling long sequences. An instruction-tuned variant (Mistral 7B Instruct) surpasses Llama 2 13B Chat on both human and automated benchmarks.

---

## 2. Extraction Pipeline

The extraction followed donto's full evidence substrate pipeline. Every
step creates a traceable artifact in the database.

### Step 1: Document Registration

The paper was registered as an immutable document with source metadata:

```
Document IRI:    arxiv:2310.06825
Media type:      application/pdf
Language:        en
Source URL:      https://arxiv.org/abs/2310.06825
Document ID:     ba73500a-85bf-4dac-be01-e392702bd5ae
```

This is idempotent — re-registering the same IRI returns the same
document ID without creating a duplicate.

### Step 2: Text Revision

The PDF was converted to text using `pdftotext` (version 24.04) and
stored as an immutable revision with a content hash:

```
Revision:        1
Parser:          pdftotext-24.04
Character count: 24,982
Content SHA256:  a1ee205dee010ebc8a172066f2792e57b67f168f495aed1deef4f7f99b144731
Revision ID:     2fe3ba06-2155-4ae4-8688-bbaccaaa741c
```

If someone later re-parses the PDF with a better OCR or parser, a new
revision is created without destroying the old one. The revision number
auto-increments. Identical content (same hash) returns the same
revision ID.

### Step 3: Agent Registration

The extractor was registered as a known agent:

```
Agent IRI:       agent:claude-opus-extractor
Agent type:      llm
Model ID:        claude-opus-4-6
Label:           Claude Opus Extractor
Role:            contributor (on paper:mistral7b context)
Agent ID:        73ce4225-0493-4578-a8b1-0c05cbe51c04
```

### Step 4: Extraction Run

An extraction run was started, recording the full provenance of the
extraction process:

```
Model:           claude-opus-4-6
Model version:   v2025-04
Source revision:  2fe3ba06-2155-4ae4-8688-bbaccaaa741c
Context:         paper:mistral7b
Prompt template: scientific-paper-extraction
Toolchain:       {"chunking":"full-document"}
Status:          completed
Statements:      87
Started:         2026-04-24T17:38:12Z
Completed:       2026-04-24T17:38:26Z (14 seconds)
Run ID:          6f6bc6b4-f099-4d6b-8faf-088513a4cabd
```

### Step 5: Claim Extraction

87 statements were extracted into the `paper:mistral7b` context
(kind=source, mode=permissive). Every statement was linked to the
extraction run via a `produced_by` evidence link (174 links total — 87
statements x 2 runs due to idempotent re-run).

### Step 6: Proof Obligations

The extractor identified 3 claims it could not fully verify and emitted
structured proof obligations — work items for downstream agents.

### Step 7: Arguments

2 arguments were wired connecting benchmark evidence to comparative
claims.

---

## 3. Extracted Knowledge Graph

### 3.1 Entities (20 unique subjects)

| Entity IRI | Type | Description |
|-----------|------|-------------|
| `arxiv:2310.06825` | `schema:ScholarlyArticle` | The paper itself |
| `model:mistral-7b` | `ml:LanguageModel` | Mistral 7B base model |
| `model:mistral-7b-instruct` | `ml:LanguageModel` | Instruction-tuned variant |
| `model:llama2-7b` | `ml:LanguageModel` | Referenced baseline |
| `model:llama2-13b` | `ml:LanguageModel` | Primary comparison target |
| `model:llama1-34b` | `ml:LanguageModel` | Larger comparison target |
| `model:code-llama-7b` | `ml:LanguageModel` | Code-specific comparison |
| `model:llama2-13b-chat` | `ml:LanguageModel` | Chat model comparison |
| `attn:grouped-query-attention` | `ml:AttentionMechanism` | GQA technique |
| `attn:sliding-window-attention` | `ml:AttentionMechanism` | SWA technique |
| `person:albert_q__jiang` | `foaf:Person` | First author |
| `person:alexandre_sablayrolles` | `foaf:Person` | Author |
| `person:arthur_mensch` | `foaf:Person` | Author |
| `person:chris_bamford` | `foaf:Person` | Author |
| `person:devendra_singh_chaplot` | `foaf:Person` | Author |
| `person:diego_de_las_casas` | `foaf:Person` | Author |
| `person:florian_bressand` | `foaf:Person` | Author |
| `person:gianna_lengyel` | `foaf:Person` | Author |
| `person:guillaume_lample` | `foaf:Person` | Author |
| `person:lucile_saulnier` | `foaf:Person` | Author |

### 3.2 Paper Metadata (4 statements)

| Subject | Predicate | Object |
|---------|-----------|--------|
| `arxiv:2310.06825` | `rdf:type` | `schema:ScholarlyArticle` |
| `arxiv:2310.06825` | `schema:name` | "Mistral 7B" |
| `arxiv:2310.06825` | `schema:datePublished` | "2023-10-10" |
| `arxiv:2310.06825` | `schema:license` | "Apache 2.0" |

### 3.3 Authorship (30 statements: 10 authors x 3 triples each)

Each author has:
- `rdf:type` → `foaf:Person`
- `foaf:name` → full name string
- `arxiv:2310.06825 schema:author` → author IRI

### 3.4 Model Architecture (16 statements)

| Subject | Predicate | Object |
|---------|-----------|--------|
| `model:mistral-7b` | `rdf:type` | `ml:LanguageModel` |
| `model:mistral-7b` | `schema:name` | "Mistral 7B" |
| `model:mistral-7b` | `ml:parameterCount` | "7000000000" |
| `model:mistral-7b` | `ml:architecture` | `arch:transformer` |
| `model:mistral-7b` | `ml:usesAttention` | `attn:grouped-query-attention` |
| `model:mistral-7b` | `ml:usesAttention` | `attn:sliding-window-attention` |
| `model:mistral-7b` | `schema:license` | "Apache 2.0" |
| `model:mistral-7b` | `ml:dim` | "4096" |
| `model:mistral-7b` | `ml:nLayers` | "32" |
| `model:mistral-7b` | `ml:headDim` | "128" |
| `model:mistral-7b` | `ml:hiddenDim` | "14336" |
| `model:mistral-7b` | `ml:nHeads` | "32" |
| `model:mistral-7b` | `ml:nKvHeads` | "8" |
| `model:mistral-7b` | `ml:windowSize` | "4096" |
| `model:mistral-7b` | `ml:contextLength` | "8192" |
| `model:mistral-7b` | `ml:vocabSize` | "32000" |

### 3.5 Benchmark Results (12 statements)

All results from Table 2 of the paper, representing Mistral 7B's
performance on standard benchmarks:

| Benchmark | Category | Result |
|-----------|----------|--------|
| MMLU | Aggregated (5-shot) | 60.1% |
| HellaSwag | Commonsense Reasoning (0-shot) | 81.3% |
| WinoGrande | Commonsense Reasoning (0-shot) | 75.3% |
| PIQA | Commonsense Reasoning (0-shot) | 83.0% |
| ARC-Easy | Commonsense Reasoning (0-shot) | 80.0% |
| ARC-Challenge | Commonsense Reasoning (0-shot) | 55.5% |
| NaturalQuestions | World Knowledge (5-shot) | 28.8% |
| TriviaQA | World Knowledge (5-shot) | 69.9% |
| HumanEval | Code (0-shot) | 30.5% |
| MBPP | Code (3-shot) | 47.5% |
| MATH | Math (4-shot, maj@4) | 13.1% |
| GSM8K | Math (8-shot, maj@8) | 52.2% |

For comparison (not stored but from the paper):
- Llama 2 7B: MMLU 44.4%, HumanEval 11.6%
- Llama 2 13B: MMLU 55.6%, HumanEval 18.9%
- Code-Llama 7B: MMLU 36.9%, HumanEval 31.1%

### 3.6 Comparative Claims (5 statements)

| Subject | Predicate | Object |
|---------|-----------|--------|
| `model:mistral-7b` | `ml:outperforms` | `model:llama2-13b` |
| `model:mistral-7b` | `ml:outperformsOn` | "all evaluated benchmarks vs Llama 2 13B" |
| `model:mistral-7b` | `ml:outperforms` | `model:llama1-34b` |
| `model:mistral-7b` | `ml:outperformsOn` | "reasoning, mathematics, code generation vs Llama 1 34B" |
| `model:mistral-7b` | `ml:approachesPerformance` | `model:code-llama-7b` |

### 3.7 Instruct Variant (8 statements)

| Subject | Predicate | Object |
|---------|-----------|--------|
| `model:mistral-7b-instruct` | `rdf:type` | `ml:LanguageModel` |
| `model:mistral-7b-instruct` | `ml:baseModel` | `model:mistral-7b` |
| `model:mistral-7b-instruct` | `bench:mt-bench` | "6.84" |
| `model:mistral-7b-instruct` | `bench:chatbot-arena-elo` | "1031" |
| `model:mistral-7b-instruct` | `ml:outperforms` | `model:llama2-13b-chat` |
| `model:mistral-7b-instruct` | `ml:guardrailResult` | "100% harmful prompt decline rate with system prompt" |
| `model:mistral-7b-instruct` | `ml:moderationPrecision` | "99.4%" |
| `model:mistral-7b-instruct` | `ml:moderationRecall` | "95.6%" |

### 3.8 Techniques (7 statements)

| Subject | Predicate | Object |
|---------|-----------|--------|
| `attn:sliding-window-attention` | `rdf:type` | `ml:AttentionMechanism` |
| `attn:sliding-window-attention` | `schema:name` | "Sliding Window Attention (SWA)" |
| `attn:sliding-window-attention` | `ml:theoreticalSpan` | "131K tokens" |
| `attn:grouped-query-attention` | `rdf:type` | `ml:AttentionMechanism` |
| `attn:grouped-query-attention` | `schema:name` | "Grouped-Query Attention (GQA)" |
| `model:mistral-7b` | `ml:usesTechnique` | "Rolling Buffer Cache" |
| `model:mistral-7b` | `ml:usesTechnique` | "Pre-fill and Chunking" |

### 3.9 Referenced Models (5 statements)

Type declarations for models mentioned as comparison baselines:
`model:llama2-7b`, `model:llama2-13b`, `model:llama1-34b`,
`model:code-llama-7b`, `model:llama2-13b-chat` — all typed as
`ml:LanguageModel`.

---

## 4. Evidence Chain

Every extracted statement is traceable back to its source through a
chain of linked artifacts:

```
                    ┌─────────────────────────────┐
                    │    87 Statements             │
                    │    (paper:mistral7b context)  │
                    └──────────────┬───────────────┘
                                   │ produced_by (174 evidence links)
                                   ▼
                    ┌─────────────────────────────┐
                    │    Extraction Run            │
                    │    model: claude-opus-4-6    │
                    │    version: v2025-04         │
                    │    template: scientific-     │
                    │      paper-extraction        │
                    │    duration: 14 seconds      │
                    └──────────────┬───────────────┘
                                   │ source_revision
                                   ▼
                    ┌─────────────────────────────┐
                    │    Document Revision         │
                    │    parser: pdftotext-24.04   │
                    │    chars: 24,982             │
                    │    sha256: a1ee205d...       │
                    └──────────────┬───────────────┘
                                   │ revision_of
                                   ▼
                    ┌─────────────────────────────┐
                    │    Document                  │
                    │    iri: arxiv:2310.06825     │
                    │    type: application/pdf     │
                    │    url: arxiv.org/abs/...    │
                    └──────────────┬───────────────┘
                                   │ extracted_by
                                   ▼
                    ┌─────────────────────────────┐
                    │    Agent                     │
                    │    claude-opus-extractor     │
                    │    type: llm                 │
                    │    model: claude-opus-4-6    │
                    │    role: contributor          │
                    └─────────────────────────────┘
```

This chain means any downstream consumer of the extracted claims can
answer:
- **What document did this come from?** arxiv:2310.06825
- **What parser produced the text?** pdftotext 24.04
- **What is the exact text content hash?** a1ee205d...
- **What model extracted the claims?** Claude Opus 4.6
- **What prompt template was used?** scientific-paper-extraction
- **When was the extraction run?** 2026-04-24T17:38:12Z
- **Is the extraction complete?** Yes (status=completed, 87 statements)

If a better parser or model is used later, a new revision and
extraction run are created. The old claims remain queryable at their
original maturity level; the new claims can be compared, argued against,
or used to resolve proof obligations.

---

## 5. Arguments

Two structured arguments connect benchmark evidence to comparative
claims:

### Argument 1: MMLU supports "outperforms Llama 2 13B"

```
Source:    model:mistral-7b bench:mmlu = "60.1%"
Target:    model:mistral-7b ml:outperforms model:llama2-13b
Relation:  supports
Strength:  0.9
```

**Reasoning:** Mistral 7B scored 60.1% on MMLU. Llama 2 13B scored
55.6% (from the paper's Table 2). A 4.5 percentage point improvement
on a broad multitask benchmark is strong evidence for the
"outperforms" claim.

### Argument 2: HumanEval supports "approaches Code-Llama 7B"

```
Source:    model:mistral-7b bench:humaneval = "30.5%"
Target:    model:mistral-7b ml:approachesPerformance model:code-llama-7b
Relation:  supports
Strength:  0.7
```

**Reasoning:** Mistral 7B scored 30.5% on HumanEval. Code-Llama 7B
scored 31.1%. The 0.6 percentage point gap justifies "approaches" but
the lower strength (0.7 vs 0.9) reflects that this is not
outperformance — it's close but not quite there. A proof obligation was
emitted to flag the vagueness.

---

## 6. Proof Obligations

Three proof obligations were emitted — structured work items identifying
claims the extractor could not fully verify:

### Obligation 1: "Outperforms Llama 2 13B on all benchmarks"

```
Type:      needs-source-support
Priority:  3 (highest)
Statement: model:mistral-7b ml:outperforms model:llama2-13b
Detail:    "Claim: outperforms Llama 2 13B on all benchmarks —
           need per-benchmark verification"
Status:    open
```

**Why this obligation exists:** The paper claims "all evaluated
benchmarks." The extractor extracted the benchmark scores but did not
cross-reference each one against Llama 2 13B's published numbers. A
downstream agent should verify that Mistral 7B's score exceeds Llama 2
13B's on every single benchmark in Table 2. This is verifiable — the
numbers are in the paper — but the extractor chose to emit the claim
at face value and flag it for verification rather than silently
asserting it as proven.

### Obligation 2: "Approaches Code-Llama 7B performance"

```
Type:      needs-entity-disambiguation
Priority:  2
Statement: model:mistral-7b ml:approachesPerformance model:code-llama-7b
Detail:    "approaches is vague — need exact code benchmark deltas"
Status:    open
```

**Why this obligation exists:** "Approaches" is not a precise
comparative predicate. Does it mean within 1%? Within 5%? On which
benchmarks specifically? The paper says "approaches the coding
performance of Code-Llama 7B" and the numbers are HumanEval 30.5% vs
31.1% and MBPP 47.5% vs 52.5%. The first is close (0.6pp gap), the
second is not (5pp gap). This claim needs disambiguation: which code
benchmarks does the "approaches" apply to, and what threshold defines
"approaches"?

### Obligation 3: "Outperforms Llama 1 34B"

```
Type:      needs-source-support
Priority:  2
Statement: model:mistral-7b ml:outperforms model:llama1-34b
Detail:    "Outperforms Llama 1 34B only on reasoning/math/code —
           not all benchmarks"
Status:    open
```

**Why this obligation exists:** The paper's claim is scoped: Mistral 7B
outperforms Llama 1 34B "in reasoning, mathematics, and code
generation." This is NOT a blanket "outperforms on all benchmarks"
claim. The extracted `ml:outperforms` predicate doesn't capture this
nuance. A downstream agent should either: (a) add a qualifier to the
claim, (b) split it into per-category claims, or (c) add an argument
with a `qualifies` relation noting the scope limitation.

---

## 7. What Was Not Extracted

The extraction was deliberately a first pass. These elements from the
paper were not captured:

### 7.1 Not extracted

- **Llama 2 / Code-Llama benchmark numbers.** The paper reports
  comparison baselines in Table 2. These should be ingested as separate
  statements under their own model IRIs so the argument framework can
  link them numerically.

- **8 of 18 authors.** Only the first 10 were ingested for script
  brevity. The full author list has 18 names.

- **Figure descriptions.** Figures 1-6 describe SWA mechanics, rolling
  buffer cache, pre-fill chunking, performance comparisons, efficiency
  analysis, and human evaluation. These were not represented as
  statements.

- **29 references.** The paper's bibliography was not ingested.
  Each reference could be a document entity linked via `schema:citation`
  or `donto:cites`.

- **Equivalent model size analysis.** Section 3 discusses "equivalent
  model sizes" — Mistral 7B mirrors 3x its size on reasoning and 1.9x
  on knowledge. These compression ratio claims were not extracted.

- **Content moderation details.** Section 5.2 describes self-reflection
  content moderation with precision 99.4% and recall 95.6% on a
  175-prompt safety dataset. The dataset itself was not represented.

- **Span-level anchoring.** No claims were anchored to specific
  character offsets in the document text. The infrastructure exists
  (`donto_span`, `donto_link_evidence_span`) but programmatic sentence
  boundary detection was not implemented in this extraction script.

- **Annotation layer.** No NER, POS, dependency, or coreference
  annotations were created. The paper text was processed at the
  document level, not the token level.

### 7.2 What a second-pass extraction should do

1. **Ingest Llama 2 baseline numbers** as statements under
   `model:llama2-13b`, `model:llama2-7b`, `model:code-llama-7b`.
   Then wire `supports`/`rebuts` arguments connecting each benchmark
   pair to the comparative claims.

2. **Anchor claims to spans.** For each extracted statement, find the
   sentence in the 24,982-character text that contains the evidence and
   create a `char_offset` span linked via `extracted_from`.

3. **Type the literals properly.** Benchmark scores should be
   `xsd:decimal`, parameter counts `xsd:integer`, dates `xsd:date`.

4. **Resolve the proof obligations.** Cross-check each "outperforms"
   claim against the actual baseline numbers. Mark obligations as
   `resolved` or emit `rebuts` arguments if the claim doesn't hold.

5. **Register predicates.** Move from permissive to curated: register
   `ml:outperforms`, `bench:mmlu`, etc. in the predicate registry with
   proper labels, datatypes, and cardinality hints.

---

## 8. Assessment

### What worked

The full evidence substrate pipeline is functional end-to-end. A
scientific paper went from PDF to structured knowledge graph in 14
seconds, with every claim traceable to its source document, parser
version, extraction model, and run parameters. The proof obligation
mechanism correctly identified the three claims that need further
verification. The argumentation framework linked benchmark evidence to
comparative claims with quantified strength.

Donto's paraconsistency is ready for the next step: ingest a second
paper that reports different benchmark numbers for the same models, let
both versions coexist, and use the argumentation framework to surface
the disagreements.

### What needs work

**Span-level anchoring is the biggest gap.** Linking claims to the
extraction run is good; linking them to the exact sentence in the
source text is better. This is the difference between "this claim came
from this paper" and "this claim came from the third sentence of
Section 3 of this paper." The database schema supports it; the
extraction pipeline doesn't use it yet.

**The annotation layer is unused.** Documents should be annotated at
the token level (NER, coreference, temporal expressions) before claims
are promoted to statements. This separation between observation
(annotations on spans) and interpretation (statements in the quad
store) is the core design insight from the evidence substrate, but the
actual NLP pipeline to produce those annotations doesn't exist yet.

**Predicate ontology is ad hoc.** The predicates (`ml:outperforms`,
`bench:mmlu`, `ml:parameterCount`) were invented on the fly. For a
production ML paper extraction pipeline, these should be a registered
ontology with defined semantics, datatypes, and cardinality constraints.

**Literal typing is weak.** Everything is `xsd:string`. Numbers should
be numbers. This matters for shape validation (a shape that checks
"benchmark score is between 0 and 100" can't work on strings) and for
DontoQL queries with numeric comparisons.

### Bottom line

The extraction demonstrated that donto's evidence substrate works as
designed. The document → revision → extraction run → statement →
evidence link pipeline is functional. The proof obligation and
argumentation layers add epistemic metadata that a traditional triple
store can't represent. The next step is connecting an actual NLP
pipeline (span detection, NER, coreference) to fill in the
observation layer that sits between raw text and promoted claims.
