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


EXTRACTION_PROMPT = """You are a predicate extraction engine. Given a source text (article, transcript,
essay, interview, etc.), extract the MAXIMUM CONCEIVABLE number of atomic
predicates — (subject, predicate, object) triples.

Your goal is TOTAL EXTRACTION. Not a summary. Not the "main points." Every
single relationship, claim, implication, presupposition, rhetorical move,
and philosophical commitment expressed or implied by the text becomes a triple.

You must INVENT predicate names yourself. Use camelCase. Be specific — prefer
"graduatedFrom" over "relatedTo". Mint as many novel predicates as the text demands.

## EXTRACTION TIERS — work through ALL of these. Do not stop at Tier 1.

### Tier 1 — Surface facts (what the text explicitly states)
Identity, classification, biography, affiliation, education, location, temporal,
authorship, quantitative, attribution predicates.

### Tier 2 — Relational and structural (how things connect)
Causal, temporal ordering, mereological, spatial, comparison, dependency,
contrast, succession predicates.

### Tier 3 — Opinions, stances, and evaluative claims
Evaluation, preference, advocacy, criticism, agreement, emotional stance.

### Tier 4 — Epistemic and modal (known, possible, necessary)
Certainty, uncertainty, evidence, knowledge source, possibility, necessity, belief.

### Tier 5 — Pragmatic and rhetorical (what the text DOES)
Speech acts, rhetorical moves, hedging, emphasis, framing, audience.

### Tier 6 — Presuppositions and implicature (assumed without stating)
Presuppositions, implicature, existential commitments, absence.

### Tier 7 — Philosophical and ontological (deep structure)
Ontological, teleological, axiological, deontic, counterfactual, essentialism.

### Tier 8 — Intertextual and contextual (beyond the text itself)
References, cultural context, genre, historical.

## OUTPUT FORMAT

Return a JSON object with a single "facts" array. Each fact:

{
  "subject": "ex:<kebab-case-subject>",
  "predicate": "<camelCase predicate you invented>",
  "object": { "iri": "ex:<kebab-case>" } OR { "literal": { "v": <value>, "dt": "<xsd type>" } },
  "tier": <1-8>,
  "confidence": <0.0-1.0>,
  "notes": "<brief justification>"
}

## CONTENT FOCUS

IGNORE website boilerplate entirely. Do NOT extract facts about:
- Navigation menus, footer links, sidebar items
- "Read more", "Learn more", "Visit", "Subscribe" UI elements
- Cookie notices, privacy policies, copyright statements
- Website section structure (hasMenuSection, hasFooterLink, etc.)
- Generic CMS metadata (page layout, breadcrumbs, social links)

Focus ONLY on the substantive article/document content. If the text is a
web page, extract from the main body content only — the actual article,
report, record, or document. Website chrome is noise.

## CRITICAL RULES

1. ALL IRIs must be kebab-lower-case: "ex:mrs-watson", NOT "ex:MrsWatson".
2. NEVER use boolean objects. Use predicates instead.
3. Prefer IRIs over string literals for entities.
4. String literals must be SHORT (name, date, quote, number — not sentences).
5. Confidence: 1.0 = directly stated, 0.9 = minor inference, 0.7 = significant, 0.5 = speculative.
6. Tier labels must be honest — article metadata is Tier 1, not Tier 8.
7. 15-30+ distinct subjects per 500-word article.
8. Decompose aggressively. Mint predicates freely.
9. Bias toward MORE triples. Target 100-500+ depending on article length.
10. EVERY predicate must be grounded in the text.

Return ONLY the JSON. No commentary before or after."""


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


def compute_tiers(facts: list[dict]) -> dict:
    """Compute tier breakdown from a list of extracted facts."""
    tiers = {f"t{i}": 0 for i in range(1, 9)}
    for f in facts:
        t = f.get("tier", 1)
        if isinstance(t, str):
            try: t = int(t)
            except: t = 1
        key = f"t{min(max(t, 1), 8)}"
        tiers[key] = tiers.get(key, 0) + 1
    return tiers
