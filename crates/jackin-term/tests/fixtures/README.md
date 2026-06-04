# Corpus Fixtures — jackin-term Differential Harness

Each file in the subdirectories below is a raw byte sequence fed to the differential harness
(`tests/differential.rs`). The harness feeds identical bytes to two terminal model
implementations and asserts identical final grids (cells, attrs, cursor, alt-screen flag).

## Format

- `.bin` — raw bytes (binary PTY capture)
- `.vt` — VT/ANSI escape sequences in a text-safe encoding (LF-delimited hex `\xNN` for
  non-printable bytes, printable ASCII/UTF-8 inline)

## Corpus categories

| Directory | Coverage goal |
|---|---|
| `basic/` | Plain text, cursor movement, SGR colors, line/screen clear |
| `wide_chars/` | CJK ideographs, emoji, combining marks, wide-char continuation cells |
| `resize/` | Content under resize (Defect 44 regression class) |
| `scrollback/` | Scrollback fill, clear-scrollback (`CSI 3J`), alternate screen |
| `alt_screen/` | Alternate screen enter/exit, content in both screens |
| `pathological/` | High-volume: `seq 1 100000` output, `yes` flood, full-screen redraw storms |

## Adding fixtures

Capture a real PTY stream during a `claude` / `codex` / `vim` / `htop` session:

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

The `basic/` and `wide_chars/` directories should eventually include the machine-generated
sequences from [vttest](https://github.com/ThomasDickey/vttest) and
[esctest](https://github.com/nfvmit/esctest) — VT-conformance test suites.
