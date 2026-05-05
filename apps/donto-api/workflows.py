"""Temporal workflow definitions for donto extraction jobs."""

from datetime import timedelta
from temporalio import workflow
from temporalio.common import RetryPolicy

with workflow.unsafe.imports_passed_through():
    from activities import extract_facts_activity, ingest_facts_activity


@workflow.defn
class ExtractionWorkflow:
    def __init__(self):
        self._status = "queued"
        self._context = ""
        self._model = ""
        self._text_length = 0
        self._facts_extracted = 0
        self._statements_ingested = 0
        self._tiers = {}
        self._usage = {}
        self._error = None
        self._llm_ms = 0
        self._ingest_ms = 0
        self._total_ms = 0

    @workflow.run
    async def run(self, text: str, context: str, model: str) -> dict:
        self._context = context
        self._model = model
        self._text_length = len(text)
        start = workflow.now().timestamp()

        try:
            self._status = "extracting"
            extract_result = await workflow.execute_activity(
                extract_facts_activity,
                args=[text, model],
                start_to_close_timeout=timedelta(minutes=10),
                retry_policy=RetryPolicy(maximum_attempts=3),
            )
            facts = extract_result["facts"]
            self._facts_extracted = len(facts)
            self._llm_ms = extract_result["llm_ms"]
            self._tiers = extract_result["tiers"]
            self._usage = extract_result["metadata"]

            self._status = "ingesting"
            ingest_result = await workflow.execute_activity(
                ingest_facts_activity,
                args=[facts, context],
                start_to_close_timeout=timedelta(minutes=5),
                retry_policy=RetryPolicy(maximum_attempts=5),
            )
            self._statements_ingested = ingest_result["statements_ingested"]
            self._ingest_ms = ingest_result["ingest_ms"]
            self._total_ms = int(workflow.now().timestamp() - start) * 1000

            self._status = "completed"
            return self._build_result()

        except Exception as e:
            self._status = "failed"
            self._error = str(e)
            self._total_ms = int(workflow.now().timestamp() - start) * 1000
            raise

    @workflow.query
    def status(self) -> dict:
        return self._build_result()

    def _build_result(self) -> dict:
        return {
            "status": self._status,
            "context": self._context,
            "model": self._model,
            "text_length": self._text_length,
            "facts_extracted": self._facts_extracted,
            "statements_ingested": self._statements_ingested,
            "tiers": self._tiers,
            "usage": self._usage,
            "error": self._error,
            "llm_ms": self._llm_ms,
            "ingest_ms": self._ingest_ms,
            "total_ms": self._total_ms,
        }
