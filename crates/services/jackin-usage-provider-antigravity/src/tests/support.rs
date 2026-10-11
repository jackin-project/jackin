// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

pub(super) const SUMMARY_FIXTURE: &str = r#"{
    "email": "operator@example.com",
    "userTier": {"id": "google_ai_pro", "name": "Google AI Pro"},
    "planInfo": {"planName": "Pro"},
    "groups": [
        {"buckets": [
            {"bucketId": "gemini-5h", "remainingFraction": 0.62, "resetTime": "2026-09-17T14:00:00Z"},
            {"bucketId": "gemini-weekly", "remainingFraction": 0.81, "resetTime": "2026-09-21T00:00:00Z"}
        ]},
        {"buckets": [
            {"bucketId": "3p-5h", "remainingFraction": 0.4, "resetTime": "2026-09-17T14:00:00Z"},
            {"bucketId": "3p-weekly", "remainingFraction": 0.9, "resetTime": "2026-09-21T00:00:00Z"}
        ]}
    ]
}"#;

pub(super) const LEGACY_FIXTURE: &str = r#"{
    "models": {
        "gemini-3-pro": {"displayName": "Gemini 3 Pro", "quotaInfo": {"remainingFraction": 0.7, "resetTime": "2026-09-17T14:00:00Z"}},
        "gemini-3-flash": {"displayName": "Gemini 3 Flash", "quotaInfo": {"remainingFraction": 0.2, "resetTime": "2026-09-17T14:00:00Z"}},
        "claude-opus": {"displayName": "Claude Opus", "quotaInfo": {"remainingFraction": 0.55, "resetTime": "2026-09-17T14:00:00Z"}},
        "internal-router": {"displayName": "Router", "isInternal": true, "quotaInfo": {"remainingFraction": 1.0}},
        "unlabeled": {"displayName": "", "label": "", "quotaInfo": {"remainingFraction": 0.1}},
        "available-only": {"displayName": "Available Only", "isAvailable": true}
    }
}"#;
