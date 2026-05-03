# Genealogy Research with Donto — From Zero to Complete Family History

A practical, step-by-step guide to using donto for genealogical research. This covers the entire workflow from "I want to research a person" to "I have a comprehensive, source-backed, contradiction-aware family knowledge graph."

**API:** https://genes.apexpots.com  
**Docs:** https://genes.apexpots.com/full-docs  
**Database:** 36M+ statements, 30-class ontology, entity resolution, temporal reasoning

---

## Table of Contents

1. [Start a research project](#1-start-a-research-project)
2. [Find and extract from sources](#2-find-and-extract-from-sources)
3. [Search and explore what you've built](#3-search-and-explore)
4. [Register entities properly](#4-register-entities)
5. [Align predicates across sources](#5-align-predicates)
6. [Resolve entity identities](#6-resolve-entity-identities)
7. [Handle dates and temporal reasoning](#7-temporal-reasoning)
8. [Record values with proper units](#8-values-and-units)
9. [Handle contradictions](#9-handle-contradictions)
10. [Build the family graph](#10-build-the-family-graph)
11. [Check proof obligations](#11-proof-obligations)
12. [Visualize and explore](#12-visualize)
13. [The class hierarchy](#13-class-hierarchy)
14. [Complete worked example](#14-worked-example)

---

## 1. Start a research project

Every research project gets its own **context** — a namespace that scopes all facts from that project.

```
Context naming: ctx:genes/<person-or-topic>/<source-type>
```

Examples:
```
ctx:genes/mary-watson/obituary
ctx:genes/mary-watson/bdm-records
ctx:genes/watson-family/newspaper-articles
ctx:genes/cooktown-history/trove-archive
```

You don't need to create contexts manually — they're auto-created when you extract or assert facts.

---

## 2. Find and extract from sources

### Where to find genealogical sources

Search these in order of reliability:

| Priority | Source Type | Where | What You Get |
|----------|-----------|-------|-------------|
**Start with a simple web search.** Google or DuckDuckGo the person's name + location + era. You'll often find:
- Family history blogs and personal genealogy sites
- Find A Grave memorials
- Ancestry/FamilySearch shared trees
- Local historical society pages
- Forum posts from other researchers who've already done the work

**Extract from anything you find.** The system handles messy, informal, contradictory sources. A blog post from a distant cousin is still useful data — it gets low maturity and the provenance is tracked.

Some specific sources to check:

| Source | Where | What You Get |
|--------|-------|-------------|
| Google/DuckDuckGo | `"firstname lastname" family history` | Blogs, shared trees, forum posts, memorials |
| Find A Grave | findagrave.com | Burial records, photos, family links, obituaries |
| Ancestry shared trees | ancestry.com | Other researchers' trees (treat as unverified) |
| FamilySearch | familysearch.org | Free records: BDM, census, church, immigration |
| Obituaries | newspapers.com, legacy.com, local papers | Family relationships, dates, locations |
| Newspaper archives | Trove (AU), newspapers.com, chroniclingamerica (US) | Articles, notices, court reports |
| Census records | Ancestry, FamilySearch | Household, ages, occupations |
| BDM registries | State government sites | Official birth/death/marriage records |
| Church records | FamilySearch, parish archives | Baptisms, marriages, burials |
| Immigration records | National archives, Ancestry | Ship manifests, arrival dates |
| Wills & probate | State archives | Family relationships, property |
| Military records | National Archives, AWM (AU) | Service, ranks, next of kin |
| DNA results | AncestryDNA, 23andMe | Matches, ethnicity estimates |
| Wikipedia | wikipedia.org | Biographical summaries for notable people |
| Historical societies | Local society websites | Journals, bulletins, transcriptions |
| Reddit/forums | r/Genealogy, rootschat.com | Other researchers, tips, shared findings |

### Extract from each source

For each source you find, copy the text and send it to the API:

```bash
curl -X POST https://genes.apexpots.com/extract-and-ingest \
  -H "Content-Type: application/json" \
  -d '{
    "text": "PASTE THE FULL TEXT OF THE SOURCE HERE",
    "context": "ctx:genes/mary-watson/obituary"
  }'
```

The extraction engine:
- Calls Grok 4.1 Fast via OpenRouter (~$0.005/source)
- Extracts facts across 8 analytical tiers (surface facts → philosophical analysis)
- Maps confidence to maturity levels (L0-L4)
- Ingests everything into the knowledge graph

**Repeat for every source you find.** Different sources go in different contexts.

### Batch extraction with the job system

For bulk ingestion (e.g. hundreds of newspaper articles), use the async job system instead of `/extract-and-ingest`. Jobs return immediately and run in the background — no timeouts.

```bash
# Submit a single job (returns instantly with a job_id)
curl -X POST https://genes.apexpots.com/jobs/extract \
  -H "Content-Type: application/json" \
  -d '{
    "text": "PASTE TEXT HERE",
    "context": "ctx:genes/topic/source"
  }'
# → {"job_id": "a1b2c3d4", "status": "queued"}

# Submit multiple jobs at once (up to 4 run concurrently)
curl -X POST https://genes.apexpots.com/jobs/batch \
  -H "Content-Type: application/json" \
  -d '{
    "items": [
      {"text": "First article...", "context": "ctx:genes/topic/article-1"},
      {"text": "Second article...", "context": "ctx:genes/topic/article-2"},
      {"text": "Third article...", "context": "ctx:genes/topic/article-3"}
    ]
  }'
# → {"job_ids": ["a1b2c3d4", "e5f6g7h8", "i9j0k1l2"], "count": 3}

# Poll for results
curl https://genes.apexpots.com/jobs/a1b2c3d4
# → {"id": "a1b2c3d4", "status": "completed", "facts_extracted": 142, ...}

# List all jobs
curl https://genes.apexpots.com/jobs
# → {"jobs": [...], "total": 45, "summary": {"completed": 40, "extracting": 3, "queued": 2}}
```

Job statuses: `queued` → `extracting` → `ingesting` → `completed` (or `failed`).

---

## 3. Search and explore

### Find a person by name

```bash
curl https://genes.apexpots.com/search?q=mary+watson
```

Returns matching entities ordered by how many facts exist about them:
```json
{"matches": [
  {"subject": "ex:mary-watson", "label": "Mary Watson", "count": 72},
  {"subject": "ctx:genealogy/research-db/iri/31448699f0e5", "label": "Mary Watson", "count": 41}
]}
```

### Get everything about a person

```bash
curl https://genes.apexpots.com/history/ex:mary-watson
```

Returns all facts, including retracted ones, with full metadata:
```json
{"count": 72, "rows": [
  {"predicate": "bornInPlace", "object_iri": "ex:cornwall-england", "maturity": 4},
  {"predicate": "hasBirthYear", "object_lit": {"v": 1860, "dt": "xsd:integer"}, "maturity": 3},
  {"predicate": "marriedTo", "object_iri": "ex:robert-watson", "maturity": 4}
]}
```

### Query with filters

```bash
# All marriages in the Watson family research
curl "https://genes.apexpots.com/match?predicate=marriedTo&context=ctx:genes/watson-family"

# Only high-confidence facts about Mary
curl "https://genes.apexpots.com/match?subject=ex:mary-watson&min_maturity=3"

# Including contradictions
curl "https://genes.apexpots.com/match?subject=ex:mary-watson&polarity=any"
```

---

## 4. Register entities properly

After extraction, every subject and object IRI should be registered as an **entity symbol** with type hints.

The system auto-registers entities when you extract, but you can explicitly register with more detail:

```sql
-- Via the database (donto CLI or direct SQL)
SELECT donto_ensure_symbol('ex:mary-watson', 'person', 'Mary Watson');
SELECT donto_ensure_symbol('ex:cornwall-england', 'place', 'Cornwall, England');
SELECT donto_ensure_symbol('ex:lizard-island', 'place', 'Lizard Island');
```

### Entity types in the ontology

The system has a 30-class hierarchy. The most useful for genealogy:

```
donto:Person     — individual humans
donto:Family     — family units
donto:Place      — locations (Settlement, AdministrativeArea, Region)
donto:Event      — life events (BirthEvent, DeathEvent, MarriageEvent, etc.)
donto:Organization — institutions (churches, government bodies)
donto:Concept    — abstract concepts (Occupation, Role, Ethnicity, Religion)
```

Type a person:
```bash
curl -X POST https://genes.apexpots.com/assert \
  -H "Content-Type: application/json" \
  -d '{
    "subject": "ex:mary-watson",
    "predicate": "rdf:type",
    "object_iri": "donto:Person",
    "context": "ctx:genes/mary-watson",
    "maturity": 4
  }'
```

### Check class relationships

The ontology supports subclass reasoning:

```sql
-- Is Person a subclass of Agent? → true
SELECT donto_is_subclass('donto:Person', 'donto:Agent');

-- Get all ancestors of Person → Agent, Entity
SELECT * FROM donto_class_ancestors('donto:Person');

-- Detect type conflicts (Person AND Place on same entity)
SELECT * FROM donto_check_disjointness('ex:cooktown');
```

---

## 5. Align predicates across sources

Different sources use different predicate names for the same relationship:
- Source 1: `bornIn`
- Source 2: `birthplaceOf`
- Source 3: `bornInPlace`

### Auto-align everything

After extracting from multiple sources, run:

```bash
curl -X POST https://genes.apexpots.com/align/auto?threshold=0.6
```

This scans all predicates, finds name-similar ones, registers `close_match` alignments, and rebuilds the closure.

### Manual alignment for known equivalences

```bash
# bornIn and birthplaceOf are inverses (swap subject/object)
curl -X POST https://genes.apexpots.com/align/register \
  -H "Content-Type: application/json" \
  -d '{
    "source": "bornIn",
    "target": "birthplaceOf",
    "relation": "inverse_equivalent",
    "confidence": 0.95
  }'

# Rebuild the expansion index
curl -X POST https://genes.apexpots.com/align/rebuild
```

### Check what predicates look similar

```bash
curl https://genes.apexpots.com/align/suggest/bornIn?threshold=0.3
```

### Alignment relation types for genealogy

| Relation | Use when | Example |
|----------|---------|---------|
| `exact_equivalent` | Same meaning, same direction | `bornIn` = `bornInPlace` |
| `inverse_equivalent` | Same meaning, swap S/O | `bornIn` ↔ `birthplaceOf` |
| `sub_property_of` | More specific implies general | `baptisedAt` → `bornIn` |
| `close_match` | Similar but not identical | `fatherOf` ≈ `paternalParentOf` |
| `not_equivalent` | Explicitly NOT the same | `diedIn` ≠ `buriedIn` |

---

## 6. Resolve entity identities

The biggest challenge in genealogy: the same person appears under many names.

```
ex:mary-watson          (from one article)
ex:mrs-watson           (from another)
ex:mary-watson-nee-oxley (from a marriage record)
ex:watson-mary          (from a census)
```

### Register entities

First register the IRIs as entity symbols:

```bash
curl -X POST https://genes.apexpots.com/entity/register/batch \
  -H "Content-Type: application/json" \
  -d '{
    "entities": [
      {"iri": "ex:mary-watson", "kind": "person", "label": "Mary Watson"},
      {"iri": "ex:mrs-watson", "kind": "person", "label": "Mrs Watson"},
      {"iri": "ex:mary-oxley", "kind": "person", "label": "Mary Oxley"}
    ]
  }'
```

### Assert identity edges

When you believe two symbols refer to the same person:

```bash
curl -X POST https://genes.apexpots.com/entity/identity \
  -H "Content-Type: application/json" \
  -d '{
    "symbol_a": "ex:mary-watson",
    "symbol_b": "ex:mrs-watson",
    "relation": "same_referent",
    "confidence": 0.95,
    "method": "human",
    "explanation": "Same spouse, same residence, same time period"
  }'
```

When you're not sure:

```bash
curl -X POST https://genes.apexpots.com/entity/identity \
  -H "Content-Type: application/json" \
  -d '{
    "symbol_a": "ex:mary-watson",
    "symbol_b": "ex:mary-oxley",
    "relation": "possibly_same_referent",
    "confidence": 0.65,
    "explanation": "Similar name, overlapping dates, but no direct evidence"
  }'
```

When you know they're different:

```bash
curl -X POST https://genes.apexpots.com/entity/identity \
  -H "Content-Type: application/json" \
  -d '{
    "symbol_a": "ex:mary-watson",
    "symbol_b": "ex:mary-watson-of-sydney",
    "relation": "distinct_referent",
    "confidence": 0.90,
    "explanation": "Different birth date, different parents, different location"
  }'
```

For bulk operations, use the batch endpoint:

```bash
curl -X POST https://genes.apexpots.com/entity/identity/batch \
  -H "Content-Type: application/json" \
  -d '{
    "edges": [
      {"symbol_a": "ex:mary-watson", "symbol_b": "ex:mrs-watson", "relation": "same_referent", "confidence": 0.95, "explanation": "Same person"},
      {"symbol_a": "ex:mary-watson", "symbol_b": "ex:mary-watson-of-sydney", "relation": "distinct_referent", "confidence": 0.90, "explanation": "Different person"}
    ]
  }'
```

### Identity hypotheses

The system maintains three default identity policies:

| Hypothesis | Threshold | Use for |
|-----------|-----------|---------|
| `strict` | ≥0.98 confidence | Official records, certified research |
| `likely` | ≥0.85 confidence | General research, family trees |
| `exploratory` | ≥0.60 confidence | Search, discovery, "who might this be?" |

Query which symbols are in the same cluster:

```bash
# Resolve Mary Watson to her referent and see who else is in the cluster
curl https://genes.apexpots.com/entity/resolve/ex:mary-watson?hypothesis=likely

# List all members of a specific cluster
curl https://genes.apexpots.com/entity/cluster/likely/3

# See all identity edges for a symbol
curl https://genes.apexpots.com/entity/ex:mary-watson/edges

# Get the full family resolution table
curl https://genes.apexpots.com/entity/family-table
```

### Key principle: merges are hypotheses, not destructive rewrites

The original statement `(ex:mrs-watson, bornIn, ex:cornwall)` is never modified. The identity layer maps `ex:mrs-watson` → same referent as `ex:mary-watson` at query time. If the merge was wrong, retract the identity edge and the original data is untouched.

---

## 7. Temporal reasoning

Genealogical dates are messy. Donto handles this properly.

### How dates work

The system parses dates with correct precision:

- `1860` → grain: year, range: [1860-01-01, 1861-01-01)
- `1860-06-15` → grain: day, range: [1860-06-15, 1860-06-16)
- `circa 1860` → grain: year, approximate: true, EDTF: `1860~`
- `1860?` → grain: year, uncertain: true, EDTF: `1860?`

**Critical rule:** "1860" is NOT "1860-01-01". It's a year-grain expression. The system preserves this distinction.

Date parsing happens automatically during extraction. For manual temporal assertions, use the `valid_lo`/`valid_hi` fields on statements.

### Record temporal relationships between events

```sql
-- Mary's birth was before her marriage
-- Allen bitset: before=1
SELECT donto_assert_temporal_relation(
    'ex:mary-watson-birth',
    'ex:mary-watson-marriage',
    1,  -- "before"
    'asserted',
    0.99
);
```

### Allen's 13 interval relations (as bitset values)

| Relation | Bit | Value | Example |
|----------|-----|-------|---------|
| before | 0 | 1 | Birth before marriage |
| meets | 1 | 2 | Education ends when career starts |
| overlaps | 2 | 4 | Two residences overlap |
| starts | 3 | 8 | Marriage starts same time as residence |
| during | 4 | 16 | Child born during marriage |
| finishes | 5 | 32 | Death finishes residence |
| equals | 6 | 64 | Same event, different sources |
| after | 7 | 128 | Inverse of before |
| met_by | 8 | 256 | Inverse of meets |
| overlapped_by | 9 | 512 | Inverse of overlaps |
| started_by | 10 | 1024 | Inverse of starts |
| contains | 11 | 2048 | Inverse of during |
| finished_by | 12 | 4096 | Inverse of finishes |

Combine with OR for uncertain relations: `1 | 4 = 5` means "before OR overlaps".

---

## 8. Values and units

When recording measurements, quantities, or statistics, use the literal canonicalization system:

```sql
-- Register a canonical literal
SELECT donto_ensure_literal(
    'xsd:decimal',                    -- datatype
    '{"v": "5 feet 10 inches"}'::jsonb,  -- raw value
    '{"v": 1.778}'::jsonb,           -- canonical value
    'http://qudt.org/vocab/unit/M',   -- unit IRI (metres)
    1.778,                            -- quantity in SI
    '{"precision": 0.005}'::jsonb     -- precision
);
```

The system tracks:
- **Raw value**: what the source said ("5 feet 10 inches")
- **Canonical value**: normalized form (1.778)
- **Unit**: standard unit IRI
- **SI quantity**: for cross-unit comparison
- **Precision**: measurement uncertainty

---

## 9. Handle contradictions

Donto is **paraconsistent** — contradictions are valuable, not errors.

### Two sources disagree on a birth year

```
Source 1 (obituary): Mary Watson born 1860
Source 2 (census):   Mary Watson born 1862
```

Both are stored. Neither is automatically rejected.

### Query to see contradictions

```bash
# Get all birth year facts for Mary, including contradictions
curl "https://genes.apexpots.com/match?subject=ex:mary-watson&predicate=hasBirthYear&polarity=any"
```

### When you know one is wrong

```bash
# Find the wrong statement's ID
curl "https://genes.apexpots.com/match?subject=ex:mary-watson&predicate=hasBirthYear"

# Retract the wrong one (it stays in the database for history)
curl -X POST "https://genes.apexpots.com/retract/STATEMENT-UUID-HERE"

# Assert the correct one with high maturity
curl -X POST https://genes.apexpots.com/assert \
  -H "Content-Type: application/json" \
  -d '{
    "subject": "ex:mary-watson",
    "predicate": "hasBirthYear",
    "object_lit": {"v": 1860, "dt": "xsd:integer"},
    "context": "ctx:genes/mary-watson/correction",
    "maturity": 4
  }'
```

### When you don't know which is right

Leave both. Add a proof obligation:

```
obligation_type: needs-source-support
detail: {"claim": "birth year 1860 vs 1862", "priority": "high"}
```

The system tracks these as open research tasks.

---

## 10. Build the family graph

### Assert family relationships

```bash
# Parent-child
curl -X POST https://genes.apexpots.com/assert/batch \
  -H "Content-Type: application/json" \
  -d '{
    "statements": [
      {"subject": "ex:mary-watson", "predicate": "parentOf", "object_iri": "ex:watson-infant", "context": "ctx:genes/watson-family"},
      {"subject": "ex:robert-watson", "predicate": "parentOf", "object_iri": "ex:watson-infant", "context": "ctx:genes/watson-family"},
      {"subject": "ex:mary-watson", "predicate": "marriedTo", "object_iri": "ex:robert-watson", "context": "ctx:genes/watson-family"}
    ]
  }'
```

### The inference rules will derive

Once the rule engine runs:
- `parentOf(Mary, child)` → `childOf(child, Mary)` (inverse rule)
- `marriedTo(Mary, Robert)` → `marriedTo(Robert, Mary)` (symmetric rule)
- `rdf:type(Mary, Person)` + `Person subClassOf Agent` → `rdf:type(Mary, Agent)` (subclass rule)

### Query the family tree

```bash
# Get the family subgraph
curl -X POST https://genes.apexpots.com/graph/subgraph \
  -H "Content-Type: application/json" \
  -d '{"predicates": ["parentOf", "childOf", "marriedTo", "siblingOf"], "limit": 500}'

# Get Mary's neighborhood (everyone connected within 2 hops)
curl -X POST https://genes.apexpots.com/graph/neighborhood \
  -H "Content-Type: application/json" \
  -d '{"subject": "ex:mary-watson", "depth": 2, "predicates": ["parentOf", "childOf", "marriedTo"]}'
```

---

## 11. Proof obligations

The system tracks open research tasks:

| Obligation Type | Meaning | Priority |
|----------------|---------|----------|
| `needs-source-support` | Claim lacks source evidence | High for L0 facts |
| `needs-entity-disambiguation` | Which Mary Watson is this? | High for common names |
| `needs-temporal-grounding` | Date is missing or vague | Medium |
| `needs-coref` | Two symbols might be same person | Medium |
| `needs-human-review` | Shape violation or low confidence | Varies |
| `needs-confidence-boost` | Only one weak source | Low |

These are automatically generated during extraction (for low-confidence claims) and during the epistemic sweep (for shape violations and ungrounded claims).

---

## 12. Visualize

### Timeline of a person's life

```bash
curl https://genes.apexpots.com/graph/timeline/ex:mary-watson
```

Returns all time-related facts sorted chronologically — births, deaths, marriages, residences, events.

### Ego graph around a person

```bash
curl -X POST https://genes.apexpots.com/graph/neighborhood \
  -H "Content-Type: application/json" \
  -d '{"subject": "ex:mary-watson", "depth": 2, "limit": 200}'
```

Returns nodes and edges ready for D3.js/Cytoscape.js rendering.

### Compare two people side-by-side

```bash
curl -X POST https://genes.apexpots.com/graph/compare \
  -H "Content-Type: application/json" \
  -d '{"subjects": ["ex:mary-watson", "ex:robert-watson"]}'
```

Returns shared predicates, unique predicates, and all facts for both.

### Graph statistics

```bash
curl https://genes.apexpots.com/graph/stats
```

---

## 13. The class hierarchy

```
donto:Entity
├── donto:Agent
│   ├── donto:Person          ← individual humans
│   ├── donto:Family          ← family units
│   ├── donto:Organization    ← churches, companies
│   └── donto:GovernmentBody  ← colonial admin, courts
├── donto:Place
│   ├── donto:Settlement      ← towns, cities
│   ├── donto:AdministrativeArea ← counties, colonies, states
│   ├── donto:Property        ← stations, farms
│   ├── donto:Building        ← churches, courthouses
│   └── donto:Region          ← geographical areas
├── donto:Event
│   ├── donto:BirthEvent
│   ├── donto:DeathEvent
│   ├── donto:MarriageEvent
│   ├── donto:ResidenceEvent
│   ├── donto:MigrationEvent
│   ├── donto:EmploymentEvent
│   ├── donto:LegalEvent
│   └── donto:PublicationEvent
├── donto:SourceArtifact      ← documents, revisions, spans
├── donto:Concept
│   ├── donto:Occupation
│   ├── donto:Role
│   ├── donto:Ethnicity
│   ├── donto:Religion
│   └── donto:Status
├── donto:TemporalExpression
└── donto:QuantityExpression
```

**Disjointness constraints:**
- Agent ≠ Place ≠ Event ≠ Concept
- If something is typed as both Person and Place, that's a contradiction (detected, not rejected)

---

## 14. Complete worked example: Researching Mary Watson

### Step 1: Extract from an obituary

```bash
curl -X POST https://genes.apexpots.com/extract-and-ingest \
  -H "Content-Type: application/json" \
  -d '{
    "text": "Mary Watson was born in Cornwall, England in 1860. She married Robert Watson in Cooktown, Queensland in 1879. Robert was a beche-de-mer fisherman who operated from Lizard Island. In September 1881, while Robert was away, Aboriginal warriors attacked the settlement on Lizard Island. Mary fled with her infant son and a Chinese servant named Ah Sam in a large iron tank, drifting for days before perishing on No. 5 island. Her diary, recovered from the tank, became one of Queensland most celebrated historical documents. A monument in Cooktown commemorates her bravery.",
    "context": "ctx:genes/mary-watson/obituary"
  }'
# → 72 facts extracted
```

### Step 2: Extract from a newspaper article

```bash
curl -X POST https://genes.apexpots.com/extract-and-ingest \
  -H "Content-Type: application/json" \
  -d '{
    "text": "DEATH OF THE HEROIC MRS WATSON. The colony received with profound sorrow...",
    "context": "ctx:genes/mary-watson/newspaper-1882"
  }'
# → 95 facts extracted
```

### Step 3: Type the key entities

```bash
curl -X POST https://genes.apexpots.com/assert/batch \
  -H "Content-Type: application/json" \
  -d '{
    "statements": [
      {"subject": "ex:mary-watson", "predicate": "rdf:type", "object_iri": "donto:Person", "context": "ctx:genes/mary-watson", "maturity": 4},
      {"subject": "ex:robert-watson", "predicate": "rdf:type", "object_iri": "donto:Person", "context": "ctx:genes/mary-watson", "maturity": 4},
      {"subject": "ex:cooktown", "predicate": "rdf:type", "object_iri": "donto:Settlement", "context": "ctx:genes/mary-watson", "maturity": 4},
      {"subject": "ex:lizard-island", "predicate": "rdf:type", "object_iri": "donto:Place", "context": "ctx:genes/mary-watson", "maturity": 4},
      {"subject": "ex:lizard-island-attack", "predicate": "rdf:type", "object_iri": "donto:Event", "context": "ctx:genes/mary-watson", "maturity": 4}
    ]
  }'
```

### Step 4: Align predicates

```bash
curl -X POST https://genes.apexpots.com/align/auto?threshold=0.6
```

### Step 5: Search and explore

```bash
# Find Mary
curl https://genes.apexpots.com/search?q=mary+watson

# Get her full profile
curl https://genes.apexpots.com/history/ex:mary-watson

# Get her timeline
curl https://genes.apexpots.com/graph/timeline/ex:mary-watson

# See her network
curl -X POST https://genes.apexpots.com/graph/neighborhood \
  -H "Content-Type: application/json" \
  -d '{"subject": "ex:mary-watson", "depth": 2}'
```

### Step 6: Compare sources

```bash
# What does each source say?
curl "https://genes.apexpots.com/match?context=ctx:genes/mary-watson/obituary"
curl "https://genes.apexpots.com/match?context=ctx:genes/mary-watson/newspaper-1882"
```

### Step 7: Record what you've verified

```bash
# High-maturity assertions for verified facts
curl -X POST https://genes.apexpots.com/assert/batch \
  -H "Content-Type: application/json" \
  -d '{
    "statements": [
      {"subject": "ex:mary-watson", "predicate": "hasBirthYear", "object_lit": {"v": 1860, "dt": "xsd:integer"}, "context": "ctx:genes/mary-watson/verified", "maturity": 4},
      {"subject": "ex:mary-watson", "predicate": "hasDeathYear", "object_lit": {"v": 1881, "dt": "xsd:integer"}, "context": "ctx:genes/mary-watson/verified", "maturity": 4},
      {"subject": "ex:mary-watson", "predicate": "bornIn", "object_iri": "ex:cornwall-england", "context": "ctx:genes/mary-watson/verified", "maturity": 4}
    ]
  }'
```

### The result

You now have:
- **167+ facts** about Mary Watson from 2 sources
- **Entity types** (Person, Place, Event, Settlement)
- **Aligned predicates** (bornIn ↔ birthplaceOf ↔ bornInPlace all unified)
- **Temporal data** (birth 1860, marriage 1879, death 1881)
- **Source provenance** (which fact came from which source)
- **Contradictions preserved** (if sources disagree)
- **Verified facts** at maturity L4
- **A queryable graph** with visualization endpoints

All of this is stored bitemporally — you can always ask "what did we know on date X?" and get the exact state of knowledge at that time.

---

## Quick Reference: All API Endpoints

| What | Method | URL |
|------|--------|-----|
| **Extract (sync)** | POST | `/extract-and-ingest` |
| **Extract (async)** | POST | `/jobs/extract` |
| **Batch extract** | POST | `/jobs/batch` |
| **Job status** | GET | `/jobs/{id}` |
| **List jobs** | GET | `/jobs` |
| **Paper extract** | POST | `/papers/ingest` |
| **Search** | GET | `/search?q=name` |
| **History** | GET | `/history/{subject}` |
| **Match** | GET | `/match?subject=&predicate=&context=` |
| **Assert** | POST | `/assert` |
| **Batch assert** | POST | `/assert/batch` |
| **Retract** | POST | `/retract/{id}` |
| **Align auto** | POST | `/align/auto?threshold=0.6` |
| **Align register** | POST | `/align/register` |
| **Align rebuild** | POST | `/align/rebuild` |
| **Align suggest** | GET | `/align/suggest/{predicate}` |
| **Graph stats** | GET | `/graph/stats` |
| **Neighborhood** | POST | `/graph/neighborhood` |
| **Timeline** | GET | `/graph/timeline/{subject}` |
| **Subgraph** | POST | `/graph/subgraph` |
| **Compare** | POST | `/graph/compare` |
| **Path** | POST | `/graph/path` |
| **Entity types** | GET | `/graph/entity-types` |
| **Evidence** | GET | `/evidence/{id}` |
| **Claim card** | GET | `/claim/{id}` |
| **Predicates** | GET | `/predicates` |
| **Contexts** | GET | `/contexts` |
| **Subjects** | GET | `/subjects` |
| **Query** | POST | `/query` |
| **Health** | GET | `/health` |
| | | |
| **Entity register** | POST | `/entity/register` |
| **Entity batch register** | POST | `/entity/register/batch` |
| **Identity edge** | POST | `/entity/identity` |
| **Identity batch** | POST | `/entity/identity/batch` |
| **Cluster membership** | POST | `/entity/membership` |
| **Entity edges** | GET | `/entity/{iri}/edges` |
| **Cluster members** | GET | `/entity/cluster/{hypothesis}/{referent}` |
| **Resolve entity** | GET | `/entity/resolve/{iri}` |
| **Family table** | GET | `/entity/family-table` |

**Set HTTP timeout to 600 seconds for all calls.** Extract endpoints take 30-120s.

---

---

## Glossary

| Term | What it means |
|------|--------------|
| **Statement** | A single fact in the graph: `(subject, predicate, object, context)` plus polarity, maturity, valid-time, and transaction-time. The atomic unit of knowledge. |
| **Subject** | The entity a fact is about. An IRI like `ex:mary-watson`. Always kebab-lower-case. |
| **Predicate** | The relationship or property. A name like `bornIn`, `marriedTo`, `hasBirthYear`. Always camelCase. LLMs mint these freely during extraction. |
| **Object** | The value of a fact. Either an **IRI** (another entity, like `ex:cornwall`) or a **literal** (a typed value, like `{"v": 1860, "dt": "xsd:integer"}`). |
| **Context** | A scope that groups facts by source, research question, or extraction run. Like `ctx:genes/mary-watson/obituary`. Facts in different contexts can be queried, compared, and retracted independently. |
| **IRI** | Internationalized Resource Identifier. The unique name for an entity, like `ex:mary-watson` or `ctx:genes/topic`. Not a URL — just a stable identifier. |
| **Quad** | A statement with four parts: subject + predicate + object + context. Donto stores quads, not triples. |
| **Polarity** | Whether a fact is `asserted` (claimed true), `negated` (claimed false), `absent` (explicitly missing), or `unknown`. |
| **Maturity** | How well-supported a fact is. L0 = raw/unverified, L1 = registered predicate, L2 = has evidence, L3 = shape-validated, L4 = certified/verified. |
| **Valid-time** | When a fact was true in the real world. `(married, 1879-1881)` means the marriage lasted from 1879 to 1881. |
| **Transaction-time** | When a fact was recorded or retracted in the database. Used for "what did we know at time T?" queries. |
| **Bitemporal** | Having both valid-time and transaction-time. Every statement in donto is bitemporal. |
| **Paraconsistent** | A system that tolerates contradictions without exploding. Two sources saying different birth years? Both are stored. Neither is automatically rejected. |
| **Retraction** | Soft-deleting a fact by closing its transaction-time. The row stays in the database for historical queries. Nothing is ever physically deleted. |
| **Extraction** | The process of sending text to an LLM (Grok 4.1 Fast) which returns structured facts (subject/predicate/object with tier and confidence). |
| **Tier** | The analytical depth of an extracted fact. T1 = surface facts, T2 = relational, T3 = opinions, T4 = epistemic, T5 = rhetorical, T6 = presuppositions, T7 = philosophical, T8 = intertextual. |
| **Predicate alignment** | Mapping different predicate names to each other so queries work across sources. `bornIn` ↔ `birthplaceOf` ↔ `bornInPlace` are all aligned. |
| **Closure** | The materialized expansion index for predicate alignment. Pre-computes all transitive chains so queries are fast. Rebuilt with `POST /align/rebuild`. |
| **Entity symbol** | A registered IRI in the symbol table with provenance (who created it, when, from what source). Every subject and object IRI should be a symbol. |
| **Entity mention** | An occurrence of a symbol in a specific document, span, or extraction run. Tracks surface text and confidence. |
| **Entity signature** | A derived feature profile for a symbol (name features, type distribution, temporal features, etc.) used for candidate generation during entity resolution. |
| **Identity edge** | A weighted assertion that two symbols refer to the same real-world entity (`same_referent`), might (`possibly_same_referent`), definitely don't (`distinct_referent`), or we don't know (`not_enough_information`). |
| **Identity hypothesis** | A named clustering solution over identity edges. Three defaults: `strict` (≥0.98), `likely` (≥0.85), `exploratory` (≥0.60). Determines which symbols are treated as the same entity at query time. |
| **Referent** | The hypothesized real-world entity that one or more symbols point to, within a specific identity hypothesis. |
| **Shadow projection** | A materialized view that applies predicate alignment + identity resolution + literal canonicalization to produce a "resolved" view of the graph for fast querying. |
| **Literal** | A typed value that isn't an entity. Has three parts: `v` (the value), `dt` (the datatype like `xsd:string` or `xsd:integer`), and optionally `lang` (language tag). |
| **Literal canonicalization** | Normalizing different representations of the same value. "5 feet 10 inches" and "178 cm" map to the same canonical quantity. |
| **Time expression** | An EDTF-compatible temporal value with grain (day/month/year), uncertainty, approximation, and probability model. "circa 1860" is a year-grain approximate expression, not "1860-01-01". |
| **Allen interval relation** | One of 13 possible temporal relationships between two events: before, meets, overlaps, starts, during, finishes, equals, and their inverses. Stored as a bitset. |
| **Class** | A type in the ontology hierarchy. `donto:Person`, `donto:Place`, `donto:Event`, etc. Entities are typed with `rdf:type`. |
| **Subclass** | A hierarchical relationship: `donto:Person` is a subclass of `donto:Agent`, which is a subclass of `donto:Entity`. Used for type reasoning. |
| **Disjointness** | A constraint that two classes cannot overlap. Agent ≠ Place ≠ Event. If something is typed as both Person and Place, that's a detected contradiction. |
| **Property constraint** | A formal rule on a predicate: domain class, range class, range datatype, functional, symmetric, transitive, etc. Violations generate proof obligations, not rejections. |
| **Inference rule** | A registered rule that derives new facts from existing ones. Example: `parentOf(x,y) → childOf(y,x)`. Rules have confidence policies and temporal policies. |
| **Derivation** | A new fact produced by applying an inference rule to premises. Tracks which rule, which premises, and what confidence. |
| **Rule agenda** | A queue of pending rule evaluations triggered by new or changed statements. |
| **Proof obligation** | An open research task. Types: `needs-source-support`, `needs-entity-disambiguation`, `needs-temporal-grounding`, `needs-coref`, `needs-human-review`, `needs-confidence-boost`. |
| **Evidence link** | A connection between a statement and its source: the document, text span, extraction run, or other statement that supports it. Types: `extracted_from`, `supported_by`, `contradicted_by`, `produced_by`. |
| **Claim card** | The full view of a statement: the fact itself plus all evidence, arguments, obligations, and blockers. Retrieved with `GET /claim/{id}`. |
| **Argument** | A relationship between two statements: `supports`, `rebuts`, `undercuts`, `endorses`, `supersedes`, `qualifies`, `potentially_same`, `same_referent`, `same_event`. |
| **Document** | A registered source text (article, obituary, record, transcript) with an IRI, media type, and immutable revisions. |
| **Span** | A character-offset range within a document revision. Links extracted facts to the exact text that supports them. |
| **Extraction run** | A tracked LLM extraction session with model identity, parameters, source revision, and output counts. PROV-O compatible. |
| **Epistemic sweep** | A batch process that validates shapes, fires derivation rules, detects contradictions, emits proof obligations, and promotes maturity. |
| **DontoQL** | Donto's query language. `MATCH ?s ?p ?o LIMIT 20`. Simpler than SPARQL, designed for the donto data model. |
| **dontosrv** | The Rust HTTP server (Axum) that provides the graph query and mutation API. Runs on port 7879. |
| **OpenRouter** | The LLM API gateway used for extraction. Routes to Grok 4.1 Fast ($0.005/article) or other models. |

---

*This guide describes donto as of May 2026. Database: 36M+ statements, 67 migrations, 14 new tables for entity resolution + ontology + temporal reasoning + inference. API: https://genes.apexpots.com*
