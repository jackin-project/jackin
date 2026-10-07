// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
use jackin_protocol::control::{FocusedUsageView, UsageSnapshotStatus};
use jackin_usage_provider_core::unsupported_snapshot;
use std::cell::RefCell;

struct MockVendors {
    calls: RefCell<Vec<String>>,
}

impl MockVendors {
    fn view(&self, name: &str, now: i64) -> FocusedUsageView {
        self.calls.borrow_mut().push(name.to_owned());
        unsupported_snapshot(name, None, now)
    }
}

impl CredentialSnapshotVendors for MockVendors {
    fn claude_oauth_wave_view(
        &self,
        secret: &str,
        now: i64,
    ) -> (
        FocusedUsageView,
        Option<jackin_usage_provider_core::ProviderRateLimit>,
    ) {
        assert_eq!(secret, "secret");
        (self.view("claude-oauth", now), None)
    }
    fn claude_api_key_view(&self, _key_name: &str, _secret: &str, now: i64) -> FocusedUsageView {
        self.view("claude-key", now)
    }
    fn openrouter_key_view(
        &self,
        _secret: &str,
        now: i64,
    ) -> (
        FocusedUsageView,
        Option<jackin_usage_provider_core::ProviderRateLimit>,
    ) {
        (self.view("openrouter", now), None)
    }
    fn amp_key_view(&self, _secret: &str, now: i64) -> FocusedUsageView {
        self.view("amp", now)
    }
    fn zai_key_view(&self, _key_name: &str, _secret: &str, now: i64) -> FocusedUsageView {
        self.view("zai", now)
    }
    fn kimi_key_view(&self, _secret: &str, now: i64) -> FocusedUsageView {
        self.view("kimi", now)
    }
    fn minimax_key_view(&self, _secret: &str, now: i64) -> FocusedUsageView {
        self.view("minimax", now)
    }
    fn grok_key_view(&self, _key_name: &str, now: i64) -> FocusedUsageView {
        self.view("grok", now)
    }
    fn gemini_key_view(&self, _key_name: &str, now: i64) -> FocusedUsageView {
        self.view("gemini", now)
    }
}

#[test]
fn surfaces_route_to_vendor_arms() {
    let vendors = MockVendors {
        calls: RefCell::new(Vec::new()),
    };
    for (surface, key, expected) in [
        (
            "claude",
            jackin_core::CLAUDE_CODE_OAUTH_TOKEN_ENV_NAME,
            "claude-oauth",
        ),
        ("claude", "ANTHROPIC_API_KEY", "claude-key"),
        ("openrouter", "OPENROUTER_API_KEY", "openrouter"),
        ("amp", "AMP_API_KEY", "amp"),
        ("zai", "ZAI_API_KEY", "zai"),
        ("kimi", "KIMI_API_KEY", "kimi"),
        ("minimax", "MINIMAX_API_KEY", "minimax"),
        ("grok", "XAI_API_KEY", "grok"),
        ("google", "GEMINI_API_KEY", "gemini"),
    ] {
        let (view, _) =
            provider_credential_snapshot_with_rate_limit(surface, key, "secret", &vendors);
        assert_eq!(view.status, UsageSnapshotStatus::Unsupported);
        assert_eq!(vendors.calls.borrow().last(), Some(&expected.to_owned()));
    }
    assert_eq!(vendors.calls.borrow().len(), 9);
}

#[test]
fn codex_and_cursor_arms_are_typed_gaps_without_vendor_calls() {
    let vendors = MockVendors {
        calls: RefCell::new(Vec::new()),
    };
    let codex = provider_credential_snapshot("codex", "OPENAI_API_KEY", "secret", &vendors);
    assert_eq!(codex.status, UsageSnapshotStatus::Unsupported);
    assert_eq!(codex.focused_provider.as_deref(), Some("OpenAI"));
    assert!(codex.last_error.is_some());
    let cursor = provider_credential_snapshot("cursor", "CURSOR_API_KEY", "secret", &vendors);
    assert_eq!(cursor.status, UsageSnapshotStatus::Unsupported);
    assert_eq!(cursor.focused_provider.as_deref(), Some("Cursor"));
    assert!(cursor.last_error.is_some());
    assert!(vendors.calls.borrow().is_empty());
}

#[test]
fn blocked_surfaces_are_unsupported_without_vendor_calls() {
    let vendors = MockVendors {
        calls: RefCell::new(Vec::new()),
    };
    for surface in ["meta", "omp", "hermes", "copilot", "unknown-vendor"] {
        let view = provider_credential_snapshot(surface, "KEY", "secret", &vendors);
        assert_eq!(view.status, UsageSnapshotStatus::Unsupported, "{surface}");
    }
    assert!(vendors.calls.borrow().is_empty());
}
