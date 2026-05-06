"""Python Temporal worker for donto extraction jobs.

Run alongside the FastAPI API server:
    python worker.py

Concurrency is controlled by MAX_CONCURRENT_ACTIVITIES (env or default 15).
To change concurrency, restart the worker with a different value — no gdb
hacking required.
"""

import asyncio
import logging
import os

from temporalio.client import Client
from temporalio.worker import Worker

from workflows import ExtractionWorkflow
from activities import (
    extract_facts_activity, ingest_facts_activity,
    align_predicates_activity, resolve_entities_activity,
)

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(name)s: %(message)s")
logger = logging.getLogger("donto-worker")

TEMPORAL_ADDRESS = os.environ.get("TEMPORAL_ADDRESS", "localhost:7233")
TASK_QUEUE = "donto-extraction"
MAX_CONCURRENT = int(os.environ.get("MAX_CONCURRENT_ACTIVITIES", "15"))


async def main():
    client = await Client.connect(TEMPORAL_ADDRESS)
    logger.info(f"connected to Temporal at {TEMPORAL_ADDRESS}")

    worker = Worker(
        client,
        task_queue=TASK_QUEUE,
        workflows=[ExtractionWorkflow],
        activities=[extract_facts_activity, ingest_facts_activity,
                   align_predicates_activity, resolve_entities_activity],
        max_concurrent_activities=MAX_CONCURRENT,
        max_concurrent_workflow_tasks=MAX_CONCURRENT,
    )
    logger.info(f"worker listening on queue={TASK_QUEUE} max_concurrent={MAX_CONCURRENT}")
    await worker.run()


if __name__ == "__main__":
    asyncio.run(main())
