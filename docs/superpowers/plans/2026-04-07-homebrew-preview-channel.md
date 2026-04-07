# Homebrew Preview Channel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Publish a `jackin@preview` Homebrew formula from the latest successful `main` commit and move the Homebrew-facing GitHub organization references from `donbeave` to `jackin-project`.

**Architecture:** Keep the stable and preview channels separate. The stable path continues to use the existing tagged release workflow, but it must target `jackin-project/homebrew-tap`. The preview path adds a new workflow in the `jackin` repo that rewrites `jackin@preview` in the sibling tap repo using a pinned commit tarball and a Ghostty-style version string (`<base>-preview+<shortsha>`).

**Implementation note:** Homebrew does not support a literal `Formula/jackin@preview.rb` file for a non-numeric `@preview` suffix. The implemented tap layout therefore uses `Formula/jackin-preview.rb` as the canonical formula file plus an `Aliases/jackin@preview` symlink so users still install the preview channel as `jackin@preview`.

**Tech Stack:** GitHub Actions, Homebrew formulae, shell scripting, Rust project metadata from `Cargo.toml`, Astro Starlight docs, Bun.

**Assumption:** This plan only migrates GitHub organization references for the `jackin` source repository and the Homebrew tap. It does not change Docker Hub image namespaces or agent repository URLs such as `jackin-agent-smith` until those are confirmed separately.

---

## File Map

- Modify: `Cargo.toml` — update the canonical repository URL for the `jackin` crate.
- Modify: `README.md` — update the source repository URL and Homebrew tap install command.
- Modify: `.github/workflows/release.yml` — retarget the stable Homebrew bump job to `jackin-project/homebrew-tap`.
- Create: `.github/workflows/preview.yml` — publish `jackin@preview` after successful CI on `main`.
- Modify: `docs/astro.config.mjs` — update GitHub and edit-link URLs for docs.
- Modify: `docs/src/content/docs/getting-started/installation.mdx` — update Homebrew/source install instructions and add preview install instructions.
- Modify: `docs/src/content/docs/index.mdx` — update Homebrew quick-start commands and repository links, and add the preview channel command.
- Modify: `../homebrew-tap/Formula/jackin.rb` — update `homepage`, `url`, and `head` to `jackin-project/jackin`.
- Create: `../homebrew-tap/Formula/jackin-preview.rb` — add the rolling preview formula.
- Create: `../homebrew-tap/Aliases/jackin@preview` — expose the preview formula under the user-facing channel name.
- Modify: `../homebrew-tap/README.md` — update the tap namespace and add preview installation instructions.

### Task 1: Migrate `jackin` Homebrew-Facing Org References

**Files:**
- Modify: `Cargo.toml`
- Modify: `README.md`
- Modify: `.github/workflows/release.yml`
- Modify: `docs/astro.config.mjs`
- Modify: `docs/src/content/docs/getting-started/installation.mdx`
- Modify: `docs/src/content/docs/index.mdx`

- [ ] **Step 1: Confirm the old `donbeave` references still exist in the `jackin` repo**

Run:

```bash
rg -n "github.com/donbeave/jackin|donbeave/homebrew-tap|brew tap donbeave/tap|donbeave/tap" \
  Cargo.toml \
  README.md \
  .github/workflows/release.yml \
  docs/astro.config.mjs \
  docs/src/content/docs/getting-started/installation.mdx \
  docs/src/content/docs/index.mdx
```

Expected: matches in all six files.

- [ ] **Step 2: Update the repository URL and stable tap target in the `jackin` repo**

Edit the files to contain these exact fragments:

```toml
# Cargo.toml
repository = "https://github.com/jackin-project/jackin"
```

```yaml
# .github/workflows/release.yml
      - uses: mislav/bump-homebrew-formula-action@v4
        with:
          formula-name: jackin
          homebrew-tap: jackin-project/homebrew-tap
          tag-name: v${{ needs.check-version.outputs.version }}
        env:
          COMMITTER_TOKEN: ${{ secrets.HOMEBREW_TAP_TOKEN }}
```

```js
// docs/astro.config.mjs
      social: [
        { icon: 'github', label: 'GitHub', href: 'https://github.com/jackin-project/jackin' },
      ],
      editLink: {
        baseUrl: 'https://github.com/jackin-project/jackin/edit/main/docs/',
      },
```

````md
<!-- README.md -->
Source code: <https://github.com/jackin-project/jackin>

```sh
brew tap jackin-project/tap
brew install jackin
```
````

- [ ] **Step 3: Update the docs pages with the new org and tap namespace**

Edit the docs content to contain these exact blocks:

````mdx
<!-- docs/src/content/docs/getting-started/installation.mdx -->
  <TabItem label="Homebrew">
    The easiest way to install on macOS or Linux:

    ```bash
    brew tap jackin-project/tap
    brew install jackin
    ```

    The Homebrew formulae are maintained in the [jackin-project/homebrew-tap](https://github.com/jackin-project/homebrew-tap) repository.
  </TabItem>
  <TabItem label="From source">
    Requires Rust 1.87 or newer:

    ```bash
    cargo install --git https://github.com/jackin-project/jackin.git
    ```
  </TabItem>
````

````mdx
<!-- docs/src/content/docs/index.mdx -->
  actions:
    - text: View on GitHub
      link: https://github.com/jackin-project/jackin
      icon: external
      variant: minimal

```bash
# Install via Homebrew
brew tap jackin-project/tap
brew install jackin
```

| [jackin](https://github.com/jackin-project/jackin) | CLI source code |
| [homebrew-tap](https://github.com/jackin-project/homebrew-tap) | Homebrew formulae for installing jackin' |
````

- [ ] **Step 4: Verify no stale `donbeave` Homebrew or repo references remain in the touched `jackin` files**

Run:

```bash
rg -n "github.com/donbeave/jackin|donbeave/homebrew-tap|brew tap donbeave/tap|donbeave/tap" \
  Cargo.toml \
  README.md \
  .github/workflows/release.yml \
  docs/astro.config.mjs \
  docs/src/content/docs/getting-started/installation.mdx \
  docs/src/content/docs/index.mdx
```

Expected: no output.

- [ ] **Step 5: Run the required `jackin` repo verification before committing**

Run:

```bash
cargo clippy && cargo nextest run
cd docs && bun run build
```

Expected: clippy passes with zero warnings promoted to errors, `cargo nextest run` passes, and the docs site builds successfully.

- [ ] **Step 6: Commit the `jackin` repo org migration changes**

Run:

```bash
git add Cargo.toml README.md .github/workflows/release.yml docs/astro.config.mjs docs/src/content/docs/getting-started/installation.mdx docs/src/content/docs/index.mdx
git commit -m "chore: move homebrew publishing to jackin-project"
```

### Task 2: Update the Tap Repo and Add `jackin@preview`

**Files:**
- Modify: `../homebrew-tap/Formula/jackin.rb`
- Create: `../homebrew-tap/Formula/jackin-preview.rb`
- Create: `../homebrew-tap/Aliases/jackin@preview`
- Modify: `../homebrew-tap/README.md`

- [ ] **Step 1: Create a dedicated branch in the sibling tap repo**

Run:

```bash
git -C ../homebrew-tap checkout -b feature/homebrew-preview-channel
```

Expected: Git reports `Switched to a new branch 'feature/homebrew-preview-channel'`.

- [ ] **Step 2: Confirm the stable tap formula and README still point at `donbeave` and that the preview formula and alias do not exist yet**

Run:

```bash
rg -n "donbeave|jackin-project" ../homebrew-tap/Formula/jackin.rb ../homebrew-tap/README.md
test ! -f ../homebrew-tap/Formula/jackin-preview.rb
test ! -L ../homebrew-tap/Aliases/jackin@preview
```

Expected: `rg` shows `donbeave` matches in the stable formula and README, and both `test` commands succeed because the preview formula and alias are not present yet.

- [ ] **Step 3: Update the stable tap formula to the `jackin-project` organization**

Replace `../homebrew-tap/Formula/jackin.rb` with this exact content:

```ruby
class Jackin < Formula
  desc "Matrix-inspired CLI for orchestrating AI coding agents at scale"
  homepage "https://github.com/jackin-project/jackin"
  url "https://github.com/jackin-project/jackin/archive/refs/tags/v0.4.0.tar.gz"
  sha256 "f28a180a1039e15e1525e7e2c0b7b3aa556d3ff15152cb91048aff4cdcff6b95"
  license "Apache-2.0"
  head "https://github.com/jackin-project/jackin.git", branch: "main"

  depends_on "rust" => :build
  depends_on "docker" => :optional

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "jackin", shell_output("#{bin}/jackin --version")
  end
end
```

- [ ] **Step 4: Generate the initial preview formula and alias from the current `origin/main` commit of `jackin`**

Run from the `jackin` repo root:

```bash
git fetch origin main
FULL_SHA=$(git rev-parse origin/main)
SHORT_SHA=$(git rev-parse --short=7 origin/main)
BASE_VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
BASE_VERSION=${BASE_VERSION%-dev}
TARBALL_URL="https://github.com/jackin-project/jackin/archive/${FULL_SHA}.tar.gz"
TMP_TARBALL=$(mktemp /tmp/jackin-preview.XXXXXX.tar.gz)
curl -L "$TARBALL_URL" -o "$TMP_TARBALL"
SHA256=$(shasum -a 256 "$TMP_TARBALL" | awk '{print $1}')
cat > ../homebrew-tap/Formula/jackin-preview.rb <<EOF
class JackinPreview < Formula
  desc "Matrix-inspired CLI for orchestrating AI coding agents at scale"
  homepage "https://github.com/jackin-project/jackin"
  url "${TARBALL_URL}"
  version "${BASE_VERSION}-preview+${SHORT_SHA}"
  sha256 "${SHA256}"
  license "Apache-2.0"

  depends_on "rust" => :build
  depends_on "docker" => :optional

  conflicts_with "jackin", because: "preview and stable install the same binary"

  def install
    system "cargo", "install", *std_cargo_args
  end

  test do
    assert_match "jackin", shell_output("#{bin}/jackin --version")
  end
end
EOF
mkdir -p ../homebrew-tap/Aliases
ln -sfn ../Formula/jackin-preview.rb ../homebrew-tap/Aliases/jackin@preview
rm -f "$TMP_TARBALL"
```

- [ ] **Step 5: Expand the tap README with the new organization and preview instructions**

Replace `../homebrew-tap/README.md` with this exact content:

````md
# Homebrew Tap

Homebrew formulae for [jackin](https://github.com/jackin-project/jackin) and related tools.

## Installation

```sh
brew tap jackin-project/tap
brew install jackin
```

## Preview Channel

```sh
brew install jackin@preview
```

## Updating

```sh
brew update
brew upgrade jackin
brew upgrade jackin@preview
```
````

- [ ] **Step 6: Audit the stable and preview formulas from the tap repo**

Run:

```bash
TAP_PATH=$(python3 - <<'PY'
import os
print(os.path.realpath('../homebrew-tap'))
PY
)
brew untap jackin-project/tap >/dev/null 2>&1 || true
brew tap jackin-project/tap "$TAP_PATH"
brew audit --strict --formula jackin-project/tap/jackin
brew audit --strict --formula jackin-project/tap/jackin@preview
```

Expected: both audits pass.

- [ ] **Step 7: Install the preview formula locally from source to verify the tap works end-to-end**

Run:

```bash
brew install --build-from-source jackin-project/tap/jackin@preview
jackin --version
```

Expected: Homebrew builds `jackin@preview` successfully and `jackin --version` prints a version string containing `jackin`.

- [ ] **Step 8: Commit the tap repo changes**

Run:

```bash
git -C ../homebrew-tap add Formula/jackin.rb Formula/jackin-preview.rb Aliases/jackin@preview README.md
git -C ../homebrew-tap commit -m "feat: add jackin preview formula"
```

### Task 3: Add Preview Automation and Final Docs Updates in `jackin`

**Files:**
- Create: `.github/workflows/preview.yml`
- Modify: `README.md`
- Modify: `docs/src/content/docs/getting-started/installation.mdx`
- Modify: `docs/src/content/docs/index.mdx`

- [ ] **Step 1: Confirm the preview workflow does not exist yet**

Run:

```bash
test ! -f .github/workflows/preview.yml
```

Expected: success.

- [ ] **Step 2: Create `.github/workflows/preview.yml` with the exact preview publication flow**

Write this exact file:

```yaml
name: Publish Homebrew Preview

on:
  workflow_run:
    workflows: ["CI"]
    types: [completed]
    branches: [main]
  workflow_dispatch:

permissions:
  contents: read

jobs:
  publish-preview:
    if: github.event_name == 'workflow_dispatch' || github.event.workflow_run.conclusion == 'success'
    runs-on: ubuntu-latest
    steps:
      - name: Resolve source SHA
        id: source
        run: |
          if [ "${{ github.event_name }}" = "workflow_dispatch" ]; then
            echo "sha=${GITHUB_SHA}" >> "$GITHUB_OUTPUT"
          else
            echo "sha=${{ github.event.workflow_run.head_sha }}" >> "$GITHUB_OUTPUT"
          fi

      - uses: actions/checkout@v4
        with:
          repository: jackin-project/jackin
          ref: ${{ steps.source.outputs.sha }}

      - name: Compute preview metadata
        id: meta
        run: |
          base_version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
          base_version=${base_version%-dev}
          full_sha="${{ steps.source.outputs.sha }}"
          short_sha=$(printf '%s' "$full_sha" | cut -c1-7)
          tarball_url="https://github.com/jackin-project/jackin/archive/${full_sha}.tar.gz"
          curl -L "$tarball_url" -o jackin-preview.tar.gz
          sha256=$(shasum -a 256 jackin-preview.tar.gz | awk '{print $1}')
          {
            echo "full_sha=$full_sha"
            echo "short_sha=$short_sha"
            echo "version=${base_version}-preview+${short_sha}"
            echo "tarball_url=$tarball_url"
            echo "sha256=$sha256"
          } >> "$GITHUB_OUTPUT"

      - uses: actions/checkout@v4
        with:
          repository: jackin-project/homebrew-tap
          ref: main
          token: ${{ secrets.HOMEBREW_TAP_TOKEN }}
          path: homebrew-tap

      - name: Verify preview alias
        run: |
          test -L homebrew-tap/Aliases/jackin@preview
          test "$(readlink homebrew-tap/Aliases/jackin@preview)" = "../Formula/jackin-preview.rb"

      - name: Rewrite preview formula
        run: |
          cat > homebrew-tap/Formula/jackin-preview.rb <<EOF
          class JackinPreview < Formula
            desc "Matrix-inspired CLI for orchestrating AI coding agents at scale"
            homepage "https://github.com/jackin-project/jackin"
            url "${{ steps.meta.outputs.tarball_url }}"
            version "${{ steps.meta.outputs.version }}"
            sha256 "${{ steps.meta.outputs.sha256 }}"
            license "Apache-2.0"

            depends_on "rust" => :build
            depends_on "docker" => :optional

            conflicts_with "jackin", because: "preview and stable install the same binary"

            def install
              system "cargo", "install", *std_cargo_args
            end

            test do
              assert_match "jackin", shell_output("#{bin}/jackin --version")
            end
          end
          EOF

      - name: Commit and push tap update
        working-directory: homebrew-tap
        run: |
          if git diff --quiet -- Formula/jackin-preview.rb; then
            echo "Preview formula already up to date"
            exit 0
          fi
          git config user.name "github-actions[bot]"
          git config user.email "41898282+github-actions[bot]@users.noreply.github.com"
          git add Formula/jackin-preview.rb
          git commit -m "jackin@preview ${{ steps.meta.outputs.version }}"
          git push
```

- [ ] **Step 3: Update the top-level README and docs to advertise the preview channel**

Edit the files so they contain these exact fragments:

````md
<!-- README.md -->
```sh
brew tap jackin-project/tap
brew install jackin

# Rolling preview channel
brew install jackin@preview
```
````

````mdx
<!-- docs/src/content/docs/getting-started/installation.mdx -->
    Install the rolling preview channel with:

    ```bash
    brew install jackin@preview
    ```
````

````mdx
<!-- docs/src/content/docs/index.mdx -->
```bash
# Install via Homebrew
brew tap jackin-project/tap
brew install jackin

# Or install the rolling preview channel
brew install jackin@preview
```
````

- [ ] **Step 4: Parse the new workflow and verify the expected keys and tap target are present**

Run:

```bash
ruby -e 'require "yaml"; workflow = YAML.load_file(".github/workflows/preview.yml"); puts workflow.fetch("jobs").keys'
rg -n "jackin-project/homebrew-tap|workflow_run|jackin@preview|preview\+" .github/workflows/preview.yml .github/workflows/release.yml
```

Expected: Ruby prints `publish-preview`, and `rg` shows the `jackin-project/homebrew-tap` target plus the preview workflow markers.

- [ ] **Step 5: Run the required `jackin` repo verification before committing the workflow and docs**

Run:

```bash
cargo clippy && cargo nextest run
cd docs && bun run build
```

Expected: all commands pass.

- [ ] **Step 6: Commit the preview workflow and docs changes**

Run:

```bash
git add .github/workflows/preview.yml README.md docs/src/content/docs/getting-started/installation.mdx docs/src/content/docs/index.mdx
git commit -m "feat: publish homebrew preview channel"
```

### Task 4: Final Cross-Repo Verification

**Files:**
- Verify only: `.github/workflows/release.yml`
- Verify only: `.github/workflows/preview.yml`
- Verify only: `../homebrew-tap/Formula/jackin.rb`
- Verify only: `../homebrew-tap/Formula/jackin-preview.rb`
- Verify only: `../homebrew-tap/Aliases/jackin@preview`
- Verify only: `../homebrew-tap/README.md`

- [ ] **Step 1: Verify the stable and preview tap files now point to `jackin-project`**

Run:

```bash
rg -n "github.com/jackin-project/jackin|jackin-project/homebrew-tap|jackin-project/tap" \
  .github/workflows/release.yml \
  .github/workflows/preview.yml \
  README.md \
  docs/src/content/docs/getting-started/installation.mdx \
  docs/src/content/docs/index.mdx \
  ../homebrew-tap/Formula/jackin.rb \
  ../homebrew-tap/Formula/jackin-preview.rb \
  ../homebrew-tap/Aliases/jackin@preview \
  ../homebrew-tap/README.md
```

Expected: matches in all intended files, all using `jackin-project`, with no `donbeave` output in this subset.

- [ ] **Step 2: Re-run the tap audits after all commits are in place**

Run:

```bash
brew audit --strict --formula jackin-project/tap/jackin
brew audit --strict --formula jackin-project/tap/jackin@preview
```

Expected: both pass.

- [ ] **Step 3: Confirm both repos are clean after their commits**

Run:

```bash
git status --short
git -C ../homebrew-tap status --short
```

Expected: no output from either command.
