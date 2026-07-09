# Plan 007: Bound the OSC 8 hyperlink maps and clear them on terminal reset

> **Executor instructions**: Follow step by step. Run every verification command
> and confirm the expected result before moving on. If a "STOP condition" occurs,
> stop and report. When done, update the status row in
> `plans/code-health/README.md`.
>
> **Read first**: `crates/jackin-term/CLAUDE.md` — the terminal model has
> load-bearing correctness gates (conformance replay, fuzz, echo-back) that must
> stay green.
>
> **Drift check (run first)**: `git diff --stat a4761957d..HEAD -- crates/jackin-term/src/grid.rs`
> If it changed, compare the excerpts below against the live code; on a mismatch,
> treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: S-M
- **Risk**: LOW
- **Depends on**: none
- **Category**: bug
- **Planned at**: commit `a4761957d`, 2026-07-09

## Why this matters

The terminal grid holds two OSC 8 hyperlink maps — `osc8_id_to_token:
HashMap<String, u32>` and `hyperlink_targets: HashMap<u32, String>` — that are
**insert-only**: there is no `.clear()`/`.remove()`/eviction anywhere, and RIS /
full reset never touches them. An agent (or a routine tool like `ls
--hyperlink`, cargo, a linter) emitting many OSC 8 hyperlinks with distinct
`id=` values grows both maps without bound for the container lifetime — a
hostile agent can spam unique `id=` values to inflate capsule RSS. The capsule
already recognizes this exact risk for OSC titles (`session.rs:60`
`OSC_EVIDENCE_MAX_CHARS`) but the hyperlink maps have no analogous bound. Bound
them and clear them on reset.

## Current state

`crates/jackin-term/src/grid.rs`:

- Fields (`:134`, `:136`):
  ```rust
  osc8_id_to_token: HashMap<String, u32>,
  hyperlink_targets: HashMap<u32, String>,
  ```
- `alloc_hyperlink_token` inserts into `osc8_id_to_token` (`:892-903`), insert at
  `:901`.
- The target insert (`:1509`): `self.hyperlink_targets.insert(token, uri.clone());`
- `reset_modes` (`:858-867`) clears active state via `clear_active_hyperlink_state()`
  but does **not** clear the two maps.
- Existing cap-constant style to match — `:202` `const KITTY_KB_STACK_CAP: usize = 64;`,
  `:376` `const MAX_RECYCLED_ROWS: usize = 4096;`.
- Tests live in `crates/jackin-term/src/grid/tests.rs` (`mod tests` inside the
  grid module, so tests may read private fields like `grid.osc8_id_to_token`).

## Commands you will need

| Purpose | Command | Expected |
|---|---|---|
| Targeted tests | `cargo nextest run -p jackin-term -E 'test(hyperlink)'` | pass |
| Crate tests + conformance | `cargo nextest run -p jackin-term` | all pass |
| Clippy | `cargo clippy -p jackin-term --all-targets --locked -- -D warnings` | exit 0 |

## Scope

**In scope**: `crates/jackin-term/src/grid.rs` and `crates/jackin-term/src/grid/tests.rs`.

**Out of scope**: the fuzz target, the conformance harness, any parser/`Perform`
dispatch logic — do not weaken a correctness gate to make this pass. No other crate.

## Git workflow

- Branch: operator's active branch, or `fix/osc8-map-bound`.
- One commit, conventional, signed. Example:
  `fix(term): bound OSC 8 hyperlink maps and clear them on reset`
- Do NOT push or open a PR unless instructed.

## Steps

### Step 1: Add a cap constant and a clear helper

Near the other grid caps (by `:202`), add:

```rust
/// Upper bound on retained OSC 8 hyperlink mappings. OSC content is untrusted
/// model output; an agent emitting many distinct `id=` hyperlinks would
/// otherwise grow these maps without bound. On overflow both maps are cleared
/// together (stale off-screen link hover is lost; memory stays bounded).
const OSC8_HYPERLINK_CAP: usize = 8192;
```

Add a private helper method on the grid impl (near `clear_active_hyperlink_state`
around `:948`):

```rust
fn clear_hyperlink_maps(&mut self) {
    self.osc8_id_to_token.clear();
    self.hyperlink_targets.clear();
}
```

### Step 2: Bound the two insert sites

In `alloc_hyperlink_token`, guard the insert at `:901` — before allocating a new
token, clear if the id map is at cap:

```rust
fn alloc_hyperlink_token(&mut self, id: &str) -> u32 {
    if let Some(&token) = self.osc8_id_to_token.get(id) {
        return token;
    }
    if self.osc8_id_to_token.len() >= OSC8_HYPERLINK_CAP {
        self.clear_hyperlink_maps();
    }
    let token = self.next_hyperlink_token;
    // …unchanged…
    self.osc8_id_to_token.insert(id.to_owned(), token);
    token
}
```

At the target insert (`:1509`), guard likewise — before inserting the new target:

```rust
if self.hyperlink_targets.len() >= OSC8_HYPERLINK_CAP {
    self.clear_hyperlink_maps();
}
self.hyperlink_targets.insert(token, uri.clone());
```

(Clearing before the insert keeps the just-created entry; the current
token/target survives.)

### Step 3: Clear both maps on reset

In `reset_modes` (`:858-867`), after the existing `self.clear_active_hyperlink_state();`
add:

```rust
self.clear_hyperlink_maps();
```

RIS is a full terminal reset — the screen is being cleared, so dropping stale
hyperlink mappings is correct here (no on-screen regression).

**Verify**: `cargo check -p jackin-term` exits 0.

### Step 4: Tests

In `crates/jackin-term/src/grid/tests.rs`, add (using whatever constructor the
existing tests use to make a grid — match a nearby test):

```rust
#[test]
fn hyperlink_id_map_stays_bounded() {
    let mut grid = /* construct as existing tests do */;
    for i in 0..(super::OSC8_HYPERLINK_CAP * 2) {
        let _ = grid.alloc_hyperlink_token(&format!("id-{i}"));
    }
    assert!(grid.osc8_id_to_token.len() <= super::OSC8_HYPERLINK_CAP);
}

#[test]
fn reset_modes_clears_hyperlink_maps() {
    let mut grid = /* construct */;
    let _ = grid.alloc_hyperlink_token("some-id");
    assert!(!grid.osc8_id_to_token.is_empty());
    grid.reset_modes();
    assert!(grid.osc8_id_to_token.is_empty());
    assert!(grid.hyperlink_targets.is_empty());
}
```

(If `alloc_hyperlink_token` / `reset_modes` are not reachable from the test
module because of visibility, they are same-module `fn`s so a `mod tests` inside
the grid module can call them; if the tests module is a separate file included
via `#[cfg(test)] mod tests;` on the grid module — which it is — private access
holds. If it does not compile due to visibility, STOP and report.)

**Verify**: `cargo nextest run -p jackin-term -E 'test(hyperlink)'` — both pass.

### Step 5: Full check including conformance

**Verify**: `cargo nextest run -p jackin-term` — all pass, including the
conformance/echo-back tests (they must be unaffected — hyperlink maps do not feed
grid content). `cargo clippy -p jackin-term --all-targets --locked -- -D warnings`
exits 0.

## Test plan

- `hyperlink_id_map_stays_bounded` (the memory-bound regression test) and
  `reset_modes_clears_hyperlink_maps`.
- The existing conformance/fuzz/echo-back suite must stay green unchanged.

## Done criteria

- [ ] `grep -n 'OSC8_HYPERLINK_CAP' crates/jackin-term/src/grid.rs` matches (constant + 2 guards + reset use)
- [ ] `grep -n 'fn clear_hyperlink_maps' crates/jackin-term/src/grid.rs` matches
- [ ] `reset_modes` calls `clear_hyperlink_maps`
- [ ] `cargo nextest run -p jackin-term` exits 0; 2 new tests pass
- [ ] clippy clean; only `grid.rs` + `grid/tests.rs` modified
- [ ] `plans/code-health/README.md` status row updated

## STOP conditions

Stop and report if:

- The maps or `reset_modes`/`alloc_hyperlink_token` no longer match the excerpts
  (grid was refactored).
- Any conformance/echo-back test fails after the change — that would mean the
  hyperlink maps are entangled with grid content in a way the excerpts don't
  show; report it rather than weakening the gate.
- Test-module visibility prevents calling the private methods/fields.

## Maintenance notes

- **Noted follow-up (not this plan):** the correctness sub-issue where reusing an
  `id=` (or empty id) with a new URI silently repoints earlier cells sharing that
  token to the newest URI. That is inherent to token-reuse semantics and lower
  priority; recorded in `plans/code-health/README.md`.
- Reviewer should confirm the clear-on-overflow only affects hover resolution,
  never grid cell content, and that `reset_modes` clearing matches RIS semantics.
