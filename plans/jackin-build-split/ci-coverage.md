# CI Coverage

- Status: IN PROGRESS
- Evidence: Static workflow and generator configuration inspection.

## Current workflow

- The repository contains one workflow file: `.github/workflows/ci.yml`.
- The workflow defines 31 jobs. The three other top-level keys counted by a text search belong to event triggers.
- The [Velnor configuration](../../.velnor/config.toml) sets four compiler processes and four test processes.
- The workflow pins MBX `1.21.0` and the `velnor-actions` release `0.1.0`.
- The [release manifest](../../.velnor/release-manifest.json) pins generator source commit `c57c700459bbe1549fe7eedcb7d8689585c38986` from `tailrocks/velnor-new`.
- The reported upstream generator main SHA is `0c40d077fcad5450521351f497ce003c69915eff`.

## Coverage questions

The current workflow inventory has no separate release, macOS Swift, Docker, Bun, or scheduled workflow files.

The generator coverage review is still active. Confirm which jobs belong in the required CI surface before changing generated files.

Do not claim generated workflow parity. Compare generator source, configuration, output, and current checks after refs are fetched.

## Owner

`velnor_recon` and `jackin_generator_config` own generator and CI coverage. Record their final job matrix and required-check result here.

See [branch findings](branches.md), [build results](build-results.md), and [reviews](reviews.md).
