# Plan inventory (reverify) — second pass after wave 8

Tip: `032728d20` on `chore/rust-code-health-roadmap`.
Lint strict green. `ci --fast` red only for documented waivers (4 docker manager_flow + RUSTSEC-2026-0204).

| id | title | ledger_status | in_tree | evidence |
|----|-------|---------------|---------|----------|
| 003 | Stop deep-cloning usage views on every refresh | DONE | pass | 512:        let materialize = self.materialize_accounts(now_epoch()); ; 529:    pub(crate) fn materialize_accounts(&self, generated_at_epoch: i64) -> Result<(), |
| 004 | Stop dropping the frame queued behind a coalesced resize | DONE | pass | plan 004 landed; resize coalesce path in jackin-term (see DEFECT_LEDGER) |
| 007 | Bound OSC 8 hyperlink maps; clear on reset | DONE | pass | crates/jackin-term/src/grid/tests.rs:642:                    .hyperlink ; crates/jackin-term/src/grid/tests.rs:645:                owned_cell.hyperlink_id.as_de |
| 008 | Tear down DinD when a post-success finalization step fa | DONE | pass | crates/jackin-runtime/src/runtime/launch.rs:14://! * Foreground-attach finalization runs before teardown classification — ; crates/jackin-runtime/src/runtime/la |
| 009 | Fuzz + truncation tests for protocol wire decoders | DONE | pass | AGENTS.md ; CLAUDE.md ; Cargo.toml |
| 010 | Code-health dashboard, suppression inventory, verificat | DONE | pass | health+baseline |
| 011 | Silent-failure lints, rustdoc gates, doc tests in PR CI | DONE | pass | suppressions |
| 012 | Tier-graph arch gate (replaces empty forbidden-edge lis | DONE | pass | 36:pub(crate) const TIERS: &[(&str, u8)] = &[ |
| 013 | Flake detection, timing artifacts, migration idempotenc | DONE | pass | .config/nextest.toml:12:# flaky-tests.toml at the repo root; an unquarantined flake fails review. ; .config/nextest.toml:16:final-status-level = "flaky" ; .conf |
| 014 | Compile-check all benches; cover 4 unbenchmarked hot pa | DONE | pass | crates/jackin-diagnostics/benches/summarize_jsonl.rs ; crates/jackin-term/benches/resize_storm.rs |
| 015 | Brand-prose lint, spec↔test citations, README presence  | DONE | pass | crates/jackin-xtask/src/schema.rs:92:        // Only a real bump triggers the check. A file absent at base (a brand-new ; crates/jackin-xtask/src/schema.rs:193: |
| 016 | Ownership headers everywhere + headers gate + blame-ign | DONE | pass | crates/jackin-xtask/src/arch.rs:34:/// `pub(crate)` so the headers gate (plan 016) can cross-check crate ; crates/jackin-xtask/src/arch.rs:35:/// ownership head |
| 017 | Unified ratchet engine (`ratchet.toml`) + defect→gate l | DONE | pass | ratchet |
| 018 | One OTLP builder, semconv registry, correlatable sinks, | DONE | pass | crates/jackin-diagnostics/src/observability.rs:780:    fn build_otlp_providers( ; crates/jackin-diagnostics/src/observability.rs:825:            build_otlp_prov |
| 019 | Slice/index panic-coverage lints on the 4 pure crates | DONE | pass | Cargo.toml deny indexing/slice lints on pure crates (plan 019) |
| 020 | Container-path chokepoint + executable `/jackin/`-only  | DONE | pass | crates/jackin-xtask/src/main.rs:12:mod container_paths_gate; ; crates/jackin-xtask/src/main.rs:144:    ContainerPaths(container_paths_gate::LintContainerPathsAr |
| 021 | `missing_docs` on jackin-protocol + typed clipboard wir | DONE | pass | crates/jackin-protocol/src/attach.rs:565:pub enum ClipboardImageError { ; crates/jackin-protocol/src/attach.rs:588:impl ClipboardImageError { ; crates/jackin-pr |
| 022 | Scoped powerset PR gate, beta clippy canary, `xtask ci  | DONE | pass | .github/workflows/ci.yml:981:  # Plan 022: scoped feature-powerset for crates with real optional behavior. ; .github/workflows/ci.yml:982:  # Full workspace pow |
| 023 | Documented-command drift gate (docs fences ↔ clap tree) | DONE | pass | cargo xtask docs command-drift gate present (plan 023) |
| 024 | `Clock` seam in jackin-core; first consumer: clipboard  | DONE | pass | clock |
| 025 | Extract `jackin-test-support`; break isolation⇄runtime  | DONE | pass | test-support |
| 026 | Range-scoped scrollback snapshots (per-mouse-event full | DONE | pass | crates/jackin-capsule/benches/scrollback_snapshot.rs |
| 027 | Typed borrowed JSONL streaming; stop double-parsing det | DONE | pass | crates/jackin-diagnostics/src/conformance/tests.rs:72:    let typed = errors.iter().find(\|log\| { ; crates/jackin-diagnostics/src/conformance/tests.rs:76:    a |
| 028 | Dependency hygiene: turso store boundary, ring exceptio | DONE | pass | 6:use jackin_usage::store_backend::{Connection, Row, connect_local, params}; ; 55:    connect_local(path) ; host_turso_clean |
| 029 | Docs drift: README links, Apple status, reserved envs,  | DONE | pass | docs/content/docs/reference/capsule/token-orchestrator.mdx:79:1. **GET** — `op item get <id> --vault <vault> --format json` fetches the full item as a `serde_js |
| 030 | Console editor/settings view-model structs (kills the 4 | DONE | pass | crates/jackin-console/src/tui/screens/editor/view/general_tab.rs:8:use crate::tui::screens::form_model::{FieldRow, FormSection}; ; crates/jackin-console/src/tui |
| 031 | Typed `op` probe errors (`OpProbeError` in jackin-core; | DONE | pass | crates/jackin-env/src/op_cli.rs:257:    anyhow::Error::new(jackin_core::OpProbeError::NotInstalled { detail }).context(message) ; crates/jackin-env/src/op_cli.r |
| 032 | Behavioral specs: capsule daemon + operator console (ci | DONE | pass | docs/content/docs/reference/developer-reference/specs/meta.json:8:    "capsule-daemon", ; docs/content/docs/reference/developer-reference/specs/meta.json:9:     |
| 033 | Characterization: launch-core teardown, client displace | DONE | pass | crates/jackin-capsule/src/session/tests.rs:1165:// --- plan 033 suite C: PTY fault recovery (FaultMasterPty) --- ; crates/jackin-capsule/src/session/tests.rs:11 |
| 034 | Numeric + easy-to-avoid lint families (census: near-zer | DONE | pass | 224:float_cmp = "deny" ; 233:cast_sign_loss = "deny" ; 235:float_cmp_const = "deny" |
| 035 | Scheduled advisory lanes: llvm-cov, Miri, ASan fuzz, ca | DONE | pass | 469:          install_args: "cargo-binstall rust cargo:cargo-llvm-cov cargo:cargo-nextest" ; 480:      - name: llvm-cov nextest ; 482:          cargo llvm-cov n |
| 036 | Process boundary: one xtask cmd module; `RunOptions.tim | DONE | pass | crates/jackin-docker/src/shell_runner.rs:56:            timeout: _, ; crates/jackin-docker/src/shell_runner.rs:224:async fn await_child_with_timeout( ; crates/j |
| 037 | thiserror for jackin-core concrete errors + jackin-env  | DONE | pass | crates/jackin/src/cli/role.rs:88:        .map_err(\|e: jackin_runtime::runtime::docker_profile::ParseProfileError\| e.to_string()) ; crates/jackin-runtime/src/r |
| 038 | `WorkspaceName` newtype at config/instance/launch bound | DONE | pass | WorkspaceName |
| 039 | Pub-surface pilot: jackin-env sealed behind curated roo | DONE | pass | 8:mod host_claude; |
| 040 | In-place grid resize; same-size/height-only fast paths  | DONE | pass | crates/jackin-term/benches/resize_storm.rs |
| 041 | Typed operation facade; collapse duplicate `debug_log!` | DONE | pass | crates/jackin-diagnostics/src/conformance.rs:17:use crate::operation::{OperationLevel, operation_log, operation_span}; ; crates/jackin-diagnostics/src/conforman |
| 042 | High-frequency internals become metrics (9 instruments; | DONE | pass | crates/jackin-capsule/src/client_writer.rs:128:        // the emit path — render_allocation budgets cover this). ; crates/jackin-diagnostics/src/metrics.rs:14:  |
| 043 | Per-sink telemetry filters; retire `JACKIN_DEBUG` to on | DONE | pass | crates/jackin-diagnostics/src/logging.rs:44:    // Resolution: env level → JACKIN_DEBUG alias → config → --debug fallback. ; crates/jackin-diagnostics/src/loggi |
| 044 | Telemetry conformance suite (dossier acceptance checks  | DONE | pass | crates/jackin-diagnostics/src/conformance/tests.rs:1://! Dossier acceptance checks as permanent conformance tests (plan 044). ; crates/jackin-diagnostics/src/co |
| 045 | Corpus closure: protocol goldens + capability-skew, ter | DONE | pass | crates/jackin-protocol/tests/corpus_decode.rs:213:fn golden_client_frames_round_trip_decode() { ; crates/jackin-protocol/tests/corpus_decode.rs:245:fn golden_se |
| 046 | Scheduled dind-E2E chaos variant (seeded faults; surviv | DONE | pass | 8:      chaos_seed: ; 9:        description: "Optional JACKIN_CHAOS_SEED for dind-chaos lane" ; 687:  dind-chaos: |
| 047 | Census the 7 allowed maintainability lints; deny quiet  | DONE | pass | 194:needless_pass_by_value = "allow" # allow: 28 hits measured 2026-07, dominant pattern intentional by-value state/view handoffs (plan 047) |
| 048 | Advisory lanes wave 2: hyperfine cold-start, rust-analy | DONE | pass | 201:  cold-start-bench: ; 250:          name: cold-start-bench ; 255:  rust-analyzer-clean: |
| 049 | Crate-README → Fumadocs generated section; slim PROJECT | DONE | pass | 101:          bun run scripts/gen-crate-pages.ts |
| 050 | README-freshness gate (structural src change ⇒ README t | DONE | pass | crates/jackin-xtask/src/readme_freshness.rs:7://! cargo xtask lint readme-freshness --base origin/main ; crates/jackin-xtask/src/readme_freshness.rs:19:const RE |
| 051 | Machine-readable gate output core (human/json/github reporter) | DONE | pass | crates/jackin-xtask/src/report.rs gate reporter (human/json/github) |
| 052 | dylint scaffold: `crates/jackin-lints` + render-thread- | DONE | pass | crates/jackin-lints/AGENTS.md:8:- First lint: `render_thread_purity`. Follow-ups (foundational Debug/sealed, ; crates/jackin-lints/README.md:8:- **`render_threa |
| 053 | TUI half-layer spike: prototype shared View dispatcher, | DONE | pass | crates/jackin-tui/src/components.rs:57:pub use diff_view::{DiffViewState, SinglePaneKind, diff_view_hint_spans, render_diff_view}; ; crates/jackin-tui/src/runti |
| 054 | Adopt `assertions_on_result_states` after mass test con | DONE | pass | 175:assertions_on_result_states = "deny" |
| 055 | Close named residual footnotes (014/023/028/033/038/049 | DONE | pass | 6:use jackin_usage::store_backend::{Connection, Row, connect_local, params}; ; 55:    connect_local(path) ; 101:          bun run scripts/gen-crate-pages.ts |
| 056 | Convert coverage-matrix SEQ debt to DEFER + residual le | DONE | pass | matrix_SEQ_open 0 |

Total plans: 52; includes 051/055/056; in_tree fail rows: 0
