# Build Results

- Status: NOT RUN
- Reason: The assigned documentation checkpoint prohibits builds and tests.

## Static setup

- Mise version: `2026.10.1`, reported by the environment inventory.
- The [Velnor configuration](../../.velnor/config.toml) sets compiler and test process budgets to four.
- The Rust stack uses MBX as its compile driver.
- The current generated workflow pins MBX `1.21.0`.
- No ambient MBX setup was reported on the host.
- Host inventory reports 96 logical processors, 125 GiB RAM, and 3.5 TiB disk.

These facts do not establish a build duration or cache hit.

## Required measurements

Every comparable build scenario requires at least three repetitions. Report median, minimum, maximum, peak memory when measurable, timing output, and MBX cache hits, misses, and bypasses. Record object provenance for every MBX run.

### Scenario matrix

| Scenario | Status | Reason |
|---|---|---|
| Empty target with an isolated empty MBX store | NOT RUN | Security review gates compilation. |
| Fresh target with a warm MBX store | NOT RUN | Security review gates compilation. |
| Unchanged source with an existing target | NOT RUN | Security review gates compilation. |
| Private change in one small crate | NOT RUN | Extraction and security review are pending. |
| Shared account-types change | NOT RUN | Extraction and security review are pending. |
| Codex discovery or authentication change | NOT RUN | Extraction and security review are pending. |
| Usage-provider change | NOT RUN | Extraction and security review are pending. |
| CLI or Console change | NOT RUN | Extraction and security review are pending. |
| Focused tests for the changed crate | NOT RUN | Tests are outside this checkpoint. |
| Workspace verification | NOT RUN | Security review gates compilation. |
| Required release build | NOT RUN | Release design and security review are pending. |
| Two consecutive compatible CI runs | NOT RUN | Coverage review and implementation remain pending. |

Do not set numeric split targets before measurements show build variation. Each scenario above must use the same reporting fields when those fields apply.

The `build_baseline` owner must record commands, elapsed time, resource use, cache state, and output hashes in a later authorized execution phase.

Security review requires MBX cache and object provenance before compilation. See [reviews](reviews.md).
