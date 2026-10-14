# C22 — Replay, observability, retention and export

**Status:** Proposed implementation contract. APIs below are targets, not existing Jackin APIs.

## 1. Purpose and ownership

Retain correlated workflow/debug evidence and safely inspect it after execution. Provide protected replay/export alongside governed operational telemetry. Capsule remains the original terminal authority; replay is explicitly a historical view.

## 2. Existing foundation and gap

Jackin requires `jackin-telemetry` and prohibits raw PTY/input/rendered content, argv, paths, names and secrets in telemetry at every level. Terminal-observation research proposes native recording rather than a nested terminal daemon. Implement workflow audit artifacts separately from general telemetry. [J18](#evidence-and-references), [J21](#evidence-and-references).

## 3. API and data contracts

```text
record_session_marker(session, run_event, artifact_offset) -> Receipt
capture_terminal(session, capture_policy) -> TerminalArtifact
replay_manifest(run, principal, filters) -> ReplayManifest
read_replay_segment(segment_ref, principal) -> SafeReplaySegment
export_run(run, ExportPolicy, principal) -> ExportManifest
evaluate_retention(now, holds, policy) -> RetentionActions
```

`ReplayManifest` correlates run/event sequences, logical and physical session epochs, terminal segment offsets, submitted prompt commands/receipts, vendor events, checkpoints/diffs, tests, findings and operator decisions. `ExportManifest` records format/version, exact plan/run identifiers, included artifacts/digests, redactions/omissions, integrity status and retention classification.

## 4. Required functionality

Capture timestamped terminal output and relevant screen/cursor/resize metadata from Capsule, not by scraping `docker exec` or nesting tmux. Persist prompt submissions and operator input as distinct protected audit records where policy permits; secret entry modes suppress plaintext capture. Preserve correlation even when content is redacted. A failed or resumed physical session starts a new segment with an explicit epoch marker.

Replay presents the original sequence with markers for automation, takeover/release, interruption, checkpoint creation and verification/publication decisions. It must expose lost/truncated ranges and unavailable content honestly. A replay of logs cannot claim a dead session is a still-running original TUI.

Never feed recorded escape sequences unfiltered into the operator’s real terminal. Use a safe offline terminal model that disables clipboard/OSC host actions, arbitrary URL opening, file transfer and dangerous control effects. Browser output uses sanitized rendering. Raw trace export, when authorized, is explicitly classified and not automatically executed or opened as active terminal input.

Register bounded workflow metrics/events through `jackin-telemetry`: transition outcomes, queue latency, active instance counts, lease conflicts, retries, evidence invalidity and recovery results. Use governed identifiers/attributes permitted by the registry. Private prompts, filenames, source content and terminal bytes stay in the protected artifact store; debug verbosity does not bypass this boundary.

## 5. Retention and export

Policies specify classes and finite normal retention for plan/source, audit events, prompts, terminal captures, diagnostic snapshots and artifacts. Mandatory acceptance/unique-work holds override ordinary TTL until safely released. Run failure and sandbox preservation are independent from eventual retention deletion. Deletion emits an audit tombstone/manifest and retains minimal non-sensitive provenance where policy permits.

Provide runtime exports in Markdown for human review, JSON for machine inspection and a manifest-based bundle for authorized artifacts. A Markdown export includes plan/run summary, node attempts, exact revisions, evidence links, findings, operator actions and publication state, with explicit omissions. Exporting does not imply redaction is perfect; classify and authorize the output destination. Protect encryption keys and credential values from all bundles.

## 6. Build instructions

Add a recording/correlation interface inside Capsule and an immutable segment format with fixtures. Implement safe offline replay and sanitizer tests before raw streaming into clients. Register operational telemetry through existing governance. Implement Markdown/JSON/bundle exports from the same host read model, then retention evaluation through reference/hold checks. Include replay format migration and corrupted-segment handling.

## 7. Dependencies and integration

C06 produces session records, C07 interventions, C08 Git changes, C10 events, C11 artifacts, C12/C13 evidence, C17 retention holds and C18 access control. Console/web clients request projections, not raw unauthorized host storage.

## 8. Acceptance tests

`OBS-01`: replay survives host restart and correlates all required evidence. `OBS-02`: raw prompts/PTY/paths/secrets never reach governed telemetry. `OBS-03`: malicious OSC/clipboard/URL/file-transfer sequences have no replay side effects. `OBS-04`: missing/truncated segments remain visible. `OBS-05`: exports contain verified manifests and explicit redactions. `OBS-06`: TTL cannot delete held unique work/evidence. `OBS-07`: failed sandbox diagnostics remain available under retention policy.

## Evidence and references

- **J18 — [Engineering rules](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/ENGINEERING.md).** Full document. Rust-first, reuse, turso general-purpose DB, governed jackin-telemetry and privacy rules.
- **J21 — [Terminal observation research](https://github.com/jackin-project/jackin/blob/453bf611fd974ff88f18caa97aa225af1f8edb2c/docs/content/research/agents/orchestration/terminal-observation/terminal-observation-automation.mdx).** First 240 lines requested; response tail truncated. Used only for the visible current-leverage and proposed API sections, not as evidence that proposals shipped.
