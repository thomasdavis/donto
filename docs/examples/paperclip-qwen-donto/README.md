# Paperclip + Qwen Donto Examples

This directory contains example outputs from a local Paperclip to Qwen to Donto
pipeline.

The paired JSON and Markdown files preserve the complete Qwen tier responses for
review. The JSONL file contains the normalized statements in Donto ingest form.

Current example batch:

- `arx_2404.12534`: Lean Copilot: Large Language Models as Copilots for Theorem Proving in Lean
- `arx_2406.16605`: CLEAR: Can Language Models Really Understand Causal Graphs?

Generation settings:

- Model: `Qwen/Qwen3.5-2B-Base`
- Condition: `donto_tier_lora`
- Adapter: `results/adapters/donto_schema_lora_qwen35_2b_base.pt`
- Input: Paperclip `content.lines`, cleaned before prompting and grounding
- Output target: Donto JSONL statements plus full raw tier outputs

Batch summary:

- Papers: 2
- Valid JSON rate: 1.0
- Mean tier coverage: 1.0
- Donto JSONL statements: 32
- Lean Copilot source-grounding grade: A
- CLEAR source-grounding grade: D

The CLEAR example is deliberately kept with its lower grounding grade because it
is useful as a review and quarantine case: the structure is importable, but some
evidence quotes should be checked before promotion into a curated context.
