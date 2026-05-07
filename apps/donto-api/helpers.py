"""Shared extraction and ingestion helpers.

Used by both the FastAPI endpoints (main.py) and the Temporal activities
(activities.py). All functions are async and use module-level httpx clients.
"""

import asyncio
import json
import logging
import os
import re
import time

import httpx

try:
    import trafilatura
except ImportError:
    trafilatura = None

logger = logging.getLogger("donto-api")

DONTOSRV = os.environ.get("DONTOSRV_URL", "http://127.0.0.1:7879")
OPENROUTER_URL = "https://openrouter.ai/api/v1/chat/completions"
OPENROUTER_KEY = os.environ.get("OPENROUTER_API_KEY", "")

DEFAULT_MODEL = "x-ai/grok-4.1-fast"
FALLBACK_MODEL = "mistralai/mistral-large-2512"

srv = httpx.AsyncClient(base_url=DONTOSRV, timeout=600.0)
openrouter = httpx.AsyncClient(timeout=600.0)


def resolve_model(name: str) -> str:
    return {"grok": DEFAULT_MODEL, "fast": DEFAULT_MODEL, "default": DEFAULT_MODEL,
            "mistral": FALLBACK_MODEL, "fallback": FALLBACK_MODEL}.get(name, name)


def confidence_to_maturity(c: float) -> int:
    if c >= 0.95: return 4
    if c >= 0.8: return 3
    if c >= 0.6: return 2
    if c >= 0.4: return 1
    return 0


def parse_fact_object(obj):
    """Parse LLM extraction output object into dontosrv assert format."""
    if isinstance(obj, dict):
        if "iri" in obj:
            return obj["iri"], None
        if "literal" in obj:
            lit = obj["literal"]
            return None, {"v": lit.get("v"), "dt": lit.get("dt", "xsd:string"), "lang": lit.get("lang")}
        if "v" in obj:
            return None, {"v": obj["v"], "dt": obj.get("dt", "xsd:string"), "lang": obj.get("lang")}
    if isinstance(obj, str):
        if obj.startswith("ex:") or obj.startswith("http") or obj.startswith("ctx:"):
            return obj, None
        return None, {"v": obj, "dt": "xsd:string"}
    return None, {"v": obj, "dt": "xsd:string"}


EXTRACTION_PROMPT = """You are a predicate extraction engine. Given a source text,
extract the MAXIMUM number of atomic (subject, predicate, object) triples.

Your goal is TOTAL EXTRACTION — every relationship, every property, every
attribution, every event the text expresses or directly implies becomes a
triple. Not a summary. Not the main points. Every single claim.

Mint predicate names yourself in camelCase. Be specific — prefer
"graduatedFromUniversity" over "relatedTo".

## OUTPUT FORMAT

Return a JSON object with a single "facts" array. Each fact:

{
  "subject":   "ex:<kebab-case-subject>",
  "predicate": "<camelCase>",
  "object":    {"iri": "ex:<kebab-case>"} | {"literal": {"v": <value>, "dt": "<xsd type>"}},
  "anchor":    {"doc": "<doc-iri>", "start": <int>, "end": <int>} | null,
  "hypothesis_only": <bool>,
  "confidence": <0.0-1.0>,
  "notes":     "<brief justification>"
}

If a claim is not supported by an explicit text span, set anchor=null
AND hypothesis_only=true. Never invent an anchor.

## CONTENT FOCUS

IGNORE website boilerplate (nav, footer, cookie notices, CMS chrome).
Extract from the substantive document content only.

## RULES

1. IRIs must be kebab-lower-case: "ex:mrs-watson", not "ex:MrsWatson".
2. Never use boolean objects — invent a predicate instead.
3. Prefer IRIs over literals for entities.
4. Literals are short: a name, a date, a quote, a number — not a sentence.
5. Confidence: 1.0 stated, 0.9 minor inference, 0.7 significant, 0.5 speculative.
6. 15-30+ distinct subjects per 500-word article.
7. Decompose aggressively. Mint predicates freely.
8. Bias toward MORE triples. Target 100-500+ depending on length.
9. Every fact must be grounded in the text or marked hypothesis_only.

For multi-pass exhaustive extraction with explicit aperture lenses
(surface / linguistic / presupposition / inferential / conceivable /
recursive), see `extraction/apertures.py` and `run_exhaustive`.

Return ONLY the JSON. No commentary."""


async def srv_post(path: str, body=None):
    for attempt in range(3):
        try:
            r = await srv.post(path, json=body)
            if r.status_code >= 400:
                raise Exception(f"dontosrv {path} returned {r.status_code}: {r.text[:300]}")
            text = r.text.strip()
            if not text:
                return {"ok": True}
            try:
                return r.json()
            except Exception:
                return {"raw": text}
        except (httpx.ConnectError, httpx.ReadError, httpx.RemoteProtocolError) as e:
            logger.warning(f"srv_post {path} attempt {attempt+1}/3 failed: {e}")
            if attempt < 2:
                await asyncio.sleep(1)
            else:
                raise Exception(f"dontosrv unreachable after 3 attempts: {e}")


def clean_web_content(text: str) -> str:
    """Strip web boilerplate from text before LLM extraction.

    Three layers:
    1. trafilatura — best-in-class content extraction (if installed)
    2. Heuristic stripping — remove nav-like patterns
    3. The LLM prompt itself also instructs ignoring boilerplate
    """
    # Preserve YAML frontmatter (metadata header from our agents)
    frontmatter = ""
    body = text
    if text.startswith("---"):
        parts = text.split("---", 2)
        if len(parts) >= 3:
            frontmatter = f"---{parts[1]}---\n\n"
            body = parts[2]

    # Layer 1: trafilatura (if the content looks like HTML or has HTML artifacts)
    if trafilatura is not None:
        has_html_signs = any(marker in body for marker in ["<html", "<div", "<nav", "<footer"])
        has_nav_patterns = body.count("\n \n") > 10 or body.count("\n\n\n") > 5
        if has_html_signs or has_nav_patterns:
            try:
                extracted = trafilatura.extract(
                    body,
                    include_comments=False,
                    include_tables=True,
                    favor_recall=True,
                    output_format="txt",
                )
                if extracted and len(extracted) > 50:
                    body = extracted
                    return frontmatter + body
            except Exception:
                pass

    # Layer 2: heuristic stripping for plain-text nav boilerplate
    lines = body.split("\n")
    cleaned_lines = []
    consecutive_short = 0
    nav_block = False

    for line in lines:
        stripped = line.strip()

        # Skip empty lines in sequences (compress whitespace)
        if not stripped:
            if cleaned_lines and cleaned_lines[-1] != "":
                cleaned_lines.append("")
            continue

        # Detect nav-like patterns: short lines (< 40 chars) in long runs
        if len(stripped) < 40 and not any(c in stripped for c in ".,:;!?()[]"):
            consecutive_short += 1
            if consecutive_short >= 3:
                nav_block = True
                continue
        else:
            if nav_block and len(stripped) < 40:
                continue
            consecutive_short = 0
            nav_block = False

        # Skip common boilerplate phrases
        boilerplate = [
            "skip to content", "skip to main", "toggle navigation",
            "cookie", "privacy policy", "terms of use", "all rights reserved",
            "© ", "copyright ", "powered by", "back to top",
            "sign in", "sign up", "log in", "subscribe",
            "read more", "learn more", "view all",
        ]
        if any(stripped.lower().startswith(bp) for bp in boilerplate):
            continue
        if stripped.lower() in [bp for bp in boilerplate]:
            continue

        cleaned_lines.append(line)

    body = "\n".join(cleaned_lines)

    # Collapse excessive whitespace
    body = re.sub(r'\n{4,}', '\n\n\n', body)

    return frontmatter + body.strip()


async def call_openrouter(text: str, model: str) -> tuple[list[dict], dict]:
    """Call OpenRouter and return (parsed_facts, metadata)."""
    cleaned_text = clean_web_content(text)
    resp = await openrouter.post(
        OPENROUTER_URL,
        headers={"Authorization": f"Bearer {OPENROUTER_KEY}", "Content-Type": "application/json"},
        json={
            "model": model,
            "temperature": 0.1,
            "max_tokens": 32768,
            "messages": [
                {"role": "system", "content": EXTRACTION_PROMPT},
                {"role": "user", "content": f"Extract all predicates from the following text:\n\n---\n{cleaned_text}\n---"},
            ],
        },
    )
    if resp.status_code != 200:
        raise Exception(f"OpenRouter returned {resp.status_code}: {resp.text[:500]}")

    raw_response = resp.json()
    usage = raw_response.get("usage", {})
    metadata = {
        "openrouter_id": raw_response.get("id"),
        "model_used": raw_response.get("model"),
        "prompt_tokens": usage.get("prompt_tokens", 0),
        "completion_tokens": usage.get("completion_tokens", 0),
        "total_tokens": usage.get("total_tokens", 0),
        "cost": usage.get("total_cost") or usage.get("cost"),
        "native_tokens_prompt": usage.get("native_tokens_prompt"),
        "native_tokens_completion": usage.get("native_tokens_completion"),
    }

    content = raw_response["choices"][0]["message"]["content"]

    cleaned = content.strip()
    if cleaned.startswith("```"):
        cleaned = re.sub(r"^```(?:json)?\s*", "", cleaned)
        cleaned = re.sub(r"\s*```$", "", cleaned)

    try:
        return json.loads(cleaned)["facts"], metadata
    except (json.JSONDecodeError, KeyError):
        repaired = cleaned
        repaired = re.sub(r',\s*([}\]])', r'\1', repaired)
        repaired = re.sub(r'}}\s*,\s*"', '},"', repaired)
        repaired = re.sub(r'},\s*"subject"\s*:', '},{"subject":', repaired)
        repaired = re.sub(r'}\s*{', '},{', repaired)
        repaired = re.sub(r'}\s*"(?!:)', '},"', repaired)
        repaired = re.sub(r'}}\s*]', '}]', repaired)
        if repaired.count('{') > repaired.count('}'):
            last_brace = repaired.rfind('}')
            if last_brace > 0:
                repaired = repaired[:last_brace+1] + ']}'
        try:
            return json.loads(repaired)["facts"], metadata
        except (json.JSONDecodeError, KeyError):
            try:
                fact_pattern = r'\{[^{}]*(?:\{[^{}]*\}[^{}]*)*\}'
                raw_facts = re.findall(fact_pattern, repaired)
                parsed = []
                for rf in raw_facts:
                    try:
                        obj = json.loads(rf)
                        if 'subject' in obj and 'predicate' in obj:
                            parsed.append(obj)
                    except json.JSONDecodeError:
                        continue
                if parsed:
                    logger.warning(f"JSON repair partial: recovered {len(parsed)} facts from regex extraction")
                    return parsed, metadata
            except Exception:
                pass
            logger.error(f"JSON repair failed completely. First 500 chars: {cleaned[:500]}")
            raise Exception(f"Failed to parse extraction output. First 300 chars: {cleaned[:300]}")


async def register_source_document(context: str, text: str, model: str, facts: list[dict] = None) -> None:
    """Register the source text and extraction output as documents in dontosrv."""
    doc_iri = f"doc:{context.replace('ctx:', '')}"
    try:
        result = await srv_post("/documents/register", {
            "iri": doc_iri,
            "media_type": "text/plain",
            "label": context,
        })
        doc_id = result.get("document_id")
        if doc_id:
            await srv_post("/documents/revision", {
                "document_id": doc_id,
                "body": text,
                "parser_version": model,
            })
            if facts:
                await srv_post("/documents/revision", {
                    "document_id": doc_id,
                    "body": json.dumps(facts),
                    "parser_version": f"{model}/extraction",
                })
    except Exception as e:
        logger.warning(f"Failed to register source document for {context}: {e}")


async def ingest_facts(facts: list[dict], context: str) -> int:
    """Convert extracted facts to dontosrv assert format and batch-insert."""
    await srv_post("/contexts/ensure", {"iri": context, "kind": "custom", "mode": "permissive"})

    statements = []
    for f in facts:
        subj = f.get("subject")
        pred = f.get("predicate")
        if not subj or not pred:
            continue
        obj_iri, obj_lit = parse_fact_object(f.get("object"))
        if not obj_iri and not obj_lit:
            continue
        confidence = f.get("confidence", 0.7)
        if isinstance(confidence, str):
            try: confidence = float(confidence)
            except: confidence = 0.7
        statements.append({
            "subject": subj,
            "predicate": pred,
            "object_iri": obj_iri,
            "object_lit": obj_lit,
            "context": context,
            "polarity": "asserted",
            "maturity": confidence_to_maturity(confidence),
        })

    if statements:
        result = await srv_post("/assert/batch", {"statements": statements})
        return result if isinstance(result, int) else len(statements)
    return 0


def compute_yield(facts: list[dict]) -> dict:
    """Compute extraction-yield metrics from a list of facts.

    Replaces the 8-tier breakdown with signals that actually scale with
    extraction effort: how many distinct claims, how anchored they are,
    how many are hypothesis-only, and (when an exhaustive multi-pass
    run is in play) how many came from each aperture.

    Reads `aperture` (set by `extraction.exhaustive`), `anchor`, and
    `hypothesis_only` from each fact. Falls back gracefully when these
    fields are absent — single-pass output still gets totals and
    diversity counts, just no per-aperture breakdown.
    """
    if not facts:
        return {
            "total_facts": 0,
            "distinct_predicates": 0,
            "distinct_subjects": 0,
            "anchor_coverage": 0.0,
            "hypothesis_density": 0.0,
            "facts_per_aperture": {},
        }
    by_aperture: dict[str, int] = {}
    anchored = 0
    hypothetical = 0
    for f in facts:
        ap = f.get("aperture")
        if ap:
            by_aperture[ap] = by_aperture.get(ap, 0) + 1
        if f.get("anchor"):
            anchored += 1
        if f.get("hypothesis_only"):
            hypothetical += 1
    return {
        "total_facts": len(facts),
        "distinct_predicates": len({f.get("predicate") for f in facts if f.get("predicate")}),
        "distinct_subjects":   len({f.get("subject")   for f in facts if f.get("subject")}),
        "anchor_coverage":     round(anchored / len(facts), 3),
        "hypothesis_density":  round(hypothetical / len(facts), 3),
        "facts_per_aperture":  by_aperture,
    }


# Back-compat shim. Existing callers still receive a dict shaped like
# the old 8-tier histogram, but the contents are zeros — tiers are no
# longer mined. Callers should migrate to `compute_yield`.
def compute_tiers(_facts: list[dict]) -> dict:
    return {f"t{i}": 0 for i in range(1, 9)}
