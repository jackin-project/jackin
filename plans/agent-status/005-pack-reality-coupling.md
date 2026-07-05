# Plan 005: Couple rule packs to reality — real captured goldens, kill the circular fixtures, fix the dead version gate

> **Executor instructions**: Run every verification command; honor STOP conditions. Update the README row.
>
> **Drift check**: `git diff --stat 5d3661cff..HEAD -- crates/jackin-capsule/src/agent_status/rules.rs docker/runtime/agent-status/packs`

## Status

- **Priority**: P2 (prerequisite for plan 007)
- **Effort**: M
- **Risk**: MED
- **Depends on**: none (blocks 007)
- **Category**: bug (detection infrastructure)
- **Planned at**: commit `5d3661cff`, 2026-07-03
- **Implementation status**: IN PROGRESS in PR 714. Real jackin❯-originated screenshots now backed focused fixtures for Codex stale-working/idle-with-footer, Kimi live working/idle-with-footer, OpenCode 1.17 working footer, and Grok working states. Full golden coverage is still incomplete: blocked and idle captures for every supported agent are not yet available, so the full anti-circularity contract remains open.

## Why this matters

The screen packs are the sole blocked/idle authority for reporter-less agents, yet the architecture has **no
coupling between a pack and the reality it must match** — so a 100%-wrong pack ships green. Two structural
gaps: (1) the pack fixtures are **authored from the same guessed strings as the packs** (circular self-validation
— `rules/tests.rs` asserts fixtures that *are* the pack literals), and (2) the one guard that would catch
pack-vs-CLI drift, `accepts_cli_version`, is **dead code** (only a test caller; no build/runtime path invokes
it), so `validated_versions` is never enforced. Combined, a pack can be entirely fabricated and CI stays green
— which is exactly how the kimi/amp/opencode/claude-idle placeholder strings (plan 007) shipped. And when a CLI
does move past a pack's validated window, detection degrades to **dark** (Unknown), whereas the reference
(herdr) always keeps its bundled manifest live as a fallback. Root cause: validation provenance is shared
between the pack author and the fixture author; and the drift signal is vaporware.

## Current state

- `crates/jackin-capsule/src/agent_status/rules.rs:363-375` — 5 packs `include_str!`-embedded (no grok).
- `crates/jackin-capsule/src/agent_status/rules/tests.rs:231-270` — `packs_load_and_match_fixtures` asserts
  fixtures under `.../screen/fixtures/<agent>/` yield the expected state — but those fixtures are hand-written
  glosses of the pack strings (e.g. `kimi/blocked.txt` = "Kimi wants permission:", `kimi/idle.txt` =
  "Kimi ready.") — same provenance as the pack.
- `crates/jackin-capsule/src/agent_status/rules.rs:387-391` — `accepts_cli_version`; `:383-390,416-436` comments
  describe an "image-build co-versioning check that must fail the build if a bundled pack does not cover the
  pinned CLI." Repo-wide grep finds **only** a test caller (`rules/tests.rs:116-120`) — no build/xtask/image
  path calls it. `validated_versions` (e.g. claude `">=2.1.173, <2.2.0"`) is never enforced.
- `min_engine_version` **is** enforced (`rules.rs:404-408`) but every pack defaults to 1 vs `RULE_ENGINE_VERSION=2`.
- Reference: herdr keeps bundled manifests always present as fallback + remote-updatable
  (`herdr/src/detect/manifest.rs:553-663`), and its manifests carry real captured fixtures + a dated version.

## Scope

**In scope:** `crates/jackin-capsule/src/agent_status/rules.rs` (version-gate wiring + runtime fallback), the
pack test harness + fixtures, and the image-build co-versioning check (`scripts/ci/check-agent-status-truthful.sh`
or an xtask). **Out of scope:** rewriting the pack *rule literals* (that's plan 007 — but this plan makes 007
verifiable).

## Steps

### Step 1: Replace circular fixtures with real captured goldens

Capture the actual visible screen (bottom ~24 rows) of each agent in each state **from a real `--debug` run**
(see `TESTING.md`'s pty-fixture flow) — capture jackin's own goldens. **Do NOT commit herdr's fixture files**:
they are files in an AGPL-3.0 repository, so copying them into this Apache-2.0 tree is a license-mixing risk,
regardless that their *content* is agent terminal output. You may *read* herdr's fixtures in place to learn
what a real screen looks like, but every committed golden must be captured by jackin from the real agent.
Store one golden per (agent, state) and point `packs_load_and_match_fixtures` at the **captured** goldens.
The rule: **a fixture must originate from
the agent, never from the pack author.** A pack that doesn't match its captured golden must fail the test.

**Verify**: `cargo nextest run -p jackin-capsule -E 'test(/packs_load_and_match_fixtures/)'` — will now **fail**
for the fabricated packs (kimi/amp/opencode/claude-idle); that failure is the point (plan 007 fixes the packs
against these goldens). If you must land 005 before 007, mark the known-failing goldens `#[ignore]` with a
`TODO(plan-007)` note so the harness exists but CI stays green until 007.

### Step 2: Make `validated_versions` advisory-at-runtime with a loud drift note (no silent dark)

Decouple runtime matching from the hard version gate: the newest loadable pack for an agent is **always** live
(herdr's always-fallback model), and when the running CLI is out of the pack's `validated_versions` window,
emit a loud, observable signal — an `EvidenceNote` (`evidence.rs:180` has the mechanism) and a `clog!` — rather
than dropping to Unknown. Drift becomes *visible*, not *dark*.

### Step 3: Either wire `accepts_cli_version` into the image build, or delete it + its misleading comments

Decide: (a) wire the co-versioning check into the derived-image build (`scripts/ci/check-agent-status-truthful.sh`
or a `cargo xtask`) so a pack lagging the pinned CLI **fails the build loudly** (making the comment true) — note
the roadmap says the daemon has no *runtime* CLI version source, but the *image build* pins the CLI, so this is
the right place; OR (b) delete `accepts_cli_version` and the ADR-style comments if co-versioning won't be
enforced. Prefer (a). Do not leave dead code describing a guard that doesn't run.

**Verify**: `grep -rn "accepts_cli_version" crates scripts` shows either a real build-path caller (a) or no
occurrences (b); `cargo clippy -p jackin-capsule -- -D warnings` → exit 0.

## Done criteria

- [ ] Pack fixtures are real captured goldens (agent-originated), not glosses of the pack strings — PARTIAL: live captures now cover several working/idle slices; full per-agent blocked/working/idle coverage remains open
- [ ] The match harness would FAIL a fabricated pack (proven by the fixtures the fabricated packs don't match) — PARTIAL with the newly captured slices; full proof waits on complete goldens
- [x] Out-of-window CLI does not make runtime matching dark; bundled packs stay live. A loud runtime drift note remains blocked until a non-invasive runtime CLI-version source exists
- [x] `accepts_cli_version` is either wired into the image build (fails on drift) or removed with its comments
- [x] `plans/agent-status/README.md` row updated

## STOP conditions

- You can't capture real agent screens (no Docker/agent access) — do **not** substitute herdr's fixture files
  (AGPL; do not commit them into this Apache-2.0 tree). Land Steps 2–3 and mark Step 1
  `BLOCKED (needs real captures)` — Steps 2/3 still remove structural rot.
- Wiring the co-versioning build check would fail the build **today** (packs already lag the pinned CLI) — that
  is the correct signal; coordinate with plan 007 so the packs are fixed in the same change, or gate the build
  check behind a warning first and escalate to error once 007 lands.

## Maintenance notes

- The durable win: fixture provenance ≠ pack provenance. A reviewer must reject any new fixture that is a
  hand-written gloss of the pack it validates.
- Real goldens make plan 007 (rewrite the pack literals) a verifiable, non-circular change instead of another
  round of guessing.
