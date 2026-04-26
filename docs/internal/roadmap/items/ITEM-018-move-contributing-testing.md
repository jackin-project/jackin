# ITEM-018: Move CONTRIBUTING.md + TESTING.md to docs/internal/

**Phase:** 1  
**Risk:** low  
**Effort:** small (< half day)  
**Requires confirmation:** yes  
**Depends on:** ITEM-005 (Starlight Developer Reference section must exist first)

## Summary

`CONTRIBUTING.md` and `TESTING.md` are contributor-facing files currently at the repo root. They belong in the `docs/internal/` Starlight section where contributors looking for development guidance will find them browsable at `jackin.tailrocks.com/internal/contributing/` and `jackin.tailrocks.com/internal/testing/`.

## Steps

1. Move `CONTRIBUTING.md` → `docs/src/content/docs/internal/contributing.mdx` (convert to MDX, add frontmatter).
2. Move `TESTING.md` → `docs/src/content/docs/internal/testing.mdx`.
3. Update `AGENTS.md` link to `TESTING.md` → new Starlight URL.
4. Add stub redirect files at old paths (or a one-liner pointing to new location) for any external links.
5. Verify `bun run check:links:fresh` passes.

## What needs confirmation

- Whether to add a stub at the old root locations for backward compatibility (e.g., GitHub shows CONTRIBUTING.md from root automatically — if it's gone, the GitHub UI loses it).
- Alternative: keep root-level `CONTRIBUTING.md` as a single-line redirect: "See [docs/src/content/docs/internal/contributing.mdx](docs/src/content/docs/internal/contributing.mdx) for the full contribution guide." This preserves the GitHub UI behavior.

## Risk note

`AGENTS.md` explicitly links to `TESTING.md` at root (`[TESTING.md](TESTING.md)`). This link must be updated. Running `grep -rn "TESTING.md\|CONTRIBUTING.md" .` before moving will find all references.
