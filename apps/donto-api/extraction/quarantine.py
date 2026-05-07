"""Quarantine sink for invalid candidates.

A `quarantine_sink` is any callable accepting `(candidate, reason)`.
Production wires this to the dontosrv `/quarantine` endpoint; tests
pass a list-collecting callable. Keeping the dependency injectable
means the orchestrator stays trivially testable.
"""

from __future__ import annotations

from typing import Callable, Protocol


class QuarantineSink(Protocol):
    def __call__(self, candidate: dict, reason: str) -> None: ...


def list_sink() -> tuple[list[tuple[dict, str]], QuarantineSink]:
    """Return (collected, sink) — useful for unit tests and for
    in-memory dry-run extraction."""
    bucket: list[tuple[dict, str]] = []

    def _sink(candidate: dict, reason: str) -> None:
        bucket.append((candidate, reason))

    return bucket, _sink


def make_http_sink(post_json: Callable[[str, dict], None], endpoint: str) -> QuarantineSink:
    """Build a sink that POSTs `{candidate, reason}` to `endpoint`.
    `post_json` is the caller's HTTP plumbing (httpx, requests, etc.)
    so this module stays import-light."""

    def _sink(candidate: dict, reason: str) -> None:
        post_json(endpoint, {"candidate": candidate, "reason": reason})

    return _sink


def quarantine_candidate(sink: QuarantineSink, candidate: dict, reason: str) -> None:
    sink(candidate, reason)
