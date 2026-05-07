"""Unit tests for the M5 extraction kernel.

Run with:
    cd apps/donto-api && python -m unittest discover tests

The tests use stdlib `unittest` so contributors don't need pytest
installed. The orchestrator is exercised against a fake model and
fake authoriser; nothing in this file touches the network.
"""

from __future__ import annotations

import asyncio
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from extraction import (  # noqa: E402
    Domain,
    ExtractionOutcome,
    pick_domain,
    prompt_for,
    decompose,
    validate_candidate,
    run_extraction,
)
from extraction.quarantine import list_sink  # noqa: E402


class DomainSelectionTests(unittest.TestCase):
    def test_explicit_hint_wins(self):
        self.assertEqual(
            pick_domain("anything at all", {"domain": "papers"}),
            Domain.PAPERS,
        )

    def test_explicit_hint_normalises_dash(self):
        self.assertEqual(
            pick_domain("anything", {"domain": "Medical-Stub"}),
            Domain.MEDICAL_STUB,
        )

    def test_unknown_hint_falls_through_to_heuristics(self):
        self.assertEqual(pick_domain("hello", {"domain": "wat"}), Domain.GENERAL)

    def test_genealogy_heuristic(self):
        text = "John Smith (1820-1880) married Jane Doe in 1842. Born 1820, died 1880."
        self.assertEqual(pick_domain(text), Domain.GENEALOGY)

    def test_linguistic_heuristic(self):
        text = (
            "The phoneme /θ/ contrasts with /ð/ in this dialect. "
            "The fricative is realised as [ʃ] before high vowels. "
            "Glottal stops mark word boundaries."
        )
        self.assertEqual(pick_domain(text), Domain.LINGUISTIC)

    def test_papers_heuristic(self):
        text = (
            "Abstract. We present results from Figure 1. "
            "Methods: a randomised trial. Smith et al. report p < 0.05. "
            "References include doi:10.1234/abc."
        )
        self.assertEqual(pick_domain(text), Domain.PAPERS)

    def test_legal_heuristic(self):
        text = "Smith v. Jones held that under § 12 U.S.C. the plaintiff prevails. The court ruled."
        self.assertEqual(pick_domain(text), Domain.LEGAL_STUB)

    def test_medical_heuristic(self):
        text = "Patient presented with comorbidity. Prescribed 5mg/kg dose. ICD-10 J45 diagnosis confirmed."
        self.assertEqual(pick_domain(text), Domain.MEDICAL_STUB)

    def test_generic_text_falls_back_to_general(self):
        self.assertEqual(pick_domain("hello world"), Domain.GENERAL)


class PromptCoverageTests(unittest.TestCase):
    def test_every_domain_has_a_prompt(self):
        for d in Domain:
            self.assertIn("OUTPUT FORMAT", prompt_for(d))

    def test_specialist_prompts_are_distinct(self):
        gen = prompt_for(Domain.GENERAL)
        self.assertNotEqual(prompt_for(Domain.GENEALOGY), gen)
        self.assertNotEqual(prompt_for(Domain.LINGUISTIC), gen)
        self.assertNotEqual(prompt_for(Domain.PAPERS), gen)


class ValidationTests(unittest.TestCase):
    def _good(self):
        return {
            "subject": "ex:alice",
            "predicate": "knows",
            "object": {"iri": "ex:bob"},
            "anchor": {"doc": "doc:1", "start": 0, "end": 10},
            "hypothesis_only": False,
            "confidence": 0.9,
        }

    def test_well_formed_candidate_passes(self):
        self.assertTrue(validate_candidate(self._good()).ok)

    def test_missing_subject_rejected(self):
        c = self._good()
        c["subject"] = ""
        self.assertFalse(validate_candidate(c).ok)

    def test_missing_predicate_rejected(self):
        c = self._good()
        del c["predicate"]
        self.assertFalse(validate_candidate(c).ok)

    def test_object_must_have_iri_or_literal(self):
        c = self._good()
        c["object"] = {}
        self.assertFalse(validate_candidate(c).ok)

    def test_literal_must_have_value_and_dt(self):
        c = self._good()
        c["object"] = {"literal": {"v": 42}}  # missing dt
        self.assertFalse(validate_candidate(c).ok)

    def test_anchor_must_have_start_end(self):
        c = self._good()
        c["anchor"] = {"doc": "doc:1"}
        self.assertFalse(validate_candidate(c).ok)

    def test_anchor_end_must_be_ge_start(self):
        c = self._good()
        c["anchor"] = {"doc": "doc:1", "start": 5, "end": 3}
        self.assertFalse(validate_candidate(c).ok)

    def test_no_anchor_requires_hypothesis_only(self):
        c = self._good()
        c["anchor"] = None
        c["hypothesis_only"] = False
        self.assertFalse(validate_candidate(c).ok)

    def test_no_anchor_with_hypothesis_only_is_accepted(self):
        c = self._good()
        c["anchor"] = None
        c["hypothesis_only"] = True
        self.assertTrue(validate_candidate(c).ok)

    def test_confidence_must_be_in_range(self):
        c = self._good()
        c["confidence"] = 1.5
        self.assertFalse(validate_candidate(c).ok)

    def test_confidence_must_be_numeric(self):
        c = self._good()
        c["confidence"] = "high"
        self.assertFalse(validate_candidate(c).ok)

    def test_hypothesis_only_must_be_bool(self):
        c = self._good()
        c["hypothesis_only"] = 1  # int, not bool
        self.assertFalse(validate_candidate(c).ok)


class DecomposerTests(unittest.TestCase):
    def test_general_decomposer_normalises_anchor_and_hypothesis(self):
        out = decompose(
            [
                {
                    "subject": "ex:alice",
                    "predicate": "knows",
                    "object": {"iri": "ex:bob"},
                }
            ],
            Domain.GENERAL,
        )
        self.assertEqual(len(out), 1)
        self.assertIsNone(out[0]["anchor"])
        self.assertTrue(out[0]["hypothesis_only"])
        self.assertEqual(out[0]["domain"], "general")

    def test_genealogy_decomposer_downgrades_unanchored_date_predicates(self):
        out = decompose(
            [
                {
                    "subject": "ex:john",
                    "predicate": "bornOn",
                    "object": {"literal": {"v": "1820", "dt": "xsd:gYear"}},
                    "confidence": 1.0,
                }
            ],
            Domain.GENEALOGY,
        )
        self.assertLessEqual(out[0]["confidence"], 0.7)

    def test_decompose_drops_non_dict_items(self):
        out = decompose([{"subject": "ex:a", "predicate": "p", "object": {"iri": "ex:b"}}, "junk", 7], Domain.GENERAL)
        self.assertEqual(len(out), 1)

    def test_decompose_returns_empty_on_non_list(self):
        self.assertEqual(decompose("not a list", Domain.GENERAL), [])


class OrchestratorTests(unittest.TestCase):
    @staticmethod
    def _async(coro):
        return asyncio.get_event_loop().run_until_complete(coro)

    def setUp(self):
        try:
            asyncio.get_event_loop()
        except RuntimeError:
            asyncio.set_event_loop(asyncio.new_event_loop())

    def test_policy_block_short_circuits_before_model_call(self):
        called = {"count": 0}

        async def fake_model(prompt, text, model):  # pragma: no cover - shouldn't run
            called["count"] += 1
            return [], {}

        def deny_all(caller, kind, tid, action):
            return False

        bucket, sink = list_sink()
        outcome = self._async(
            run_extraction(
                text="The phoneme /θ/.",
                source_iri="src:restricted",
                model="m",
                model_caller=fake_model,
                authoriser=deny_all,
                quarantine_sink=sink,
                hints={"domain": "linguistic"},
            )
        )
        self.assertTrue(outcome.blocked_by_policy)
        self.assertEqual(called["count"], 0)
        self.assertEqual(outcome.accepted, [])
        self.assertEqual(bucket, [])

    def test_happy_path_accepts_well_formed_candidate(self):
        async def model(prompt, text, m):
            return (
                [
                    {
                        "subject": "ex:alice",
                        "predicate": "knows",
                        "object": {"iri": "ex:bob"},
                        "anchor": {"doc": "doc:1", "start": 0, "end": 5},
                        "hypothesis_only": False,
                        "confidence": 0.9,
                    }
                ],
                {"prompt_tokens": 12},
            )

        def allow(caller, kind, tid, action):
            return True

        bucket, sink = list_sink()
        outcome = self._async(
            run_extraction(
                text="alice knows bob",
                source_iri="src:s1",
                model="m",
                model_caller=model,
                authoriser=allow,
                quarantine_sink=sink,
            )
        )
        self.assertFalse(outcome.blocked_by_policy)
        self.assertEqual(len(outcome.accepted), 1)
        self.assertEqual(outcome.metadata.get("prompt_tokens"), 12)
        self.assertEqual(bucket, [])

    def test_invalid_candidates_routed_to_quarantine(self):
        async def model(prompt, text, m):
            return (
                [
                    {"subject": "", "predicate": "p", "object": {"iri": "ex:x"}},
                    {"subject": "ex:s", "predicate": "p", "object": {}},
                    {
                        "subject": "ex:s2",
                        "predicate": "p",
                        "object": {"iri": "ex:y"},
                        "anchor": None,
                        "hypothesis_only": True,
                    },
                ],
                {},
            )

        def allow(caller, kind, tid, action):
            return True

        bucket, sink = list_sink()
        outcome = self._async(
            run_extraction(
                text="x",
                source_iri="src:s1",
                model="m",
                model_caller=model,
                authoriser=allow,
                quarantine_sink=sink,
            )
        )
        self.assertEqual(len(outcome.accepted), 1)
        self.assertEqual(len(outcome.quarantined), 2)
        self.assertEqual(len(bucket), 2)

    def test_outcome_records_chosen_domain(self):
        async def model(prompt, text, m):
            return [], {}

        def allow(caller, kind, tid, action):
            return True

        _bucket, sink = list_sink()
        outcome: ExtractionOutcome = self._async(
            run_extraction(
                text="The phoneme /θ/ contrasts with /ð/. The fricative is [ʃ].",
                source_iri="src:s1",
                model="m",
                model_caller=model,
                authoriser=allow,
                quarantine_sink=sink,
            )
        )
        self.assertEqual(outcome.domain, Domain.LINGUISTIC)


if __name__ == "__main__":
    unittest.main()
