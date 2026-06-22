# Corpus Fixtures — jackin-term Conformance Harness

Each file in the subdirectories below is a raw byte sequence fed to the conformance harness
(`tests/conformance.rs`). The harness feeds identical bytes to `DamageGrid` in one chunk and
byte-by-byte, then asserts identical final grids (cells, attrs, cursor, alt-screen flag).

## Format

- `.bin` — raw bytes (binary PTY capture)
- `.vt` — VT/ANSI escape sequences in a text-safe encoding (LF-delimited hex `\xNN` for
  non-printable bytes, printable ASCII/UTF-8 inline)
- `.cast` — asciinema v2 JSONL; the harness replays every output (`"o"`) event

## Corpus categories

| Directory | Coverage goal |
|---|---|
| `basic/` | Plain text, cursor movement, SGR colors, line/screen clear |
| `wide_chars/` | CJK ideographs, emoji, combining marks, wide-char continuation cells |
| `resize/` | Content under resize (Defect 44 regression class) |
| `scrollback/` | Scrollback fill, clear-scrollback (`CSI 3J`), alternate screen |
| `alt_screen/` | Alternate screen enter/exit, content in both screens |
| `vttest/` | Representative VT conformance sequences derived from vttest classes |
| `esctest/` | Representative CSI/DEC/SGR sequences derived from esctest classes |
| `real/` | Real CLI/TUI PTY-output captures that are geometry-safe at 24x80 (`claude`, `codex`, `vim`, `htop`, `tmux`) |
| `asciinema/` | Asciinema v2 `.cast` files; output events are replayed through the harness |
| `tool_archetypes/` | Tool-shaped fixtures for binaries unavailable in this environment |
| `pathological/` | High-volume: `seq 1 100000` tail windows, `yes` flood, full-screen redraw storms |

## Adding fixtures

Capture a real PTY stream during a `claude` / `codex` / `vim` / `htop` / `tmux` session:

```sh
# Record a PTY session to a binary file.
script -q -F ~/fixtures/session.bin
# ... do the thing ...
exit

# Or capture just the output bytes:
strace -e write -p <pty-pid> -o /tmp/trace.txt
```

Name the fixture descriptively (`claude-compact-mode.bin`, `vim-syntax-heavy.bin`, etc.)
and commit it to the appropriate subdirectory.

## vttest / esctest sequences

The `vttest/` and `esctest/` directories hold committed representative sequences from those
conformance families. When importing larger upstream slices, keep each fixture geometry-safe at
24x80 or add a dedicated test with the required geometry.
