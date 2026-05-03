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
| 1 | Birth/Death/Marriage records | State BDM registries, FamilySearch | Exact dates, parents, witnesses |
| 2 | Church records | FamilySearch, parish archives | Baptisms, marriages, burials, godparents |
| 3 | Census records | Ancestry, FamilySearch | Household, ages, occupations, birthplaces |
| 4 | Obituaries | newspapers.com, local papers | Family relationships, dates, locations, achievements |
| 5 | Immigration records | National archives, Ancestry | Ship manifests, arrival dates, ages |
| 6 | Wills & probate | State archives, probate courts | Family relationships, property |
| 7 | Newspaper articles | Trove (AU), newspapers.com, chroniclingamerica (US) | Events, quotes, public activities |
| 8 | Military records | National Archives, AWM (AU) | Service dates, ranks, next of kin |
| 9 | DNA results | AncestryDNA, 23andMe | Relative matches, ethnicity estimates |
| 10 | Wikipedia | wikipedia.org | Biographical summaries |

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

### Assert identity edges

When you believe two symbols refer to the same person:

```sql
-- Via database
SELECT donto_assert_identity(
    donto_symbol_id('ex:mary-watson'),
    donto_symbol_id('ex:mrs-watson'),
    'same_referent',
    0.95,           -- confidence
    'human',        -- method
    'Same spouse, same residence, same time period'  -- explanation
);
```

When you're not sure:

```sql
SELECT donto_assert_identity(
    donto_symbol_id('ex:mary-watson'),
    donto_symbol_id('ex:mary-oxley'),
    'possibly_same_referent',
    0.65,
    'trigram',
    'Similar name, overlapping dates, but no direct evidence'
);
```

When you know they're different:

```sql
SELECT donto_assert_identity(
    donto_symbol_id('ex:mary-watson'),
    donto_symbol_id('ex:mary-watson-of-sydney'),
    'distinct_referent',
    0.90,
    'human',
    'Different birth date, different parents, different location'
);
```

### Identity hypotheses

The system maintains three default identity policies:

| Hypothesis | Threshold | Use for |
|-----------|-----------|---------|
| `strict` | ≥0.98 confidence | Official records, certified research |
| `likely` | ≥0.85 confidence | General research, family trees |
| `exploratory` | ≥0.60 confidence | Search, discovery, "who might this be?" |

Query which symbols are in the same cluster:

```sql
-- What referent is Mary Watson in the "likely" hypothesis?
SELECT donto_resolve_referent(2, donto_symbol_id('ex:mary-watson'));

-- Who else is in that cluster?
SELECT * FROM donto_referent_symbols(2, <referent_id>);
```

### Key principle: merges are hypotheses, not destructive rewrites

The original statement `(ex:mrs-watson, bornIn, ex:cornwall)` is never modified. The identity layer maps `ex:mrs-watson` → same referent as `ex:mary-watson` at query time. If the merge was wrong, retract the identity edge and the original data is untouched.

---

## 7. Temporal reasoning

Genealogical dates are messy. Donto handles this properly.

### Parse dates with correct precision

```sql
-- Exact date
SELECT donto_parse_time_expression('1860-06-15');
-- → grain: day, canonical: [1860-06-15, 1860-06-16)

-- Year only
SELECT donto_parse_time_expression('1860');
-- → grain: year, canonical: [1860-01-01, 1861-01-01)

-- Approximate
SELECT donto_parse_time_expression('circa 1860');
-- → grain: year, approximate: true

-- Uncertain
SELECT donto_parse_time_expression('1860?');
-- → grain: year, uncertain: true
```

**Critical rule:** "1860" is NOT "1860-01-01". It's a year-grain expression. The system preserves this distinction.

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
| **Extract** | POST | `/extract-and-ingest` |
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

**Set HTTP timeout to 600 seconds for all calls.** Extract endpoints take 30-120s.

---

*This guide describes donto as of May 2026. Database: 36M+ statements, 67 migrations, 14 new tables for entity resolution + ontology + temporal reasoning + inference. API: https://genes.apexpots.com*
