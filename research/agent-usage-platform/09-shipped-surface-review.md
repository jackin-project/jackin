# 09 — Shipped surface review

Vetted: 2026-08-21
Source revision: `8400e14d`
Questions: When the shipped usage surfaces are run against a real operator configuration, which settled contracts hold and which do not?
Informs: unified-agent-usage

## Method

Every surface was executed on macOS against the operator's real
`~/.config/jackin` and `~/.jackin`, from a clean build of this revision
(`cargo build --bin jackin`, `mise run desktop-build`). Raw command output, exact
line references, and the executed proof-command results are recorded in
[05 — Verification ledger](05-verification-ledger.md#post-implementation-execution--2026-08-21).
The desktop composition gap is not describable in prose and is captured
separately in
[ShippedComparison.html](../../native/Design/UnifiedAgentUsage/ShippedComparison.html).

This chapter is the reviewed reading of that evidence: what is broken, how
severely, and in what order it should be addressed.

## Summary

| Measure | Result |
|---|---|
| Command entry points that run | 0 of 3 — `jackin console`, `jackin usage`, `jackin usage --format json` all abort |
| Completed provider fetches reaching a surface | 2 of 6 |
| Providers shown by jackin❯ desktop on live data | 3 of 7, with the sidebar and table disagreeing |
| Required proof commands returning tests | 3 of 7 |
| Independent projection builders | 4 — the settled contract specifies one |
| Workspace tests passing | 2,232, none of which reach the three broken entry points |

The mechanical quality gates are healthy. `cargo clippy --workspace
--all-targets --all-features --locked -- -D warnings`, `cargo fmt --check`, and
`mise run desktop-lint` all pass, and every changed `ratchet.toml` bound moved
down rather than up. No debt was loosened to make a gate pass. The gap is that
no gate observes the running product.

## Blocking defects

### B1 — `jackin console` aborts at startup

`run_console` is asynchronous, and this branch added a synchronous
`load_console_usage_state()` call to its startup path
(`crates/jackin/src/console/adapter/run.rs:1056`). That call reaches
`usage_snapshot_store::block_on_store`, which constructs a Tokio runtime while
one is already driving the thread, and the process aborts with exit 101 before
the first frame.

The condition is `store_path.exists()`
(`crates/jackin-usage/src/host/accounts.rs:362`), which resolves to
`~/.jackin/data/usage-menu-bar/snapshots.db` — present for any operator who has
run the menu bar. `main` contains no usage call in that adapter, so the console
regression is introduced here rather than pre-existing.

The store module documents the invariant it violates, in a comment at
`crates/jackin-usage/src/usage_snapshot_store.rs:97`. Three new callers break it
and no build-time or test gate notices.

### B2 — both `jackin usage` forms abort identically

`run_bare_host` (`crates/jackin/src/cli/usage.rs:241`) repeats the violation, as
does the Usage route's refresh handler. Exit code is 101 rather than the settled
nonzero failure contract, and stdout is empty, so neither the
`No usage accounts found.` empty state nor the `Agent usage unavailable` failure
state is reachable.

Separately, a first invocation before the broker process exists blocks for about
85 seconds. `run_bare_host` iterates capabilities serially and calls
`client.join(…, Duration::from_secs(30))` for each, on the runtime thread, so
worst-case latency scales with the number of configured accounts.

### B3 — completed fetches are discarded, then reported as `needs login`

With a clean data directory the panic does not trigger and the broker completes
six provider fetches. Four never reach the projection. The discriminator is the
account label: `CanonicalAccountIdentity::from_view`
(`crates/jackin-usage/src/host/accounts.rs:77`) derives identity solely from
`view.account.account_label`, and `stable_account_label` rejects an empty one.
Providers that authenticate without exposing an address — Anthropic through
OAuth, Kimi, Z.AI through API keys — therefore cannot become canonical accounts,
regardless of how much quota data they returned.

`CanonicalAccountSubject::ProviderId` exists but is never constructed from a
view, and discovery already carries a stable, deduplicated `capability_id` that
would serve as the fallback rung of the evidence ladder this topic's conclusion 2
called for.

A second defect compounds it: `crates/jackin-usage/src/host/projection.rs:243`
assigns `UsageLifecycleV1::NeedsLogin` unconditionally to every unresolved
capability. A rate-limited Anthropic account, an unavailable Amp, and a Z.AI
account that returned three healthy buckets all instruct the operator to log in.
`issues` is hard-coded empty at both the unresolved and the projection level, so
discovery diagnostics never surface either.

### B4 — the broker's canonical projection is never published

`dispatch` (`crates/jackin-usage/src/host/broker.rs:964`) answers
`CurrentProjection`, `RequestRefresh`, and `JoinPublication` by cloning a stored
value. Nothing assigns to that value anywhere in the crate. `load_projection`
seeds `empty_projection(build_id)` and persists it, so
`~/.jackin/data/usage-broker/projection.json` remains at `<build>:empty` with an
empty `providers` array after live refreshes have completed.

Its only consumer is the lifecycle suite, which asserts that concurrent clients
observe an equal `projection_id` — a property the empty value satisfies. Every
real surface rebuilds the projection in-process instead, which is precisely how
the store call reaches the async thread in B1 and B2.

### B5 — broker failures are undiagnosable and the desktop retries hot

`ensure_usage_broker_process` (`crates/jackin-usage/src/host/broker.rs:718`)
spawns the service with `.stderr(Stdio::null())`, so every startup failure is
discarded by construction. The service returns one message for structurally
different faults; a data directory long enough to push the socket past the
104-byte `sun_path` limit exits with `usage broker unavailable: "usage broker is
unavailable"` and no path, length, or cause.
`crates/jackin-runtime/src/bin/usage-broker.rs` emits no telemetry at all.

Against live data the desktop logs `coordination_unavailable` every five seconds
indefinitely, which also contradicts the settled 2/5/15/30-minute adaptive
cadence.

### B6 — one canonical projection is four, and the desktop uses none of them

This explains most of the desktop findings. Four independent builders each call
`materialize_account_catalog()` directly and apply their own inclusion rule,
selection reconciliation, and formatting:

| Builder | Location | Consumer |
|---|---|---|
| `canonical_projection` | `host/projection.rs` | CLI, console |
| `desktop_inventory` | `host.rs:1196` | desktop Overview table |
| `provider_glance_rows` | `host.rs:1348` | desktop sidebar, status items |
| `list_accounts` | `host.rs:837` | FFI bridge, `jackin-usage-ffi/src/bridge.rs:243` |

`jackin-usage-ffi` contains no reference to `UsageProjectionV1`,
`canonical_account_id`, or `UsageLifecycleV1`. Canonical identity, the lifecycle
enum, window categories, ranks, `unresolved` rows, and locale-aware ordering
exist only on the CLI and console side; the desktop reimplements the same
concepts as loose strings in `AccountDescriptorDto` — `plan_or_status_label`,
`remaining_label`, `reset_display_label`.

The consequence is directly visible. At one generation and one mutex hold, the
Usage window sidebar lists a different provider set than its own Overview table,
because two hand-maintained predicates diverged. It is deterministic, not a race.

Rust is also shaped to the table: `host.rs:1268-1271` and `:1777-1788` place
literal `—` filler into `account_column_label`, `remaining_label`,
`reset_display_label`, and `provider_column_label` so provider rows have cells to
occupy. A table layout decision living in the Rust layer inverts the settled rule
that consumers adapt layout only.

## Contract drift

### Desktop composition

The shipped Overview is a five-column `Table` where the blessed prototype is a
grid of provider cards. Reset is empty for every row on both fixture and live
data, provider rows are entirely em dashes, account labels wrap mid-word, the
debug string `Fixture` appears in the toolbar, and the Overview selection pill
uses system blue — which `BrandColors.swift` explicitly forbids for selection
wells. Meters appear only in the detail view and the popover. The prototype
design system was ported at roughly a fifth of its size, and no Liquid Glass
container or button style appears anywhere in `native/Sources/`. The blessed
fixture matrix runs F00–F29; production `VisualQAFixtureID` stops at F14. Full
visual evidence:
[ShippedComparison.html](../../native/Design/UnifiedAgentUsage/ShippedComparison.html).

### Bare CLI output

The confirmed schematic specifies `Agent usage · Updated now`, the plan on the
account line, and compact one-line limits. The shipped renderer emits an ANSI
brand pill and the word `usage`, prints the lifecycle where the plan belongs
although `plan_label` is present in the JSON envelope, renders undiscovered
providers as bare headings, and dumps unresolved capabilities at the end as
`anthropic · NeedsLogin` — lowercase surface id, Rust `{:?}` on an enum, repeated
three times for one provider. ANSI escapes are written even when stdout is a
pipe; there is no terminal check.

### Console Usage route

Refresh is a no-op: `KeyCode::Char('r') => {}` in the screen, a panicking handler
in the adapter, and no binding at all for the confirmed capital `R`. The
dispatcher intercepts every key while the route is open, so `Ctrl-Q` cannot quit.
`Enter` toggles a flag that changes only a panel title, and `Esc` closes the
whole route instead of stepping back to Overview. Loading, refreshing, stale,
global failure, and the account-removal notice are absent — which the branch's
own `plans/unified-agent-usage/coverage.md` records as pending for S3–S6 even
while the item status reads otherwise. The screen is built on raw ratatui blocks
and literal colours rather than TermRock, and recomputes severity thresholds that
Rust already ships.

### Retained bypasses

Three usage stores are live: `usage-menu-bar/snapshots.db`,
`usage-broker/accounts/*.json`, and `daemon/accounts.db`. `jackin usage cache
accounts` reads the third directly (`crates/jackin/src/cli/usage.rs:417`),
`--sync-host-cache` still writes it, and `--no-refresh` remains on `jackin usage
host snapshot`. The settled decision required these removed or redefined rather
than retained.

### Quota semantics

`UsageWindowCategoryV1::Model` is never produced, so every model-specific window
falls into `Other` and the settled summary ranking cannot be honoured. Windows
are emitted in raw provider order with no sort, so the "first Rust-ranked limit"
is really the first provider bucket. `runs_out_label` is hard-coded `None`. Money
caps collapse to a percentage and the amounts never enter the projection, while
`StatusSlot::Spend` documents that it must render as money; there is no monthly
slot for the Codex individual limit to occupy.

### Capsule

The Capsule change is 54 insertions and 18 deletions across eight files, of which
the Usage dialog receives seven lines. The agent tab strip derived from resolved
launch configuration, the conditional canonical-account tab strip, per-agent
Overview grouping, and the empty-launch-config state are absent.
`AgentUninitialized` — the lifecycle state this topic's vocabulary is built
around — appears in the protocol, the projection, and the console, and nowhere in
`jackin-capsule`.

Separately, `discover_forwarded_sources`
(`crates/jackin-usage/src/host/discovery.rs:574`) filters forwarded capabilities
through `HostSurfaceId::DESKTOP_PROVIDER_ORDER`. OpenCode is excluded from the
desktop catalog by settled decision; nothing excludes it from a Capsule. A
forwarded OpenCode account is dropped silently.

### Documentation

Four new operator-facing surfaces landed with twenty inserted documentation
lines. The new Console Usage section in `docs/content/reference/tui/navigation.mdx`
describes the implementation rather than the contract — it records the 30/70
split, states that `Enter` toggles a presentation, states that `Esc` returns to
the workspace list, and omits `R` and every degraded state, because none of those
work. `docs/content/(public)/commands/usage.mdx` is untouched and now asserts
that the running Capsule daemon owns provider refresh, caching, retry, and
stale-state decisions, which is the opposite of the architecture this work
introduces.

### Performance

`materialize_account_catalog` (`crates/jackin-usage/src/host.rs:1496`) re-reads
the snapshot store on every call and caches nothing. One `desktop_projection`
reaches it once through `desktop_inventory`, once through
`provider_glance_rows`, and once per provider through `snapshot`. A
`canonical_projection_cache` field exists on the runtime; this path does not use
it.

## What holds up

Recorded so the remaining work is attributable rather than diffuse.

- Clippy, formatting, and Swift lint gates are clean, and ratchet bounds
  tightened rather than loosened.
- The UniFFI to boltffi migration is complete and enforced: six MPL-2.0
  exceptions were removed from `deny.toml` and replaced by a ban entry carrying a
  reason.
- Broker threads route through `jackin_telemetry::spawn::thread_joined_named`
  rather than raw threads.
- The new OpenCode adapter refuses to derive identity from its bearer key and
  documents why. That is exactly the reasoning the identity layer above it needs.
- The desktop detail view and the status popover are close to the blessed
  prototype. The composition gap is concentrated in Overview and the sidebar.

## Recommended order

1. Remove the runtime-nesting hazard as a class: make the snapshot store
   asynchronous and delete `block_on_store`, or have the synchronous facade
   assert that no runtime is current, so the invariant fails at the boundary
   rather than four frames deep.
2. Add one integration test per entry point, executing inside a Tokio runtime
   against a populated store fixture. Four tests cover both blockers.
3. Make canonical identity total — provider stable id, then provider label, then
   the discovery `capability_id` — and add the invariant as a test: every broker
   capability in `phase=completed` yields exactly one account row.
4. Carry the broker's terminal error kind into the unresolved state and populate
   `issues` from discovery diagnostics. Stop reporting `needs login` for causes
   that are not a missing login.
5. Implement broker publication or remove the operation. If it stays, recompute
   and atomically publish on every completed refresh and have all surfaces
   consume it.
6. Collapse the four builders into filters over one projection, export
   `UsageProjectionV1` across the boltffi boundary, and move the `—` filler out
   of Rust. This is a precondition for the desktop rebuild.
7. Rebuild the desktop Overview against the prototype, extend the fixture matrix
   through F29, and clear the `NSTableView` reentrancy warning.
8. Rewrite the bare CLI renderer against its schematic, including the terminal
   check and the exit contract.
9. Rebuild the console route on TermRock with all six states and the confirmed
   navigation grammar.
10. Implement the frozen quota semantics: the model category, a monthly slot,
    ranked window ordering, `runs_out_label`, and money-cap amounts.
11. Deliver the Capsule contract or reopen its coverage rows, and drop the
    desktop provider filter from Capsule discovery.
12. Cache the catalog for the length of one projection.
13. Rewrite the documentation against the contract rather than the current
    behaviour, and reconcile the roadmap status, the coverage ledger, and the
    pull-request description, which currently describe three different states of
    the same work.
