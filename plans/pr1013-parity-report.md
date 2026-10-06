# PR #1013 Console↔Capsule usage parity report (S4/S5)

Harness: `crates/jackin-usage/src/host/projection/tests.rs` (`parity_*` agreement
tests, `documented_delta_*` accepted-difference locks). One fixture scenario feeds
both renderers under a fixed clock; see the harness header for the method.

Delta numbers (D1–D12) below are positional in harness order and map 1:1 to the
test names; no other D-numbering exists on this tree.

## Fixed deltas

| ID | Test (now) | Fix | Owner layer |
|---|---|---|---|
| D1 | `parity_overview_summary_first_ranked_limit` | Both surfaces select the first available Rust-ranked limit (D30: long-range, model-specific, session, other). Console had provider-first (`.first()`); capsule had most-constrained (Bug-5). D30 (`roadmap/unified-agent-usage/README.md`, 2026-08-21) settles it: ranked-first. | console `summary_window` (`crates/jackin-console/src/tui/screens/usage.rs`), capsule `summary_bucket` (`crates/jackin-usage/src/usage/view.rs`) |
| D2 | `parity_spend_meter_fills_by_remaining` | Capsule spend meters fill by remaining (was: used). Console windows carry no spend marker (`Spend`→`Other`), so the console cannot flip; remaining-fill everywhere is the only renderer-scope convergence. Spend *text* keeps reading used. | capsule `usage_bucket_presentation` (`crates/jackin-usage/src/usage/format.rs`) |
| D10 | `parity_overage_magnitude_matches_raw_money` | Capsule spend text recovers over-100% magnitude from the money ratio via the shared `Money::raw_percent_of` rule ("150% used", meter empty), matching the console window. | capsule `usage_bucket_presentation` + shared helper (`crates/jackin-protocol/src/control.rs`) |
| D11 | `parity_meter_percent_inputs_agree` | Follows from D2+D10: every console window meter equals the capsule bucket meter, spend and overage included (mirror branch deleted). | — (agreement proof) |

Supporting change: `Money::raw_percent_of` is the single checked money-ratio rule
shared by the projection (`project_window`) and the capsule presentation,
replacing the projection's private copy. The broker publisher keeps its own
private copy (broker is out of scope for this slice).

## Accepted documented differences

| ID | Test | Rationale |
|---|---|---|
| D3 | `documented_delta_severity_color_inputs` | Quota-state-driven console color vs severity-driven capsule accent agree via the projection's `Danger`→`Exhausted` / `Warn`→`Warning` mapping; pinned. |
| D4 | `documented_delta_reset_and_freshness_wording` | Same epochs, renderer-owned formats (countdown buckets, day vs hour aging, case). All epochs asserted equal. |
| D5 | `documented_delta_status_wording` | Projection-owned status words plus the renderer's stale override; unifying needs a projection vocabulary decision. |
| D6 | `documented_delta_credential_expiry_capsule_gap` | `FocusedUsageView` has no expiry channel (only `last_error` beside buckets). In production the console lacks it too: the projection leaves `credential_expires_at_epoch` unset and only a future broker overlay populates it. Needs a view-protocol field + collector/broker producer. |
| D7 | `documented_delta_issue_retry_capsule_gap` | Projection emits no issues (`issues: Vec::new()`); codes/retry epochs arrive only via the broker overlay, and the view has no typed-issue channel. The capsule must not parse retry times out of `last_error` prose. |
| D8 | `documented_delta_balance_value_and_uncapped_spend` | Projection-owned (`value_label` fallback; slot-blind spend-group emission); the console cannot recover either from its inputs. |
| D9 | `documented_delta_model_scope_capsule_gap` | `QuotaBucketView` lacks scope axes and the projection drops `username`; both need protocol changes. |
| D12 | `documented_delta_error_buckets_carry_no_percent` | Residual: quota-driven console color vs severity-driven capsule accent can only disagree on error-status buckets carrying a percent; no fixture adapter emits one. |

## runs_out dead path — resolved by removal (console side)

Both window builders hard-code `runs_out_label: None` (projection +
broker publisher); the only `Some` values on the tree were console test
fixtures. The console render arm could never fire. Provider run-out text
already reaches both surfaces inside `pace_label` composites
(e.g. `"On pace · Runs out in 5d"`), so deriving a separate burn-rate estimate
would be fabrication (see `docs/content/research/product/desktop/
usage-provider-apis/07-runout-projection.mdx` for the failure modes).

- Removed: `UsageWindow.runs_out_label`, the `from_projection` clone, the
  `runs out:` render arm, and the console test fixtures/assertions.
- Kept: the optional protocol field `UsageLimitWindowV1.runs_out_label`,
  now documented as reserved (no producer). Full field removal would break
  the broker publisher's struct literal, which is out of scope.

## Checks

- `cargo test -p jackin-usage --lib host::projection`: 36 passed (parity
  suite, incl. 4 rewritten agreement tests).
- `cargo test -p jackin-usage -p jackin-protocol`: 679 passed, 1 ignored.
- `cargo test -p jackin-console`: 1363 passed, 1 ignored.
- `cargo test -p jackin-capsule`: green on rerun (one flake of
  `daemon::tests::capsule_widget_focus_lifecycle_…`, a timing-sensitive
  widget-focus test in an untouched area; passes alone).
- `cargo test -p jackin-usage-ffi`: 16 passed.
- `cargo clippy -p jackin-protocol -p jackin-usage -p jackin-console
  -p jackin-capsule --all-targets`: clean. `cargo fmt --check`: clean.

Downstream-visible behavior changes (via FFI DTOs to native surfaces):
spend meters now fill by remaining, overage spend reads raw (e.g. "150%
used"), and overview/tab summaries select the D30 first-ranked limit.

## Remaining issues

- D8 (balance blank console value / slot-blind spend groups) and D9
  (model/scope axes, username) still need protocol + producer work; tests
  lock current behavior.
- Money-without-remaining spend buckets (no adapter emits one today): the
  console shows a used-mirror bar while the capsule tab selector skips the
  bucket (it cannot render `{n}% left` without a remaining). Residual noted
  in D11 scope; no fixture covers it.
- The broker publisher keeps a private money-ratio copy (broker out of
  scope); the projection and capsule share `Money::raw_percent_of`.
- Full `runs_out_label` protocol-field removal is blocked on the broker
  publisher's struct literal (out of scope); the field stays reserved.
