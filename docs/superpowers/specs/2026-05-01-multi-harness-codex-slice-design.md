# Multi-Harness Foundation — Codex Vertical Slice

**Status:** Proposed
**Date:** 2026-05-01
**Scope:** `jackin` crate, `docker/construct/`, `docker/runtime/entrypoint.sh`, manifest schema, workspace config schema, first-party agent repo follow-up
**PR shape:** single coordinated feature branch; agent-smith repo update lands as a small follow-up PR
**Related roadmap:** [Multi-Runtime Support for Codex and Amp](../../src/content/docs/reference/roadmap/multi-runtime-support.mdx)

## Problem

jackin's stated value is operator-side isolation: agent classes as Dockerfiles, named workspaces, scoped mounts, isolated container lifecycles. None of that value is intrinsically tied to Claude. But today every layer of the implementation — manifest schema, derived image, entrypoint, runtime user, persisted state, auth forwarding, version probing, mount destinations — assumes Claude Code in concrete, not just naming.

That coupling makes the project look more harness-agnostic than it really is, and it blocks the next obvious operator win: running the same agent class under Codex (or, later, Amp) without forking the agent repo.

The roadmap describes a five-phase plan to land Codex and Amp. This spec is **not** that whole plan. It is a tightly scoped vertical slice that:

1. proves a second harness can actually launch under jackin's existing model,
2. lets the friction of building it inform the seam shape, and
3. lands the unavoidable user/path rename in the same atomic change so we don't pay for it twice.

After this slice merges, Amp and the deferred items become smaller, well-scoped follow-up specs against a foundation that has already absorbed real second-harness pressure.

## Goals

1. Add `Codex` as a second supported harness alongside `Claude`, selectable per launch.
2. Introduce a small built-in harness abstraction (enum + per-harness data + small fns) that all harness-shaped code in the crate routes through. No trait, no marketplace.
3. Rename the in-container OS user from `claude` to `agent` and `/home/claude` to `/home/agent`. This is a deliberate breaking change.
4. Move harness selection to the **workspace** config, with a per-launch CLI override.
5. Use **one image per agent class**, with all supported harnesses installed at build time. Container identity does not carry the harness; `JACKIN_HARNESS` is passed at `docker run` time.
6. Keep Claude behavior bit-identical from the operator's perspective except for the `/home/agent` path change.
7. Land the construct image rename in-place on `:trixie` so existing `FROM` lines keep working after a `docker pull`.
8. Ship the slice atomically: no intermediate state where the user is named `claude` but runs Codex.

## Non-Goals

- TUI harness picker. `jackin console` ships unchanged in this slice; the picker is a follow-up spec.
- Per-agent-class harness override. Workspace is the only scope in V1. The agent class manifest declares which harnesses it *supports*; it does not declare a default.
- Amp support. Phase 3 of the roadmap. The seam is designed to admit Amp later by adding one enum variant + one profile + match arms.
- Codex auto-update / version probe. `--rebuild` is the V1 update path for Codex.
- `jackin sync` for Codex. Sync is Claude-OAuth-shaped; for Codex it returns a clear "Claude-only in V1" error.
- A rich `[codex]` manifest section. V1 supports at most a `model` field; sandbox/approval policy lives in jackin-generated `config.toml` only.
- Generalizing the legacy `JACKIN_CLAUDE_ENV` env name. That cleanup gets its own small spec.
- Designing a third-party harness plugin system.
- Re-cutting the construct image's tag (e.g. `:trixie-2`). Tag stays `:trixie`, rebuilt in place, with `--pull` added to derived builds to ensure operators pick up the new digest.

## Design

### Naming and concept

The concept currently called "runtime" in the roadmap is renamed to **`harness`** in this spec, in code, in manifest tables, and in operator-facing docs. The agent class (jackin-agent-smith, jackin-the-architect) stays the unit of "what tools and Dockerfile shape." The harness (claude, codex) is the unit of "what AI CLI runs inside."

Reasoning: "runtime" is overloaded inside jackin (Docker runtime, jackin runtime) and the operator-facing concept is closer to "the harness that runs the model."

### Harness abstraction (Approach B — enum + data + small fns)

New module: `src/harness/mod.rs`.

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Harness {
    Claude,
    Codex,
}

impl Harness {
    pub fn slug(self) -> &'static str { /* "claude" | "codex" */ }
}

impl FromStr for Harness { /* parse "claude" / "codex"; reject others with clear error */ }
```

Per-harness data, in `src/harness/profile.rs`:

```rust
pub struct HarnessProfile {
    /// Lines appended to the derived Dockerfile to install this harness.
    pub install_block: &'static str,
    /// argv `exec`'d by the entrypoint when this harness is selected.
    pub launch_argv: &'static [&'static str],
    /// Env vars that must be present at launch; absence is a hard error.
    pub required_env: &'static [&'static str],
    /// Whether this harness loads jackin-managed plugins at startup.
    pub installs_plugins: bool,
    /// Container-side paths this harness expects state to be mounted at.
    pub container_state_paths: ContainerStatePaths,
}

pub struct ContainerStatePaths {
    pub home_subpaths: &'static [(&'static str, MountKind)],
    // e.g. Claude: [(".claude", Dir), (".claude.json", File), (".jackin/plugins.json", File)]
    //      Codex:  [(".codex/config.toml", File)]
}

pub fn profile(h: Harness) -> &'static HarnessProfile { match h { ... } }
```

For behavior that genuinely differs and cannot be reduced to data — auth provisioning, version probing — small fns are also matched on `Harness`, kept colocated where their Claude-only ancestor lives today (e.g. in `instance/auth.rs`).

No trait. The roadmap explicitly asks for "a small built-in harness abstraction, not a marketplace for harnesses," and a trait designed against only two implementations is more likely to misshape than to help. When Amp lands (its own spec), the right move is to either keep the enum or refactor toward a trait with concrete pressure from three implementations, not to guess up-front.

### Manifest schema

Backward-compatible changes to `src/manifest/mod.rs`:

```toml
dockerfile = "Dockerfile"

[identity]
name = "Agent Smith"

[harness]                          # NEW, optional
supported = ["claude", "codex"]    # which harnesses this agent class can run

[claude]                           # required iff "claude" in [harness].supported
plugins = []
marketplaces = []

[codex]                            # required iff "codex" in [harness].supported (empty in V1)
```

**Default behavior when `[harness]` is absent:** treat as `supported = ["claude"]`. Every existing agent repo and manifest in the wild keeps working without edits.

**Validation rules** (new, in `src/manifest/validate.rs`):

- `[harness].supported` must be non-empty if present.
- For every harness `H` in `supported`, the corresponding `[H]` table must exist (even if empty). This makes "does this manifest know about codex?" a single grep.
- Unknown harness names in `supported` are rejected.
- The manifest does **not** declare a default harness. Default is the workspace's responsibility.

### Workspace config

The harness selection is a workspace-level setting:

```toml
[workspaces.prod]
harness = "claude"           # NEW; defaults to "claude" if omitted
agents = ["agent-smith", "the-architect"]
mounts = [...]
```

When `jackin load <agent>` runs inside `prod`:

1. Resolve workspace's `harness` (default `"claude"` if absent).
2. CLI `--harness <h>` overrides for one launch.
3. Validate the chosen harness is in the agent class's manifest `[harness].supported`. If not, fail-fast with: `agent "<class>" does not support harness "<h>"; supported: [...]`.

Per-agent harness override is not in this slice. If someone wants `agent-smith` on Claude but `the-architect` on Codex within the same workspace, V1 says: use two workspaces. We can add per-agent override later if real demand emerges.

### Image identity

**One image per agent class.** No harness suffix in image or container name. `image_name(selector)` keeps its current shape: `jackin-<class>`.

The derived Dockerfile installs *every* supported harness from the manifest. For an `agent-smith` whose manifest declares `supported = ["claude", "codex"]`, the image contains both Claude and Codex CLIs.

`JACKIN_HARNESS` is passed at `docker run` time (not baked as `ENV` in the image). The same image serves any harness; the entrypoint dispatches.

**Failure isolation tradeoff:** if a Codex install regression breaks the install layer, Claude users on the same image experience a broken rebuild until Codex's install is fixed. We accept this for V1 simplicity. If it becomes a real operational problem, we revisit by moving to layered/staged installs that fail per-harness independently.

**No host data-dir migration is required.** Container name is unchanged for existing operators; `~/.jackin/data/jackin-agent-smith/` keeps its bare path.

### OS user / home rename

This slice ships the `claude` → `agent` and `/home/claude` → `/home/agent` rename atomically.

**Construct image** (`docker/construct/Dockerfile`):

| Today | After |
|---|---|
| `claude` Linux user/group, primary shell user | `agent` |
| `/home/claude` | `/home/agent` |
| `install -d -o claude -g claude /home/claude/.claude /home/claude/.jackin` | `install -d -o agent -g agent /home/agent/.claude /home/agent/.jackin` |
| `ENV PATH="/home/claude/.local/share/mise/shims:/home/claude/.local/bin:${PATH}"` | `/home/agent/...` |
| `install-plugins.sh` (Claude-specific bootstrap) | renamed to `install-claude-plugins.sh` |
| zshrc, oh-my-zsh customizations under `/home/claude/...` | `/home/agent/...` |

**Tag strategy:** `:trixie` is rebuilt in place with the new shape. The image's content digest changes; the tag does not. Operators who pull `:trixie` fresh after the slice ships pick up the new shape automatically. Operators with cached `:trixie` need a `docker pull` — handled by adding `--pull` to jackin's derived build invocation.

**Why in-place rather than `:trixie-2`:** smaller surface to coordinate. Agent repos do not need to update `FROM` lines, which means there is no version skew window where some agent repos point at the old tag and some at the new. Failure mode (cached old `:trixie` produces broken builds) is mitigated by the auto-`--pull`.

**Derived image** (`src/derived_image.rs`):

`render_derived_dockerfile(base, hook, supported: &[Harness])` becomes harness-aware. The function:

1. Inserts the existing UID/GID rewrite block, but targeting `agent` instead of `claude`.
2. Concatenates each `profile(h).install_block` for `h` in `supported`.
3. Copies the entrypoint to `/home/agent/entrypoint.sh`.
4. Sets `ENTRYPOINT ["/home/agent/entrypoint.sh"]`.
5. Does **not** set `ENV JACKIN_HARNESS=...` — that comes at `docker run`.

**Claude install_block** (unchanged content, restated for clarity):

```
USER agent
ARG JACKIN_CACHE_BUST=0
RUN curl -fsSL https://claude.ai/install.sh | bash
RUN claude --version
```

**Codex install_block** (new, version-pinned):

```
USER agent
RUN curl -fsSL https://github.com/openai/codex/releases/download/v<PINNED>/codex-x86_64-unknown-linux-musl \
      -o /tmp/codex \
 && sudo install -m 0755 /tmp/codex /usr/local/bin/codex \
 && rm /tmp/codex \
 && codex --version
```

The exact pinned Codex version is selected during implementation against whatever upstream tag is current and stable on the day the implementation PR is opened. Updating Codex requires `--rebuild`.

### Entrypoint dispatch

`docker/runtime/entrypoint.sh` becomes a single dispatch script:

```bash
#!/bin/bash
set -euo pipefail
[ "${JACKIN_DEBUG:-0}" = "1" ] && set -x

# ── runtime-neutral setup (unchanged) ───────────────────────────────
configure_git_identity_from_host_env
authenticate_gh_if_present

# ── harness-specific setup ──────────────────────────────────────────
case "${JACKIN_HARNESS:?JACKIN_HARNESS must be set}" in
  claude)
    run_maybe_quiet /home/agent/install-claude-plugins.sh
    [ "${JACKIN_DISABLE_TIRITH:-0}" = "1" ] || run_maybe_quiet claude mcp add tirith -- tirith mcp-server || true
    [ "${JACKIN_DISABLE_SHELLFIRM:-0}" = "1" ] || run_maybe_quiet claude mcp add shellfirm -- shellfirm mcp || true
    LAUNCH=(claude --dangerously-skip-permissions --verbose)
    ;;
  codex)
    # config.toml is mounted RO from host (see "State and auth"); no in-container generation needed.
    LAUNCH=(codex)
    ;;
  *)
    echo "[entrypoint] unknown JACKIN_HARNESS: $JACKIN_HARNESS" >&2
    exit 2
    ;;
esac

# ── pre-launch hook (runtime-neutral) ───────────────────────────────
[ -x /home/agent/.jackin-runtime/pre-launch.sh ] && /home/agent/.jackin-runtime/pre-launch.sh

[ "${JACKIN_DEBUG:-0}" = "1" ] && { echo "[entrypoint] Setup complete. Press Enter to launch ${JACKIN_HARNESS}..."; read -r; }
printf '\033[2J\033[H'
exec "${LAUNCH[@]}"
```

The exact codex argv (interactive default vs subcommand) is finalized during implementation by reading the current Codex CLI release notes; the `LAUNCH=(codex)` placeholder above represents the expected interactive default.

### State and auth

**Host state directory layout** (`~/.jackin/data/jackin-<class>/`):

```
~/.jackin/data/jackin-agent-smith/
  .claude.json                   # Claude (existing)
  .claude/                       # Claude (existing)
  plugins.json                   # Claude (existing)
  config.toml                    # Codex (NEW; only present when codex used)
  gh/                            # both (existing)
```

Files do not collide between harnesses. Each harness writes only its own files. The directory continues to be keyed on the bare container name, so existing Claude operators see no path change.

**Mount construction** (`src/runtime/launch.rs`):

A new fn `harness_mounts(h: Harness, &state) -> Vec<MountSpec>` returns the per-harness mounts. The launch flow concatenates this with the harness-neutral mount set (workspace, terminfo, gh config, etc.).

| Harness | Host source | Container destination | Mode |
|---|---|---|---|
| Claude | `<datadir>/.claude/` | `/home/agent/.claude/` | RW |
| Claude | `<datadir>/.claude.json` | `/home/agent/.claude.json` | RW |
| Claude | `<datadir>/plugins.json` | `/home/agent/.jackin/plugins.json` | RO |
| Codex | `<datadir>/config.toml` | `/home/agent/.codex/config.toml` | RW |

**Mode rationale.** Read-only is reserved for jackin-directive files — those where jackin's intent is canonical and the runtime is expected to consume but never write back (`plugins.json` is the only one in V1). Operator-style runtime configs (`.claude.json`, Codex's `config.toml`) are mounted RW because the runtime owns its own config evolution: Claude may persist login state, MRU lists, and similar; Codex may persist last-used model, history pointers, and similar. Mounting these RO would surface as cryptic permission errors at runtime with no upside, since jackin can simply rewrite the file on the next launch if it wants to win.

**Claude auth** (`src/instance/auth.rs::provision_claude_auth`): preserved verbatim, gated by `harness == Harness::Claude` at the call site in `instance/mod.rs`.

**Codex auth**: env-only. New `provision_codex_auth(state, manifest)` writes the host-side `config.toml`:

```toml
# Generated by jackin; do not edit.
approval_policy = "never"
sandbox_mode = "danger-full-access"
model = "<from manifest [codex].model, or omitted to use Codex default>"
```

Reasoning for the policy values: jackin's container is already the operator's trust boundary; Codex's internal sandbox/approval add friction without isolation gain. Roadmap calls this out as the expected V1 posture.

**`OPENAI_API_KEY`** flows through jackin's existing operator-env mechanism. The harness profile's `required_env` for Codex is `["OPENAI_API_KEY"]`; absence at launch is a hard error: `harness "codex" requires OPENAI_API_KEY in operator env`.

**`AuthForwardMode`** (Ignore/Sync/Token) stays a Claude concept. When harness is Codex, the field is ignored but not removed from config — forward-compat for operators who switch back to Claude.

### CLI surface

- **New flag:** `jackin load <agent> [--harness <claude|codex>]`. Resolution order: CLI flag → workspace config → `"claude"` fallback.
- **`jackin status`, `jackin attach`, `jackin destroy`:** operate on bare container names; no surface change.
- **`jackin sync`:** Claude-only in V1. For Codex emits `jackin sync is Claude-only in V1; OpenAI keys are forwarded via OPENAI_API_KEY in operator env`.
- **`jackin console`:** unchanged in this slice. Harness picker is a deferred spec.

### Identity-related changes summary

| Area | Today | After slice |
|---|---|---|
| OS user inside container | `claude` | `agent` |
| Home dir | `/home/claude` | `/home/agent` |
| Image name | `jackin-<class>` | `jackin-<class>` (unchanged) |
| Container name | `jackin-<class>` | `jackin-<class>` (unchanged) |
| Data dir | `~/.jackin/data/jackin-<class>/` | `~/.jackin/data/jackin-<class>/` (unchanged) |
| Construct tag | `:trixie` | `:trixie` (in-place rebuild, new digest) |
| Plugin script | `install-plugins.sh` | `install-claude-plugins.sh` |
| Harness selector | (none) | `JACKIN_HARNESS` env at `docker run` |

The only operator-visible breakage is mount destinations pointing at `/home/claude/...` in operator config or workspace config — those need to be updated to `/home/agent/...`.

## Migration and breaking-change communication

- **DEPRECATED.md entry:** mount destinations under `/home/claude/...` are deprecated; operators must update to `/home/agent/...`. Old construct `:trixie` digest is implicitly deprecated by the in-place rebuild.
- **Manifest validator warnings** (`src/manifest/validate.rs`): when a Dockerfile `FROM` references `:trixie` and any path in the operator's mount config still points at `/home/claude/`, emit a clear warning at validation time pointing at the rename.
- **Auto-`--pull` on derived build:** added to `src/runtime/image.rs`'s `docker build` invocation so cached old `:trixie` digests are refreshed without operator action.
- **agent-smith repo follow-up PR** (separate, in `jackin-project/jackin-agent-smith`):
  - Add `[harness] supported = ["claude", "codex"]` to manifest.
  - Optionally add empty `[codex]` table.
  - No `FROM` change required (in-place tag rebuild handles it).
- **No CHANGELOG update** is prescribed by this spec. Release notes are managed separately at release time by the `jackin-dev:release-notes` flow.

## Testing

### Unit tests

- `src/harness/profile.rs`: every `Harness` variant has a profile; no panic in `match`.
- `src/derived_image.rs`:
  - `render_derived_dockerfile` with `supported = [Claude]` produces only the Claude install block.
  - With `supported = [Claude, Codex]` produces both blocks in stable order.
  - UID/GID rewrite targets `agent`, not `claude`.
  - Entrypoint copy is `/home/agent/entrypoint.sh`.
  - Does NOT set `ENV JACKIN_HARNESS`.
- `src/manifest/mod.rs`:
  - `[harness]` table parses; legacy manifests without it default to `supported = [Claude]`.
  - `supported = ["codex"]` without a `[codex]` table is rejected.
  - Unknown harness names in `supported` are rejected.
- `src/manifest/validate.rs`: warning emitted when harness/path mismatch is detected.
- `src/instance/mod.rs`: `AgentState::prepare` invokes `provision_claude_auth` only when harness is Claude; invokes `provision_codex_auth` only when harness is Codex.
- `src/runtime/launch.rs::harness_mounts`: per-harness mount set assembly is correct for each variant.
- Workspace config: `harness` field parses, defaults to `"claude"` when absent, CLI override wins over workspace.

### Integration tests

- `tests/codex_launch.rs`: full Codex launch with mock `CommandRunner`. Asserts:
  - Image build invoked once, with `--pull`.
  - Derived Dockerfile contains both Claude and Codex install blocks (when both supported).
  - `docker run` argv includes `-e JACKIN_HARNESS=codex` and `-e OPENAI_API_KEY=...`.
  - Codex `config.toml` is written to `~/.jackin/data/jackin-<class>/config.toml`.
  - Container destination mounts include `/home/agent/.codex/config.toml` (RW).
  - Does NOT include `/home/agent/.claude*` mounts.
- `tests/claude_launch.rs` (existing tests, updated): every assertion that mentions `/home/claude` is updated to `/home/agent`. No semantic change.
- `tests/harness_validation.rs` (new): manifest validation paths for `[harness]` table edge cases.

### Entrypoint script tests

- `docker/runtime/entrypoint.sh` is exercised via a test harness (e.g. `bats` or a small Rust integration test that runs the script under bash with stubs):
  - Missing `JACKIN_HARNESS` exits 2.
  - `JACKIN_HARNESS=claude` invokes `install-claude-plugins.sh`.
  - `JACKIN_HARNESS=codex` does NOT invoke `install-claude-plugins.sh`.
  - Pre-launch hook runs in both branches.

### Manual smoke test plan (in PR description)

Pre-requisite for steps 3–5: the agent-smith follow-up branch (with `[harness] supported = ["claude", "codex"]` in its manifest) must be pushed and locally checked out, since the slice's main PR cannot mutate a separate repo. Step 2 works against unmodified agent-smith because legacy manifests default to `supported = ["claude"]`.

1. `docker pull projectjackin/construct:trixie` (verify new digest pulls clean).
2. `cargo run --bin jackin -- load agent-smith --debug` — Claude regression smoke against an unmodified agent-smith manifest (default harness, legacy single-harness path).
3. With agent-smith follow-up branch checked out: `cargo run --bin jackin -- load agent-smith --harness codex --debug` — Codex slice smoke.
4. Verify `~/.jackin/data/jackin-agent-smith/` contains the expected per-harness file set after each launch.
5. `docker exec` into each container, verify `whoami` returns `agent` and `pwd` is `/home/agent`.

## Files touched

Direct edits in this slice:

- `src/harness/mod.rs` (new)
- `src/harness/profile.rs` (new)
- `src/manifest/mod.rs`
- `src/manifest/validate.rs`
- `src/derived_image.rs`
- `src/runtime/launch.rs`
- `src/runtime/image.rs` (add `--pull`)
- `src/runtime/naming.rs` (no behavior change; spot-update of comments)
- `src/instance/mod.rs`
- `src/instance/auth.rs` (Codex provisioning fn added)
- `src/instance/plugins.rs` (still Claude-only; gated by harness check at call site)
- `src/cli/load.rs` (add `--harness` flag)
- `src/cli/config.rs` (CLI help/example strings referencing `/home/claude/...` flip to `/home/agent/...`)
- `src/config/persist.rs` (workspace `harness` field round-trip)
- `src/workspace/resolve.rs` (resolve workspace harness)
- `src/version_check.rs` (Claude-only fns gated by harness check at call site)
- `docker/construct/Dockerfile`
- `docker/construct/install-plugins.sh` → renamed to `install-claude-plugins.sh`
- `docker/runtime/entrypoint.sh`
- `tests/codex_launch.rs` (new)
- `tests/harness_validation.rs` (new)
- existing `tests/*.rs` updated for `/home/agent` paths

Docs touched:

- `docs/src/content/docs/developing/agent-manifest.mdx` — document `[harness]` table.
- `docs/src/content/docs/guides/authentication.mdx` — note Codex env-only auth.
- `docs/src/content/docs/reference/architecture.mdx` — refresh runtime/harness terminology.
- `docs/src/content/docs/developing/creating-agents.mdx` — example multi-harness manifest.
- `docs/src/content/docs/reference/roadmap/multi-runtime-support.mdx` — annotate that the foundation slice has shipped under "harness" terminology; preserve as historical roadmap context.
- `DEPRECATED.md` — `/home/claude` mount paths.

Follow-up PR (separate, `jackin-project/jackin-agent-smith`):

- `jackin.agent.toml` — add `[harness] supported = ["claude", "codex"]`.

## Open questions

- **Codex argv default.** Whether `codex` (bare) or `codex chat` (or another subcommand) is the right interactive launch is decided during implementation against the pinned Codex version. The entrypoint script's `LAUNCH=(codex)` is a placeholder.
- **`OPENAI_API_KEY` vs separate Anthropic vs OpenAI env namespacing.** This slice uses the standard `OPENAI_API_KEY`. If operators commonly want both Claude (subscription) and Codex (key) on the same machine, the existing operator-env mechanism handles namespacing already — but worth re-checking against real operator workflows after the slice lands.
- **Whether the `harness` workspace field should error or warn when set to a value no agent in the workspace supports.** Current spec: hard error at launch. Could be relaxed to a warning if real workflows want to mix.
- **Whether `docker pull` on every derived build adds enough latency to matter.** Likely no; most operators rebuild infrequently. Worth measuring after landing.

## Why this is the right shape

Three forces shaped this design:

1. **Vertical slice over abstraction-first.** A `RuntimeAdapter` trait designed against only Claude and one half-imagined Codex would miss the actual seams. Letting Codex push back on real code produces a smaller, sharper interface.
2. **Workspace-level harness scope keeps the slice atomic.** If harness lived on the agent class, every agent repo would need updating in lockstep. If harness were per-launch only, workspaces would have no harness identity at all. Workspace scope sits at the right altitude for jackin's existing model.
3. **One image with both harnesses installed makes the workspace abstraction feel free.** The alternative — image-per-(class,harness) — meant container-name suffixing, which meant data-directory migration. Each of those was real operator pain that didn't deliver isolation jackin actually needed (since Docker containers are already isolated). One image keeps the mental model simple: an agent class is a unit of tools; a harness is a unit of "which AI runs."

The remaining unavoidable cost is the `agent` user rename. That cost was always going to come due — the roadmap is explicit that Claude-shape leakage into paths is the foundation problem. Paying it once, atomically, in the same slice that adds Codex, is cheaper than paying it twice or leaving it half-done.
