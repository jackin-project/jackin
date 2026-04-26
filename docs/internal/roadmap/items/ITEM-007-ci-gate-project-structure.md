# ITEM-007: CI gate for PROJECT_STRUCTURE.md freshness

**Phase:** 1  
**Risk:** low  
**Effort:** small (< 1 day)  
**Requires confirmation:** no  
**Depends on:** ITEM-006 (fix existing staleness first)

## Summary

Add a git-diff-scoped check to `ci.yml` that fails if a PR adds new `.rs` module files without updating `PROJECT_STRUCTURE.md`. This prevents the staleness that accumulated since PR #166.

## Implementation (Option B from research)

Add a step to the `check` job in `.github/workflows/ci.yml`:

```yaml
- name: Check PROJECT_STRUCTURE.md covers new modules
  run: |
    missing=()
    while IFS= read -r f; do
      stem="${f%.rs}"          # strip .rs extension
      dir="${stem%/mod}"        # strip /mod for directory modules
      basename="${dir##*/}"     # just the last path component
      if ! grep -qF "$basename" PROJECT_STRUCTURE.md 2>/dev/null; then
        missing+=("$f")
      fi
    done < <(git diff --name-only origin/main...HEAD -- 'src/**/*.rs')
    if [ ${#missing[@]} -gt 0 ]; then
      printf 'Not mentioned in PROJECT_STRUCTURE.md: %s\n' "${missing[@]}"
      exit 1
    fi
```

## Design decisions

- **Scoped to PR diff only** — checks only files added in the current PR, not all 94 existing files. This avoids requiring retroactive fixes before the gate can be enabled.
- **Greps for basename** (e.g., `op_picker` for `src/console/widgets/op_picker/mod.rs`) because PROJECT_STRUCTURE.md uses prose descriptions containing the module name.
- **False negative risk**: a file named `mod.rs` won't be caught (too generic). Only named modules are checked.

## Steps

1. Add CONTRIBUTING.md step: "When adding a new `.rs` module file, update PROJECT_STRUCTURE.md."
2. Add the YAML step above to `.github/workflows/ci.yml` after the `cargo fmt --check` step.
3. Verify the step passes on the current branch before merging (git diff should show no new .rs files).

## Caveats

- The script uses `git diff --name-only origin/main...HEAD` which requires the repo to have `origin/main` available in CI (already true since `actions/checkout` fetches it for the PR check job).
