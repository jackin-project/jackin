# Agent Source Trust Model

**Status**: Deferred — needs design work

## Problem

`resolve_agent_source()` auto-constructs a GitHub URL from namespace/name and clones directly without any trust verification. This exposes a typosquatting and untrusted repo execution risk.

## Why It Matters

- Any namespace/name pair is accepted and cloned without confirmation
- No mechanism to distinguish a trusted, previously-used agent from a novel one
- Agents execute in a build context with access to the Dockerfile and build instructions

## Desired Behavior

A trust-on-first-use model similar to `mise trust`:
- First time an agent source is encountered, clone the repo but prompt the user for confirmation before running it
- Store trusted sources in config (allowlist)
- Subsequent runs of trusted agents proceed without prompts
- Optional security mode: always show agent source output before running, allowing AI agent analysis of the content

## Related Files

- `src/config.rs` — `resolve_agent_source()`, trust store format
- `src/runtime.rs` — calls `resolve_agent_source()` during agent load
- `src/selector.rs` — namespace/name parsing
