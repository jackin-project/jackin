# ITEM-010: Author first Architecture Decision Records (ADRs)

**Phase:** 1  
**Risk:** low  
**Effort:** small (1 day)  
**Requires confirmation:** no (content only — ITEM-005 must exist first for the directory)  
**Depends on:** ITEM-005 (Starlight Developer Reference section)

## Summary

No ADR directory exists. Design context lives only in PR descriptions (not committed) and `docs/superpowers/specs/` (hidden from contributors). Three foundational decisions should be documented as ADRs so future contributors (human and AI) understand why the current architecture exists.

## Three priority ADRs

**ADR-001: Single-crate vs workspace**  
Path: `docs/src/content/docs/internal/decisions/001-single-crate.mdx`  
Decision: stay single-crate while LOC < 150K and no external library consumers. Evidence: starship and fd-find stay single-crate at similar/larger scale. Trigger for workspace: LOC > 150K, or `jackin-core` types needed by external agent manifest tooling. See greenfield architecture in roadmap research for the target 6-crate structure when the trigger fires.

**ADR-002: Rust 1.95.0 toolchain + 1.94 MSRV**  
Path: `docs/src/content/docs/internal/decisions/002-toolchain.mdx`  
Decision: pin to 1.95.0 in mise.toml and CI; declare 1.94 as MSRV in Cargo.toml. Documents the `dtolnay/rust-toolchain` SHA encoding convention (does not read rust-toolchain.toml) and the edition = "2024" floor (≥ 1.85).

**ADR-003: ratatui selection (tui-rs → ratatui)**  
Path: `docs/src/content/docs/internal/decisions/003-ratatui.mdx`  
Decision: use ratatui for all TUI rendering. Documents the tui-rs → ratatui fork history, why ratatui is the correct successor (actively maintained, tui-rs abandoned), and the version currently in use.

## ADR frontmatter format

```yaml
---
title: "ADR-NNN: Title"
status: accepted   # proposed | accepted | superseded | deprecated
date: YYYY-MM-DD
---
```

## Steps

1. Ensure ITEM-005 is done (directory exists).
2. Create the three ADR MDX files with the frontmatter above and prose sections: Context, Decision, Consequences.
3. Keep each ADR to 1–2 pages — decisions, not dissertations.
