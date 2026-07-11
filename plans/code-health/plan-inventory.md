# Plan inventory (reverify)

Branch: `chore/rust-code-health-roadmap`

| id | title | ledger_status | in_tree | evidence |
|----|-------|---------------|---------|----------|
| 003 | Stop deep-cloning usage views on every refresh | DONE | pass | 20:    AccountUsageSnapshotView, FocusedAccountHeader, FocusedUsageView, Money, QuotaBucketView, \| 174:    pub(crate) view: FocusedUsageView, \| 179:    pub(crate) view: FocusedUs |
| 004 | Stop dropping the frame queued behind a coalesced  | DONE | pass | crates/jackin-term/src/grid/tests.rs:418:    grid.process(b"0123456789"); // fills row 0, arms pending wrap \| crates/jackin-tui/src/animation.rs:284:fn drain_pending_input(host_sc |
| 007 | Bound OSC 8 hyperlink maps; clear on reset | DONE | pass | crates/jackin-term/src/grid/tests.rs:642:                    .hyperlink \| crates/jackin-term/src/grid/tests.rs:645:                owned_cell.hyperlink_id.as_deref() \| crates/jac |
| 008 | Tear down DinD when a post-success finalization st | DONE | pass | crates/jackin-runtime/src/runtime/launch/restore/tests.rs:45:        cleanup_status: status, \| crates/jackin-runtime/src/runtime/launch/launch_pipeline/tests.rs:3://! field needs  |
| 009 | Fuzz + truncation tests for protocol wire decoders | DONE | pass | Cargo.lock \| Cargo.toml \| artifacts \| corpus |
| 010 | Code-health dashboard, suppression inventory, veri | DONE | pass | present |
| 011 | Silent-failure lints, rustdoc gates, doc tests in  | DONE | pass | present |
| 012 | Tier-graph arch gate (replaces empty forbidden-edg | DONE | pass | 4://! has a declared tier (`TIERS`); production edges must point at a \| 6://! appearing in `TIERS`. Dev-dependencies may point anywhere except into \| 36:pub(crate) const TIERS: & |
| 013 | Flake detection, timing artifacts, migration idemp | DONE | pass | flaky-tests.toml \| .config/nextest.toml:12:# flaky-tests.toml at the repo root; an unquarantined flake fails review. \| .config/nextest.toml:16:final-status-level = "flaky" \| .co |
| 014 | Compile-check all benches; cover 4 unbenchmarked h | DONE | pass | crates/jackin-diagnostics/benches: \| summarize_jsonl.rs \|  \| crates/jackin-term/benches: |
| 015 | Brand-prose lint, spec↔test citations, README pres | DONE | pass | crates/jackin-xtask/src/lint.rs:14://!      design — see `roadmap/test-infra-behavioral-specs/` for the long-term \| crates/jackin-xtask/src/construct/tests.rs:4:fn inspect_success |
| 016 | Ownership headers everywhere + headers gate + blam | DONE | pass | crates/jackin-xtask/src/main.rs:14:mod headers; \| crates/jackin-xtask/src/main.rs:146:    Headers(headers::LintHeadersArgs), \| crates/jackin-xtask/src/main.rs:166:    headers::en |
| 017 | Unified ratchet engine (`ratchet.toml`) + defect→g | DONE | pass | present |
| 018 | One OTLP builder, semconv registry, correlatable s | DONE | pass | crates/jackin-docker/src/shell_runner.rs:245:        jackin_diagnostics::otel_events::PROCESS_EXECUTE, \| crates/jackin-runtime/src/runtime/attach.rs:422:            jackin_diagnos |
| 019 | Slice/index panic-coverage lints on the 4 pure cra | DONE | pass | plan file 019-slice-index-lints-pure-crates.md exists; ledger claims DONE |
| 020 | Container-path chokepoint + executable `/jackin/`- | DONE | pass | crates/jackin-xtask/src/pr.rs:80:        \|\| file.starts_with("crates/jackin/src/manifest/") \| crates/jackin-xtask/src/pr.rs:81:        \|\| file.starts_with("crates/jackin/tests |
| 021 | `missing_docs` on jackin-protocol + typed clipboar | DONE | pass | crates/jackin-protocol/src/attach.rs:565:pub enum ClipboardImageError { \| crates/jackin-protocol/src/attach.rs:588:impl ClipboardImageError { \| crates/jackin-protocol/src/attach. |
| 022 | Scoped powerset PR gate, beta clippy canary, `xtas | DONE | pass | crates/jackin-xtask/src/ci.rs:11:/// CI partition names for `--only` selection. \| crates/jackin-xtask/src/ci.rs:13:/// `lint` \| `policy` \| `tests` \| `msrv` \| `powerset` \| `do |
| 023 | Documented-command drift gate (docs fences ↔ clap  | DONE | pass | plan file 023-docs-command-drift-gate.md exists; ledger claims DONE |
| 024 | `Clock` seam in jackin-core; first consumer: clipb | DONE | pass | clock |
| 025 | Extract `jackin-test-support`; break isolation⇄run | DONE | pass | present |
| 026 | Range-scoped scrollback snapshots (per-mouse-event | DONE | pass | pane_body.rs \| scrollback_snapshot.rs |
| 027 | Typed borrowed JSONL streaming; stop double-parsin | DONE | pass | crates/jackin-diagnostics/src/conformance/tests.rs:72:    let typed = errors.iter().find(\|log\| { \| crates/jackin-diagnostics/src/conformance/tests.rs:76:    assert!(typed.is_som |
| 028 | Dependency hygiene: turso store boundary, ring exc | DONE | pass | crates/jackin-usage/src/telemetry_store.rs:13:use crate::store_backend::{Connection, Row, connect_local, params}; \| crates/jackin-usage/src/telemetry_store.rs:107:    let conn = c |
| 029 | Docs drift: README links, Apple status, reserved e | DONE | pass | docs/content/docs/reference/capsule/token-orchestrator.mdx:79:1. **GET** — `op item get <id> --vault <vault> --format json` fetches the full item as a `serde_json::Value`. The raw  |
| 030 | Console editor/settings view-model structs (kills  | DONE | pass | crates/jackin-console/src/tui/screens.rs:4:pub mod form_model; \| crates/jackin-console/src/tui/screens/form_model.rs:12:pub struct FieldRow { \| crates/jackin-console/src/tui/scre |
| 031 | Typed `op` probe errors (`OpProbeError` in jackin- | DONE | pass | crates/jackin-console/src/tui/components/op_picker/tests.rs:155:    let not_installed = anyhow::Error::new(jackin_core::OpProbeError::NotInstalled { \| crates/jackin-console/src/tu |
| 032 | Behavioral specs: capsule daemon + operator consol | DONE | pass | docs/content/docs/reference/developer-reference/specs/meta.json:8:    "capsule-daemon", \| docs/content/docs/reference/developer-reference/specs/meta.json:9:    "operator-console"  |
| 033 | Characterization: launch-core teardown, client dis | DONE | pass | crates/jackin-capsule/src/session/tests.rs:1165:// --- plan 033 suite C: PTY fault recovery (FaultMasterPty) --- \| crates/jackin-capsule/src/session/tests.rs:1168:struct FaultMast |
| 034 | Numeric + easy-to-avoid lint families (census: nea | DONE | pass | 224:float_cmp = "deny" \| 233:cast_sign_loss = "deny" \| 235:float_cmp_const = "deny" |
| 035 | Scheduled advisory lanes: llvm-cov, Miri, ASan fuz | DONE | pass | 469:          install_args: "cargo-binstall rust cargo:cargo-llvm-cov cargo:cargo-nextest" \| 480:      - name: llvm-cov nextest \| 482:          cargo llvm-cov nextest \ \| 486:   |
| 036 | Process boundary: one xtask cmd module; `RunOption | DONE | pass | crates/jackin-docker/src/shell_runner.rs:3://! The `CommandRunner` trait and `RunOptions` are re-exported from \| crates/jackin-docker/src/shell_runner.rs:16:pub use jackin_core::{ |
| 037 | thiserror for jackin-core concrete errors + jackin | DONE | pass | crates/jackin-core/src/docker_security.rs:45:    type Err = ParseProfileError; \| crates/jackin-core/src/docker_security.rs:53:            other => Err(ParseProfileError(other.to_o |
| 038 | `WorkspaceName` newtype at config/instance/launch  | DONE | pass | WorkspaceName |
| 039 | Pub-surface pilot: jackin-env sealed behind curate | DONE | pass | 8:mod host_claude; |
| 040 | In-place grid resize; same-size/height-only fast p | DONE | pass | crates/jackin-term/benches/resize_storm.rs |
| 041 | Typed operation facade; collapse duplicate `debug_ | DONE | pass | crates/jackin-docker/src/shell_runner.rs:87:    jackin_diagnostics::operation_record_exit_code(status.code()); \| crates/jackin-docker/src/shell_runner.rs:388:                jacki |
| 042 | High-frequency internals become metrics (9 instrum | DONE | pass | crates/jackin-diagnostics/src/lib.rs:42:    container_otlp, init_capsule_tracing, init_tracing, otel_events, otel_keys, otel_metrics, \| crates/jackin-diagnostics/src/observability |
| 043 | Per-sink telemetry filters; retire `JACKIN_DEBUG`  | DONE | pass | crates/jackin-capsule/src/session.rs:67:    "JACKIN_DEBUG", \| crates/jackin-capsule/src/session.rs:877:                // firehose breadcrumb so JACKIN_DEBUG=1 surfaces the drift. |
| 044 | Telemetry conformance suite (dossier acceptance ch | DONE | pass | crates/jackin-diagnostics/src/conformance/tests.rs:1://! Dossier acceptance checks as permanent conformance tests (plan 044). \| crates/jackin-diagnostics/src/conformance/tests.rs: |
| 045 | Corpus closure: protocol goldens + capability-skew | DONE | pass | crates/jackin-protocol/src/attach.rs:2://! Attach protocol handshake: initial capability negotiation and session-ID \| crates/jackin-protocol/src/attach.rs:198:    /// `capability_ |
| 046 | Scheduled dind-E2E chaos variant (seeded faults; s | DONE | pass | 8:      chaos_seed: \| 9:        description: "Optional JACKIN_CHAOS_SEED for dind-chaos lane" \| 687:  dind-chaos: \| 688:    name: DinD chaos E2E |
| 047 | Census the 7 allowed maintainability lints; deny q | DONE | pass | 194:needless_pass_by_value = "allow" # allow: 28 hits measured 2026-07, dominant pattern intentional by-value state/view handoffs (plan 047) \| 207:cognitive_complexity = "warn" |
| 048 | Advisory lanes wave 2: hyperfine cold-start, rust- | DONE | pass | 199:  # Advisory cold-start wall-clock for the jackin binary (plan 048). \| 201:  cold-start-bench: \| 229:          shared-key: cold-start-v1 \| 234:      - name: Hyperfine cold-s |
| 049 | Crate-README → Fumadocs generated section; slim PR | DONE | pass | 101:          bun run scripts/gen-crate-pages.ts |
| 050 | README-freshness gate (structural src change ⇒ REA | DONE | pass | crates/jackin-xtask/src/readme_freshness.rs:7://! cargo xtask lint readme-freshness --base origin/main \| crates/jackin-xtask/src/readme_freshness.rs:19:const RERUN: &str = "cargo  |
| 051 | Machine-readable gate output core (reporter; 2 exemplars) | DONE | pass | crates/jackin-xtask/src/lint.rs:50:    /// Output format (`human`, `json`, `github`). Defaults to human; under \| crates/jackin-xtask/src/report.rs:1://! Shared gate reporter: huma |
| 052 | dylint scaffold: `crates/jackin-lints` + render-th | DONE | pass | lints-crate \| crates/jackin-lints/src/lib.rs:1://! jackin❯ project lints (dylint). \| crates/jackin-lints/src/lib.rs:4://! workspace member — dylint compiles against rustc-private |
| 053 | TUI half-layer spike: prototype shared View dispat | DONE | pass | crates/jackin-tui/src/components.rs:57:pub use diff_view::{DiffViewState, SinglePaneKind, diff_view_hint_spans, render_diff_view}; \| crates/jackin-tui/src/components/diff_view/tes |
| 054 | Adopt `assertions_on_result_states` after mass tes | DONE | pass | 175:assertions_on_result_states = "deny" |
| 055 | Close named residual footnotes (014/023/028/033/03 | DONE | pass | 6:use jackin_usage::store_backend::{Connection, Row, connect_local, params}; \| 55:    connect_local(path) \| 101:          bun run scripts/gen-crate-pages.ts |
| 056 | Convert coverage-matrix SEQ debt to DEFER + residu | DONE | pass | matrix_SEQ_open 0 |

Total plan files: 52
Includes 051: yes
Includes 055-056: yes
in_tree fail rows: 0
