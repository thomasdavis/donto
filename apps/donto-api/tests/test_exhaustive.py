"""Tests for the exhaustive multi-aperture extractor.

Run with: cd apps/donto-api && python -m unittest discover tests
"""

from __future__ import annotations

import asyncio
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from extraction import (  # noqa: E402
    Aperture,
    DEFAULT_APERTURES,
    prompt_for_aperture,
    run_exhaustive,
)
from extraction.quarantine import list_sink  # noqa: E402


def _good(subject: str, predicate: str, ap: Aperture | None = None,
          hyp: bool = False, anchor: bool = True) -> dict:
    fact = {
        "subject": subject,
        "predicate": predicate,
        "object": {"iri": "ex:obj"},
        "anchor": {"doc": "doc:1", "start": 0, "end": 5} if anchor else None,
        "hypothesis_only": hyp,
        "confidence": 0.9 if not hyp else 0.7,
    }
    if not anchor:
        fact["hypothesis_only"] = True
    if ap:
        fact["aperture"] = ap.value
    return fact


class ApertureCoverageTests(unittest.TestCase):
    def test_every_aperture_has_a_prompt(self):
        for a in Aperture:
            if a == Aperture.RECURSIVE:
                p = prompt_for_aperture(a, seeds=["ex:foo"])
            else:
                p = prompt_for_aperture(a)
            self.assertIn("OUTPUT FORMAT", p)
            self.assertIn(a.value.upper(), p.upper())

    def test_default_apertures_excludes_recursive(self):
        self.assertNotIn(Aperture.RECURSIVE, DEFAULT_APERTURES)
        self.assertEqual(len(DEFAULT_APERTURES), 5)

    def test_recursive_prompt_lists_seeds(self):
        p = prompt_for_aperture(Aperture.RECURSIVE, seeds=["ex:alice", "ex:bob"])
        self.assertIn("ex:alice", p)
        self.assertIn("ex:bob", p)


class OrchestratorTests(unittest.TestCase):
    @staticmethod
    def _async(coro):
        return asyncio.get_event_loop().run_until_complete(coro)

    def setUp(self):
        try:
            asyncio.get_event_loop()
        except RuntimeError:
            asyncio.set_event_loop(asyncio.new_event_loop())

    def test_blocked_by_policy_runs_no_apertures(self):
        called = {"n": 0}

        async def trapped(prompt, text, m):
            called["n"] += 1
            return [], {}

        _bucket, sink = list_sink()
        outcome = self._async(
            run_exhaustive(
                text="hello",
                source_iri="src:denied",
                model="m",
                model_caller=trapped,
                authoriser=lambda *_: False,
                quarantine_sink=sink,
            )
        )
        self.assertTrue(outcome.blocked_by_policy)
        self.assertEqual(called["n"], 0)
        self.assertEqual(outcome.facts, [])

    def test_each_aperture_is_called_once_plus_recursive(self):
        seen_prompts: list[str] = []

        async def model(prompt, text, m):
            seen_prompts.append(prompt)
            return [_good("ex:a", "p1"), _good("ex:b", "p2")], {}

        _bucket, sink = list_sink()
        outcome = self._async(
            run_exhaustive(
                text="x",
                source_iri="s",
                model="m",
                model_caller=model,
                authoriser=lambda *_: True,
                quarantine_sink=sink,
                apertures=(Aperture.SURFACE, Aperture.LINGUISTIC),
                recurse=True,
            )
        )
        # 2 apertures + 1 recursive = 3 calls.
        self.assertEqual(len(seen_prompts), 3)
        self.assertEqual(outcome.recursion_depth, 1)

    def test_dedup_collapses_identical_facts_across_apertures(self):
        async def model(prompt, text, m):
            # Every aperture returns the same fact.
            return [_good("ex:s", "knows")], {}

        _bucket, sink = list_sink()
        outcome = self._async(
            run_exhaustive(
                text="x",
                source_iri="s",
                model="m",
                model_caller=model,
                authoriser=lambda *_: True,
                quarantine_sink=sink,
                apertures=DEFAULT_APERTURES,
                recurse=False,
            )
        )
        self.assertEqual(len(outcome.facts), 1, "dedup must collapse identical facts")
        self.assertGreaterEqual(outcome.dedup_collisions, len(DEFAULT_APERTURES) - 1)

    def test_yield_metrics_populated(self):
        async def model(prompt, text, m):
            ap = Aperture.SURFACE  # the orchestrator overrides this anyway
            return [
                _good("ex:a", "p1"),
                _good("ex:b", "p2", anchor=False),  # hypothesis_only auto-set
                _good("ex:c", "p3"),
            ], {}

        _bucket, sink = list_sink()
        outcome = self._async(
            run_exhaustive(
                text="x", source_iri="s", model="m",
                model_caller=model, authoriser=lambda *_: True,
                quarantine_sink=sink,
                apertures=(Aperture.SURFACE,), recurse=False,
            )
        )
        ym = outcome.yield_metrics
        self.assertEqual(ym["total_facts"], 3)
        self.assertEqual(ym["distinct_predicates"], 3)
        self.assertEqual(ym["distinct_subjects"], 3)
        self.assertGreater(ym["anchor_coverage"], 0)
        self.assertGreater(ym["hypothesis_density"], 0)
        self.assertEqual(ym["facts_per_aperture"]["surface"], 3)

    def test_invalid_candidate_quarantined_does_not_count_in_facts(self):
        async def model(prompt, text, m):
            good = _good("ex:s", "p")
            bad = {"subject": "", "predicate": "p", "object": {"iri": "ex:o"}}
            return [good, bad], {}

        bucket, sink = list_sink()
        outcome = self._async(
            run_exhaustive(
                text="x", source_iri="s", model="m",
                model_caller=model, authoriser=lambda *_: True,
                quarantine_sink=sink,
                apertures=(Aperture.SURFACE,), recurse=False,
            )
        )
        self.assertEqual(len(outcome.facts), 1)
        self.assertGreaterEqual(len(outcome.quarantined), 1)
        self.assertGreaterEqual(len(bucket), 1)

    def test_recursive_pass_uses_extracted_entities_as_seeds(self):
        seeds_seen: list[list[str]] = []

        async def model(prompt, text, m):
            # Detect the recursive prompt by its header text.
            if "RECURSIVE aperture" in prompt:
                # Capture seeds embedded in the prompt.
                lines = [l.strip()[2:] for l in prompt.splitlines() if l.strip().startswith("- ")]
                seeds_seen.append(lines)
            return [_good("ex:from-recursion", "newPred")], {}

        _bucket, sink = list_sink()
        # Surface returns ex:alice/ex:bob/ex:carol so recursion has seeds.
        async def surface_then_recursive(prompt, text, m):
            if "SURFACE aperture" in prompt:
                return [
                    _good("ex:alice", "p1"),
                    _good("ex:bob", "p2"),
                    _good("ex:carol", "p3"),
                ], {}
            return await model(prompt, text, m)

        outcome = self._async(
            run_exhaustive(
                text="x", source_iri="s", model="m",
                model_caller=surface_then_recursive,
                authoriser=lambda *_: True,
                quarantine_sink=sink,
                apertures=(Aperture.SURFACE,), recurse=True,
            )
        )
        self.assertEqual(outcome.recursion_depth, 1)
        self.assertGreater(len(seeds_seen), 0)
        # Subjects must all be seeds; objects (ex:obj from the helper) may also
        # appear since recursion mines every entity, not just subjects.
        self.assertTrue({"ex:alice", "ex:bob", "ex:carol"}.issubset(set(seeds_seen[0])))


class YieldShimTests(unittest.TestCase):
    def test_compute_yield_handles_aperture_field(self):
        from helpers import compute_yield  # noqa: WPS433
        facts = [
            {"subject": "ex:a", "predicate": "p", "aperture": "surface",
             "anchor": {"doc": "d", "start": 0, "end": 1}, "hypothesis_only": False},
            {"subject": "ex:b", "predicate": "p", "aperture": "conceivable",
             "anchor": None, "hypothesis_only": True},
            {"subject": "ex:a", "predicate": "q", "aperture": "linguistic",
             "anchor": {"doc": "d", "start": 0, "end": 1}, "hypothesis_only": False},
        ]
        y = compute_yield(facts)
        self.assertEqual(y["total_facts"], 3)
        self.assertEqual(y["distinct_predicates"], 2)
        self.assertEqual(y["distinct_subjects"], 2)
        self.assertAlmostEqual(y["anchor_coverage"], 2/3, places=2)
        self.assertAlmostEqual(y["hypothesis_density"], 1/3, places=2)
        self.assertEqual(y["facts_per_aperture"], {"surface": 1, "conceivable": 1, "linguistic": 1})

    def test_compute_yield_empty(self):
        from helpers import compute_yield
        self.assertEqual(compute_yield([])["total_facts"], 0)


if __name__ == "__main__":
    unittest.main()
