# ITEM-019: Greenfield workspace split (future — deferred)

**Phase:** 3 (deferred)  
**Risk:** high  
**Effort:** large (1–2 weeks)  
**Requires confirmation:** yes — major architectural decision  
**Depends on:** ITEM-012, 013, 014, 015 (all Phase 2 splits done first, establishing clean module boundaries)

## Summary

`jackin` today is 43,587L — well below the ~150K threshold where workspace benefits outweigh overhead (matklad's rule of thumb, validated against starship/fd-find at similar scale staying single-crate). This item is **not ready to execute** and is here to capture the target architecture for when the trigger fires.

## Trigger conditions (any one is sufficient)

- LOC exceeds ~150K, OR
- A sub-component (e.g., `jackin-core` domain types) needs external consumers for third-party agent manifest tooling, OR
- Compile times on the CI `check` job exceed 5 minutes cold cache

## Target 6-crate structure (follows matklad's virtual manifest + flat crates/ pattern)

```
jackin/
├── Cargo.toml             ← virtual workspace manifest (no [package])
├── crates/
│   ├── jackin-core/       ← Tier 0: workspace types, manifest, selector, paths, docker trait
│   ├── jackin-config/     ← Tier 1: TOML persistence (AppConfig, ConfigEditor)
│   ├── jackin-tui/        ← Tier 1: terminal output, animation, prompts
│   ├── jackin-runtime/    ← Tier 2: container bootstrap pipeline
│   ├── jackin-console/    ← Tier 3: workspace manager TUI + operator_env
│   └── jackin-shell/      ← ShellRunner (concrete subprocess impl)
├── src/                   ← thin binary (CLI dispatch)
└── validate/              ← jackin-validate binary
```

## Key architectural insight (verified iteration 37)

`console/` has NO import from `runtime/` — this pre-existing clean boundary is the most important structural asset. `jackin-console` and `jackin-runtime` can become separate crates TODAY without breaking anything.

The Phase 2 file splits (ITEM-012 through 015) are pre-work that makes the workspace migration easier by establishing clean internal module boundaries first.

## What needs confirmation

Everything — this is the biggest structural change in the project's history. Confirm the trigger has fired before starting. Confirm the 6-crate boundary decisions against the actual dependency graph at migration time (the graph may have changed since this was analyzed).
