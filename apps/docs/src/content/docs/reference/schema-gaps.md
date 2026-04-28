---
title: Schema Gaps
description: What donto needs to handle any extraction domain
---

Audit of what's missing from the evidence substrate for donto to be a
general-purpose extraction target — not just scientific papers, but
legal documents, medical records, financial filings, genealogy sources,
web scrapes, conversation logs, codebases, and any other domain where
an AI extractor turns unstructured content into structured claims.

## 1. Structured Document Regions (Tables, Figures, Sections)

We have `donto_span` for character-offset regions, but no structured
representation for tables, figures, or document sections. An extractor
processing a financial filing needs to say "this revenue number came
from row 3, column Q2-2025, of the income statement on page 4."

**Solution:** Three tables — `donto_document_section` (hierarchical structure),
`donto_table` (table metadata), `donto_table_cell` (individual cells with
row/column identity). Implemented in migration 0035.

## 2. Mentions and Entity Resolution

An extractor sees "Mistral 7B" in a document 15 times. Each occurrence
is a **mention** — a text span that refers to something. Those mentions
need to be grouped into entities.

**Solution:** `donto_mention` (span identified as referring to something),
`donto_coref_cluster` and `donto_coref_member` (groups of mentions referring
to the same entity). Implemented in migration 0036.

## 3. Extraction Chunks

LLMs have context windows. A 25K-character paper gets chunked into
segments. Each chunk produces some claims. If a claim is wrong, you
need to know which chunk it came from.

**Solution:** `donto_extraction_chunk` — per-chunk tracking with offsets,
token counts, prompt/response hashes, and latency. Implemented in migration 0037.

## 4. Statement-Level Confidence

Every real extractor produces per-claim confidence.

**Solution:** `donto_stmt_confidence` — sparse overlay table (not a column
on `donto_statement`). Implemented in migration 0038.

## 5. Units and Normalization

"60.1%", "0.601", "60.1 percent" are the same number.
"700 attoseconds" and "0.7 femtoseconds" are the same measurement.

**Solution:** `donto_unit` — unit registry with dimension-aware conversion.
Seeded with 26 common units across 7 dimensions. Implemented in migration 0039.

## 6. Candidate Claims (Pre-Statement Staging)

Everything goes straight into `donto_statement`. There's no "I think
this might be a claim but I haven't decided to assert it."

**Solution:** A new `candidate` context kind. Candidate claims live in a candidate
context; promotion to a source/pipeline context = assertion. Implemented in
migration 0043.

## 7. Event/Temporal Expression Layer

Many extracted claims have temporal semantics that need parsing and
normalizing into `valid_time` ranges.

**Solution:** `donto_temporal_expression` — parsed temporal expressions linked
to spans, with resolution type and confidence. Implemented in migration 0040.

## 8. Multi-Modal Content

Documents aren't just text. PDFs contain images, charts, diagrams.

**Solution:** `donto_content_region` — non-textual regions within a revision,
typed and optionally anchored to spans and sections. Implemented in migration 0041.

## 9. Source Quality and Bias Metadata

Not all sources are equal. Use `donto_context_env` with a standard vocabulary
(`source:type`, `source:reliability`, `source:peer-reviewed`, etc.) rather
than a separate table.

## 10. Cross-Domain Identifier Registry

The same entity has different identifiers in different systems.

**Solution:** `donto_entity_alias` — cross-system identifier mappings with
confidence. Implemented in migration 0042.

## What does NOT need a new table

- **Source quality metadata** — use `donto_context_env` with standard keys
- **Candidate staging** — use a new context kind (`candidate`), not a new table
- **Provenance chains** — covered by `donto_evidence_link` + `donto_stmt_lineage`
- **Argumentation** — covered by `donto_argument`

## What to explicitly NOT build

- **A full NLP pipeline.** Donto stores the outputs of NLP. It does not run NLP.
- **A search engine.** Donto has FTS and vector similarity for retrieval. It is not Elasticsearch.
- **A workflow engine.** Proof obligations are work items, but scheduling lives outside.
- **An ontology editor.** Predicate registration and shape definitions live in donto. Design tooling does not.
