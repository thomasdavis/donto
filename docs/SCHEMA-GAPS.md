# Schema Gaps: What Donto Needs to Handle Any Extraction Domain

Audit of what's missing from the evidence substrate for donto to be a
general-purpose extraction target — not just scientific papers, but
legal documents, medical records, financial filings, genealogy sources,
web scrapes, conversation logs, codebases, and any other domain where
an AI extractor turns unstructured content into structured claims.

**Current state:** 34 tables, 34 migrations, 35.5M statements, serving
genealogy research, ML paper extraction, and salon business data.

---

## 1. Structured Document Regions (Tables, Figures, Sections)

### The gap

We have `donto_span` for character-offset regions, but no structured
representation for tables, figures, or document sections. An extractor
processing a financial filing needs to say "this revenue number came
from row 3, column Q2-2025, of the income statement on page 4." An
extractor processing a medical record needs "this diagnosis came from
the Assessment section." A legal extractor needs "this clause is in
§4.2(b) of the contract."

Spans alone can't represent this. A table cell is not just a character
range — it has a row index, column index, row header, and column
header. A section has a level (h1/h2/h3), a title, and nesting.

### The solution

Three new tables:

**`donto_document_section`** — Hierarchical document structure.

```sql
create table donto_document_section (
    section_id    uuid primary key default gen_random_uuid(),
    revision_id   uuid not null references donto_document_revision(revision_id),
    parent_section_id uuid references donto_document_section(section_id),
    level         smallint not null default 1,  -- 1=h1, 2=h2, etc.
    title         text,
    ordinal       int not null default 0,       -- position among siblings
    span_id       uuid references donto_span(span_id), -- optional anchor
    metadata      jsonb not null default '{}'
);
```

**`donto_table`** — A table within a document revision.

```sql
create table donto_table (
    table_id      uuid primary key default gen_random_uuid(),
    revision_id   uuid not null references donto_document_revision(revision_id),
    section_id    uuid references donto_document_section(section_id),
    label         text,          -- "Table 2", "Income Statement"
    caption       text,
    row_count     int,
    col_count     int,
    span_id       uuid references donto_span(span_id),
    metadata      jsonb not null default '{}'
);
```

**`donto_table_cell`** — Individual cells with row/column identity.

```sql
create table donto_table_cell (
    cell_id       uuid primary key default gen_random_uuid(),
    table_id      uuid not null references donto_table(table_id),
    row_idx       int not null,
    col_idx       int not null,
    is_header     boolean not null default false,
    row_header    text,          -- resolved header for this row
    col_header    text,          -- resolved header for this column
    value         text,
    value_numeric double precision, -- parsed numeric value if applicable
    span_id       uuid references donto_span(span_id),
    metadata      jsonb not null default '{}'
);
```

### Why this matters beyond papers

- **Financial:** Revenue tables, balance sheets, quarterly comparisons
- **Legal:** Contract clause tables, compliance matrices
- **Medical:** Lab result tables, medication lists, vitals charts
- **Genealogy:** Census record tables, parish register grids
- **Code:** API documentation tables, changelog matrices

### What this enables

An extractor can say: "The claim that Mistral 7B scored 60.1% on MMLU
came from `donto_table_cell(table='Table 2', row='Mistral 7B',
col='MMLU')` which anchors to `donto_span(start=X, end=Y)` in
revision `2fe3ba06`." The evidence chain is fully grounded in
document structure.

---

## 2. Mentions and Entity Resolution

### The gap

An extractor sees "Mistral 7B" in a document 15 times. Each occurrence
is a **mention** — a text span that refers to something. Those mentions
need to be grouped into entities. Currently the extractor has to jump
straight from span to statement, losing the intermediate layer.

This matters because:
- The same entity may be referred to by different names ("Mistral 7B",
  "the model", "it", "our model")
- Different entities may share a name ("Cambridge" the city vs
  "Cambridge" the university)
- Coreference resolution is uncertain — the extractor may not be sure
  whether two mentions refer to the same thing

### The solution

**`donto_mention`** — A span identified as referring to something.

```sql
create table donto_mention (
    mention_id    uuid primary key default gen_random_uuid(),
    span_id       uuid not null references donto_span(span_id),
    mention_type  text not null check (mention_type in (
        'entity', 'event', 'relation', 'attribute',
        'temporal', 'quantity', 'citation', 'custom'
    )),
    entity_iri    text,          -- resolved entity IRI (null if unresolved)
    candidate_iris text[],       -- alternative entity IRIs if ambiguous
    confidence    double precision,
    run_id        uuid references donto_extraction_run(run_id),
    metadata      jsonb not null default '{}'
);
```

**`donto_coref_cluster`** — Groups of mentions that refer to the same
entity.

```sql
create table donto_coref_cluster (
    cluster_id    uuid primary key default gen_random_uuid(),
    revision_id   uuid not null references donto_document_revision(revision_id),
    resolved_iri  text,          -- the entity IRI this cluster resolves to
    confidence    double precision,
    run_id        uuid references donto_extraction_run(run_id),
    metadata      jsonb not null default '{}'
);

create table donto_coref_member (
    cluster_id    uuid not null references donto_coref_cluster(cluster_id),
    mention_id    uuid not null references donto_mention(mention_id),
    is_representative boolean not null default false,
    primary key (cluster_id, mention_id)
);
```

### Why this matters beyond papers

- **Legal:** "the Defendant", "Smith Corp", "the Company", "it" all
  refer to the same entity — coreference is critical for contract
  analysis
- **Medical:** "the patient", "Mr. Jones", "he" — wrong coreference
  means wrong diagnosis attribution
- **Genealogy:** "Alice", "Mrs. Smith", "the deceased" — genealogy
  is fundamentally about entity resolution across records that use
  different names for the same person
- **Web scraping:** The same business appears as "Max's Salon",
  "Maks Salons", "Max Salon Fitzroy" — fuzzy entity resolution
- **Conversations:** Speaker diarization + coreference across turns

### What this enables

The pipeline becomes: span → mention → coref cluster → entity IRI →
statement. Each step is recorded, traceable, and correctable. A proof
obligation can target a specific level: "needs-coref" means the
mentions exist but the cluster hasn't been resolved. "needs-entity-
disambiguation" means the cluster exists but the entity IRI is
ambiguous.

---

## 3. Extraction Chunks

### The gap

LLMs have context windows. A 25K-character paper gets chunked into
segments. Each chunk produces some claims. If a claim is wrong, you
need to know which chunk it came from. The extraction run records
`chunking_strategy` but not the individual chunks.

### The solution

**`donto_extraction_chunk`** — A segment of a document processed in
one LLM call.

```sql
create table donto_extraction_chunk (
    chunk_id      uuid primary key default gen_random_uuid(),
    run_id        uuid not null references donto_extraction_run(run_id),
    revision_id   uuid not null references donto_document_revision(revision_id),
    chunk_index   int not null,
    start_offset  int,           -- character offset in the revision body
    end_offset    int,
    token_count   int,           -- estimated token count
    prompt_hash   bytea,         -- hash of the prompt sent for this chunk
    response_hash bytea,         -- hash of the raw LLM response
    latency_ms    int,           -- how long the LLM call took
    metadata      jsonb not null default '{}'
);
```

### Why this matters beyond papers

- **Long documents:** Legal contracts (100+ pages), medical records
  (years of notes), codebases (thousands of files)
- **Debugging:** "The extractor hallucinated a date" → which chunk? →
  what was the prompt? → what was the response?
- **Reproducibility:** Re-running a single chunk instead of the whole
  document when you fix a prompt
- **Cost tracking:** Token count × model pricing per chunk

---

## 4. Statement-Level Confidence

### The gap

`donto_annotation` has confidence. `donto_evidence_link` has
confidence. `donto_statement` does not. The extractor has to store
extraction confidence indirectly — either on the evidence link or as a
separate shape annotation.

The PRD says confidence is a Phase 5+ sparse overlay. But every real
extractor produces per-claim confidence, and storing it sideways is
awkward.

### The solution

A sparse overlay table, not a column on `donto_statement` (adding a
column would require a full table rewrite on 35.5M rows):

**`donto_stmt_confidence`** — Per-statement confidence overlay.

```sql
create table donto_stmt_confidence (
    statement_id  uuid primary key
                  references donto_statement(statement_id) on delete cascade,
    confidence    double precision not null check (confidence >= 0 and confidence <= 1),
    confidence_source text not null default 'extraction',
    run_id        uuid references donto_extraction_run(run_id),
    set_at        timestamptz not null default now(),
    metadata      jsonb not null default '{}'
);
```

This is consistent with the existing pattern: `donto_retrofit`,
`donto_stmt_certificate`, and `donto_stmt_shape_annotation` are all
sparse overlays on `donto_statement`.

### Why this matters beyond papers

Every extraction domain produces confidence:
- **Medical NER:** "I'm 92% sure this is a drug name"
- **Legal clause classification:** "85% likely this is an indemnity
  clause"
- **Genealogy:** "70% confident this is the same Alice Smith"
- **Sentiment analysis:** "0.6 positive"
- **OCR:** Character-level confidence from the OCR engine

### What this enables

Shape validation: "flag all statements with confidence < 0.5."
Maturity promotion: "don't promote to Level 2 if extraction confidence
is below threshold." DontoQL queries: "show me low-confidence claims
about this entity."

---

## 5. Units and Normalization

### The gap

"60.1%", "0.601", "60.1 percent" are the same number.
"700 attoseconds" and "0.7 femtoseconds" are the same measurement.
"$1.2B" and "1200000000 USD" are the same amount.
"2023-10-10" and "October 10, 2023" are the same date.

There's no unit registry, no normalization layer, and no way for a
shape to say "this benchmark score must be between 0 and 1" if some
extractors emit percentages and some emit decimals.

### The solution

**`donto_unit`** — Unit registry with conversion rules.

```sql
create table donto_unit (
    iri           text primary key,
    label         text,
    dimension     text,          -- 'time', 'ratio', 'currency', 'length', etc.
    si_base       text,          -- SI base unit IRI (e.g., 'unit:second')
    si_factor     double precision, -- multiplier to convert to SI base
    metadata      jsonb not null default '{}'
);
```

Seed with common units:

```sql
insert into donto_unit (iri, label, dimension, si_base, si_factor) values
  ('unit:accuracy', 'accuracy', 'ratio', 'unit:ratio', 1.0),
  ('unit:percent', 'percent', 'ratio', 'unit:ratio', 0.01),
  ('unit:bleu', 'BLEU score', 'score', null, null),
  ('unit:attosecond', 'attosecond', 'time', 'unit:second', 1e-18),
  ('unit:femtosecond', 'femtosecond', 'time', 'unit:second', 1e-15),
  ('unit:usd', 'US dollar', 'currency', null, null),
  ('unit:year', 'year', 'time', 'unit:second', 31557600),
  ('unit:kelvin', 'kelvin', 'temperature', 'unit:kelvin', 1.0)
on conflict do nothing;
```

**`donto_normalize_value`** — SQL function for value normalization.

```sql
create function donto_normalize_value(
    p_value double precision,
    p_from_unit text,
    p_to_unit text
) returns double precision
```

### Why this matters beyond papers

- **Finance:** Revenue in millions vs billions vs raw numbers,
  different currencies
- **Medicine:** Dosage in mg vs g, temperature in F vs C, blood
  pressure in mmHg
- **Physics:** SI vs CGS vs natural units
- **Genealogy:** Dates in different calendar systems (Julian vs
  Gregorian, Japanese era names, Hebrew calendar)
- **Any cross-source comparison:** Two extractors emit the same fact
  in different units

---

## 6. Candidate Claims (Pre-Statement Staging)

### The gap

Everything goes straight into `donto_statement`. There's no "I think
this might be a claim but I haven't decided to assert it." The
maturity ladder (Level 0 = raw) handles this conceptually, but
structurally a tentative extraction and a committed assertion are
indistinguishable.

This matters because:
- An extractor may produce 500 candidate claims from a document, only
  200 of which survive filtering
- The filtering logic (deduplication, confidence threshold, schema
  conformance) should be queryable
- Rejected candidates are valuable — they're negative examples for
  extractor tuning

### The solution

Use `donto_statement` itself with a new context kind and maturity
semantics rather than a separate table. This is the donto way — the
atom is the statement.

```sql
-- Add 'candidate' as a context kind
alter table donto_context drop constraint donto_context_kind_check;
alter table donto_context add constraint donto_context_kind_check
    check (kind in (
        'source','snapshot','hypothesis','user','pipeline',
        'trust','derivation','quarantine','custom','system',
        'candidate'
    ));
```

Candidate claims live in a `candidate` context. Promotion to a
`source` or `pipeline` context = assertion. The original candidate
stays in history (retracted when promoted, traceable via lineage).

A function to promote:

```sql
create function donto_promote_candidate(
    p_statement_id uuid,
    p_target_context text,
    p_actor text default null
) returns uuid
```

### Why this matters beyond papers

- **Any extraction pipeline:** Candidates → filter → promote → enrich
- **Human-in-the-loop:** Show candidates to a curator, let them
  accept/reject/edit
- **Active learning:** Use rejected candidates to improve the
  extractor
- **Audit:** "Why was this claim asserted?" → "It was promoted from
  candidate context X after passing filter Y"

---

## 7. Event/Temporal Expression Layer

### The gap

Many extracted claims have temporal semantics: "In 2023, Mistral was
released." "The patient was admitted on March 3rd." "During the
Victorian era." "Last quarter's revenue."

`donto_statement.valid_time` handles absolute date ranges, but there's
no layer for parsing and normalizing temporal expressions from text
into those ranges.

### The solution

**`donto_temporal_expression`** — Parsed temporal expressions linked
to spans.

```sql
create table donto_temporal_expression (
    expression_id uuid primary key default gen_random_uuid(),
    span_id       uuid not null references donto_span(span_id),
    raw_text      text not null,         -- "last quarter", "2023-10-10"
    resolved_from date,                  -- normalized lower bound
    resolved_to   date,                  -- normalized upper bound
    resolution    text not null default 'exact'
                  check (resolution in (
                      'exact', 'day', 'month', 'year', 'decade',
                      'century', 'relative', 'vague'
                  )),
    reference_date date,                 -- anchor for relative expressions
    confidence    double precision,
    run_id        uuid references donto_extraction_run(run_id),
    metadata      jsonb not null default '{}'
);
```

### Why this matters beyond papers

- **Legal:** Contract effective dates, statute of limitations, filing
  deadlines
- **Medical:** Symptom onset, medication start/stop, appointment dates
- **Finance:** Quarter boundaries, fiscal year vs calendar year,
  "trailing twelve months"
- **Genealogy:** Approximate dates ("circa 1850"), date ranges ("between
  1840 and 1850"), partial dates ("June 1843")
- **News:** "yesterday", "last week", "earlier this year"

---

## 8. Multi-Modal Content

### The gap

Documents aren't just text. PDFs contain images, charts, diagrams,
code blocks, and mathematical formulas. Web pages have screenshots,
videos, and interactive elements. Medical records have imaging data.

`donto_document_revision` stores `body` (text) and `body_bytes`
(binary), but there's no structured way to represent non-textual
content regions within a document.

### The solution

**`donto_content_region`** — Non-textual regions within a revision.

```sql
create table donto_content_region (
    region_id     uuid primary key default gen_random_uuid(),
    revision_id   uuid not null references donto_document_revision(revision_id),
    region_type   text not null check (region_type in (
        'image', 'chart', 'diagram', 'code_block', 'formula',
        'video', 'audio', 'map', 'custom'
    )),
    label         text,               -- "Figure 1", "Equation 3"
    caption       text,
    content_hash  bytea,              -- hash of the extracted content
    content_bytes bytea,              -- the raw content (optional)
    alt_text      text,               -- textual description
    span_id       uuid references donto_span(span_id),
    section_id    uuid references donto_document_section(section_id),
    metadata      jsonb not null default '{}'
);
```

### Why this matters beyond papers

- **Medical:** X-rays, MRI scans, pathology slides — claims extracted
  from image analysis need to anchor to the image
- **Legal:** Signatures, stamps, exhibit photos
- **Real estate:** Floor plans, property photos
- **Finance:** Charts showing revenue trends — a claim about "revenue
  grew 20%" might come from a chart, not text
- **Code:** Code blocks in documentation — a claim about an API's
  behavior should anchor to the code example

---

## 9. Source Quality and Bias Metadata

### The gap

Not all sources are equal. A peer-reviewed journal article, a blog
post, a Wikipedia edit, and an LLM hallucination all produce
statements that look the same in `donto_statement`. The context kind
distinguishes source vs pipeline vs user, but there's no structured
assessment of source quality.

We have `donto_context_env` for advisory overlays ("location=London",
"era=Victorian"), but nothing specifically for source reliability,
bias indicators, or editorial standards.

### The solution

Extend `donto_context_env` with a standard vocabulary rather than
adding a new table. The system already supports arbitrary key-value
overlays per context — define a standard set:

```sql
-- Standard source quality keys
select donto_context_env_set('paper:mistral7b', 'source:type', '"peer-reviewed-preprint"');
select donto_context_env_set('paper:mistral7b', 'source:reliability', '"high"');
select donto_context_env_set('paper:mistral7b', 'source:peer-reviewed', 'false');
select donto_context_env_set('paper:mistral7b', 'source:conflict-of-interest',
    '"Authors are employees of Mistral AI"');
select donto_context_env_set('paper:mistral7b', 'source:retraction-status', '"none"');
```

Plus a function that computes a composite source score:

```sql
create function donto_source_quality(p_context text)
returns jsonb  -- { "reliability": "high", "peer_reviewed": false, ... }
```

### Why this matters beyond papers

- **News:** Source bias (left/right/center), editorial standards,
  fact-checking history
- **Medical:** Evidence level (RCT vs case report vs expert opinion),
  journal impact factor
- **Legal:** Jurisdiction, court level, precedential value
- **Genealogy:** Primary vs secondary vs derivative source
  classification
- **Web scraping:** Domain authority, last-updated freshness, whether
  the content is user-generated

---

## 10. Cross-Domain Identifier Registry

### The gap

The same entity has different identifiers in different systems. A
paper is `arxiv:2310.06825` in arXiv, `doi:10.48550/arXiv.2310.06825`
in DOI, and an internal UUID in donto. A person is a FOAF IRI in one
context and an ORCID in another. A molecule is a PubChem CID, a
ChEBI ID, an InChI string, and a common name.

`donto_predicate` has `canonical_of` for predicate aliases, but
there's no general-purpose entity alias registry.

### The solution

Reuse the existing `SameMeaning` infrastructure for entity identity,
or add a dedicated table:

**`donto_entity_alias`** — Cross-system identifier mappings.

```sql
create table donto_entity_alias (
    alias_iri     text not null,
    canonical_iri text not null,
    system        text,              -- "arxiv", "doi", "orcid", "pubchem"
    confidence    double precision default 1.0,
    registered_by text,
    registered_at timestamptz not null default now(),
    primary key (alias_iri, canonical_iri),
    constraint donto_entity_alias_distinct
        check (alias_iri <> canonical_iri)
);
```

### Why this matters beyond papers

- **Any data integration task:** Merging records from different
  databases
- **Genealogy:** The same person in census, birth certificate, church
  register, all with different name spellings
- **Business:** Company in SEC filings (CIK), stock exchange (ticker),
  tax records (EIN)
- **Medicine:** Drug by brand name, generic name, NDC code, RxNorm CUI

---

## Implementation Plan

### Migration sequence

```
0035_document_sections.sql   — sections, tables, table cells
0036_mentions.sql            — mentions, coref clusters, coref members
0037_extraction_chunks.sql   — per-chunk tracking
0038_confidence.sql          — statement-level confidence overlay
0039_units.sql               — unit registry + normalization function
0040_temporal_expressions.sql — temporal expression parsing layer
0041_content_regions.sql     — non-textual content regions
0042_entity_aliases.sql      — cross-system entity identity
0043_candidate_promotion.sql — candidate context kind + promote function
```

### Priority order

**Must have for any extraction pipeline:**
1. Mentions + entity resolution (0036) — without this, every
   extraction pipeline is reinventing entity resolution ad hoc
2. Statement-level confidence (0038) — every extractor produces it
3. Extraction chunks (0037) — debuggability
4. Table/figure structure (0035) — quantitative claims live in tables

**Important for cross-domain use:**
5. Units and normalization (0039) — cross-source comparison is
   impossible without it
6. Temporal expressions (0040) — valid_time is useless if you can't
   parse "last quarter"
7. Entity aliases (0042) — any data integration task needs this

**Nice to have:**
8. Content regions (0041) — multi-modal extraction
9. Candidate promotion (0043) — can be approximated with context kinds
10. Source quality is covered by existing `donto_context_env`

### What does NOT need a new table

- **Source quality metadata** — use `donto_context_env` with standard
  keys
- **Candidate staging** — use a new context kind (`candidate`), not a
  new table
- **Provenance chains** — already covered by `donto_evidence_link` +
  `donto_stmt_lineage`
- **Argumentation** — already covered by `donto_argument`
- **Proof obligations** — already covered, might need more
  `obligation_type` values

### What to explicitly NOT build

- **A full NLP pipeline.** Donto stores the outputs of NLP. It does
  not run NLP. Tokenization, dependency parsing, NER model inference,
  coreference resolution algorithms — these live in the extraction
  project, not in donto.
- **A search engine.** Donto has FTS and vector similarity for
  retrieval. It is not trying to be Elasticsearch. Dense retrieval
  pipelines live outside.
- **A workflow engine.** Proof obligations are work items. Agent
  bindings define who can work on them. But task scheduling, retry
  logic, and queue management are orchestration concerns, not storage
  concerns.
- **An ontology editor.** Predicate registration and shape definitions
  live in donto. Ontology design tooling does not.
