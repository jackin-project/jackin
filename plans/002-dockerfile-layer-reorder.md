# Plan 002: Reorder the derived Dockerfile so the jackin runtime payload lands after the heavy agent layers

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md` — unless a reviewer dispatched you and told you they
> maintain the index.
>
> **Drift check (run first)**: `git diff --stat a2ec1b23..HEAD -- crates/jackin-image/src/derived_image.rs crates/jackin-image/src/image_recipe.rs`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: none (independent of 001)
- **Category**: perf
- **Planned at**: commit `a2ec1b23`, 2026-07-03

## Why this matters

The generated derived Dockerfile copies the `jackin-capsule` multiplexer binary
(and entrypoint/agent-status/zsh-title-shim) **before** the per-agent install
layers, the Claude plugin layer, and the default-home snapshot layers. The
capsule binary changes with every jackin release (its version string is
`<cargo-version>+<git-sha>`, `crates/jackin-build-meta/src/lib.rs:59-62`), so
every jackin upgrade — and every dev build — invalidates the Docker layer cache
for the entire tail: all agent installs (including `RUN <agent> --version`
verification layers), the network-bound `claude plugin marketplace add … &&
claude plugin install …` layer, and the expensive default-home `mv`/recursive
`find`/`chmod` layers. A measured launch
(`~/.jackin/data/diagnostics/runs/fec638.jsonl`, cache-miss reason
`capsule_version_changed`) paid a **105-second** overlay `docker_build` whose
build-step log shows all 24 steps re-running. With this reorder, a
capsule-version-only rebuild re-runs 3 `COPY --link` layers plus two cheap
`RUN`s — seconds instead of minutes. The same reorder means an agent-version
bump no longer re-copies the runtime payload (it still rebuilds later agent
layers — that narrower issue stays as-is; see Maintenance notes).

## Current state

Relevant files:

- `crates/jackin-image/src/derived_image.rs` — `render_derived_dockerfile`
  builds the Dockerfile string; the ordering lives in the `format!` template at
  lines 426–463.
- `crates/jackin-image/src/image_recipe.rs` — `IMAGE_RECIPE_VERSION: &str = "v8"`
  (line 29) and `generated_runtime_hash` (line 121) hash the rendered
  Dockerfile, so this change automatically invalidates existing images exactly
  once per role (intended; bump the version constant to make it explicit).
- Tests: `crates/jackin-image/src/derived_image/tests.rs`,
  `crates/jackin-image/src/image_recipe/tests.rs`.

The template today (`derived_image.rs:426-463`), abridged to the load-bearing
ordering:

```rust
    format!(
        "\
# syntax=docker/dockerfile:1.7
{base_dockerfile}
USER root
...
ARG JACKIN_RUN_UID=1000

# ── jackin runtime payload (entrypoint, multiplexer, shell-title shim) ──
{hook_copy_section}COPY --link --chmod=0755 .jackin-runtime/entrypoint.sh /jackin/runtime/entrypoint.sh
COPY --link --chmod=0755 .jackin-runtime/agent-status /jackin/runtime/agent-status
COPY --link --chown=agent:0 --chmod=0644 {zsh_title_shim_path} /jackin/runtime/zsh-title-shim
{jackin_capsule_section}
# ── Agent CLIs (D1: each agent's binary baked from its install_block) ──
{agent_install_sections}{claude_plugin_section}
# ── Default-home snapshot (D4): ... ──
RUN {default_home_commands}
RUN {default_home_guard}

# ── Runtime finalization: shell-title shim into .zshrc + jackin runtime dirs ──
RUN {hook_final_commands}{shell_title_and_runtime_dir_commands}

# ── Runtime home mutability: ... ──
RUN {runtime_home_writable_commands}

...
ENV PATH=\"/jackin/runtime:{agent_path_segment}:${{PATH}}\"
USER agent
ENTRYPOINT [\"/jackin/runtime/jackin-capsule\"]
",
```

where `{jackin_capsule_section}` is
`COPY --link --chmod=0755 {src} /jackin/runtime/jackin-capsule\n`
(`derived_image.rs:401-404`).

Ordering constraints found in the code (must be honored):

1. `shell_title_and_runtime_dir_commands` (`derived_image.rs:419-423`) `cat`s
   `/jackin/runtime/zsh-title-shim` into `/home/agent/.zshrc` — it must run
   **after** the zsh-title-shim COPY **and after** the default-home snapshot
   (it appends to the post-snapshot `/home/agent/.zshrc`; today it already runs
   after both).
2. `{hook_copy_section}` copies role-declared hook files into
   `/jackin/runtime/hooks/` (`derived_image.rs:129-133`); `hook_final_commands`
   references them. Hooks are role-owned and change rarely — they may stay
   early or move late; keep them adjacent to the other runtime-payload COPYs
   for readability.
3. `{default_home_commands}` (`render_default_home_commands`,
   `derived_image.rs:175-231`) operates on `/home/agent/*` produced by agent
   installs — it must stay after `{agent_install_sections}` and
   `{claude_plugin_section}`. It does not read `/jackin/runtime/*`.
4. `ENTRYPOINT` references a path only — position-independent.
5. All four runtime-payload COPYs use `--link`, which BuildKit rebases
   independently of parent layers, so moving them later does not change their
   own cacheability — it removes them from the *prefix* that keys the
   expensive `RUN` layers.

Repo conventions: comments explain WHY only; tests in `tests.rs` files;
`docs/content/docs/developing/construct-image.mdx` documents the derived-image
layering and must be updated in the same PR (PROJECT_STRUCTURE.md maps
`image_recipe.rs` Dockerfile-gen changes to that page).

## Commands you will need

| Purpose   | Command                                                                    | Expected on success |
|-----------|----------------------------------------------------------------------------|---------------------|
| Format    | `cargo fmt --check`                                                        | exit 0              |
| Lint      | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | exit 0        |
| Typecheck | `cargo check --all-targets`                                                | exit 0              |
| Tests     | `cargo nextest run -p jackin-image`                                        | all pass            |
| Full tests| `cargo nextest run --all-features`                                         | all pass            |

## Scope

**In scope** (the only files you should modify):
- `crates/jackin-image/src/derived_image.rs`
- `crates/jackin-image/src/derived_image/tests.rs`
- `crates/jackin-image/src/image_recipe.rs` (only the `IMAGE_RECIPE_VERSION`
  constant and its comment)
- `crates/jackin-image/src/image_recipe/tests.rs` (expectation updates)
- `crates/jackin-runtime/src/runtime/image/tests.rs` (only if it embeds
  rendered-Dockerfile expectations — check with
  `grep -rn "jackin runtime payload\|entrypoint.sh /jackin/runtime" crates/jackin-runtime/src/runtime/image/`)
- `docs/content/docs/developing/construct-image.mdx`

**Out of scope**:
- `docker/construct/Dockerfile` (the construct base — different lifecycle).
- Splitting per-agent install stages / `COPY --link` multi-stage refactor to
  stop one agent's bump rebuilding sibling layers — larger redesign, deferred
  (see Maintenance notes).
- Anything in `crates/jackin-runtime`'s decision logic (Plans 001/006).

## Git workflow

- Branch off `main`: `git checkout -b perf/dockerfile-layer-reorder`.
- `git commit -s -m "perf(image): order jackin runtime payload after heavy agent layers"`.
- Push after commit per CONTRIBUTING.md unless dispatch forbids it.

## Steps

### Step 1: Reorder the template

In `render_derived_dockerfile` (`derived_image.rs:426-463`), move the
runtime-payload block —
`{hook_copy_section}`, the `entrypoint.sh` COPY, the `agent-status` COPY, the
`zsh-title-shim` COPY, and `{jackin_capsule_section}` — from its current
position (immediately after `ARG JACKIN_RUN_UID`) to **after**
`RUN {default_home_guard}` and **before**
`RUN {hook_final_commands}{shell_title_and_runtime_dir_commands}`.

Resulting order:

1. base / `USER root` / `ARG JACKIN_RUN_UID`
2. `{agent_install_sections}{claude_plugin_section}`  ← heavy, now first
3. `RUN {default_home_commands}` + `RUN {default_home_guard}`
4. runtime payload COPYs (hooks, entrypoint.sh, agent-status, zsh-title-shim,
   jackin-capsule)  ← volatile, now last
5. `RUN {hook_final_commands}{shell_title_and_runtime_dir_commands}` (needs the
   shim + hooks present, and the post-snapshot home — both satisfied)
6. `RUN {runtime_home_writable_commands}`, `ENV PATH`, `USER agent`,
   `ENTRYPOINT`

Update the section comments to explain the WHY: "volatile jackin-owned files
last so a jackin upgrade rebuilds only these layers".

**Verify**: `cargo check --all-targets` → exit 0.

### Step 2: Check hidden dependencies of the moved block

Confirm none of `{agent_install_sections}`, `{claude_plugin_section}`,
`{default_home_commands}`, `{default_home_guard}` reference
`/jackin/runtime/…` paths (they must not depend on the moved COPYs):

```
grep -n "jackin/runtime" crates/jackin-image/src/derived_image.rs crates/jackin-core/src/agent/adapters/*.rs crates/jackin-core/src/agent/runtime.rs
```

Expected: matches only in the runtime-payload section, `ENV PATH`, ENTRYPOINT,
`shell_title_and_runtime_dir_commands`, and hook plumbing — none inside agent
install blocks or default-home command builders. If an agent install block
references `/jackin/runtime`, STOP.

**Verify**: grep output matches expectation above.

### Step 3: Bump the recipe version

In `image_recipe.rs:26-29`, bump `IMAGE_RECIPE_VERSION` to `"v9"` and update the
comment: layer reorder — one-time rebuild so all cached images adopt the
capsule-last layout. (The `generated_runtime_hash` would force this anyway;
the version bump makes the invalidation reason legible as
`RecipeVersionChanged` instead of a hash mystery.)

**Verify**: `cargo nextest run -p jackin-image` → failures only in tests that
assert `v8`/old ordering; fix them in Step 4.

### Step 4: Update tests

- `derived_image/tests.rs`: update rendered-Dockerfile expectations; add a
  dedicated assertion `capsule COPY appears after the last agent install
  section and after default_home_guard` (string-index comparison on the
  rendered output — follow the existing rendered-output test style).
- `image_recipe/tests.rs`: version constant expectations.
- Any `jackin-runtime` test embedding the rendered Dockerfile (Step 2 grep).

**Verify**: `cargo nextest run -p jackin-image -p jackin-runtime` → all pass.

### Step 5: Docs + gates

Update `docs/content/docs/developing/construct-image.mdx` where the derived
image layer order is described (search "layer"): document the new order and the
upgrade property (jackin version bump = cheap rebuild).

**Verify**:
- `cargo fmt --check` → exit 0
- `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` → exit 0
- `cargo nextest run --all-features` → all pass

## Test plan

- New assertion in `derived_image/tests.rs`: ordering invariant (capsule COPY
  index > last agent-install index > 0; `shell_title` RUN index > shim COPY
  index).
- Updated golden/rendered expectations.
- Verification: `cargo nextest run -p jackin-image` all pass.

## Done criteria

- [ ] Rendered Dockerfile places all four runtime-payload COPYs after
      `default_home_guard` and before the shell-title RUN (test asserts it)
- [ ] `IMAGE_RECIPE_VERSION == "v9"`
- [ ] fmt/clippy/check/nextest all green (`--all-features`)
- [ ] `docs/content/docs/developing/construct-image.mdx` updated in same branch
- [ ] No files outside in-scope list modified (`git status`)
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back if:

- Step 2's grep shows an agent install block or default-home builder reading
  `/jackin/runtime/…` — the reorder would break that layer.
- The zsh-title append (`shell_title_and_runtime_dir_commands`) turns out to
  run before the default-home snapshot in the current template (it should not —
  re-read lines 446-453; if the live code moved it, re-derive the constraint).
- Any test asserts BuildKit step *numbers* from recorded fixtures that cannot
  be regenerated deterministically.

## Maintenance notes

- One-time cost at rollout: every existing derived image rebuilds once
  (`RecipeVersionChanged`). Release notes should say so (`CHANGELOG.md`
  Unreleased per PRERELEASE.md hold rules).
- Deferred (recorded in plans/README.md): per-agent stage isolation so one
  agent's version bump doesn't rebuild sibling agent layers + default-home tail
  (audit finding PERF-07). That needs multi-stage `COPY --link=from` design and
  a recipe redesign — do not attempt it inside this plan.
- Reviewer focus: diff of the rendered Dockerfile for a representative role
  (tests should include one full rendered snapshot) — check layer semantics,
  not just ordering strings.
