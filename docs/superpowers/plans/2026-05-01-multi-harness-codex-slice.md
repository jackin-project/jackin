# Multi-Harness Foundation — Codex Vertical Slice — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Codex as a second selectable harness alongside Claude in jackin, introducing the `harness` concept (workspace-scoped, with CLI override), the per-harness profile abstraction, and atomically renaming the in-container OS user `claude` → `agent` and `/home/claude` → `/home/agent`. After this slice ships, an operator can run `jackin load agent-smith --harness codex` against an agent class whose manifest declares `[harness] supported = ["claude", "codex"]` and have it launch Codex via `OPENAI_API_KEY`.

**Architecture:** Approach B from the spec — `enum Harness { Claude, Codex }` plus a `HarnessProfile` data struct returned by a single `profile(h)` match, plus small per-harness fns for behavior that can't be reduced to data (auth provisioning). One image per agent class with both harnesses installed; `JACKIN_HARNESS` is passed at `docker run` time so the same image serves either harness. Container/image names are unchanged from today; no host data-dir migration. The `agent` user rename is the only operator-visible breaking change and lands atomically with the rest of the slice.

**Tech Stack:** Rust 1.95.0 (pinned via `rust-toolchain.toml`), `serde` + `toml` for manifest/config, `clap` 4.x for CLI, `cargo-nextest` + `tempfile` for tests, Docker BuildKit (`TARGETARCH` automatic ARG) for multi-arch Codex install, bash for `docker/runtime/entrypoint.sh`.

**Branch:** `feature/multi-harness-codex-slice` (per `BRANCHING.md`).
**Commit style:** Conventional Commits with DCO `Signed-off-by` and `Co-authored-by: <agent> <email>` per `AGENTS.md`. Use `git commit -s` to attach the sign-off; agent attribution trailer is added manually until each agent emits it natively.
**Spec:** `docs/superpowers/specs/2026-05-01-multi-harness-codex-slice-design.md`.

### Assumptions

- `main` is on a state where the spec at the path above is already merged (the design spec PR #204 lands first, this is its implementation).
- `cargo clippy --all-targets -- -D warnings` is currently red on `main` due to lints introduced by the Rust 1.95.0 toolchain bump (~207 errors). Task 1 re-greens clippy before any feature work; if reviewers prefer, that task's commit can be cherry-picked to a precursor PR (see Task 1's Notes).
- The `jackin-project/jackin-agent-smith` repo is unchanged at branch-off; this plan touches it via a small follow-up PR (Task 28). The slice's main PR cannot mutate that repo.
- The construct image `:trixie` will be rebuilt in place. CI has push permissions to `projectjackin/construct:trixie` (existing setup; verify in Task 0.4).

---

## File Structure

| File | Purpose |
| --- | --- |
| `src/harness/mod.rs` *(NEW)* | `Harness` enum, `FromStr`, `slug()`, `Display`, `Serialize`/`Deserialize`. |
| `src/harness/profile.rs` *(NEW)* | `HarnessProfile`, `ContainerStatePaths`, `MountKind`, `profile(h)` fn. |
| `src/lib.rs` | Add `pub mod harness;`. |
| `src/manifest/mod.rs` | Add `HarnessConfig` and `CodexConfig`; make `[claude]` and `[codex]` conditional on `[harness].supported`. |
| `src/manifest/validate.rs` | Validate `[harness].supported` non-empty, harnesses recognized, required tables present, `/home/claude` mount-path warnings. |
| `src/derived_image.rs` | `render_derived_dockerfile` takes `supported: &[Harness]`; targets `agent` user; concatenates per-harness install blocks; entrypoint at `/home/agent/entrypoint.sh`. |
| `src/runtime/launch.rs` | New `harness_mounts(h, &state) -> Vec<String>`; replace `/home/claude/*` mount destinations with `/home/agent/*`; pass `-e JACKIN_HARNESS=<slug>`. |
| `src/runtime/image.rs` | Add `--pull` to `docker build` invocation. |
| `src/instance/mod.rs` | `AgentState::prepare` takes `harness: Harness`, dispatches; add `codex_config_toml: Option<PathBuf>`; gate `plugins.json` write on Claude. |
| `src/instance/auth.rs` | Add `provision_codex_auth` associated fn. |
| `src/instance/plugins.rs` | No changes (call site is gated). |
| `src/cli/agent.rs` | Add `harness: Option<Harness>` to `LoadArgs`; thread through `Command::Load` dispatch sites. |
| `src/cli/dispatch.rs` | Resolve harness (CLI → workspace → claude default) and pass to launch flow. |
| `src/cli/config.rs` | Update `/home/claude` → `/home/agent` in help/example strings. |
| `src/config/mod.rs` | Surface workspace `harness` field in serialization examples / round-trip tests. |
| `src/config/persist.rs` | Round-trip `harness` field in `[workspaces.<name>]`. |
| `src/workspace/mod.rs` | Add `harness: Option<Harness>` to `WorkspaceConfig`. |
| `src/workspace/resolve.rs` | Surface resolved harness for the launch flow. |
| `src/version_check.rs` | Gate Claude-specific version probing on `harness == Claude`. |
| `docker/construct/Dockerfile` | `claude` → `agent` user, `/home/claude` → `/home/agent`, `install-plugins.sh` → `install-claude-plugins.sh`. |
| `docker/construct/zshrc` | Path references `/home/claude` → `/home/agent`. |
| `docker/construct/install-plugins.sh` *(RENAMED)* | → `docker/construct/install-claude-plugins.sh` (no content change). |
| `docker/runtime/entrypoint.sh` | Full rewrite: harness-neutral header, dispatch on `JACKIN_HARNESS`, branches for claude/codex, error on unknown. |
| `tests/codex_launch.rs` *(NEW)* | Integration: full Codex launch with mock `CommandRunner`. |
| `tests/harness_validation.rs` *(NEW)* | Manifest validation edge cases for `[harness]` table. |
| `tests/*.rs` (existing) | Update `/home/claude` → `/home/agent` assertions. |
| `docs/src/content/docs/developing/agent-manifest.mdx` | Document `[harness]` table. |
| `docs/src/content/docs/guides/authentication.mdx` | Codex env-only auth section. |
| `docs/src/content/docs/reference/architecture.mdx` | Refresh terminology to "harness". |
| `docs/src/content/docs/developing/creating-agents.mdx` | Multi-harness manifest example. |
| `docs/src/content/docs/reference/roadmap/multi-runtime-support.mdx` | Annotate slice as shipped under "harness" terminology. |
| `DEPRECATED.md` | `/home/claude` mount destinations entry. |

Files renamed are listed once. No file outside `src/`, `tests/`, `docs/`, `docker/`, or `DEPRECATED.md` is touched.

---

## Preflight

- [ ] **Step 0.1: Ensure clean tree on `main`**

```bash
git fetch origin
git checkout main
git pull --ff-only
git status
```

Expected: `nothing to commit, working tree clean`. If dirty, stop and investigate.

- [ ] **Step 0.2: Confirm spec is on `main`**

```bash
ls docs/superpowers/specs/2026-05-01-multi-harness-codex-slice-design.md
```

Expected: file exists. If not, the design PR (#204) hasn't merged yet. Stop.

- [ ] **Step 0.3: Confirm Rust toolchain pin**

```bash
cat rust-toolchain.toml
rustc --version
```

Expected: toolchain channel is `1.95.0`; `rustc` reports `1.95.0`. If not, run `rustup show` and ensure mise/rustup honor the pin.

- [ ] **Step 0.4: Confirm CI publishes the construct image**

```bash
grep -rn "projectjackin/construct" .github/workflows/ docker-bake.hcl
```

Expected: the construct image is built/pushed by an existing workflow keyed off the `:trixie` tag (or by docker-bake from a workflow). If the publish path is unfamiliar, read the workflow definition before Task 21 (construct rename) lands so the rebuild lights up.

- [ ] **Step 0.5: Create the feature branch**

```bash
git checkout -b feature/multi-harness-codex-slice
```

---

## Task 1 — Re-green clippy on main

**Why this is Task 1.** `cargo clippy --all-targets -- -D warnings` currently fails with ~207 errors due to lints introduced by Rust 1.95.0. Per `COMMITS.md`, every commit must pass clippy with `-D warnings`. The slice cannot land otherwise. This task fixes them all in one commit so subsequent tasks have a clean baseline.

**Notes:** If reviewers prefer this commit on its own PR before the slice, cherry-pick the resulting commit onto a fresh branch and open a precursor PR. The remainder of this plan assumes clippy is green; everything stays the same either way.

**Files:** Many across `src/`. Determined by clippy output.

- [ ] **Step 1.1: Run clippy autofix**

```bash
cargo clippy --fix --all-targets --allow-dirty --allow-staged -- -D warnings 2>&1 | tail -20
```

Expected: many lints auto-fixed. The autofix handles `format!` inlining, `redundant_clone`, `redundant_closure`, `unnecessary_hashes`, doc-backticks (the 95-error category) — the bulk.

- [ ] **Step 1.2: Re-run clippy and inspect remaining errors**

```bash
cargo clippy --all-targets -- -D warnings 2>&1 | tail -60
```

Expected: a small remainder. If clean, skip to Step 1.4.

- [ ] **Step 1.3: Fix remaining lints by hand**

Apply each fix in the file the lint points at. Common manual ones (from current snapshot):

- `binding's name is too similar to existing binding` — rename one of the bindings to a distinct word.
- `pub(crate) function inside private module` — change `pub(crate)` to `pub(super)` or expose the module.
- `this function has too many lines` — extract a helper or add `#[allow(clippy::too_many_lines)]` only if the function is genuinely cohesive.
- `let...else` rewrites — apply mechanically.

After each fix, re-run `cargo clippy --all-targets -- -D warnings` until it exits 0.

- [ ] **Step 1.4: Verify formatting and tests still pass**

```bash
cargo fmt --check
cargo nextest run
```

Expected: both clean.

- [ ] **Step 1.5: Commit**

```bash
git add -A
git commit -s -m "$(cat <<'EOF'
chore(lints): re-green clippy under Rust 1.95.0

Apply cargo clippy --fix for the mechanical lint categories (format!
inlining, redundant clones/closures, unnecessary raw-string hashes,
doc-backticks) and hand-fix the remainder so the workspace builds
cleanly under cargo clippy --all-targets -- -D warnings.

No behavior changes.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 2 — Add `Harness` enum and `slug()`

**Files:**
- Create: `src/harness/mod.rs`
- Modify: `src/lib.rs` (add `pub mod harness;`)
- Test: `src/harness/mod.rs` (inline `#[cfg(test)]`)

- [ ] **Step 2.1: Write the failing tests first**

Append to a new file `src/harness/mod.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

pub mod profile;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Harness {
    Claude,
    Codex,
}

impl Harness {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }
}

impl fmt::Display for Harness {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.slug())
    }
}

#[derive(Debug, thiserror::Error)]
#[error("unknown harness: {got:?}; supported: claude, codex")]
pub struct ParseHarnessError {
    got: String,
}

impl FromStr for Harness {
    type Err = ParseHarnessError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "claude" => Ok(Self::Claude),
            "codex" => Ok(Self::Codex),
            other => Err(ParseHarnessError { got: other.to_string() }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_round_trip() {
        for h in [Harness::Claude, Harness::Codex] {
            assert_eq!(Harness::from_str(h.slug()).unwrap(), h);
        }
    }

    #[test]
    fn display_matches_slug() {
        assert_eq!(format!("{}", Harness::Claude), "claude");
        assert_eq!(format!("{}", Harness::Codex), "codex");
    }

    #[test]
    fn rejects_unknown_harness() {
        let err = Harness::from_str("amp").unwrap_err();
        assert!(err.to_string().contains("amp"));
        assert!(err.to_string().contains("claude"));
    }

    #[test]
    fn serializes_lowercase() {
        let json = serde_json::to_string(&Harness::Claude).unwrap();
        assert_eq!(json, "\"claude\"");
    }

    #[test]
    fn deserializes_lowercase() {
        let h: Harness = serde_json::from_str("\"codex\"").unwrap();
        assert_eq!(h, Harness::Codex);
    }
}
```

Add to `src/lib.rs` at the appropriate location (alphabetical with other `pub mod` lines):

```rust
pub mod harness;
```

`src/harness/profile.rs` will be created in Task 3. To make this task compile, create a stub:

```bash
mkdir -p src/harness
cat > src/harness/profile.rs <<'EOF'
// Stub; populated in Task 3.
EOF
```

- [ ] **Step 2.2: Add `thiserror` to Cargo.toml if not already present**

```bash
grep -n "thiserror" Cargo.toml
```

If absent, add to `[dependencies]`:

```toml
thiserror = "1"
```

(Already present in many jackin modules; if so, this step is a no-op.)

- [ ] **Step 2.3: Run tests; expect them to pass**

```bash
cargo nextest run -p jackin harness::tests
```

Expected: 5 tests pass.

- [ ] **Step 2.4: Verify clippy is still clean**

```bash
cargo clippy --all-targets -- -D warnings
```

Expected: exit 0.

- [ ] **Step 2.5: Commit**

```bash
git add src/harness/ src/lib.rs Cargo.toml
git commit -s -m "$(cat <<'EOF'
feat(harness): add Harness enum and slug

Introduces the Harness type that selects between claude and codex
runtimes. Includes FromStr, Display, Serde lowercase serialization,
and a stub profile submodule populated in the next commit.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 3 — Add `HarnessProfile` data and `profile()` fn

**Files:**
- Modify: `src/harness/profile.rs` (replace stub)
- Test: same file (`#[cfg(test)]`)

- [ ] **Step 3.1: Replace the stub with the profile module**

```rust
use crate::harness::Harness;

/// Per-harness data returned by `profile(harness)`.
///
/// Owned types (not `&'static`) so the profile can grow runtime
/// parameterization later without churning consumers. `required_env`
/// keeps `&'static str` because env-var names are inherent literals.
#[derive(Debug, Clone)]
pub struct HarnessProfile {
    pub install_block: String,
    pub launch_argv: Vec<String>,
    pub required_env: Vec<&'static str>,
    pub installs_plugins: bool,
    pub container_state_paths: ContainerStatePaths,
}

#[derive(Debug, Clone)]
pub struct ContainerStatePaths {
    /// Pairs of (path-relative-to-/home/agent, kind).
    pub home_subpaths: Vec<(String, MountKind)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountKind {
    File,
    Dir,
}

const CLAUDE_INSTALL_BLOCK: &str = "\
USER agent
ARG JACKIN_CACHE_BUST=0
RUN curl -fsSL https://claude.ai/install.sh | bash
RUN claude --version
";

const CODEX_INSTALL_BLOCK: &str = "\
USER agent
ARG TARGETARCH
RUN set -eux; \\
    case \"${TARGETARCH:-amd64}\" in \\
      amd64) ARCH=x86_64-unknown-linux-musl ;; \\
      arm64) ARCH=aarch64-unknown-linux-musl ;; \\
      *) echo \"unsupported arch ${TARGETARCH}\"; exit 1 ;; \\
    esac; \\
    TAG=$(curl -sfIL -o /dev/null -w '%{url_effective}' \\
            https://github.com/openai/codex/releases/latest \\
          | sed 's|.*/tag/||'); \\
    curl -fsSL \"https://github.com/openai/codex/releases/download/${TAG}/codex-${ARCH}.tar.gz\" \\
      | tar -xz -C /usr/local/bin; \\
    chmod +x /usr/local/bin/codex; \\
    mkdir -p /etc/jackin && codex --version > /etc/jackin/codex.version
";

pub fn profile(h: Harness) -> HarnessProfile {
    match h {
        Harness::Claude => HarnessProfile {
            install_block: CLAUDE_INSTALL_BLOCK.to_string(),
            launch_argv: vec![
                "claude".to_string(),
                "--dangerously-skip-permissions".to_string(),
                "--verbose".to_string(),
            ],
            required_env: vec![],
            installs_plugins: true,
            container_state_paths: ContainerStatePaths {
                home_subpaths: vec![
                    (".claude".to_string(), MountKind::Dir),
                    (".claude.json".to_string(), MountKind::File),
                    (".jackin/plugins.json".to_string(), MountKind::File),
                ],
            },
        },
        Harness::Codex => HarnessProfile {
            install_block: CODEX_INSTALL_BLOCK.to_string(),
            launch_argv: vec!["codex".to_string()],
            required_env: vec!["OPENAI_API_KEY"],
            installs_plugins: false,
            container_state_paths: ContainerStatePaths {
                home_subpaths: vec![
                    (".codex/config.toml".to_string(), MountKind::File),
                ],
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_profile_installs_plugins() {
        let p = profile(Harness::Claude);
        assert!(p.installs_plugins);
        assert!(p.required_env.is_empty());
        assert!(p.install_block.contains("claude.ai/install.sh"));
        assert!(p.launch_argv[0] == "claude");
    }

    #[test]
    fn codex_profile_requires_openai_key_and_skips_plugins() {
        let p = profile(Harness::Codex);
        assert!(!p.installs_plugins);
        assert_eq!(p.required_env, vec!["OPENAI_API_KEY"]);
        assert!(p.install_block.contains("openai/codex/releases"));
        assert!(p.install_block.contains("TARGETARCH"));
        assert_eq!(p.launch_argv, vec!["codex"]);
    }

    #[test]
    fn claude_state_paths_match_existing_layout() {
        let p = profile(Harness::Claude);
        let names: Vec<&str> = p
            .container_state_paths
            .home_subpaths
            .iter()
            .map(|(n, _)| n.as_str())
            .collect();
        assert!(names.contains(&".claude"));
        assert!(names.contains(&".claude.json"));
        assert!(names.contains(&".jackin/plugins.json"));
    }

    #[test]
    fn codex_state_paths_only_have_config_toml() {
        let p = profile(Harness::Codex);
        assert_eq!(p.container_state_paths.home_subpaths.len(), 1);
        let (path, kind) = &p.container_state_paths.home_subpaths[0];
        assert_eq!(path, ".codex/config.toml");
        assert_eq!(*kind, MountKind::File);
    }
}
```

- [ ] **Step 3.2: Run tests**

```bash
cargo nextest run -p jackin harness::profile::tests
```

Expected: 4 tests pass.

- [ ] **Step 3.3: Re-check that the parent module still passes**

```bash
cargo nextest run -p jackin harness::
```

Expected: all 9 tests (5 from Task 2 + 4 here) pass.

- [ ] **Step 3.4: Commit**

```bash
git add src/harness/profile.rs
git commit -s -m "$(cat <<'EOF'
feat(harness): add HarnessProfile data and profile() lookup

Adds the per-harness profile struct that downstream code reads instead
of hardcoding harness-specific values. Claude profile mirrors today's
behavior (Claude install + plugin loading); Codex profile defines the
multi-arch install block and the OPENAI_API_KEY requirement.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 4 — Add `[harness]` and `[codex]` tables to manifest schema

**Files:**
- Modify: `src/manifest/mod.rs`
- Test: same file (`#[cfg(test)]`)

- [ ] **Step 4.1: Add the new types**

In `src/manifest/mod.rs`, after the existing `ClaudeConfig` block, add:

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HarnessConfig {
    pub supported: Vec<crate::harness::Harness>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodexConfig {
    /// Optional model override; passed into the generated config.toml
    /// when present, otherwise Codex's own default is used.
    #[serde(default)]
    pub model: Option<String>,
}
```

Update the `AgentManifest` struct to add the two optional fields:

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentManifest {
    pub dockerfile: String,
    #[serde(default)]
    pub identity: Option<IdentityConfig>,
    #[serde(default)]
    pub harness: Option<HarnessConfig>,
    #[serde(default)]
    pub claude: Option<ClaudeConfig>,
    #[serde(default)]
    pub codex: Option<CodexConfig>,
    #[serde(default)]
    pub hooks: Option<HooksConfig>,
    #[serde(default)]
    pub env: BTreeMap<String, EnvVarDecl>,
}
```

(`claude` was previously required; making it `Option` is the schema migration. We will require it in `validate.rs` only when the manifest declares Claude as a supported harness.)

- [ ] **Step 4.2: Add a helper that returns supported harnesses with the legacy default**

Inside `impl AgentManifest`:

```rust
/// Returns the harnesses this manifest supports. Legacy manifests
/// without a `[harness]` table default to claude-only.
pub fn supported_harnesses(&self) -> Vec<crate::harness::Harness> {
    self.harness
        .as_ref()
        .map(|h| h.supported.clone())
        .unwrap_or_else(|| vec![crate::harness::Harness::Claude])
}
```

- [ ] **Step 4.3: Add tests for the new schema**

In the same file's `#[cfg(test)]` module:

```rust
#[test]
fn loads_manifest_with_harness_table() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        r#"dockerfile = "Dockerfile"

[harness]
supported = ["claude", "codex"]

[claude]
plugins = []

[codex]
"#,
    )
    .unwrap();

    let m = AgentManifest::load(temp.path()).unwrap();
    assert_eq!(
        m.supported_harnesses(),
        vec![
            crate::harness::Harness::Claude,
            crate::harness::Harness::Codex
        ]
    );
    assert!(m.codex.is_some());
}

#[test]
fn legacy_manifest_without_harness_table_defaults_to_claude_only() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let m = AgentManifest::load(temp.path()).unwrap();
    assert_eq!(
        m.supported_harnesses(),
        vec![crate::harness::Harness::Claude]
    );
}

#[test]
fn loads_codex_only_manifest() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        r#"dockerfile = "Dockerfile"

[harness]
supported = ["codex"]

[codex]
model = "gpt-5"
"#,
    )
    .unwrap();

    let m = AgentManifest::load(temp.path()).unwrap();
    assert_eq!(
        m.supported_harnesses(),
        vec![crate::harness::Harness::Codex]
    );
    assert_eq!(m.codex.as_ref().unwrap().model.as_deref(), Some("gpt-5"));
}

#[test]
fn rejects_unknown_harness_name() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        r#"dockerfile = "Dockerfile"

[harness]
supported = ["claude", "amp"]

[claude]
plugins = []
"#,
    )
    .unwrap();

    let err = AgentManifest::load(temp.path()).unwrap_err();
    assert!(err.to_string().contains("amp") || err.to_string().contains("unknown"));
}
```

- [ ] **Step 4.4: Run tests**

```bash
cargo nextest run -p jackin manifest::tests
```

Expected: all tests pass (existing ones still pass because `claude` is now `Option<ClaudeConfig>` with `#[serde(default)]`, and the existing test fixtures already include a `[claude]` table).

- [ ] **Step 4.5: Run clippy and fmt**

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

Expected: clean.

- [ ] **Step 4.6: Commit**

```bash
git add src/manifest/mod.rs
git commit -s -m "$(cat <<'EOF'
feat(manifest): add [harness] and [codex] tables

[harness].supported declares which harnesses an agent class can run.
Legacy manifests without [harness] default to claude-only via
supported_harnesses(). The [claude] table becomes optional in the
schema; validation (next commit) requires it when Claude is supported.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 5 — Manifest validation for the `[harness]` table

**Files:**
- Modify: `src/manifest/validate.rs`
- Test: same file

- [ ] **Step 5.1: Read the existing validate.rs to find the right insertion point**

```bash
sed -n '1,40p' src/manifest/validate.rs
grep -n "pub fn\|fn validate" src/manifest/validate.rs
```

Find the top-level validation entry point (typically named `validate_manifest` or invoked from `repo::validate_agent_repo`).

- [ ] **Step 5.2: Add the harness validation function**

Add to `src/manifest/validate.rs`:

```rust
/// Validate the [harness] / [<harness>] table consistency.
///
/// Rules enforced:
/// - If [harness] is present, supported must be non-empty.
/// - For every harness H in supported, the corresponding [H] table
///   must exist (even if empty), so a single grep tells you whether
///   a manifest knows about a given harness.
/// - Without a [harness] table, the manifest must declare [claude]
///   (legacy default).
pub fn validate_harness_consistency(manifest: &AgentManifest) -> anyhow::Result<()> {
    use crate::harness::Harness;

    let supported = manifest.supported_harnesses();

    if let Some(h) = &manifest.harness {
        if h.supported.is_empty() {
            anyhow::bail!("[harness].supported must not be empty");
        }
    }

    for h in &supported {
        match h {
            Harness::Claude => {
                if manifest.claude.is_none() {
                    anyhow::bail!(
                        "[claude] table required when claude is in [harness].supported"
                    );
                }
            }
            Harness::Codex => {
                if manifest.codex.is_none() {
                    anyhow::bail!(
                        "[codex] table required when codex is in [harness].supported"
                    );
                }
            }
        }
    }

    Ok(())
}
```

- [ ] **Step 5.3: Wire it into the existing entry point**

Find the function that runs all manifest validations (e.g. `validate_manifest` or per-step calls in `repo.rs`). Add a call:

```rust
crate::manifest::validate::validate_harness_consistency(&manifest)?;
```

If the existing validate.rs has a single `validate(...)` entry point, append `validate_harness_consistency(&manifest)?;` to it.

- [ ] **Step 5.4: Add tests**

```rust
#[test]
fn rejects_empty_supported_list() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        r#"dockerfile = "Dockerfile"

[harness]
supported = []

[claude]
plugins = []
"#,
    )
    .unwrap();

    let m = AgentManifest::load(temp.path()).unwrap();
    let err = validate_harness_consistency(&m).unwrap_err();
    assert!(err.to_string().contains("must not be empty"));
}

#[test]
fn rejects_codex_supported_without_codex_table() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        r#"dockerfile = "Dockerfile"

[harness]
supported = ["claude", "codex"]

[claude]
plugins = []
"#,
    )
    .unwrap();

    let m = AgentManifest::load(temp.path()).unwrap();
    let err = validate_harness_consistency(&m).unwrap_err();
    assert!(err.to_string().contains("[codex]"));
}

#[test]
fn legacy_manifest_with_claude_passes() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();

    let m = AgentManifest::load(temp.path()).unwrap();
    validate_harness_consistency(&m).unwrap();
}
```

- [ ] **Step 5.5: Run tests, fmt, clippy**

```bash
cargo nextest run -p jackin manifest::
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

- [ ] **Step 5.6: Commit**

```bash
git add src/manifest/validate.rs
git commit -s -m "$(cat <<'EOF'
feat(manifest): validate harness consistency

Enforces non-empty [harness].supported and the presence of the
matching [<harness>] table for every harness an agent class
declares as supported. Legacy manifests (no [harness] table) keep
working as long as [claude] is present.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 6 — Add `harness` field to `WorkspaceConfig`

**Files:**
- Modify: `src/workspace/mod.rs`
- Test: same file

- [ ] **Step 6.1: Add the field**

In `WorkspaceConfig` (around line 37 in the current file), add:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub harness: Option<crate::harness::Harness>,
```

Place after `default_agent` for consistency with adjacent optional fields.

- [ ] **Step 6.2: Add a helper for resolved harness**

Inside `impl WorkspaceConfig` (or as a free fn if the file doesn't have one):

```rust
/// Returns the workspace's selected harness, defaulting to Claude
/// when no harness field is set (legacy workspace).
pub fn resolved_harness(&self) -> crate::harness::Harness {
    self.harness.unwrap_or(crate::harness::Harness::Claude)
}
```

- [ ] **Step 6.3: Add round-trip test**

In `src/workspace/mod.rs` `#[cfg(test)]`:

```rust
#[test]
fn workspace_serializes_harness_when_set() {
    let mut ws = WorkspaceConfig::default();
    ws.workdir = "/tmp/x".to_string();
    ws.harness = Some(crate::harness::Harness::Codex);

    let toml = toml::to_string(&ws).unwrap();
    assert!(toml.contains("harness = \"codex\""));
}

#[test]
fn workspace_omits_harness_field_when_unset() {
    let mut ws = WorkspaceConfig::default();
    ws.workdir = "/tmp/x".to_string();

    let toml = toml::to_string(&ws).unwrap();
    assert!(!toml.contains("harness"));
}

#[test]
fn workspace_resolves_to_claude_when_unset() {
    let mut ws = WorkspaceConfig::default();
    ws.workdir = "/tmp/x".to_string();
    assert_eq!(ws.resolved_harness(), crate::harness::Harness::Claude);
}

#[test]
fn workspace_resolves_to_codex_when_set() {
    let mut ws = WorkspaceConfig::default();
    ws.workdir = "/tmp/x".to_string();
    ws.harness = Some(crate::harness::Harness::Codex);
    assert_eq!(ws.resolved_harness(), crate::harness::Harness::Codex);
}
```

If `WorkspaceConfig` doesn't `derive(Default)`, add it (other adjacent structs do; this is a minor follow-on).

- [ ] **Step 6.4: Run tests, fmt, clippy**

```bash
cargo nextest run -p jackin workspace::tests
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

- [ ] **Step 6.5: Commit**

```bash
git add src/workspace/mod.rs
git commit -s -m "$(cat <<'EOF'
feat(workspace): add harness field to WorkspaceConfig

Optional, defaults to Claude when omitted via resolved_harness().
Serialization skips the field entirely when unset so legacy config
files stay byte-for-byte stable.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 7 — Round-trip workspace harness in config persistence

**Files:**
- Modify: `src/config/persist.rs`
- Modify: `src/config/fixtures/config.round_trip.toml` (add a workspace with harness set)
- Test: existing round-trip test exercises the path

- [ ] **Step 7.1: Verify the round-trip already works**

The fields on `WorkspaceConfig` derive `Serialize`/`Deserialize`, so `persist.rs`'s round-trip should already preserve `harness` if the input contains it. Confirm by examining how `persist.rs` reads/writes workspaces:

```bash
grep -n "harness\|WorkspaceConfig" src/config/persist.rs | head -20
```

If `persist.rs` uses `toml_edit` to surgically rewrite tables (rather than parsing into `WorkspaceConfig` and re-serializing), it may need an explicit branch for the new field. Most jackin config fields use the `toml_edit` upsert pattern; the harness field needs the same.

- [ ] **Step 7.2: Add the harness round-trip to the fixture**

In `src/config/fixtures/config.round_trip.toml`, find an existing `[workspaces.<name>]` block and add `harness = "codex"`:

```toml
[workspaces.prod]
workdir = "/tmp/work"
harness = "codex"
```

- [ ] **Step 7.3: Run the existing round-trip test**

```bash
cargo nextest run -p jackin config::tests::round_trip
```

Expected: passes. If it fails, the persist layer needs the explicit upsert. In that case, add to `persist.rs`:

```rust
// Where workspace fields are upserted in toml_edit:
if let Some(h) = ws.harness {
    table["harness"] = toml_edit::value(h.slug());
} else {
    table.remove("harness");
}
```

Re-run the test until green.

- [ ] **Step 7.4: Commit**

```bash
git add src/config/persist.rs src/config/fixtures/config.round_trip.toml
git commit -s -m "$(cat <<'EOF'
feat(config): persist workspace harness field

Round-trips the new [workspaces.<name>] harness field through the
toml_edit-based persistence layer so saved workspaces preserve the
harness selection across jackin sessions.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 8 — Add `--harness` CLI flag to `LoadArgs`

**Files:**
- Modify: `src/cli/agent.rs`
- Test: same file

- [ ] **Step 8.1: Add the field to `LoadArgs`**

In `src/cli/agent.rs`, find the `LoadArgs` struct and add:

```rust
/// Harness to launch under (claude or codex). Overrides the
/// workspace's `harness` field for this launch only. When neither
/// is set, defaults to claude.
#[arg(long, value_parser = parse_harness)]
pub harness: Option<crate::harness::Harness>,
```

Add a parser fn at the bottom of the file (or wherever helpers live):

```rust
fn parse_harness(s: &str) -> Result<crate::harness::Harness, String> {
    s.parse().map_err(|e: crate::harness::ParseHarnessError| e.to_string())
}
```

- [ ] **Step 8.2: Update every `Command::Load(super::LoadArgs { ... })` construction**

`grep -n "LoadArgs {" src/cli/agent.rs` finds the dispatch sites. Each one already names existing fields; add `harness: None` to each construction (or whatever value is appropriate for that path — most are constructing default args from console flow, so `None`).

- [ ] **Step 8.3: Add CLI parsing test**

```rust
#[test]
fn load_args_parses_harness_flag() {
    use clap::Parser;

    #[derive(Parser)]
    struct TestCli {
        #[command(flatten)]
        args: LoadArgs,
    }

    let parsed = TestCli::parse_from(["test", "agent-smith", "--harness", "codex"]);
    assert_eq!(parsed.args.harness, Some(crate::harness::Harness::Codex));
}

#[test]
fn load_args_rejects_unknown_harness() {
    use clap::Parser;
    #[derive(Parser)]
    struct TestCli {
        #[command(flatten)]
        args: LoadArgs,
    }

    let res = TestCli::try_parse_from(["test", "agent-smith", "--harness", "amp"]);
    assert!(res.is_err());
}

#[test]
fn load_args_harness_optional() {
    use clap::Parser;
    #[derive(Parser)]
    struct TestCli {
        #[command(flatten)]
        args: LoadArgs,
    }

    let parsed = TestCli::parse_from(["test", "agent-smith"]);
    assert_eq!(parsed.args.harness, None);
}
```

- [ ] **Step 8.4: Run tests, fmt, clippy**

```bash
cargo nextest run -p jackin cli::agent::tests
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

- [ ] **Step 8.5: Commit**

```bash
git add src/cli/agent.rs
git commit -s -m "$(cat <<'EOF'
feat(cli): add --harness flag to load command

Optional flag accepting claude or codex. Overrides the workspace's
harness field when provided. Unknown values are rejected at clap
parse time with a clear error.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 9 — Construct image: rename `claude` → `agent`

**Files:**
- Modify: `docker/construct/Dockerfile`
- Modify: `docker/construct/zshrc`
- Rename: `docker/construct/install-plugins.sh` → `docker/construct/install-claude-plugins.sh`

- [ ] **Step 9.1: Rename the plugins script**

```bash
git mv docker/construct/install-plugins.sh docker/construct/install-claude-plugins.sh
```

- [ ] **Step 9.2: Search-and-replace `claude` → `agent` in Dockerfile**

In `docker/construct/Dockerfile`:

```bash
sed -i.bak \
  -e 's|/home/claude|/home/agent|g' \
  -e 's|-o claude -g claude|-o agent -g agent|g' \
  -e 's|--chown=claude:claude|--chown=agent:agent|g' \
  -e 's|install-plugins\.sh|install-claude-plugins.sh|g' \
  docker/construct/Dockerfile
rm docker/construct/Dockerfile.bak
```

Then manually inspect for the user-creation block — the `useradd`/`adduser` line that creates the OS user. It will reference `claude` as the username; change it to `agent`. Same for any `groupadd`. Read the file end-to-end; the search-and-replace above only catches paths and flags, not literal usernames in `useradd` arguments.

```bash
grep -n "claude" docker/construct/Dockerfile
```

Expected after manual edit: zero matches.

- [ ] **Step 9.3: Update zshrc**

```bash
grep -n "claude" docker/construct/zshrc
sed -i.bak 's|/home/claude|/home/agent|g; s|^export CLAUDE|export AGENT|g' docker/construct/zshrc
rm docker/construct/zshrc.bak
grep -n "claude\|CLAUDE" docker/construct/zshrc
```

Manually decide on each remaining match: prompt strings that say "claude" can stay (they're branding); shell paths/env names should be agent-neutral.

- [ ] **Step 9.4: Build the construct image locally to verify**

```bash
docker buildx build \
  --platform linux/amd64 \
  -t projectjackin/construct:trixie-test \
  docker/construct/
```

Expected: builds clean. If the cache picked up an old layer, add `--no-cache`.

- [ ] **Step 9.5: Smoke-test the new construct image**

```bash
docker run --rm projectjackin/construct:trixie-test bash -lc 'whoami; pwd; id; ls -la $HOME | head'
```

Expected: `whoami` → `agent`; `pwd` → `/home/agent`; `id` shows `uid=1000(agent) gid=1000(agent)`; HOME contains `.zshrc`, `install-claude-plugins.sh`, `.claude/` dir.

- [ ] **Step 9.6: Commit**

```bash
git add docker/construct/
git commit -s -m "$(cat <<'EOF'
feat(construct)!: rename claude OS user to agent

Renames the in-container Linux user from claude to agent and home
directory from /home/claude to /home/agent in the construct base
image. Renames install-plugins.sh to install-claude-plugins.sh to
make the script's harness-specific role explicit; the script
itself is unchanged.

The :trixie tag is rebuilt in place; jackin's derived build picks
up the new digest via --pull (added in a later commit).

BREAKING CHANGE: agent classes whose Dockerfiles or workspace
mount config reference /home/claude/... must update to /home/agent.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 10 — Derived image: harness-aware rendering

**Files:**
- Modify: `src/derived_image.rs`
- Test: same file

- [ ] **Step 10.1: Update `render_derived_dockerfile` signature**

Replace the existing function:

```rust
pub fn render_derived_dockerfile(
    base_dockerfile: &str,
    pre_launch_hook: Option<&str>,
    supported: &[crate::harness::Harness],
) -> String {
    use crate::harness::profile::profile;

    let hook_section = pre_launch_hook.map_or_else(String::new, |hook_path| {
        format!(
            "\
USER root
COPY {hook_path} /home/agent/.jackin-runtime/pre-launch.sh
RUN chmod +x /home/agent/.jackin-runtime/pre-launch.sh
USER agent
"
        )
    });

    // Concatenate per-harness install blocks. Claude, when present,
    // MUST come first so its ARG JACKIN_CACHE_BUST invalidates the
    // layer chain downstream into Codex's RUN. The slice's V1
    // invariant is "every agent class supports Claude"; if that ever
    // changes, Codex's profile install_block will need its own
    // ARG JACKIN_CACHE_BUST line.
    let mut install_blocks = String::new();
    let mut sorted: Vec<crate::harness::Harness> = supported.to_vec();
    sorted.sort_by_key(|h| match h {
        crate::harness::Harness::Claude => 0,
        crate::harness::Harness::Codex => 1,
    });
    for h in sorted {
        install_blocks.push_str(&profile(h).install_block);
    }

    format!(
        "\
{base_dockerfile}
USER root
ARG JACKIN_HOST_UID=1000
ARG JACKIN_HOST_GID=1000
RUN current_gid=\"$(id -g agent)\" \\
    && current_uid=\"$(id -u agent)\" \\
    && if [ \"$current_gid\" != \"$JACKIN_HOST_GID\" ]; then \\
         groupmod -o -g \"$JACKIN_HOST_GID\" agent \\
         && usermod -g \"$JACKIN_HOST_GID\" agent; \\
       fi \\
    && if [ \"$current_uid\" != \"$JACKIN_HOST_UID\" ]; then \\
         usermod -o -u \"$JACKIN_HOST_UID\" agent; \\
       fi \\
    && chown -R agent:agent /home/agent
USER agent
{install_blocks}{hook_section}USER root
COPY .jackin-runtime/entrypoint.sh /home/agent/entrypoint.sh
RUN chmod +x /home/agent/entrypoint.sh
USER agent
ENTRYPOINT [\"/home/agent/entrypoint.sh\"]
"
    )
}
```

- [ ] **Step 10.2: Update the single caller in `create_derived_build_context`**

```rust
// In create_derived_build_context, where render_derived_dockerfile is called:
let supported = validated.manifest.supported_harnesses();
std::fs::write(
    &dockerfile_path,
    render_derived_dockerfile(
        &validated.dockerfile.dockerfile_contents,
        pre_launch_hook,
        &supported,
    ),
)?;
```

- [ ] **Step 10.3: Update existing tests**

Existing tests pass two args. Update each `render_derived_dockerfile(...)` call to pass `&[Harness::Claude]` (legacy default). Then add new tests:

```rust
#[test]
fn renders_dockerfile_with_codex_install_when_supported() {
    use crate::harness::Harness;

    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:trixie\n",
        None,
        &[Harness::Claude, Harness::Codex],
    );

    assert!(dockerfile.contains("https://claude.ai/install.sh"));
    assert!(dockerfile.contains("openai/codex/releases"));
    // Claude block precedes Codex (cache-bust ordering).
    let claude_pos = dockerfile.find("claude.ai/install.sh").unwrap();
    let codex_pos = dockerfile.find("openai/codex/releases").unwrap();
    assert!(claude_pos < codex_pos);
}

#[test]
fn renders_codex_only_dockerfile_without_claude_install() {
    use crate::harness::Harness;

    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:trixie\n",
        None,
        &[Harness::Codex],
    );

    assert!(!dockerfile.contains("https://claude.ai/install.sh"));
    assert!(dockerfile.contains("openai/codex/releases"));
}

#[test]
fn renders_dockerfile_targets_agent_user_not_claude() {
    use crate::harness::Harness;

    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:trixie\n",
        None,
        &[Harness::Claude],
    );

    assert!(dockerfile.contains("/home/agent"));
    assert!(dockerfile.contains("groupmod -o -g \"$JACKIN_HOST_GID\" agent"));
    assert!(dockerfile.contains("ENTRYPOINT [\"/home/agent/entrypoint.sh\"]"));
    assert!(!dockerfile.contains("/home/claude"));
}

#[test]
fn renders_dockerfile_does_not_set_jackin_harness_env() {
    use crate::harness::Harness;

    let dockerfile = render_derived_dockerfile(
        "FROM projectjackin/construct:trixie\n",
        None,
        &[Harness::Claude, Harness::Codex],
    );

    assert!(!dockerfile.contains("ENV JACKIN_HARNESS"));
}
```

Update the existing tests that assert `/home/claude` paths to use `/home/agent`.

- [ ] **Step 10.4: Run tests**

```bash
cargo nextest run -p jackin derived_image::tests
```

- [ ] **Step 10.5: Run fmt and clippy**

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

- [ ] **Step 10.6: Commit**

```bash
git add src/derived_image.rs
git commit -s -m "$(cat <<'EOF'
feat(derived-image): harness-aware rendering and agent user

render_derived_dockerfile now takes the manifest's supported
harnesses and concatenates each profile's install_block. Claude is
ordered first so its JACKIN_CACHE_BUST invalidates downstream layers.

UID/GID rewrite, mount targets, and entrypoint path are switched
from /home/claude to /home/agent. JACKIN_HARNESS is intentionally
not baked into the image; it comes at docker run time.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 11 — Add `--pull` to derived `docker build` invocation

**Files:**
- Modify: `src/runtime/image.rs`

- [ ] **Step 11.1: Add `--pull` to the build args**

In `build_agent_image` (currently around line 71), update:

```rust
let mut build_args: Vec<&str> = vec![
    "build",
    "--pull",
    "--build-arg",
    &build_arg_uid,
    "--build-arg",
    &build_arg_gid,
    "--build-arg",
    &cache_bust,
];
```

- [ ] **Step 11.2: Update any test that asserts the build argv**

```bash
grep -n "\"build\"" src/runtime/image.rs tests/*.rs
```

Each test assertion that checks `docker build` args needs to expect `--pull` after `build`.

- [ ] **Step 11.3: Run tests**

```bash
cargo nextest run -p jackin
```

Expected: green. If a snapshot test fails, update the snapshot and re-run.

- [ ] **Step 11.4: Commit**

```bash
git add src/runtime/image.rs tests/
git commit -s -m "$(cat <<'EOF'
feat(image): add --pull to derived builds

Ensures the construct base image is refreshed on every derived
build, so operators with cached old :trixie digests automatically
pick up the agent-user rename without manual intervention.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 12 — Rewrite entrypoint to dispatch on `JACKIN_HARNESS`

**Files:**
- Modify: `docker/runtime/entrypoint.sh`
- Modify: `src/derived_image.rs` (entrypoint test assertions)

- [ ] **Step 12.1: Rewrite the entrypoint**

Replace `docker/runtime/entrypoint.sh` entirely with:

```bash
#!/bin/bash
set -euo pipefail

# Trace all commands in debug mode
if [ "${JACKIN_DEBUG:-0}" = "1" ]; then
    set -x
fi

run_maybe_quiet() {
    if [ "${JACKIN_DEBUG:-0}" = "1" ]; then
        "$@"
    else
        "$@" > /dev/null 2>&1
    fi
}

# ── runtime-neutral setup ──────────────────────────────────────────
# Configure git identity from host environment
if [ -n "${GIT_AUTHOR_NAME:-}" ]; then
    git config --global user.name "$GIT_AUTHOR_NAME"
fi
if [ -n "${GIT_AUTHOR_EMAIL:-}" ]; then
    git config --global user.email "$GIT_AUTHOR_EMAIL"
fi

# Authenticate with GitHub if gh is installed in the container
if [ -x /usr/bin/gh ]; then
    if gh auth status &>/dev/null; then
        echo "[entrypoint] GitHub CLI already authenticated"
        gh auth setup-git
        git config --global url."https://github.com/".insteadOf "git@github.com:"
    else
        echo "[entrypoint] GitHub CLI not authenticated — skipping login (run 'gh auth login' inside the runtime if needed)"
    fi
else
    echo "[entrypoint] GitHub CLI not installed — skipping auth"
fi

# ── harness-specific setup ─────────────────────────────────────────
case "${JACKIN_HARNESS:?JACKIN_HARNESS must be set}" in
  claude)
    run_maybe_quiet /home/agent/install-claude-plugins.sh

    # Register security tool MCP servers (ignore "already exists" on subsequent runs)
    if [[ "${JACKIN_DISABLE_TIRITH:-0}" != "1" ]]; then
        run_maybe_quiet claude mcp add tirith -- tirith mcp-server || true
    else
        echo "[entrypoint] tirith disabled (JACKIN_DISABLE_TIRITH=1)"
    fi
    if [[ "${JACKIN_DISABLE_SHELLFIRM:-0}" != "1" ]]; then
        run_maybe_quiet claude mcp add shellfirm -- shellfirm mcp || true
    else
        echo "[entrypoint] shellfirm disabled (JACKIN_DISABLE_SHELLFIRM=1)"
    fi

    LAUNCH=(claude --dangerously-skip-permissions --verbose)
    ;;
  codex)
    # config.toml is mounted RW from host; no in-container generation needed.
    LAUNCH=(codex)
    ;;
  *)
    echo "[entrypoint] unknown JACKIN_HARNESS: $JACKIN_HARNESS" >&2
    exit 2
    ;;
esac

# ── pre-launch hook (runtime-neutral) ──────────────────────────────
if [ -x /home/agent/.jackin-runtime/pre-launch.sh ]; then
    echo "Running pre-launch hook..."
    /home/agent/.jackin-runtime/pre-launch.sh
fi

# In debug mode, pause so the operator can review logs before the harness clears the screen
if [ "${JACKIN_DEBUG:-0}" = "1" ]; then
    set +x
    echo ""
    echo "[entrypoint] Setup complete. Press Enter to launch ${JACKIN_HARNESS}..."
    read -r
    set -x
fi

printf '\033[2J\033[H'

exec "${LAUNCH[@]}"
```

- [ ] **Step 12.2: Update embedded-entrypoint tests in derived_image.rs**

The existing tests in `src/derived_image.rs` (`entrypoint_does_not_override_claude_env`, `entrypoint_registers_security_tool_mcp_servers`, `entrypoint_mcp_registration_respects_disable_guards`) inspect the `ENTRYPOINT_SH` constant. Update each one:

```rust
#[test]
fn entrypoint_dispatches_on_jackin_harness() {
    assert!(ENTRYPOINT_SH.contains("case \"${JACKIN_HARNESS:?"));
    assert!(ENTRYPOINT_SH.contains("  claude)"));
    assert!(ENTRYPOINT_SH.contains("  codex)"));
}

#[test]
fn entrypoint_claude_branch_invokes_install_claude_plugins() {
    assert!(ENTRYPOINT_SH.contains("/home/agent/install-claude-plugins.sh"));
}

#[test]
fn entrypoint_codex_branch_does_not_invoke_install_claude_plugins() {
    let codex_section = ENTRYPOINT_SH
        .split("codex)")
        .nth(1)
        .unwrap()
        .split(";;")
        .next()
        .unwrap();
    assert!(!codex_section.contains("install-claude-plugins.sh"));
}

#[test]
fn entrypoint_registers_security_tool_mcp_servers_in_claude_branch() {
    let claude_section = ENTRYPOINT_SH
        .split("claude)")
        .nth(1)
        .unwrap()
        .split(";;")
        .next()
        .unwrap();
    assert!(claude_section.contains("claude mcp add tirith"));
    assert!(claude_section.contains("claude mcp add shellfirm"));
}

#[test]
fn entrypoint_mcp_registration_respects_disable_guards() {
    assert!(ENTRYPOINT_SH.contains("JACKIN_DISABLE_TIRITH"));
    assert!(ENTRYPOINT_SH.contains("JACKIN_DISABLE_SHELLFIRM"));
}

#[test]
fn entrypoint_pre_launch_hook_path_uses_agent_home() {
    assert!(ENTRYPOINT_SH.contains("/home/agent/.jackin-runtime/pre-launch.sh"));
    assert!(!ENTRYPOINT_SH.contains("/home/claude"));
}
```

Delete the old tests this replaces.

- [ ] **Step 12.3: Run tests, fmt, clippy**

```bash
cargo nextest run -p jackin derived_image::tests
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

- [ ] **Step 12.4: Commit**

```bash
git add docker/runtime/entrypoint.sh src/derived_image.rs
git commit -s -m "$(cat <<'EOF'
feat(entrypoint): dispatch on JACKIN_HARNESS

Single entrypoint script now branches on JACKIN_HARNESS. The claude
branch keeps today's behavior (install-claude-plugins.sh, MCP server
registration, claude --dangerously-skip-permissions --verbose). The
codex branch execs codex directly. Unknown harness exits 2.

All paths flip from /home/claude to /home/agent.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 13 — `AgentState`: harness dispatch in `prepare`

**Files:**
- Modify: `src/instance/mod.rs`
- Test: same file

- [ ] **Step 13.1: Add the new field and update `prepare`**

Replace the `AgentState` struct and `prepare` impl:

```rust
#[derive(Debug, Clone)]
pub struct AgentState {
    pub root: PathBuf,
    pub claude_dir: PathBuf,
    pub claude_json: PathBuf,
    pub jackin_dir: PathBuf,
    pub plugins_json: PathBuf,
    pub gh_config_dir: PathBuf,
    /// Set only when harness == Codex; the path to the host-side
    /// config.toml that gets mounted at /home/agent/.codex/config.toml.
    pub codex_config_toml: Option<PathBuf>,
}

impl AgentState {
    pub fn prepare(
        paths: &JackinPaths,
        container_name: &str,
        manifest: &AgentManifest,
        auth_forward: AuthForwardMode,
        host_home: &Path,
        harness: crate::harness::Harness,
    ) -> anyhow::Result<(Self, AuthProvisionOutcome)> {
        let root = paths.data_dir.join(container_name);
        let claude_dir = root.join(".claude");
        let claude_json = root.join(".claude.json");
        let jackin_dir = root.join(".jackin");
        let plugins_json = jackin_dir.join("plugins.json");
        let gh_config_dir = root.join(".config/gh");
        let codex_config_toml = root.join("config.toml");

        std::fs::create_dir_all(&claude_dir)?;
        std::fs::create_dir_all(&jackin_dir)?;
        std::fs::create_dir_all(&gh_config_dir)?;

        let outcome = match harness {
            crate::harness::Harness::Claude => {
                let outcome = Self::provision_claude_auth(
                    &claude_json,
                    &claude_dir,
                    auth_forward,
                    host_home,
                )?;

                if let Some(claude_cfg) = manifest.claude.as_ref() {
                    std::fs::write(
                        &plugins_json,
                        serde_json::to_string_pretty(&PluginState {
                            marketplaces: &claude_cfg.marketplaces,
                            plugins: &claude_cfg.plugins,
                        })?,
                    )?;
                }
                outcome
            }
            crate::harness::Harness::Codex => {
                Self::provision_codex_auth(&codex_config_toml, manifest)?;
                AuthProvisionOutcome::Skipped
            }
        };

        let codex_config_toml_field = match harness {
            crate::harness::Harness::Codex => Some(codex_config_toml),
            _ => None,
        };

        Ok((
            Self {
                root,
                claude_dir,
                claude_json,
                jackin_dir,
                plugins_json,
                gh_config_dir,
                codex_config_toml: codex_config_toml_field,
            },
            outcome,
        ))
    }
}
```

- [ ] **Step 13.2: Update existing tests**

The existing test `prepares_persisted_claude_state` now must pass `Harness::Claude` as the last arg. Same for the test in `instance/plugins.rs`.

```bash
grep -n "AgentState::prepare(" src/instance/ tests/
```

Update each call. Also update `instance/plugins.rs` tests where they call `prepare`.

- [ ] **Step 13.3: Add a Codex-path test**

```rust
#[test]
fn prepares_codex_state_writes_config_toml_and_skips_plugins_json() {
    let temp = tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());

    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        r#"dockerfile = "Dockerfile"

[harness]
supported = ["codex"]

[codex]
"#,
    )
    .unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:trixie\n",
    )
    .unwrap();

    let manifest = AgentManifest::load(temp.path()).unwrap();

    let (state, outcome) = AgentState::prepare(
        &paths,
        "jackin-agent-smith",
        &manifest,
        AuthForwardMode::Ignore,
        temp.path(),
        crate::harness::Harness::Codex,
    )
    .unwrap();

    assert_eq!(outcome, AuthProvisionOutcome::Skipped);
    assert!(state.codex_config_toml.is_some());
    assert!(state.codex_config_toml.as_ref().unwrap().is_file());
    // plugins.json is NOT written for codex.
    assert!(!state.plugins_json.exists());
}
```

(The test depends on `provision_codex_auth` from Task 14 being available; if Task 14 is sequenced AFTER this task, write the stub `provision_codex_auth` in this task's commit and fill it in next.)

For ordering simplicity, add a stub to `instance/auth.rs` in this commit:

```rust
impl AgentState {
    pub(super) fn provision_codex_auth(
        config_toml: &std::path::Path,
        _manifest: &crate::manifest::AgentManifest,
    ) -> anyhow::Result<()> {
        std::fs::write(config_toml, "# Generated by jackin; do not edit.\n")?;
        Ok(())
    }
}
```

Real content lands in Task 14.

- [ ] **Step 13.4: Run tests, fmt, clippy**

```bash
cargo nextest run -p jackin instance::
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

- [ ] **Step 13.5: Commit**

```bash
git add src/instance/
git commit -s -m "$(cat <<'EOF'
feat(instance): dispatch state preparation on harness

AgentState::prepare gains a harness parameter and routes:
- Claude: existing behavior (provision_claude_auth + plugins.json).
- Codex: writes a stub config.toml; full content in next commit.

AgentState gains codex_config_toml: Option<PathBuf>, populated only
on the Codex path. Existing call sites updated to pass Harness::Claude.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 14 — `provision_codex_auth`: real config.toml generation

**Files:**
- Modify: `src/instance/auth.rs`
- Test: same file

- [ ] **Step 14.1: Replace the stub with real content generation**

```rust
impl AgentState {
    /// Provision Codex's host-side config.toml. Mounted RW into the
    /// container at /home/agent/.codex/config.toml.
    ///
    /// The generated file sets approval_policy = "never" and
    /// sandbox_mode = "danger-full-access" because jackin's container
    /// is already the operator's trust boundary; Codex's internal
    /// sandbox/approval would add friction without isolation gain.
    pub(super) fn provision_codex_auth(
        config_toml: &std::path::Path,
        manifest: &crate::manifest::AgentManifest,
    ) -> anyhow::Result<()> {
        let mut content = String::from(
            "# Generated by jackin; do not edit.\n\
             approval_policy = \"never\"\n\
             sandbox_mode = \"danger-full-access\"\n",
        );

        if let Some(codex_cfg) = manifest.codex.as_ref() {
            if let Some(model) = &codex_cfg.model {
                content.push_str(&format!("model = \"{model}\"\n"));
            }
        }

        std::fs::write(config_toml, content)?;
        Ok(())
    }
}
```

- [ ] **Step 14.2: Add tests**

```rust
#[cfg(test)]
mod codex_auth_tests {
    use crate::instance::AgentState;
    use crate::manifest::AgentManifest;
    use tempfile::tempdir;

    fn manifest_with_codex_model(temp: &tempfile::TempDir, model: Option<&str>) -> AgentManifest {
        let codex_section = match model {
            Some(m) => format!("[codex]\nmodel = \"{m}\"\n"),
            None => "[codex]\n".to_string(),
        };
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            format!(
                r#"dockerfile = "Dockerfile"

[harness]
supported = ["codex"]

{codex_section}"#
            ),
        )
        .unwrap();
        std::fs::write(
            temp.path().join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();
        AgentManifest::load(temp.path()).unwrap()
    }

    #[test]
    fn provisions_minimal_config_when_no_model() {
        let temp = tempdir().unwrap();
        let manifest = manifest_with_codex_model(&temp, None);
        let path = temp.path().join("config.toml");

        AgentState::provision_codex_auth(&path, &manifest).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("approval_policy = \"never\""));
        assert!(content.contains("sandbox_mode = \"danger-full-access\""));
        assert!(!content.contains("model ="));
    }

    #[test]
    fn provisions_config_with_model_when_set() {
        let temp = tempdir().unwrap();
        let manifest = manifest_with_codex_model(&temp, Some("gpt-5"));
        let path = temp.path().join("config.toml");

        AgentState::provision_codex_auth(&path, &manifest).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("model = \"gpt-5\""));
    }
}
```

- [ ] **Step 14.3: Run tests, fmt, clippy**

```bash
cargo nextest run -p jackin instance::auth
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

- [ ] **Step 14.4: Commit**

```bash
git add src/instance/auth.rs
git commit -s -m "$(cat <<'EOF'
feat(auth): provision Codex config.toml with policy defaults

provision_codex_auth writes approval_policy = "never" and
sandbox_mode = "danger-full-access" since jackin's container is
already the trust boundary. The optional manifest [codex].model
field is threaded into the generated TOML when present.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 15 — `harness_mounts` fn in launch.rs and Claude path migration

**Files:**
- Modify: `src/runtime/launch.rs`
- Test: same file

- [ ] **Step 15.1: Add `harness_mounts`**

Find a sensible insertion point near the existing mount construction (around line 450). Add:

```rust
/// Returns the per-harness mount strings in jackin's "src:dst" /
/// "src:dst:ro" idiom, ready to be passed to `docker run -v`.
fn harness_mounts(
    harness: crate::harness::Harness,
    state: &crate::instance::AgentState,
) -> Vec<String> {
    use crate::harness::Harness;

    match harness {
        Harness::Claude => vec![
            format!("{}:/home/agent/.claude", state.claude_dir.display()),
            format!("{}:/home/agent/.claude.json", state.claude_json.display()),
            format!(
                "{}:/home/agent/.jackin/plugins.json:ro",
                state.plugins_json.display()
            ),
        ],
        Harness::Codex => {
            let path = state
                .codex_config_toml
                .as_ref()
                .expect("codex_config_toml set when harness == Codex");
            vec![format!("{}:/home/agent/.codex/config.toml", path.display())]
        }
    }
}
```

- [ ] **Step 15.2: Replace inline `/home/claude/*` mount construction with the call**

Find the Claude-specific mount block (currently around lines 453-457):

```rust
let claude_dir_mount = format!("{}:/home/claude/.claude", state.claude_dir.display());
let claude_json_mount = format!("{}:/home/claude/.claude.json", state.claude_json.display());
let gh_config_mount = format!("{}:/home/claude/.config/gh", state.gh_config_dir.display());
// ... format!("{}:/home/claude/.jackin/plugins.json:ro", ...);
```

Replace with:

```rust
let harness_specific_mounts = harness_mounts(harness, state);
let gh_config_mount = format!("{}:/home/agent/.config/gh", state.gh_config_dir.display());
```

(`harness` is now a parameter to whatever fn this lives in — typically `prepare_run_args` or similar; it propagates through from the launch entry point.)

- [ ] **Step 15.3: Update every other `/home/claude` reference in launch.rs**

```bash
grep -n "/home/claude" src/runtime/launch.rs
```

Replace each with `/home/agent`. Categories:
- `terminfo` mount (line 212)
- gh config mount destination
- workspace mount destinations (`/home/claude/home`)
- any test fixtures that assert mount strings

- [ ] **Step 15.4: Replace mount-spec consumers downstream**

Where the old mount strings were collected into the `docker run -v ...` invocation, swap in the harness-specific list. Pseudocode (the actual structure of launch.rs varies):

```rust
let mut all_mounts = vec![/* harness-neutral mounts */];
all_mounts.extend(harness_mounts(harness, state));
// ... pass all_mounts to docker run -v <mount> -v <mount> ...
```

- [ ] **Step 15.5: Pass `-e JACKIN_HARNESS=<slug>` to docker run**

Find where env flags are assembled for `docker run` (typically a `Vec<String>` of `-e KEY=value` pairs). Add:

```rust
docker_run_args.push("-e".to_string());
docker_run_args.push(format!("JACKIN_HARNESS={}", harness.slug()));
```

- [ ] **Step 15.6: Add a `harness_mounts` unit test**

```rust
#[test]
fn harness_mounts_for_claude_includes_claude_state() {
    use crate::harness::Harness;
    use crate::instance::AgentState;
    use crate::paths::JackinPaths;

    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let manifest_temp = tempfile::tempdir().unwrap();
    std::fs::write(
        manifest_temp.path().join("jackin.agent.toml"),
        r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();
    std::fs::write(
        manifest_temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:trixie\n",
    )
    .unwrap();
    let manifest = crate::manifest::AgentManifest::load(manifest_temp.path()).unwrap();

    let (state, _) = AgentState::prepare(
        &paths,
        "jackin-agent-smith",
        &manifest,
        crate::config::AuthForwardMode::Ignore,
        temp.path(),
        Harness::Claude,
    )
    .unwrap();

    let mounts = harness_mounts(Harness::Claude, &state);
    assert!(mounts.iter().any(|m| m.contains("/home/agent/.claude:")
        || m.ends_with("/home/agent/.claude")));
    assert!(mounts.iter().any(|m| m.contains("/home/agent/.claude.json")));
    assert!(mounts.iter().any(|m| m.contains("/home/agent/.jackin/plugins.json:ro")));
}

#[test]
fn harness_mounts_for_codex_only_has_config_toml() {
    use crate::harness::Harness;
    use crate::instance::AgentState;
    use crate::paths::JackinPaths;

    let temp = tempfile::tempdir().unwrap();
    let paths = JackinPaths::for_tests(temp.path());
    let manifest_temp = tempfile::tempdir().unwrap();
    std::fs::write(
        manifest_temp.path().join("jackin.agent.toml"),
        r#"dockerfile = "Dockerfile"

[harness]
supported = ["codex"]

[codex]
"#,
    )
    .unwrap();
    std::fs::write(
        manifest_temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:trixie\n",
    )
    .unwrap();
    let manifest = crate::manifest::AgentManifest::load(manifest_temp.path()).unwrap();

    let (state, _) = AgentState::prepare(
        &paths,
        "jackin-agent-smith",
        &manifest,
        crate::config::AuthForwardMode::Ignore,
        temp.path(),
        Harness::Codex,
    )
    .unwrap();

    let mounts = harness_mounts(Harness::Codex, &state);
    assert_eq!(mounts.len(), 1);
    assert!(mounts[0].contains("/home/agent/.codex/config.toml"));
    assert!(!mounts[0].ends_with(":ro"));
}
```

- [ ] **Step 15.7: Update launch.rs's existing snapshot/string-match tests**

```bash
grep -n "/home/claude" src/runtime/launch.rs
```

Each remaining match in test bodies needs updating. The pattern is mechanical: `/home/claude` → `/home/agent`.

- [ ] **Step 15.8: Run tests, fmt, clippy**

```bash
cargo nextest run -p jackin runtime::launch
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

- [ ] **Step 15.9: Commit**

```bash
git add src/runtime/launch.rs
git commit -s -m "$(cat <<'EOF'
feat(launch): harness-aware mount construction

Adds harness_mounts() returning the per-harness mount strings.
Replaces every /home/claude/* mount destination in runtime/launch.rs
with /home/agent/*, both in production code and existing test
assertions. Threads JACKIN_HARNESS=<slug> into the docker run env
flags.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 16 — Wire harness resolution from CLI through workspace into launch

**Files:**
- Modify: `src/cli/dispatch.rs` (or whichever file dispatches `Command::Load` into the runtime layer)
- Modify: `src/runtime/launch.rs` (entry-point fn signature)
- Modify: `src/workspace/resolve.rs` (if needed to surface the resolved harness)

- [ ] **Step 16.1: Find the launch-flow entry point**

```bash
grep -n "fn launch\|pub fn run_load\|Command::Load" src/cli/ src/runtime/ | head
```

The chain is: clap → `Command::Load(LoadArgs)` → cli dispatch → workspace resolution → `runtime::launch::launch_agent` (or similarly named).

- [ ] **Step 16.2: Add the resolution helper**

Where workspace resolution happens (likely `src/cli/dispatch.rs`):

```rust
fn resolve_harness(
    cli_override: Option<crate::harness::Harness>,
    workspace: &crate::workspace::WorkspaceConfig,
) -> crate::harness::Harness {
    cli_override.unwrap_or_else(|| workspace.resolved_harness())
}
```

Add a unit test for the precedence rule:

```rust
#[test]
fn cli_override_wins_over_workspace() {
    use crate::harness::Harness;
    let mut ws = crate::workspace::WorkspaceConfig::default();
    ws.harness = Some(Harness::Claude);
    assert_eq!(resolve_harness(Some(Harness::Codex), &ws), Harness::Codex);
}

#[test]
fn workspace_used_when_cli_absent() {
    use crate::harness::Harness;
    let mut ws = crate::workspace::WorkspaceConfig::default();
    ws.harness = Some(Harness::Codex);
    assert_eq!(resolve_harness(None, &ws), Harness::Codex);
}

#[test]
fn defaults_to_claude_when_neither_set() {
    use crate::harness::Harness;
    let ws = crate::workspace::WorkspaceConfig::default();
    assert_eq!(resolve_harness(None, &ws), Harness::Claude);
}
```

- [ ] **Step 16.3: Validate harness against manifest support**

After resolution, check the manifest:

```rust
let resolved = resolve_harness(args.harness, &workspace);
let supported = manifest.supported_harnesses();
if !supported.contains(&resolved) {
    anyhow::bail!(
        "agent {:?} does not support harness {:?}; supported: {:?}",
        selector.class_name(),
        resolved,
        supported
    );
}
```

- [ ] **Step 16.4: Thread `harness` into `runtime::launch::launch_agent` or equivalent**

Update the entry-point fn signature to accept `harness: crate::harness::Harness` and pass it down to `AgentState::prepare`, `harness_mounts`, and the `docker run` env flag.

- [ ] **Step 16.5: Validate `OPENAI_API_KEY` for Codex**

Either at launch time after env resolution, or via the `HarnessProfile::required_env` field:

```rust
for var in profile(harness).required_env {
    if !resolved_env.vars.contains_key(var) {
        anyhow::bail!(
            "harness {:?} requires {} in operator env; declare it in workspace or global env",
            harness,
            var
        );
    }
}
```

- [ ] **Step 16.6: Run tests, fmt, clippy**

```bash
cargo nextest run -p jackin
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

- [ ] **Step 16.7: Commit**

```bash
git add src/cli/ src/runtime/ src/workspace/
git commit -s -m "$(cat <<'EOF'
feat(launch): resolve harness from CLI/workspace and validate

Threads the resolved harness (CLI flag → workspace harness → claude
fallback) through the launch flow. Validates the resolved harness is
in the agent class manifest's [harness].supported list, and that
profile(harness).required_env keys are present in the resolved
operator env (e.g. OPENAI_API_KEY for codex).

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 17 — Update CLI help/example strings (`/home/claude` → `/home/agent`)

**Files:**
- Modify: `src/cli/config.rs`

- [ ] **Step 17.1: Find and update**

```bash
grep -n "/home/claude" src/cli/config.rs
```

Edit each match: `/home/claude` → `/home/agent`. Two known sites:
- Line 139: `jackin config mount add gradle-cache --src ~/.gradle/caches --dst /home/claude/.gradle/caches --readonly`
- Line 257 (test fixture string): `/home/claude/.gradle/caches`

- [ ] **Step 17.2: Run any associated tests**

```bash
cargo nextest run -p jackin cli::config
```

- [ ] **Step 17.3: Commit**

```bash
git add src/cli/config.rs
git commit -s -m "$(cat <<'EOF'
docs(cli): update mount-config help to use /home/agent

Help text and example fixtures referencing /home/claude flip to
/home/agent to match the renamed in-container user.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 18 — Gate Claude version-probe on harness

**Files:**
- Modify: `src/version_check.rs`
- Modify: `src/runtime/image.rs` (where `needs_claude_update` is called)

- [ ] **Step 18.1: Inspect the call site**

```bash
grep -rn "needs_claude_update\|stored_image_version" src/
```

- [ ] **Step 18.2: Gate the call**

In `src/runtime/image.rs`, find the `needs_claude_update` call and wrap:

```rust
if harness == crate::harness::Harness::Claude {
    if version_check::needs_claude_update(...) {
        // existing rebuild trigger
    }
}
```

(Codex's auto-update is explicitly out of scope per the spec.)

- [ ] **Step 18.3: Run tests, fmt, clippy**

```bash
cargo nextest run -p jackin
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

- [ ] **Step 18.4: Commit**

```bash
git add src/runtime/image.rs src/version_check.rs
git commit -s -m "$(cat <<'EOF'
fix(version-check): gate claude version probe on harness

needs_claude_update queries npm for @anthropic-ai/claude-code; that
probe is meaningless for codex builds. Skip it when harness != Claude.
Codex's update path is --rebuild for V1.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 19 — Codex launch integration test

**Files:**
- Create: `tests/codex_launch.rs`

- [ ] **Step 19.1: Read an existing integration test for the mock harness shape**

```bash
ls tests/
grep -l "CommandRunner\|mock" tests/
```

Pick one (e.g. `tests/launch_smoke.rs` if present) and skim it to see how the mock `CommandRunner` is wired.

- [ ] **Step 19.2: Write the new test file**

```rust
//! Integration test: full Codex launch with mock CommandRunner.

use jackin::config::AuthForwardMode;
use jackin::harness::Harness;

#[test]
fn codex_launch_invokes_docker_run_with_jackin_harness_codex() {
    // ... mirroring the existing claude smoke test, but driving
    // harness = Codex and asserting:
    // - `docker build --pull ...` was called once
    // - the rendered Dockerfile contains both Claude and Codex install blocks
    // - `docker run` argv contains `-e JACKIN_HARNESS=codex`
    // - `docker run` argv contains `-e OPENAI_API_KEY=...` (passthrough)
    // - mount destinations include `/home/agent/.codex/config.toml`
    // - mount destinations do NOT include `/home/agent/.claude*`
    // - host-side `<datadir>/config.toml` was created
    //
    // Implementer: copy the structure from tests/launch_smoke.rs (or
    // whichever existing test exercises CommandRunner mocking),
    // change harness to Codex, swap the assertions accordingly.
}
```

The exact mock-runner pattern depends on jackin's existing test infrastructure. The implementer should mirror an existing claude integration test rather than invent a new mocking style.

- [ ] **Step 19.3: Run the new test**

```bash
cargo nextest run -p jackin --test codex_launch
```

Expected: passes.

- [ ] **Step 19.4: Run the full test suite to catch regressions**

```bash
cargo nextest run -p jackin
```

- [ ] **Step 19.5: Commit**

```bash
git add tests/codex_launch.rs
git commit -s -m "$(cat <<'EOF'
test(integration): full Codex launch path

Mirrors the existing Claude launch integration test but drives
harness = Codex. Asserts docker build --pull, the rendered
Dockerfile contains both Claude and Codex install blocks, the
JACKIN_HARNESS env flag is set to "codex", OPENAI_API_KEY is
forwarded, and only Codex mount destinations are present.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 20 — Manifest validation integration test

**Files:**
- Create: `tests/harness_validation.rs`

- [ ] **Step 20.1: Write the test**

```rust
//! Integration test: manifest [harness] table edge cases.

use jackin::manifest::{AgentManifest, validate::validate_harness_consistency};
use tempfile::tempdir;

#[test]
fn rejects_supported_harness_without_corresponding_table() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        r#"dockerfile = "Dockerfile"

[harness]
supported = ["claude", "codex"]

[claude]
plugins = []
"#,
    )
    .unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:trixie\n",
    )
    .unwrap();

    let m = AgentManifest::load(temp.path()).unwrap();
    let err = validate_harness_consistency(&m).unwrap_err();
    assert!(err.to_string().contains("[codex]"));
}

#[test]
fn legacy_manifest_passes_validation() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:trixie\n",
    )
    .unwrap();

    let m = AgentManifest::load(temp.path()).unwrap();
    validate_harness_consistency(&m).unwrap();
}

#[test]
fn codex_only_manifest_with_codex_table_passes() {
    let temp = tempdir().unwrap();
    std::fs::write(
        temp.path().join("jackin.agent.toml"),
        r#"dockerfile = "Dockerfile"

[harness]
supported = ["codex"]

[codex]
"#,
    )
    .unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:trixie\n",
    )
    .unwrap();

    let m = AgentManifest::load(temp.path()).unwrap();
    validate_harness_consistency(&m).unwrap();
}
```

This will require either making `validate_harness_consistency` `pub` (currently `pub(crate)` based on Task 5's wording — adjust) and re-exporting it from `manifest::mod.rs`, or moving the test into a `#[cfg(test)] mod` inside the crate. The integration-test approach requires `pub` exports.

If exposing `validate_harness_consistency` publicly is undesirable, move these tests into `src/manifest/validate.rs`'s `#[cfg(test)] mod` (where Task 5's tests already live) and skip creating `tests/harness_validation.rs`. Either way is acceptable; the spec lists the test file but the location is a judgment call.

- [ ] **Step 20.2: Run the tests**

```bash
cargo nextest run -p jackin --test harness_validation
# OR if moved inline:
cargo nextest run -p jackin manifest::validate::tests
```

- [ ] **Step 20.3: Commit**

```bash
git add tests/harness_validation.rs src/manifest/
git commit -s -m "$(cat <<'EOF'
test(manifest): integration coverage for [harness] validation

Covers the three flagship cases: legacy claude-only manifest passes,
codex-only manifest with [codex] passes, and mixed supported list
without a matching [codex] table fails clearly.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 21 — Update existing integration tests for `/home/agent`

**Files:**
- Modify: existing files under `tests/`

- [ ] **Step 21.1: Find every assertion**

```bash
grep -rln "/home/claude" tests/
```

- [ ] **Step 21.2: Mass-update**

Per file, replace `/home/claude` → `/home/agent` in test assertions. Read each file before edit; some may have intentional references (e.g. testing legacy migration messaging). Most are mechanical.

- [ ] **Step 21.3: Run the full suite**

```bash
cargo nextest run -p jackin
```

Expected: green.

- [ ] **Step 21.4: Commit**

```bash
git add tests/
git commit -s -m "$(cat <<'EOF'
test: update existing assertions for /home/agent

Mechanical update of all existing integration test assertions that
referenced /home/claude. No behavior change.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 22 — Docs: agent-manifest.mdx

**Files:**
- Modify: `docs/src/content/docs/developing/agent-manifest.mdx`

- [ ] **Step 22.1: Add a `[harness]` section**

After the existing `[claude]` documentation (find with `grep -n "claude" docs/src/content/docs/developing/agent-manifest.mdx`), insert a section documenting:

- The `[harness]` table is optional; legacy manifests default to claude-only.
- `supported` is a list of harness slugs (`"claude"`, `"codex"`).
- Each declared harness must have a corresponding `[<harness>]` table (even if empty).
- Example multi-harness manifest matching agent-smith's expected shape.

Use the existing prose style (Starlight Aside callouts, code-block fences). Reference the spec for further context.

- [ ] **Step 22.2: Build docs and verify**

```bash
cd docs
bun install --frozen-lockfile
bun run build
bun run check:links
```

Expected: clean build, no broken links.

- [ ] **Step 22.3: Commit**

```bash
git add docs/src/content/docs/developing/agent-manifest.mdx
git commit -s -m "$(cat <<'EOF'
docs: document [harness] manifest table

New section in agent-manifest.mdx covering the optional [harness]
table, its supported list semantics, the matching [<harness>] table
requirement, and an example multi-harness manifest.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 23 — Docs: authentication.mdx

**Files:**
- Modify: `docs/src/content/docs/guides/authentication.mdx`

- [ ] **Step 23.1: Add Codex section**

After the Claude auth modes section, add:

- Codex uses env-only auth (`OPENAI_API_KEY`).
- jackin forwards the key from operator env (resolved via the existing workspace/global env mechanism).
- No equivalent of `auth_forward = "sync"` for Codex; `jackin sync` errors out for Codex with a clear message.

- [ ] **Step 23.2: Build, link-check, commit**

```bash
cd docs
bun run build
bun run check:links
cd ..
git add docs/src/content/docs/guides/authentication.mdx
git commit -s -m "$(cat <<'EOF'
docs(auth): document Codex env-only auth

Codex authenticates via OPENAI_API_KEY forwarded from operator env.
jackin sync remains Claude-only and emits a clear error when run
against a Codex harness.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 24 — Docs: architecture.mdx

**Files:**
- Modify: `docs/src/content/docs/reference/architecture.mdx`

- [ ] **Step 24.1: Update terminology**

Find every "runtime" that means "the AI CLI inside the container" and update to "harness". Leave "Docker runtime" and "jackin runtime" alone.

- [ ] **Step 24.2: Add the harness layer to any architecture diagram or text**

If the doc has a layered description (workspace → agent → runtime), make harness a peer of agent class.

- [ ] **Step 24.3: Build, link-check, commit**

```bash
cd docs
bun run build
bun run check:links
cd ..
git add docs/src/content/docs/reference/architecture.mdx
git commit -s -m "$(cat <<'EOF'
docs(architecture): introduce harness layer

Adds the harness concept to the architecture reference and updates
"runtime" to "harness" in the AI-CLI sense throughout. Docker runtime
and jackin runtime references are preserved.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 25 — Docs: creating-agents.mdx and roadmap annotation

**Files:**
- Modify: `docs/src/content/docs/developing/creating-agents.mdx`
- Modify: `docs/src/content/docs/reference/roadmap/multi-runtime-support.mdx`

- [ ] **Step 25.1: Add multi-harness example**

In creating-agents.mdx, after the basic `jackin.agent.toml` example, add a section "Supporting multiple harnesses" with the example manifest from the spec.

- [ ] **Step 25.2: Annotate the roadmap**

In multi-runtime-support.mdx, add a callout near the top:

```mdx
<Aside type="note">
The foundation slice has shipped under "harness" terminology. The roadmap below
preserves "runtime" wording as written for historical context; in code and docs
the concept is now called `harness`. See [creating agents](/developing/creating-agents/)
and [agent manifest](/developing/agent-manifest/) for current usage.
</Aside>
```

- [ ] **Step 25.3: Build, link-check, commit**

```bash
cd docs
bun run build
bun run check:links
cd ..
git add docs/src/content/docs/
git commit -s -m "$(cat <<'EOF'
docs: multi-harness example and roadmap annotation

creating-agents.mdx gains a multi-harness example. The roadmap's
multi-runtime-support page is annotated noting the foundation slice
has shipped under "harness" terminology while preserving the original
roadmap wording.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 26 — DEPRECATED.md

**Files:**
- Modify: `DEPRECATED.md`

- [ ] **Step 26.1: Add the entry**

Append:

```markdown
## `/home/claude/...` mount destinations

**Deprecated:** 2026-05-01 (multi-harness slice).
**Removed:** when no operators report broken paths after one release cycle.
**Migration:** flip mount destinations to `/home/agent/...` in workspace
config (`jackin config mount add ...`) and any agent class Dockerfile that
references the old path. The construct image's `:trixie` tag has been
rebuilt in place; running `docker pull projectjackin/construct:trixie`
once is sufficient to refresh local caches.
```

- [ ] **Step 26.2: Commit**

```bash
git add DEPRECATED.md
git commit -s -m "$(cat <<'EOF'
docs(deprecated): /home/claude mount destinations

Records the deprecation of mount destinations under /home/claude in
favor of /home/agent. Construct image rebuilt in place; operators
get the new shape via docker pull.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

---

## Task 27 — Final verification

**Files:** none (verification only).

- [ ] **Step 27.1: Full pre-commit check**

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo nextest run
```

Expected: all clean.

- [ ] **Step 27.2: Manual smoke test — Claude regression**

In a real shell, against an unmodified copy of `agent-smith` (legacy single-harness path):

```bash
cargo run --bin jackin -- load agent-smith --debug
```

Expected: builds, runs, Claude launches as today. `docker exec` into the container verifies `whoami` → `agent`, `pwd` → `/home/agent`.

- [ ] **Step 27.3: Manual smoke test — Codex slice**

Pre-requisite: agent-smith branch with `[harness] supported = ["claude", "codex"]` checked out locally (Task 28's PR).

```bash
export OPENAI_API_KEY=<your-key>
cargo run --bin jackin -- load agent-smith --harness codex --debug
```

Expected: builds (both Claude and Codex install blocks run), Codex launches, `docker exec` shows `agent` user and `/home/agent`. `~/.jackin/data/jackin-agent-smith/config.toml` exists with the expected approval/sandbox/model content.

- [ ] **Step 27.4: Push and open PR**

```bash
git push -u origin feature/multi-harness-codex-slice
gh pr create --title "feat: multi-harness foundation — Codex vertical slice" --body "$(cat <<'EOF'
## Summary

Implements the multi-harness foundation per the spec at
`docs/superpowers/specs/2026-05-01-multi-harness-codex-slice-design.md`.

- Introduces the `Harness` enum and `HarnessProfile` data layer.
- Adds `[harness]` and `[codex]` manifest tables (legacy claude-only manifests still work).
- Adds workspace-level `harness` field with `--harness` CLI override.
- Renames in-container OS user `claude` → `agent` and `/home/claude` → `/home/agent`.
- Single image per agent class; `JACKIN_HARNESS` passed at `docker run` time.
- Codex install via multi-arch `releases/latest` resolution.
- `provision_codex_auth` writes the host-side `config.toml` mounted RW.

## Breaking change

Mount destinations under `/home/claude/...` are deprecated. Operators must update workspace mount config to `/home/agent/...`. Construct image `:trixie` is rebuilt in place; `--pull` on derived builds picks up the new digest automatically.

## Test plan

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo nextest run`
- [x] Manual: `jackin load agent-smith --debug` (Claude regression smoke)
- [x] Manual: `jackin load agent-smith --harness codex --debug` against an updated agent-smith branch
- [x] `docker exec` verifies `agent` user and `/home/agent` in both runs

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

Stop here per AGENTS.md — do not merge until the operator explicitly confirms.

---

## Task 28 — Follow-up PR: agent-smith manifest

**Repo:** `jackin-project/jackin-agent-smith` (separate repo)
**Branch:** `feat/harness-supported`

This task is intentionally out of band from the slice's main PR because it lives in a different repo.

- [ ] **Step 28.1: Clone or update the agent-smith repo**

```bash
gh repo clone jackin-project/jackin-agent-smith ~/jackin-agent-smith || (cd ~/jackin-agent-smith && git pull --ff-only)
cd ~/jackin-agent-smith
git checkout -b feat/harness-supported
```

- [ ] **Step 28.2: Update `jackin.agent.toml`**

Add the `[harness]` and `[codex]` tables:

```toml
[harness]
supported = ["claude", "codex"]

[codex]
```

- [ ] **Step 28.3: Commit**

```bash
git add jackin.agent.toml
git commit -s -m "$(cat <<'EOF'
feat(manifest): declare claude and codex harness support

Enables agent-smith to launch under either harness via jackin's
multi-harness foundation. Empty [codex] table accepts the runtime's
default model.

Co-authored-by: Claude <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 28.4: Push and open PR**

```bash
git push -u origin feat/harness-supported
gh pr create --title "feat(manifest): declare claude and codex harness support" --body "Updates the agent-smith manifest to opt into both Claude and Codex harnesses now that jackin's multi-harness foundation has shipped."
```

---

## Self-review notes

The plan was reviewed against the spec section-by-section. Coverage map:

| Spec section | Plan task(s) |
| --- | --- |
| Naming and concept | Task 22, 24, 25 (docs); Tasks 2-3 (code) |
| Harness abstraction | Tasks 2-3 |
| Manifest schema | Tasks 4-5 |
| Workspace config | Tasks 6-7 |
| Image identity (one image, JACKIN_HARNESS at run) | Tasks 10, 15 |
| OS user / home rename | Tasks 9, 10, 12, 15, 17, 21 |
| Construct image in-place tag | Task 9 |
| Derived image | Task 10 |
| Entrypoint dispatch | Task 12 |
| State + auth (prepare dispatch, codex auth) | Tasks 13-14 |
| Mount construction (harness_mounts) | Task 15 |
| CLI surface | Tasks 8, 16 |
| Migration / DEPRECATED.md | Task 26 |
| Auto-`--pull` | Task 11 |
| Tests (unit / integration / entrypoint) | Tasks 19-21 (plus inline tests in 2-15) |
| Manual smoke test | Task 27 |
| Docs | Tasks 22-25 |
| agent-smith follow-up | Task 28 |

No spec section is uncovered. Pre-existing main breakage handled by Task 1.

---

**Plan complete and saved to `docs/superpowers/plans/2026-05-01-multi-harness-codex-slice.md`.**
