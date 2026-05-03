---
title: Paperclip + Qwen Donto Extraction
description: Local arXiv and paper claim extraction into Donto-ready JSONL
---

This guide describes a local pipeline for turning dense scientific papers into
Donto statements:

1. Find and fetch full text with Paperclip.
2. Run a small local Qwen model over the paper with explicit Donto tier passes.
3. Preserve the complete model response for review.
4. Convert normalized statements into Donto JSONL.
5. Ingest the statements into a source or pipeline context.

The intended use case is high-volume paper triage and replication planning:
extract the paper's explicit claims, testable hypotheses, evidence, conclusions,
hedges, and assumptions, then hand those statements to Donto and downstream
replication tools.

## Paper discovery

Paperclip can search arXiv and return stable paper handles:

```bash
paperclip search "Lean theorem proving proof automation large language models" \
  -s arxiv \
  -n 5

paperclip search "causal reasoning graphs large language models causal graph" \
  -s arxiv \
  -n 5
```

Fetch the full text as line-oriented content:

```bash
paperclip cat --full /papers/arx_2404.12534/content.lines \
  > data/paperclip_papers/arx_2404.12534.lines
```

When using Paperclip `content.lines`, strip transport prefixes such as
`L123:` before prompting or source-grounding. Keep the original line numbers in
metadata or citation fields when they are useful for review.

## Extraction shape

The extraction prompt should work through all Donto claim tiers, not just
surface facts:

| Tier | Focus |
|---|---|
| 1 | Surface facts explicitly stated in the text |
| 2 | Relational, causal, temporal, structural, and comparison claims |
| 3 | Opinions, stances, criticism, advocacy, and evaluation |
| 4 | Epistemic and modal claims: evidence, certainty, possibility, necessity |
| 5 | Pragmatic and rhetorical moves: hedging, framing, emphasis, speech acts |
| 6 | Presuppositions, implicature, commitments, and notable absences |
| 7 | Philosophical or ontological structure, including axiological and deontic claims |
| 8 | Intertextual and contextual references beyond the paper text itself |

For reproducibility, store both forms:

- `raw_tier_outputs`: the complete Qwen response for each tier, without
  truncating the model's rationale or extracted claims.
- `parsed_donto_bundle`: normalized JSON containing `donto_import.statements`
  that can be converted into Donto JSONL.

## Local Qwen run

One tested local run used `Qwen/Qwen3.5-2B-Base` with a lightweight Donto schema
LoRA adapter and Paperclip inputs:

```bash
python3 llm_memory_system/bench_paper_analysis.py \
  --model Qwen/Qwen3.5-2B-Base \
  --task donto \
  --conditions donto_tier_lora \
  --format-lora-load results/adapters/donto_schema_lora_qwen35_2b_base.pt \
  --no-default-papers \
  --paperclip-ids arx_2404.12534 arx_2406.16605 \
  --grade-against-source \
  --device mps \
  --max-input-tokens 4096 \
  --max-new-tokens 3072 \
  --output results/donto_qwen35_lora_lean_causal_two_papers.json
```

The key operational choice is that the run is tier-stepped: the model is asked
for each tier separately so omissions are easier to spot and rerun. If an
adapter makes the answer more detailed, grade against source grounding rather
than brevity.

## Donto JSONL mapping

Each normalized statement in the Qwen bundle has this shape:

```json
{
  "subject": "paper:arx_2404.12534",
  "predicate": "donto:claims",
  "object_iri": null,
  "object_lit": {
    "v": "Lean Copilot integrates LLMs into Lean workflows.",
    "dt": "xsd:string"
  },
  "context": "ctx:paperclip/qwen/arx_2404.12534",
  "polarity": "asserted",
  "maturity": 1
}
```

Convert that into Donto ingest JSONL:

```jsonl
{"s":"paper:arx_2404.12534","p":"donto:claims","o":{"v":"Lean Copilot integrates LLMs into Lean workflows.","dt":"xsd:string"},"c":"ctx:paperclip/qwen/arx_2404.12534","pol":"asserted","maturity":1}
```

The object mapping is:

| Qwen bundle field | Donto JSONL field |
|---|---|
| `subject` | `s` |
| `predicate` | `p` |
| `object_iri` | `o.iri` |
| `object_lit` | `o` |
| `context` | `c` |
| `polarity` | `pol` |
| `maturity` | `maturity` |

Then ingest:

```bash
donto ingest claims.jsonl \
  --format jsonl \
  --default-context ctx:paperclip/qwen/run-2026-05-03
```

## Suggested contexts

Use separate contexts so raw extraction, curated claims, and replication
hypotheses can coexist:

```sql
select donto_ensure_context(
  'ctx:src/paperclip/arx_2404.12534',
  'source',
  'permissive',
  null
);

select donto_ensure_context(
  'ctx:paperclip/qwen/arx_2404.12534',
  'pipeline',
  'permissive',
  'ctx:src/paperclip/arx_2404.12534'
);

select donto_ensure_context(
  'ctx:hypo/replication/arx_2404.12534',
  'hypothesis',
  'permissive',
  'ctx:paperclip/qwen/arx_2404.12534'
);
```

## Quality gates

Before ingesting a batch, check:

- The model completed all eight tiers for every paper.
- The saved JSON includes model, adapter, prompt condition, paper IDs, token
  limits, and generation timestamp.
- Evidence quotes are grounded in the cleaned source text.
- Prompt text, Paperclip line-prefix artifacts, and extraction instructions do
  not leak into claims.
- Claims that are uncertain, counterfactual, or merely implied are marked with
  appropriate predicates, polarity, maturity, or hypothesis contexts.

The examples in `docs/examples/paperclip-qwen-donto` show a complete Qwen
response bundle plus the derived JSONL statements. They intentionally keep the
source-grounding grades with the artifacts; a batch can be structurally valid
and still need review before promotion into a curated context.
