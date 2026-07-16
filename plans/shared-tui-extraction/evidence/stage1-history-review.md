# Stage 1 filtered-history review

Recorded 2026-07-15. Dedicated clone: `/home/agent/termrock-extraction`.

## Filter

Frozen donor revision: `33896a504e19ef13adb8692550c1845cb86a9504`.
Filtered imported-history boundary: `d8006e707b42f5edc27a424fcd21dc0ded60d52b`.

```sh
git-filter-repo --force \
  --path crates/jackin-tui \
  --path crates/jackin-tui-lookbook \
  --path docs/content/docs/reference/tui/lookbook \
  --path docs/public/tui-lookbook \
  --path LICENSE \
  --path NOTICE
```

The filtered checkout contains only those retained paths. It preserves 32
commits reachable from the filtered tip. Mixed `jackin-core` files were not
retained; their neutral helpers are reimplemented in signed bootstrap commits.
The donor-to-target reorganization remains an ordinary signed move/redesign
series in Stages 2–3 rather than another history rewrite.

## Audit results

- Authors: only `Alexey Zhokhov <alexey@zhokhov.com>`; inherited author and timestamp metadata preserved.
- Secrets: `gitleaks 8.28.0 git --redact .` scanned 58 historical commits and 1.69 MB; zero findings.
- Size: largest historical object is a 39,983-byte generated SVG; no unexpected binary or oversized object.
- License: Apache-2.0 `LICENSE` and donor `NOTICE` retained with history. Current files are either SPDX-marked or will be covered by the TermRock-specific `REUSE.toml` in the first bootstrap commit.
- Reference projects: case-insensitive scan for `tablepro|tableplus|zedis` returned zero matches.
- Provenance: clone-root `provenance.toml` parses with Python `tomllib` and records the literal filter command, retained paths, donor revision, boundary, and attribution policy.

## Public repository

Created `tailrocks/termrock` as a public, unseeded repository with the approved
description. GitHub reports size 0 and zero branches. The clone has a `termrock`
remote, but no ref or source has been pushed.
