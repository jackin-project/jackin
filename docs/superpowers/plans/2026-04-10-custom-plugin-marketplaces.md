# Custom Plugin Marketplaces Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `[[claude.marketplaces]]` support to `jackin.agent.toml` so jackin can add custom Claude plugin marketplaces, including optional `sparse` paths, before installing manifest-declared plugins.

**Architecture:** Keep `jackin` as a thin wrapper over Claude Code’s existing CLI flow. Rust handles manifest parsing and runtime-state serialization, while the existing container bootstrap script consumes that state and runs `claude plugin marketplace add ...` followed by `claude plugin install ...` in order.

**Tech Stack:** Rust, serde, serde_json, TOML, bash, jq-compatible JSON parsing, Vocs/MDX docs

**Spec:** [docs/superpowers/specs/2026-04-10-custom-plugin-marketplaces-design.md](file:///Users/donbeave/Projects/jackin-project/jackin/docs/superpowers/specs/2026-04-10-custom-plugin-marketplaces-design.md)

---

## File Structure

```text
src/manifest.rs                         # Manifest schema, marketplace config type, parsing tests
src/instance.rs                         # Runtime bootstrap JSON serialization and tests
docker/construct/install-plugins.sh     # Startup bootstrap for marketplace add + plugin install
tests/install_plugins_bootstrap.rs      # Regression test for shell bootstrap command ordering and sparse flags
docs/pages/developing/agent-manifest.mdx  # Manifest schema docs and examples
docs/pages/developing/creating-agents.mdx # Agent authoring example with custom marketplace
todo/custom-plugin-marketplace.md       # Mark TODO item resolved with required sections
TODO.md                                 # Move item from Open Items to Resolved
docs/pages/reference/roadmap.mdx        # Reflect completed marketplace support in roadmap
```

---

### Task 1: Extend the Manifest Schema for Marketplace Blocks

**Files:**
- Modify: `src/manifest.rs`

- [ ] **Step 1: Write the failing manifest parsing tests**

Add these tests near the existing manifest tests in `src/manifest.rs`:

```rust
    #[test]
    fn loads_manifest_with_marketplaces_and_plugins() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = ["superpowers@superpowers-marketplace"]

[[claude.marketplaces]]
source = "obra/superpowers-marketplace"
sparse = ["plugins", ".claude-plugin"]
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();

        assert_eq!(manifest.claude.plugins, vec!["superpowers@superpowers-marketplace"]);
        assert_eq!(manifest.claude.marketplaces.len(), 1);
        assert_eq!(
            manifest.claude.marketplaces[0].source,
            "obra/superpowers-marketplace"
        );
        assert_eq!(
            manifest.claude.marketplaces[0].sparse,
            vec!["plugins", ".claude-plugin"]
        );
    }

    #[test]
    fn loads_manifest_without_marketplaces_defaults_to_empty() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();

        assert!(manifest.claude.marketplaces.is_empty());
    }

    #[test]
    fn loads_manifest_marketplace_without_sparse_defaults_to_empty() {
        let temp = tempdir().unwrap();
        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = []

[[claude.marketplaces]]
source = "donbeave/jackin-marketplace"
"#,
        )
        .unwrap();

        let manifest = AgentManifest::load(temp.path()).unwrap();

        assert_eq!(manifest.claude.marketplaces.len(), 1);
        assert!(manifest.claude.marketplaces[0].sparse.is_empty());
    }
```

- [ ] **Step 2: Run the targeted tests and verify they fail for the right reason**

Run:

```bash
cargo test loads_manifest_with_marketplaces_and_plugins -- --exact
cargo test loads_manifest_without_marketplaces_defaults_to_empty -- --exact
cargo test loads_manifest_marketplace_without_sparse_defaults_to_empty -- --exact
```

Expected: at least the first test fails because `ClaudeConfig` does not yet accept `marketplaces`, producing a TOML unknown-field or missing-field assertion failure.

- [ ] **Step 3: Implement the minimal manifest schema changes**

Update `src/manifest.rs` so the marketplace block is part of the existing manifest model:

```rust
use serde::{Deserialize, Serialize};
```

```rust
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ClaudeMarketplaceConfig {
    pub source: String,
    #[serde(default)]
    pub sparse: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClaudeConfig {
    #[serde(default)]
    pub marketplaces: Vec<ClaudeMarketplaceConfig>,
    #[serde(default)]
    pub plugins: Vec<String>,
}
```

Keep `marketplaces` and `plugins` both additive and default-empty so existing manifests remain valid.

- [ ] **Step 4: Re-run the targeted tests and verify they pass**

Run:

```bash
cargo test loads_manifest_with_marketplaces_and_plugins -- --exact
cargo test loads_manifest_without_marketplaces_defaults_to_empty -- --exact
cargo test loads_manifest_marketplace_without_sparse_defaults_to_empty -- --exact
```

Expected: all three tests pass.

- [ ] **Step 5: Commit the manifest schema change**

```bash
git add src/manifest.rs
git commit -m "feat: add manifest support for Claude marketplaces"
```

---

### Task 2: Persist Marketplace Data into Runtime Bootstrap State

**Files:**
- Modify: `src/instance.rs`

- [ ] **Step 1: Write the failing runtime state test**

Replace the existing JSON bootstrap assertion in `src/instance.rs` with a marketplace-aware test:

```rust
    #[test]
    fn prepares_plugins_json_with_marketplaces_for_runtime_bootstrap() {
        let temp = tempdir().unwrap();
        let paths = JackinPaths::for_tests(temp.path());

        std::fs::write(
            temp.path().join("jackin.agent.toml"),
            r#"dockerfile = "Dockerfile"

[claude]
plugins = ["superpowers@superpowers-marketplace"]

[[claude.marketplaces]]
source = "obra/superpowers-marketplace"
sparse = ["plugins", ".claude-plugin"]
"#,
        )
        .unwrap();
        std::fs::write(
            temp.path().join("Dockerfile"),
            "FROM projectjackin/construct:trixie\n",
        )
        .unwrap();

        let manifest = crate::manifest::AgentManifest::load(temp.path()).unwrap();
        let state = AgentState::prepare(&paths, "jackin-agent-smith", &manifest).unwrap();

        assert_eq!(
            std::fs::read_to_string(&state.plugins_json).unwrap(),
            r#"{
  "marketplaces": [
    {
      "source": "obra/superpowers-marketplace",
      "sparse": [
        "plugins",
        ".claude-plugin"
      ]
    }
  ],
  "plugins": [
    "superpowers@superpowers-marketplace"
  ]
}"#
        );
    }
```

- [ ] **Step 2: Run the targeted test and verify it fails**

Run:

```bash
cargo test prepares_plugins_json_with_marketplaces_for_runtime_bootstrap -- --exact
```

Expected: failure because the serialized JSON still contains only the `plugins` array.

- [ ] **Step 3: Implement the minimal runtime-state serialization change**

Update `src/instance.rs` so the mounted bootstrap file includes marketplace objects:

```rust
use crate::manifest::{AgentManifest, ClaudeMarketplaceConfig};
```

```rust
#[derive(Debug, Serialize)]
struct PluginState<'a> {
    marketplaces: &'a [ClaudeMarketplaceConfig],
    plugins: &'a [String],
}
```

```rust
        std::fs::write(
            &plugins_json,
            serde_json::to_string_pretty(&PluginState {
                marketplaces: &manifest.claude.marketplaces,
                plugins: &manifest.claude.plugins,
            })?,
        )?;
```

- [ ] **Step 4: Re-run the targeted test and verify it passes**

Run:

```bash
cargo test prepares_plugins_json_with_marketplaces_for_runtime_bootstrap -- --exact
```

Expected: PASS.

- [ ] **Step 5: Commit the runtime-state change**

```bash
git add src/instance.rs
git commit -m "feat: persist marketplace bootstrap state"
```

---

### Task 3: Add Marketplace Bootstrap Commands and Cover Them with a Regression Test

**Files:**
- Create: `tests/install_plugins_bootstrap.rs`
- Modify: `docker/construct/install-plugins.sh`

- [ ] **Step 1: Write the failing integration test for the bootstrap script**

Create `tests/install_plugins_bootstrap.rs`:

```rust
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn install_plugins_script_adds_marketplaces_before_installing_plugins() {
    let temp = tempdir().unwrap();
    let plugins_file = temp.path().join("plugins.json");
    fs::write(
        &plugins_file,
        r#"{
  "marketplaces": [
    {
      "source": "obra/superpowers-marketplace",
      "sparse": ["plugins", ".claude-plugin"]
    },
    {
      "source": "donbeave/jackin-marketplace",
      "sparse": []
    }
  ],
  "plugins": [
    "superpowers@superpowers-marketplace",
    "jackin-dev@jackin-marketplace"
  ]
}"#,
    )
    .unwrap();

    let bin_dir = temp.path().join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let log_file = temp.path().join("claude.log");

    let claude_path = bin_dir.join("claude");
    fs::write(
        &claude_path,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\n",
            log_file.display()
        ),
    )
    .unwrap();
    let mut claude_perms = fs::metadata(&claude_path).unwrap().permissions();
    claude_perms.set_mode(0o755);
    fs::set_permissions(&claude_path, claude_perms).unwrap();

    let jq_path = bin_dir.join("jq");
    fs::write(
        &jq_path,
        r#"#!/usr/bin/env python3
import json
import sys

args = sys.argv[1:]
if args and args[0] == "-r":
    args = args[1:]
flt = args[0]
if len(args) > 1:
    with open(args[1], "r", encoding="utf-8") as fh:
        data = json.load(fh)
else:
    data = json.load(sys.stdin)

if flt == ".marketplaces[]?":
    for item in data.get("marketplaces", []):
        print(json.dumps(item))
elif flt == ".plugins[]?":
    for item in data.get("plugins", []):
        print(item)
elif flt == ".source":
    print(data["source"])
elif flt == ".sparse[]?":
    for item in data.get("sparse", []):
        print(item)
else:
    raise SystemExit(f"unsupported filter: {flt}")
"#,
    )
    .unwrap();
    let mut jq_perms = fs::metadata(&jq_path).unwrap().permissions();
    jq_perms.set_mode(0o755);
    fs::set_permissions(&jq_path, jq_perms).unwrap();

    let status = Command::new("bash")
        .arg("docker/construct/install-plugins.sh")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("JACKIN_PLUGINS_FILE", &plugins_file)
        .env(
            "PATH",
            format!("{}:{}", bin_dir.display(), std::env::var("PATH").unwrap()),
        )
        .status()
        .unwrap();

    assert!(status.success());
    assert_eq!(
        fs::read_to_string(log_file).unwrap(),
        "plugin marketplace add anthropics/claude-plugins-official\n\
plugin marketplace add obra/superpowers-marketplace --sparse plugins --sparse .claude-plugin\n\
plugin marketplace add donbeave/jackin-marketplace\n\
plugin install superpowers@superpowers-marketplace\n\
plugin install jackin-dev@jackin-marketplace\n"
    );
}
```

- [ ] **Step 2: Run the integration test and verify it fails**

Run:

```bash
cargo test install_plugins_script_adds_marketplaces_before_installing_plugins --test install_plugins_bootstrap -- --exact
```

Expected: FAIL because `install-plugins.sh` neither reads the test override path nor emits custom marketplace add commands.

- [ ] **Step 3: Implement the bootstrap script changes with the smallest test seam**

Update `docker/construct/install-plugins.sh` so it can be tested outside the container and so it adds marketplaces before installing plugins:

```bash
plugins_file="${JACKIN_PLUGINS_FILE:-/home/claude/.jackin/plugins.json}"
```

```bash
jq -c '.marketplaces[]?' "$plugins_file" | while IFS= read -r marketplace; do
    [ -n "$marketplace" ] || continue
    source=$(printf '%s' "$marketplace" | jq -r '.source')
    args=(claude plugin marketplace add "$source")
    while IFS= read -r sparse; do
        [ -n "$sparse" ] || continue
        args+=(--sparse "$sparse")
    done < <(printf '%s' "$marketplace" | jq -r '.sparse[]?')
    run_maybe_quiet "${args[@]}"
done

jq -r '.plugins[]?' "$plugins_file" | while IFS= read -r plugin; do
    [ -n "$plugin" ] || continue
    run_maybe_quiet claude plugin install "$plugin"
done
```

Keep the existing official marketplace bootstrap line:

```bash
run_maybe_quiet claude plugin marketplace add anthropics/claude-plugins-official || true
```

Do not add extra aliasing or plugin name rewriting logic.

- [ ] **Step 4: Re-run the integration test and verify it passes**

Run:

```bash
cargo test install_plugins_script_adds_marketplaces_before_installing_plugins --test install_plugins_bootstrap -- --exact
```

Expected: PASS, with the logged command order matching official marketplace add, custom marketplace adds, then plugin installs.

- [ ] **Step 5: Commit the bootstrap change**

```bash
git add docker/construct/install-plugins.sh tests/install_plugins_bootstrap.rs
git commit -m "feat: bootstrap custom Claude marketplaces"
```

---

### Task 4: Update Docs and Resolve the TODO Item

**Files:**
- Modify: `docs/pages/developing/agent-manifest.mdx`
- Modify: `docs/pages/developing/creating-agents.mdx`
- Modify: `todo/custom-plugin-marketplace.md`
- Modify: `TODO.md`
- Modify: `docs/pages/reference/roadmap.mdx`

- [ ] **Step 1: Update the manifest reference docs**

In `docs/pages/developing/agent-manifest.mdx`, update the schema examples and the `[claude]` section to document marketplace blocks:

```mdx
[claude]
plugins = ["superpowers@superpowers-marketplace"]

[[claude.marketplaces]]
source = "obra/superpowers-marketplace"
sparse = ["plugins", ".claude-plugin"]
```

Add a `[claude]` field table like this:

```mdx
| Field | Required | Description |
|---|---|---|
| `plugins` | No | List of Claude plugin identifiers to install at runtime |
| `marketplaces` | No | List of marketplace registrations to add before plugin installation |
```

Add a short explanation for each marketplace block:

```mdx
Each `[[claude.marketplaces]]` block maps to `claude plugin marketplace add <source>`.
Use `sparse` to pass one or more `--sparse` paths for monorepo-backed marketplaces.
```

- [ ] **Step 2: Update the agent creation guide with a custom marketplace example**

In `docs/pages/developing/creating-agents.mdx`, replace the plugin example with one that shows both official-only and custom-marketplace usage:

```toml [jackin.agent.toml]
dockerfile = "Dockerfile"

[claude]
plugins = [
  "superpowers@superpowers-marketplace",
  "jackin-dev@jackin-marketplace",
]

[[claude.marketplaces]]
source = "obra/superpowers-marketplace"
sparse = ["plugins", ".claude-plugin"]

[[claude.marketplaces]]
source = "donbeave/jackin-marketplace"

[identity]
name = "My Agent"
```

Then add this sentence directly below the example:

```text
jackin adds each declared marketplace at container startup, then installs each plugin ID exactly as written.
```

- [ ] **Step 3: Mark the TODO item resolved and bring it into TODO-file convention**

Rewrite `todo/custom-plugin-marketplace.md` to include the required sections:

```md
# Custom Plugin Marketplace Support in Agent Config

## Status

Resolved

## Problem

`jackin.agent.toml` could declare Claude plugin IDs, but the runtime bootstrap only added the official Anthropic marketplace. Custom marketplaces such as `donbeave/jackin-marketplace` still required manual `/plugin marketplace add` steps inside the container.

## Why It Matters

Agent repos need reproducible startup. Without manifest-declared marketplaces, project-specific plugins such as `jackin-dev` could not be installed automatically, which made agent setup less portable and less trustworthy.

## Related Files

- `src/manifest.rs`
- `src/instance.rs`
- `docker/construct/install-plugins.sh`
- `docs/pages/developing/agent-manifest.mdx`
- `docs/pages/developing/creating-agents.mdx`
```

- [ ] **Step 4: Move the item from open to resolved indexes**

In `TODO.md`, remove the open-item bullet:

```md
- [Custom Plugin Marketplace Support](todo/custom-plugin-marketplace.md) — auto-install GitHub-hosted plugins from `jackin.agent.toml`
```

and add a resolved section:

```md
## Resolved

- [Custom Plugin Marketplace Support](todo/custom-plugin-marketplace.md) — auto-install custom Claude marketplaces and plugins from `jackin.agent.toml`
```

In `docs/pages/reference/roadmap.mdx`, add a completed bullet:

```mdx
- Custom Claude plugin marketplaces in `jackin.agent.toml`
```

- [ ] **Step 5: Commit the docs and TODO updates**

```bash
git add docs/pages/developing/agent-manifest.mdx docs/pages/developing/creating-agents.mdx todo/custom-plugin-marketplace.md TODO.md docs/pages/reference/roadmap.mdx
git commit -m "docs: document custom Claude marketplaces"
```

---

### Task 5: Run Full Verification Before Declaring Success

**Files:**
- Modify: `src/manifest.rs` (formatting only if needed)
- Modify: `src/instance.rs` (formatting only if needed)
- Modify: `tests/install_plugins_bootstrap.rs` (formatting only if needed)

- [ ] **Step 1: Run the formatter**

Run:

```bash
cargo fmt
```

Expected: command succeeds and rewrites Rust files only if formatting is needed.

- [ ] **Step 2: Verify formatting stays clean**

Run:

```bash
cargo fmt -- --check
```

Expected: PASS with no diff-producing formatting complaints.

- [ ] **Step 3: Run clippy**

Run:

```bash
cargo clippy
```

Expected: PASS with zero warnings promoted to failure.

- [ ] **Step 4: Run the full Rust test suite**

Run:

```bash
cargo nextest run
```

Expected: PASS for the existing suite plus the new marketplace coverage.

- [ ] **Step 5: Build the docs site**

Run:

```bash
bun run build
```

Run it from:

```bash
docs/
```

Expected: PASS, proving the updated MDX content still builds.

- [ ] **Step 6: Commit formatting fixes if `cargo fmt` changed tracked files**

If `git status --short` shows formatter-only changes, run:

```bash
git add src/manifest.rs src/instance.rs tests/install_plugins_bootstrap.rs
git commit -m "style: format custom marketplace changes"
```

If `git status --short` is empty, do not create an extra commit.

---

## Self-Review

- The plan covers the spec’s structured `[[claude.marketplaces]]` design, raw `[claude].plugins` strings, per-marketplace `sparse`, runtime-state serialization, startup bootstrap ordering, docs, and TODO/roadmap sync.
- There are no `TODO`, `TBD`, or “implement later” placeholders in the tasks.
- Type names and property names are consistent across tasks: `ClaudeMarketplaceConfig`, `marketplaces`, `source`, and `sparse` are used everywhere.
