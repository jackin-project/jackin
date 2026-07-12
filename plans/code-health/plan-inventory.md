# Plan inventory (reverify) — goal pass after plan 066

Tip: `chore/rust-code-health-roadmap` after 064–066 residual wave.
Lint strict green. Residuals DEFER only with measured ledger reasons.

| id | title | ledger_status | in_tree | evidence |
|----|-------|---------------|---------|----------|
| 003 | Stop deep-cloning usage views on every refresh | DONE | pass | planfile |
| 004 | Stop dropping the frame queued behind a coalesced resiz | DONE | pass | planfile |
| 007 | Bound OSC 8 hyperlink maps; clear on reset | DONE | pass | planfile |
| 008 | Tear down DinD when a post-success finalization step fa | DONE | pass | planfile |
| 009 | Fuzz + truncation tests for protocol wire decoders | DONE | pass | planfile |
| 010 | Code-health dashboard, suppression inventory, verificat | DONE | pass | ok |
| 011 | Silent-failure lints, rustdoc gates, doc tests in PR CI | DONE | pass | planfile |
| 012 | Tier-graph arch gate (replaces empty forbidden-edge lis | DONE | pass | 36:pub(crate) const TIERS: &[(&str, u8)] = &[ |
| 013 | Flake detection, timing artifacts, migration idempotenc | DONE | pass | planfile |
| 014 | Compile-check all benches; cover 4 unbenchmarked hot pa | DONE | pass | planfile |
| 015 | Brand-prose lint, spec↔test citations, README presence  | DONE | pass | planfile |
| 016 | Ownership headers everywhere + headers gate + blame-ign | DONE | pass | planfile |
| 017 | Unified ratchet engine (`ratchet.toml`) + defect→gate l | DONE | pass | ok |
| 018 | One OTLP builder, semconv registry, correlatable sinks, | DONE | pass | planfile |
| 019 | Slice/index panic-coverage lints on the 4 pure crates ( | DONE | pass | planfile |
| 020 | Container-path chokepoint + executable `/jackin/`-only  | DONE | pass | planfile |
| 021 | `missing_docs` on jackin-protocol + typed clipboard wir | DONE | pass | planfile |
| 022 | Scoped powerset PR gate, beta clippy canary, `xtask ci  | DONE | pass | planfile |
| 023 | Documented-command drift gate (docs fences ↔ clap tree) | DONE | pass | planfile |
| 024 | `Clock` seam in jackin-core; first consumer: clipboard  | DONE | pass | planfile |
| 025 | Extract `jackin-test-support`; break isolation⇄runtime  | DONE | pass | ok |
| 026 | Range-scoped scrollback snapshots (per-mouse-event full | DONE | pass | planfile |
| 027 | Typed borrowed JSONL streaming; stop double-parsing det | DONE | pass | planfile |
| 028 | Dependency hygiene: turso store boundary, ring exceptio | DONE | pass | 6:use jackin_usage::store_backend::{Connection, Row, connect_local, params}; |
| 029 | Docs drift: README links, Apple status, reserved envs,  | DONE | pass | planfile |
| 030 | Console editor/settings view-model structs (kills the 4 | DONE | pass | planfile |
| 031 | Typed `op` probe errors (`OpProbeError` in jackin-core; | DONE | pass | planfile |
| 032 | Behavioral specs: capsule daemon + operator console (ci | DONE | pass | planfile |
| 033 | Characterization: launch-core teardown, client displace | DONE | pass | planfile |
| 034 | Numeric + easy-to-avoid lint families (census: near-zer | DONE | pass | planfile |
| 035 | Scheduled advisory lanes: llvm-cov, Miri, ASan fuzz, ca | DONE | pass | planfile |
| 036 | Process boundary: one xtask cmd module; `RunOptions.tim | DONE | pass | planfile |
| 037 | thiserror for jackin-core concrete errors + jackin-env  | DONE | pass | planfile |
| 038 | `WorkspaceName` newtype at config/instance/launch bound | DONE | pass | ok |
| 039 | Pub-surface pilot: jackin-env sealed behind curated roo | DONE | pass | planfile |
| 040 | In-place grid resize; same-size/height-only fast paths  | DONE | pass | planfile |
| 041 | Typed operation facade; collapse duplicate `debug_log!` | DONE | pass | planfile |
| 042 | High-frequency internals become metrics (9 instruments; | DONE | pass | planfile |
| 043 | Per-sink telemetry filters; retire `JACKIN_DEBUG` to on | DONE | pass | planfile |
| 044 | Telemetry conformance suite (dossier acceptance checks  | DONE | pass | planfile |
| 045 | Corpus closure: protocol goldens + capability-skew, ter | DONE | pass | planfile |
| 046 | Scheduled dind-E2E chaos variant (seeded faults; surviv | DONE | pass | planfile |
| 047 | Census the 7 allowed maintainability lints; deny quiet  | DONE | pass | planfile |
| 048 | Advisory lanes wave 2: hyperfine cold-start, rust-analy | DONE | pass | planfile |
| 049 | Crate-README → Fumadocs generated section; slim PROJECT | DONE | pass | 101:          bun run scripts/gen-crate-pages.ts |
| 050 | README-freshness gate (structural src change ⇒ README t | DONE | pass | planfile |
| 051 | Machine-readable gate output core (human\ | DONE | pass | report.rs |
| 052 | dylint scaffold: `crates/jackin-lints` + render-thread- | DONE | pass | planfile |
| 053 | TUI half-layer spike: prototype shared View dispatcher, | DONE | pass | planfile |
| 054 | Adopt `assertions_on_result_states` after mass test con | DONE | pass | 175:assertions_on_result_states = "deny" |
| 055 | Close named residual footnotes (014/023/028/033/038/049 | DONE | pass | 6:use jackin_usage::store_backend::{Connection, Row, connect_local, params}; |
| 056 | Convert coverage-matrix SEQ debt to DEFER + residual le | DONE | pass | matrix_SEQ_open 0 |
| 057 | Residual R1: materialize bench + export-volume ratchet  | DONE | pass | 133:# Plan 044 export-volume budgets (R-export-volume-ratchet): constants live in |
| 058 | Residual R1: complexity floor + env doctor WorkspaceNam | DONE | pass | 16:cognitive-complexity-threshold = 58 |
| 059 | WorkspaceName on config roles resolve APIs | DONE | pass | 16:fn workspace_key(workspace: Option<&WorkspaceName>) -> &str { |
| 060 | WorkspaceName on env operator-resolve APIs | DONE | pass | 333:    workspace_name: Option<&WorkspaceName>, |
| 061 | WorkspaceName on console save/launch + ConfigEditor wri | DONE | pass | 68:    workspace_name: &WorkspaceName, |
| 062 | WorkspaceName on AppConfig require/edit/remove | DONE | pass | 16:    pub fn require_workspace(&self, name: &WorkspaceName) -> anyhow::Result<&WorkspaceConfig> { |
| 063 | WorkspaceName on isolation list + drift detect | DONE | pass | 219:pub fn list_records_for_workspace( |
| 064 | WorkspaceName on auth_error traces + token revoke/expiry | DONE | pass | planfile + auth_error/token_setup |
| 065 | thiserror mid-tranche jackin-instance | DONE | pass | InstanceError + SyncSourceValidationError |
| 066 | thiserror mid-tranche jackin-isolation | DONE | pass | IsolationError |
| 067 | thiserror mid-tranche jackin-docker | DONE | pass | DockerError |
| 068 | thiserror mid-tranche jackin-image | DONE | pass | ImageError |
| 069 | thiserror mid-tranche jackin-config | DONE | pass | ConfigError; R-thiserror CLOSED |

Total plans: 65; in_tree fail rows: 0
