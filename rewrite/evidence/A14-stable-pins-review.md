# A14 stable pins independent review

Date: 2026-11-09.

Reviewed commit: `ebfb26c9c3961ceba28e0cab84396c5530648b20`.

## Verdict

PASS. No findings.

## Implementation and evidence review

The committed diff and `rewrite/evidence/A14-stable-pins.md` satisfy every approved amendment.

- The eight-row literal release matrix checks each version, URL template, archive-member list, platform identifier, target triple, SHA-256 value, and rendered URL through `Agent::spec()` and `install_target()`. The production `ImagePlan::final_builds` checks assert all five derived build arguments for Codex, Kimi Code, and omp.
- Official latest-release metadata and vendor checksum files confirm Codex `0.167.0`, Kimi Code `2.5.2`, omp `18.18.3`, and their six Linux SHA-256 values. MiMo remains `0.1.16`, and its two release hashes match the vendor metadata.
- The stable pointers confirm Claude Code `2.1.311`, Grok Build `1.0.77`, Muse Code `1.4.14-R7309.1`, and Antigravity `1.5.3-6604829099950080`. Vendor manifests match the Claude Code and Muse hashes. Direct downloads match both Grok SHA-256 values and both Antigravity SHA-256 values. The Antigravity downloads also match the vendor SHA-512 manifests.
- T01 and T05 have matching `allowed_models` and `default_model` declarations in both the agent specs and tuples. T11 retains lowercase `minimax-m3` and `Supported`. T15 and T19 accept exact uppercase `MiniMax-M3` through `check_model`, but remain `Unavailable` with reason `uppercase MiniMax-M3 live proof pending`. T05, T21, and T22 retain their required unavailable reasons. T21 uses `claude-haiku-5-5`, and T22 retains `gpt-5.1-codex`.
- `plan_multi` calls the same instance validator as `plan_launch`. The tests distinguish `CatalogError::UnknownModel`, `PlanError::TupleNotSupported`, and `ManifestError::InvalidValue` at the public interfaces that expose them.
- The fixture inventory covers instance identity hashes, labels, gateway labels, role manifests, console role edits, launch plans, and the e2e scenario values. Remaining `gpt-5.1-codex` occurrences are T22 positives, removed-model negatives, arbitrary `ModelName` parser input, or historical evidence. No valid T01 fixture retains the removed model.

## Red and green record

`/tmp/a14-mutation-proof.log` records 58 source mutations with assertion failures and restored green runs. Its rows cover every URL template and target triple, every changed version and hash, every unchanged release row, the spec and tuple model declarations, and the changed T15 and T19 states and reasons. The separate three production URL mutations failed through `ImagePlan::final_builds`. The post-mutation agent-table, catalog, and build-plan logs pass.

`/tmp/a14-red-plan-multi-validator.log` shows T15, T17, and T19 failing with `expected unavailable tuple refusal` after removal of the shared validator. `/tmp/a14-green-plan-multi-validator.log` shows all three passing after restoration.

## Independent verification

The affected crate suites ran from the repository root with this command:

`mise exec -- mbx +1.99.1 nextest run -p jackin-core -p jackin-image -p jackin-session`

Result: exit 0. Nextest ran 921 tests, with 921 passed and 0 skipped.

G-FAST ran from the same commit with this command:

`mise exec -- mbx +1.99.1 xtask ci --fast`

Result: exit 0. Nextest ran 2168 tests, with 2168 passed and 0 skipped. fmt, clippy, deny, shear, comments, layers, pins, containers, errors, baseline, screen, demo-gate, and reuse also passed. The fast gate skipped the docs build as defined by the repository gate.

The review used official network metadata and artifact downloads for release verification. It made no live agent request. T15 and T19 remain unavailable pending exact uppercase live replies. No product or test code was changed.
