"""Donto Analytical IR — typed intermediate representation for research reports.

Every research report is a structured artifact that can be generated, validated,
rendered as charts/PDF/narrative, consumed by AI agents, and acted upon.
"""

from pydantic import BaseModel, Field
from typing import Optional


class ResearchIntent(BaseModel):
    question: str = Field(..., description="The research question in natural language")
    audience: str = Field("researcher", description="researcher | family_member | agent")
    assumptions: list[str] = Field(default_factory=list)
    scope_entities: list[str] = Field(default_factory=list, description="Entity IRIs to investigate")
    scope_contexts: Optional[list[str]] = Field(None, description="Restrict to these contexts")
    min_maturity: int = Field(0, ge=0, le=4)


class EntityRef(BaseModel):
    iri: str
    label: Optional[str] = None
    fact_count: int = 0
    entity_type: Optional[str] = None


class PredicateRef(BaseModel):
    iri: str
    category: str
    usage_count: int = 0


class ContextRef(BaseModel):
    iri: str
    statement_count: int = 0


class SemanticScope(BaseModel):
    entities: list[EntityRef] = Field(default_factory=list)
    predicates: list[PredicateRef] = Field(default_factory=list)
    contexts: list[ContextRef] = Field(default_factory=list)


class ContradictionVariant(BaseModel):
    value: Optional[str]
    context: str
    maturity: int


class Contradiction(BaseModel):
    entity: str
    predicate: str
    variants: list[ContradictionVariant]
    severity: str = "soft"


class Corroboration(BaseModel):
    entity: str
    predicate: str
    value: Optional[str]
    source_contexts: list[str]
    combined_maturity: int = 0


class TimelineEvent(BaseModel):
    entity: str
    predicate: str
    value: Optional[str]
    date_sort_key: Optional[str] = None
    valid_from: Optional[str] = None
    valid_to: Optional[str] = None
    context: str = ""
    maturity: int = 0


class FamilyLink(BaseModel):
    subject: str
    predicate: str
    object: str
    context: str = ""
    maturity: int = 0


class FamilyTree(BaseModel):
    root_entity: str
    members: list[EntityRef] = Field(default_factory=list)
    links: list[FamilyLink] = Field(default_factory=list)
    generations: dict[str, int] = Field(default_factory=dict)


class EvidenceGap(BaseModel):
    entity: str
    missing_category: str
    missing_predicates: list[str] = Field(default_factory=list)
    priority: str = "medium"
    suggestion: str = ""


class QualityScore(BaseModel):
    source_reliability: float = 0.0
    extraction_completeness: float = 0.0
    corroboration_rate: float = 0.0
    contradiction_density: float = 0.0
    predicate_coverage: float = 0.0
    overall: float = 0.0


class ActionItem(BaseModel):
    action_type: str
    description: str
    priority: str = "medium"
    target_entities: list[str] = Field(default_factory=list)
    suggested_sources: list[str] = Field(default_factory=list)


class MigrationStep(BaseModel):
    location: str
    predicate: str
    date_hint: Optional[str] = None
    context: str = ""
    maturity: int = 0


class NameVariant(BaseModel):
    value: str
    contexts: list[str] = Field(default_factory=list)
    occurrences: int = 0


class ChartSpec(BaseModel):
    chart_type: str
    title: str
    data: dict = Field(default_factory=dict)
    description: str = ""


class AnalyticalReport(BaseModel):
    intent: ResearchIntent
    scope: SemanticScope
    family_tree: Optional[FamilyTree] = None
    timeline: list[TimelineEvent] = Field(default_factory=list)
    contradictions: list[Contradiction] = Field(default_factory=list)
    corroborations: list[Corroboration] = Field(default_factory=list)
    evidence_gaps: list[EvidenceGap] = Field(default_factory=list)
    migrations: list[MigrationStep] = Field(default_factory=list)
    name_variants: list[NameVariant] = Field(default_factory=list)
    quality: QualityScore = Field(default_factory=QualityScore)
    actions: list[ActionItem] = Field(default_factory=list)
    charts: list[ChartSpec] = Field(default_factory=list)
    narrative: str = ""
    generated_at: str = ""
