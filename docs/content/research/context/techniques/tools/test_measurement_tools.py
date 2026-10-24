# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0
"""Offline regressions for concrete accounting and unintended-access defects."""
import json
from pathlib import Path
import tempfile
import unittest
import count_tokens
import image_tokens
import session_cost
import provider_usage


class AccountingTests(unittest.TestCase):
    def records(self, records):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "fixture.jsonl"
            path.write_text("\n".join(json.dumps(r) for r in records), encoding="utf-8")
            return session_cost.collect([path])

    def response(self, mid="one", session="session", model="fixture-model", **usage):
        usage = {"input_tokens": 0, "output_tokens": 0, **usage}
        return {"type": "assistant", "sessionId": session,
                "message": {"id": mid, "model": model, "usage": usage}}

    def normalized(self, **usage):
        return session_cost.normalize({"input_tokens": 0, "output_tokens": 0, **usage})

    def test_repeated_blocks_not_rebilled(self):
        item = self.response(input_tokens=100, output_tokens=9)
        result = self.records([item, item, item])
        self.assertEqual(result["unique_responses"], 1)
        self.assertEqual(result["tokens_by_model"]["fixture-model"]["output"], 9)

    def test_streaming_counters_use_final_total(self):
        result = self.records([self.response(output_tokens=1), self.response(output_tokens=12)])
        self.assertEqual(result["tokens_by_model"]["fixture-model"]["output"], 12)

    def test_nonmonotone_duplicate_rejected(self):
        with self.assertRaisesRegex(ValueError, "non-monotone"):
            self.records([self.response(output_tokens=12), self.response(output_tokens=1)])

    def test_sessions_and_models_stay_separate(self):
        result = self.records([self.response(session="a", model="a", output_tokens=2),
                               self.response(session="b", model="b", output_tokens=4)])
        self.assertEqual(result["unique_responses"], 2)
        self.assertEqual(set(result["tokens_by_model"]), {"a", "b"})

    def test_missing_write_breakdown_not_zero(self):
        counts = self.normalized(cache_creation_input_tokens=100)
        self.assertEqual(counts["cache_write_unknown"], 100)
        rates = {k: 1 for k in session_cost.FIELDS if k != "cache_write_unknown"}
        self.assertIsNone(session_cost.price({"m": counts}, {"m": rates})["m"]["usd"])

    def test_write_detail_does_not_double_count(self):
        result = self.normalized(cache_creation_input_tokens=100,
            cache_creation={"ephemeral_5m_input_tokens": 70, "ephemeral_1h_input_tokens": 30})
        self.assertEqual(sum(result.values()), 100)

    def test_invalid_breakdown_fails(self):
        with self.assertRaises(ValueError):
            self.normalized(cache_creation_input_tokens=2,
                cache_creation={"ephemeral_5m_input_tokens": 3})

    def test_zero_cost_is_zero_and_unknown_price_is_unknown(self):
        counts = self.normalized()
        rates = {k: 1 for k in session_cost.FIELDS if k != "cache_write_unknown"}
        self.assertEqual(session_cost.price({"m": counts}, {"m": rates})["m"]["usd"], 0)
        self.assertIsNone(session_cost.price({"m": counts}, {})["m"]["usd"])

    def test_missing_message_id_not_silently_dropped(self):
        with self.assertRaisesRegex(ValueError, "no message ID"):
            self.records([self.response(mid=None, input_tokens=2)])

    def test_incomplete_receipt_rejected(self):
        with self.assertRaisesRegex(ValueError, "incomplete"):
            session_cost.normalize({"output_tokens": 5})

    def test_offline_request_preserves_exact_text(self):
        text = "μ\r\nexact no-final-newline"
        self.assertEqual(count_tokens.body("pinned", text)["messages"][0]["content"], text)

    def test_png_fixture_header_and_dimensions(self):
        data = image_tokens.png(280, 140)
        self.assertEqual(data[:8], b"\x89PNG\r\n\x1a\n")
        self.assertEqual(int.from_bytes(data[16:20], "big"), 280)
        self.assertEqual(int.from_bytes(data[20:24], "big"), 140)

    def test_compaction_iterations_replace_top_level(self):
        usage = {"input_tokens": 100, "output_tokens": 10, "iterations": [
            {"type": "compaction", "input_tokens": 200, "output_tokens": 20},
            {"type": "message", "input_tokens": 100, "output_tokens": 10}]}
        result = session_cost.normalize(usage)
        self.assertEqual((result["input"], result["output"]), (300, 30))
        dedup = self.records([self.response(**usage), self.response(**usage)])
        self.assertEqual(dedup["tokens_by_model"]["fixture-model"]["output"], 30)

    def test_openai_paid_write_classes_are_disjoint(self):
        usage = {"input_tokens": 12000, "output_tokens": 800,
            "input_tokens_details": {"cached_tokens": 4000, "cache_write_tokens": 7000,
                "cache_write_tokens_details": {"ttl_30m_tokens": 3000, "ttl_12h_tokens": 4000}},
            "output_tokens_details": {"reasoning_tokens": 300}}
        normalized = provider_usage.normalize("openai-responses", usage, "paid")
        self.assertEqual(normalized["tokens"]["input"], 1000)
        cost = provider_usage.price(normalized, {"input": 2, "cache_read": .1,
            "cache_write_30m": 2.5, "cache_write_12h": 5, "output": 10})
        self.assertAlmostEqual(cost["usd"], .0379)
        usage["input_tokens_details"]["cache_write_tokens"] = 7001
        with self.assertRaisesRegex(ValueError, "do not sum"):
            provider_usage.normalize("openai-responses", usage, "paid")

    def test_openai_paid_writes_cannot_be_silently_absent(self):
        usage = {"input_tokens": 10, "output_tokens": 0,
                 "input_tokens_details": {"cached_tokens": 0}}
        with self.assertRaisesRegex(ValueError, "cache_write_tokens"):
            provider_usage.normalize("openai-responses", usage, "paid")
        self.assertEqual(provider_usage.normalize("openai-responses", usage, "ordinary-input")["tokens"]["input"], 10)

    def test_gemini_interactions_thought_is_disjoint(self):
        result = provider_usage.normalize("gemini-interactions", {
            "total_input_tokens": 62, "total_output_tokens": 171,
            "total_thought_tokens": 297, "total_tokens": 530})
        self.assertEqual(result["tokens"]["output"], 468)
        with self.assertRaisesRegex(ValueError, "total_thought_tokens"):
            provider_usage.normalize("gemini-interactions", {"total_input_tokens": 62, "total_output_tokens": 171})

    def test_reasoning_subset_and_missing_split(self):
        result = provider_usage.normalize("claude-messages", {"input_tokens": 1, "output_tokens": 10})
        self.assertIsNone(result["reasoning_output_tokens"])
        with self.assertRaisesRegex(ValueError, "exceeds"):
            provider_usage.normalize("claude-messages", {"input_tokens": 1, "output_tokens": 10,
                "output_tokens_details": {"thinking_tokens": 11}})


if __name__ == "__main__":
    unittest.main()
