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

| Measurement | Status | Reason |
|---|---|---|
| Initial `jackin` build baseline | NOT RUN | Build execution is outside this checkpoint. |
| Second baseline before extraction | NOT RUN | No source extraction exists. |
| MBX activity and object provenance | NOT RUN | No compilation ran. |
| Post-split comparison | NOT RUN | No implementation change exists. |

The `build_baseline` owner must record commands, elapsed time, resource use, cache state, and output hashes in a later authorized execution phase.

Security review requires MBX cache and object provenance before compilation. See [reviews](reviews.md).
