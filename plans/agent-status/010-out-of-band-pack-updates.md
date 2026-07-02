# Plan 010: Decouple detection packs from the image — signed out-of-band pack updates (a TUI restyle is a data push, not a release)

> **Executor instructions**: Design + implement the remote-pack channel. This is security-sensitive (packs
> are regex rules matched against untrusted agent output, fetched over a network) — follow the trust model
> exactly. Do plan 011 (extract the detection crate) first so this lives in the crate that owns pack
> provenance. Run every verification command. Update the README row when done.
>
> **Drift check**: `git diff --stat 5d3661cff..HEAD -- crates/jackin-capsule/src/agent_status/rules.rs`
> (after plan 011: `crates/jackin-agent-status/src/rules.rs`)

## Status

- **Priority**: P2
- **Effort**: M–L
- **Risk**: MED (network + trust boundary in a security-focused product)
- **Depends on**: 011 (detection crate — the home for pack loading/verification), 005 (advisory versioning)
- **Category**: direction / detection infrastructure
- **Planned at**: commit `5d3661cff`, 2026-07-03

## Why this matters

Detection packs (the per-agent TOML rules that map screen text → blocked/working/idle) are `include_str!`-baked
into the binary and copied into the derived image. So the **only** way to fix a rule is to rebuild jackin and
cut a release. Coding-agent TUIs restyle frequently; the day Claude renames `"esc to interrupt"`, jackin's
baked rule stops matching and **every install's Claude tab goes dark until the jackin project ships a new
release**. Detection correctness is gated on jackin's release cadence. The reference (herdr) avoids this by
keeping bundled manifests as a fallback and layering **remotely-updatable** manifests on top, so a restyle is
fixed same-day by publishing data, not by shipping a binary. This plan gives jackin the same resilience —
but within jackin's security model (container boundary, no surprise mutation, signed artifacts), so it is a
**signed, verified, fallback-always** channel, not an arbitrary-URL fetch.

## What already exists (reuse it)

- jackin already overlays operator-provided packs from a runtime directory: `load_packs_from_dir`
  (`rules.rs:810-832`) reads `/jackin/runtime/agent-status/packs` and `~/.jackin/...` and overlays them onto
  the embedded packs (skip-and-log on a bad pack). So the **local apply mechanism exists** — the missing
  pieces are (a) a distribution channel, (b) trust/verification, (c) refresh.
- jackin already does signature verification elsewhere: the capsule binary is sigstore-verified
  (`crates/jackin-capsule/src/capsule_binary.rs`). Reuse that verification stack, don't invent one.
- The `regex` crate (already the engine's matcher) is **linear-time / no catastrophic backtracking**, so
  classic ReDoS is largely mitigated by the crate choice — but still bound pack size and validate regexes at
  load (a remote pack is untrusted input).

## Recommended implementation (the "how")

The design in four parts. Keep the image-baked packs as the **guaranteed floor** at every step.

### 1. Distribution: a versioned, signed pack bundle — not arbitrary URLs

Publish a single signed **pack bundle** (all agents' packs + a manifest: `{rule_engine_version, generated_at,
per-agent validated_versions}`) to a well-known, org-controlled location:
- **Preferred**: an **OCI artifact** pushed alongside the construct image to the same registry
  (`ghcr.io/jackin-project/...`), tagged by `RULE_ENGINE_VERSION`. jackin already pulls from that registry, so
  no new trust root. Or a **GitHub release asset** in `jackin-project/jackin` (or a dedicated
  `jackin-project/agent-status-packs` repo).
- **Not** an operator-supplied arbitrary URL by default (that widens the attack surface); an advanced operator
  override MAY point at a private bundle, but the default source is org-controlled and signed.

### 2. Trust: signature-verify before load; unverified → baked fallback

- Every bundle is **signed** (sigstore/cosign, reusing `capsule_binary.rs`'s verification). The crate verifies
  the signature + the identity (the jackin-project signer) **before** parsing any pack.
- A bundle that fails verification is **rejected**; the crate keeps the baked packs and emits a loud
  `EvidenceNote` + `clog!` ("remote pack bundle failed verification — using baked packs"). Never load an
  unverified pack.
- Validate at load: bound total bundle size, bound per-pack size, reject packs whose `rule_engine_version`
  exceeds the running engine (`min_engine_version` is already enforced, `rules.rs:404-408`), and compile-check
  every regex (a malformed remote regex must not panic the daemon — `finalize`'s `?` already returns an error;
  ensure the remote path skip-and-logs one bad pack, never aborts the registry — see plan 006 step 3).

### 3. Refresh: overlay newer, never remove the floor

- The remote bundle **only ever upgrades** an agent's pack to a newer validated one; it never deletes the
  baked floor. Precedence: `verified-remote (newer) > baked > nothing`. So a fetch failure or an empty bundle
  degrades to exactly today's behavior, never to "dark."
- Refresh cadence: on capsule startup (fetch-then-launch is too slow — do it **non-blocking** in the
  background and hot-swap the registry when a verified newer bundle arrives), plus an optional periodic check.
  Never block the PTY/daemon loop on a network fetch.

### 4. Consent + observability (honor jackin's no-surprise-mutation rule)

- Make the remote channel an **operator-visible capability**, surfaced in the launch summary
  (`HOST_AND_CONTAINER.md` rule). Decide the default with the maintainer: **baked-only** (opt-in to remote) is
  the conservative default and matches "no surprise mutation"; **remote-on-with-verification** maximizes
  freshness. Recommend **opt-in** initially, promote to default once the channel is proven.
- Log every applied remote pack: which agent, which version, verified-by-whom — so an operator can always see
  what detection rules are live (ties to plan 005's advisory drift note).

## Scope

**In scope (all in the `jackin-agent-status` crate from plan 011):** a `PackSource` abstraction
(`Embedded`, `LocalDir`, `SignedRemoteBundle`), the fetch + verify + validate + hot-swap path, config/consent
surface, and tests with a fake source. **Out of scope:** running a pack-publishing pipeline (that's org
infra / a CI job, a separate deliverable); changing the rule *engine* or pack *content* (plans 005/007).

## Steps

1. **Model pack sources.** In the detection crate, add a `PackSource` enum and make the registry build from an
   ordered list `[Embedded (floor), LocalDir, SignedRemoteBundle]`, newest-validated-wins, skip-and-log bad.
2. **Verify.** Wire sigstore verification (reuse `capsule_binary.rs`'s stack) so a bundle is signature+identity
   checked before parse; unverified → rejected + loud note; baked floor retained.
3. **Fetch (non-blocking).** Background fetch on startup + optional periodic; hot-swap the registry on a
   verified newer bundle; never block the daemon loop.
4. **Consent + logging.** Launch-summary surface + per-pack applied log; default opt-in (confirm with maintainer).
5. **Tests.** Fake `PackSource`: verified-newer → applied; unverified → rejected, floor kept; oversized/bad-regex
   → skipped, floor kept; fetch failure → floor kept. Assert the daemon loop is never blocked (inject a slow
   fake source, assert ticks still fire — reuse plan 008's seam).

**Verify**: `cargo nextest run -p jackin-agent-status -E 'test(/pack_source|remote|verify/)'` → all pass;
`cargo clippy -p jackin-agent-status -- -D warnings` → exit 0.

## Done criteria

- [ ] Registry builds from `[Embedded, LocalDir, SignedRemoteBundle]`, newest-validated-wins, floor never removed
- [ ] A remote bundle is signature+identity verified before any pack is parsed; unverified → rejected + loud note
- [ ] Bundle/pack size bounds + regex compile-validation on the remote path; one bad pack never aborts the registry
- [ ] Fetch is non-blocking; the daemon loop is provably not stalled by a slow source (test)
- [ ] Remote channel is operator-visible (launch summary) with a chosen default; applied packs are logged
- [ ] `cargo nextest run -p jackin-agent-status` green; clippy clean
- [ ] `plans/agent-status/README.md` row updated

## STOP conditions

- Plan 011 hasn't landed — this belongs in the detection crate, not in capsule; do 011 first (or land them
  together).
- No org-controlled signed distribution point exists and standing one up is out of scope — implement the
  `PackSource`/verify/fallback plumbing against a fake/local signed bundle and mark the live distribution
  `BLOCKED (needs org publishing pipeline)`; the plumbing + trust model is the durable part.
- The maintainer wants remote packs **off** by design (baked-only is acceptable given plan 005 already makes
  drift graceful) — then mark this plan `REJECTED (baked-only chosen; 005 covers graceful drift)` with that
  rationale. This is a legitimate outcome: 010 is the resilience *ceiling*, 005 is the *floor*.

## Maintenance notes

- The security review is the crux: a remote pack is untrusted input entering a security-boundary product. A
  reviewer must confirm verify-before-parse, floor-always, size bounds, and non-blocking fetch. Never relax
  verification for convenience.
- This is the one herdr capability jackin structurally lacked (herdr updates manifests remotely). With it,
  detection tracks agent restyles as data; without it, plan 005 keeps drift graceful but still release-gated.
