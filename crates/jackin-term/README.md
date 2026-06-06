# jackin-term

Owned terminal model for the `jackin-capsule` re-emitting PTY multiplexer.

This crate is the implementation of [Defect 45 (post-restructure-fixes-checklist.mdx)][defect45]
and the [Terminal Emulation Crate roadmap item][roadmap]. The README is the engineering record
required by the defect — a cold reader must finish it understanding what problem we hit, what we
tried, what we analyzed, and why we built this. Read it instead of the roadmap item if you are
new to the codebase.

---

## What problem this solves

`jackin-capsule` is a re-emitting PTY multiplexer. It runs as PID 1 inside a role container,
owns N PTY sessions (one per agent/shell pane), and presents the focused session's screen to
whatever host terminal the operator attached — possibly over SSH. To do this it must:

1. **Parse** raw bytes from each agent's PTY (escape sequences, UTF-8, cursor moves, colors,
   scroll regions, OSC passthrough, wide chars).
2. **Maintain** an in-memory cell grid — so it can repaint the screen on focus changes,
   resize events, and after dialog overlays close.
3. **Emit** only the changed cells as a minimal escape byte sequence to the host terminal.
   On SSH links where bandwidth matters, emitting the full screen every frame is unacceptable.

The **hot path** is: present the focused pane every render tick (~33 ms), emit only what changed.

---

## What we used before, and the exact problems we hit

`jackin-capsule` was built on `vt100` (Jesse Luehrs / `doy`), a fork pinned at a SHA:

```toml
vt100 = { git = "https://github.com/donbeave/vt100-rust", rev = "527f0715..." }
```

The `TODO.md` historically called this a temporary bridge. The June 2026 audit established the
truth is worse:

1. **We never used `vt100`'s emit.** `rg "contents_diff|contents_formatted" crates/jackin-capsule/src`
   returns **nothing**. The defining feature of `vt100` — "diff two screens, serialize to escapes"
   — is dead code for us. We replaced it with our own snapshot+diff+emit in
   `crates/jackin-capsule/src/tui/render.rs` because `vt100`'s emit knows nothing about our
   pane offsets, borders, or dialog overlays.

2. **`vt100` exposes no damage.** To find what changed, our `render.rs` re-reads and re-diffs
   **the entire grid** every frame — O(rows × cols) even when 3 lines changed. This is because
   `vt100` has no damage-tracking API; damage must be recomputed from a full snapshot compare.

3. **A heap `String` allocated per non-blank cell per frame.** `cell.contents().to_string()` in
   `render.rs`. A tall pane = thousands of `String`s allocated and dropped every 33 ms even
   when nothing changed. This is `vt100`'s representation choice: cells store graphemes as owned
   `String`s rather than interned or packed.

4. **Two grids that drift.** The old `vt100` path paired a terminal snapshot with separate
   render-side pane-body diff state. When they disagreed (resize, reflow, filter — Defect 44),
   stale cells appeared on screen as ghost rows.

5. **Upstream is effectively abandoned.** Last *substantive* PR merged: April 2023. As of June 2026:
   10 open PRs (oldest from January 2021), 8 of them opened Dec 2025–May 2026 — zero merged.
   Our `clear_scrollback` patch is PR #31, open since May 22 2026, unmerged. The fork is permanent.

---

## What we analyzed (the full survey)

| Candidate | Parser | Damage | Packed cells | Passthrough | Diff-to-escape | Verdict |
|---|---|---|---|---|---|---|
| `vt100` (fork) | own SM | **none** | `String`/cell | `Callbacks` (forked) | ✓ (unused) | Current; wrong shape |
| `vte` alone | ✓ | n/a | n/a | n/a | n/a | Parser only; always depend |
| `alacritty_terminal` | `vte` | ✓ line-level | ✓ packed | GPU-shaped | **none** | Grid+damage fit; passthrough gap |
| `termwiz` / `wezterm-term` | own | ✓ (`Change`) | partial | ✓ | ✓ (unused for us) | Too broad; unstable pub API |
| Rio / `copa` | own | unknown | unknown | unknown | n/a | Very low adoption; not evaluated |
| `avt` | own | none | n/a | none | none | Snapshot-only; not a grid |
| libvterm | C library | ✓ | ✓ | ✓ | ✓ | **Ruled out: non-Rust.** See constraint. |
| Ghostty / Alacritty (full) | `vte` / own | ✓ | ✓ packed | partial | GPU only | GPU-first; wrong output stage for us |
| Zellij's `grid` | own | ✓ per-row | ✓ | partial | own | Closest design; not published |

**Key finding:** the "fast engines have no diff-to-escape" argument that originally dismissed
`alacritty_terminal` is a **strawman for our case** — we own our emit already. What we need from
a grid crate is **damage + packed cells**, and there `alacritty_terminal` is strong while `vt100`
is the weakest option.

---

## What we tried before building

**Baseline audit (June 2026):** mapped every `vt100` call site in `crates/jackin-capsule/src`.
Found we use the grid (cell read), geometry, modes, scrollback, and `Callbacks` — never the emit.
Confirmed the per-cell `String` alloc and the O(grid) snapshot rebuild by reading `render.rs`.

**`alacritty_terminal` spike (planned, Phase 1 gate):** adopt `alacritty_terminal` purely as
the grid+damage source, keeping our existing emit. This is the cheapest "buy" path and may
eliminate the need to build from scratch. The spike is the first Phase 1 experiment.

**`termwiz` spike:** confirmed `wezterm-term` is not a separately published crate and `termwiz`
has no stability guarantees on its grid API.

---

## Why we decided to build, and why it's better here

The operator decision (June 2026): **quality/perf/stability are the goals; cost/time/maintenance
are not constraints.** Owning the layer is the only path where:

- The API is our nouns, shaped for a re-emitting mux (not a GPU renderer).
- It never breaks under `cargo update`.
- Passthrough is first-class (typed `PassthroughEvents`), not a callback shim on someone else's API.
- Damage is recorded at mutation time, collapsing the two-grid drift that causes Defect 44's
  resize-ghost class **structurally** — not with workarounds.
- Zero per-frame allocation is achievable with a packed cell and a reused emit buffer.

---

## What we depend on vs re-implement (the borrow ledger)

| Source | What we take | How | License | Attribution |
|---|---|---|---|---|
| `vte` | VT/ANSI parser state machine | **Depend** — never rebuild | MIT | `vte` crate, doy |
| Ghostty `PageList` | Arena-page memory model for the cell grid | **Re-implement** in Rust | MIT | Ghostty project, Mitchell Hashimoto |
| Alacritty ring-`Storage` | Packed cell ring-buffer grid layout | **Re-implement** in Rust | Apache-2.0/MIT | Alacritty project |
| Zellij `OutputBuffer` / `changed_lines` | Damage discipline: track dirty rows, emit only changed | **Re-implement** / reference | MIT | Zellij project |
| libvterm VT coverage checklist | Conformance test reference | **Reference** | MIT/X11 | libvterm, Leonard Richardson |
| libvterm / vttest / esctest | Conformance coverage references | **Reference** | MIT/X11 / public test suites | upstream projects |

Every STORE/BORROW site carries an attribution comment in the source naming the project + license.
`vte` is the only parser dependency; everything else is re-implemented code or committed corpus
coverage with an inline comment pointing at the original where applicable.

---

## Architecture and design invariants

```
vte (dep)           ← parse: bytes → Perform events
   ↓
DamageGrid (build)  ← packed Cell(style_id, grapheme_id), ring-buffer rows, scrollback
   │  dirty_spans() ← damage recorded AS Perform mutates (not recomputed by re-read)
   ↓
PassthroughEvents   ← typed: title/clipboard/kitty/focus/OSC-7/csi/scrollback-clear
   ↓
[existing WireDiff] ← render_snapshot_rows, fed dirty_spans() instead of full re-read
                      erase-to-EOL after each row (Defect 44 fix; permanent invariant)
```

**Invariants:**
- Every geometry change fully covers each rect: after any resize, the next emit erases to EOL
  after each row, and vacated rows are blanked. No stale cell ever survives a frame.
- Damage recorded at mutation, not recomputed by re-read.
- Zero per-frame heap allocation in the focused-pane render path.
- Minimal wire bytes: cursor-move + run-length SGR tuned for byte count.
- Pure Rust, no foreign bindings, no C/Zig libraries.

---

## How correctness is guaranteed

1. **Conformance replay harness** (`tests/conformance.rs`): feeds identical byte streams to
   `DamageGrid` in one chunk and byte-by-byte, then asserts identical final grids plus cursor,
   geometry, wide-cell, style, and alt-screen invariants. This proves parser carry state is
   deterministic across PTY read boundaries without carrying a second terminal model.
2. **Conformance corpus** (`tests/fixtures/`): vttest/esctest sequences, real `claude`/`codex`/
   `vim`/`htop` PTY captures, asciinema casts, pathological sets (`yes`, `seq 1 100000`,
   full-screen redraw storms).
3. **Fuzz target** (`fuzz/src/damage_grid_process.rs`): feeds arbitrary bytes to the parser and
   asserts zero panics plus one-shot vs byte-split determinism.
4. **Golden wire-emit snapshots** (Phase 2+): byte-exact emit snapshots for representative frames
   including resize/shrink, locking the Defect 44 erase-to-EOL contract.
5. **Round-trip property test** (Phase 2+): "any mutation sequence → full re-emit → reproduces
   the ground-truth grid."

`vt100` is fully retired from this crate: no production dependency, dev-dependency, fuzz
dependency, benchmark baseline, or source-policy exception remains.

---

## Pure-Rust, no-foreign-bindings constraint

This crate contains no `unsafe` code, no FFI, no C/Zig dependencies. The build has no non-Rust
build dependency. Non-Rust terminal emulators (libvterm, Ghostty) appear in the analysis above as
**design references only** — their algorithms are re-implemented in Rust with attribution.

## Host-side effects: None

`jackin-term` is a library with no host-side effects. It does not write to the filesystem, make
network calls, or mutate host state. All mutation is in-memory, scoped to the `DamageGrid` and
`PassthroughEvents` types.

---

[defect45]: ../../docs/content/docs/reference/roadmap/post-restructure-fixes-checklist.mdx
[roadmap]: ../../docs/content/docs/reference/roadmap/terminal-emulation-crate.mdx
