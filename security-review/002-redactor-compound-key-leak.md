# Plan 002: Fix the diagnostics redactor so compound credential env names are masked

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `security-review/README.md`.
>
> **Drift check (run first)**: `git diff --stat a4761957d..HEAD -- crates/jackin-diagnostics/src/redact.rs crates/jackin-diagnostics/src/redact/tests.rs`
> If `redact.rs` changed since this plan was written, compare the "Current
> state" excerpt against the live code before proceeding; on a mismatch, treat
> it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: security
- **Planned at**: commit `a4761957d`, 2026-07-09

## Why this matters

`jackin-diagnostics::redact::redact_text` is the redactor that guards the JSONL
diagnostics file and the OTLP export sink (`observability.rs` calls it before
emit). Its key-name pattern begins with a word boundary `\b` immediately before
the secret keyword (`token`, `api[_-]?key`, …). But **every** credential env var
jackin❯ forwards has a word character (`_`) right before the keyword —
`GH_TOKEN`, `GITHUB_TOKEN`, `GH_ENTERPRISE_TOKEN`, `ANTHROPIC_API_KEY`,
`XAI_API_KEY`, `AMP_API_KEY`, `CLAUDE_CODE_OAUTH_TOKEN`. There is no boundary
between `_` and `TOKEN`, so `\btoken` never matches these names and the key-name
redaction rule never fires for them. The only remaining protection is
value-shape patterns (`ghp_`, `sk-`, JWT, hex, base64) — which miss any token
whose value shape isn't hard-coded (e.g. an `xai-`-prefixed key, a lowercase-hex
OAuth token). Net effect: strings of the form `XAI_API_KEY=<value>` can reach
the JSONL file and the OTLP backend in clear. This is the concrete current
instance of the roadmap's known "token-shaped values reached the OTLP backend"
item. The sibling redactor `secret_scrub::is_secret_key` (substring match) does
catch these names, so the two redactors disagree and the weaker one guards the
exported sink.

The tight fix: let the key-name pattern match a credential keyword that appears
as the **suffix of a longer identifier**, so `GH_TOKEN`/`XAI_API_KEY` redact.

## Current state

`crates/jackin-diagnostics/src/redact.rs:36-59` — `redaction_patterns()` returns
a `Vec<Regex>`; `redact_text` (`:9-17`) applies each with `replace_all(…,
"<redacted>")`. The load-bearing key-name pattern is the third entry
(`redact.rs:42`), exactly:

```rust
r"(?i)\b(?:authorization|bearer|token|secret|password|passwd|credential|api[_-]?key|access[_-]?key|private[_-]?key)\b\s*[:=]\s*['\x22]?[^\s,'\x22}\]]+",
```

The problem is the leading `\b` — it requires a boundary right before the
keyword, which a compound key like `GH_TOKEN` does not have (`_T` is not a
boundary).

The sibling redactor that already handles this correctly, for reference —
`crates/jackin-diagnostics/src/secret_scrub.rs:161-170`:

```rust
fn is_secret_key(key: &str) -> bool {
    let key = key.to_ascii_uppercase();
    key.contains("TOKEN") || key.contains("SECRET") || key.contains("PASSWORD")
        || key.contains("PASSWD") || key.contains("API_KEY") || key.contains("AUTH")
        || key.contains("CREDENTIAL")
}
```

Existing tests — `crates/jackin-diagnostics/src/redact/tests.rs` — follow a
plain `assert_eq!(redact_text(input), expected)` shape (see
`redacts_named_secret_values` at the top of that file). Match it.

**Why not just broaden to a bare `auth` substring like `secret_scrub` does:**
`redact_text` runs over free-text log lines, not `KEY=VALUE` pairs, so matching
a bare `auth` keyword would redact benign prose like `author=alice`. The fix
below keeps the existing curated keyword list and only relaxes the *prefix*, so
it cannot introduce that false positive.

## Commands you will need

| Purpose | Command | Expected on success |
|---|---|---|
| Targeted tests | `cargo nextest run -p jackin-diagnostics -E 'test(redact)'` | all pass |
| Crate tests | `cargo nextest run -p jackin-diagnostics` | all pass |
| Clippy | `cargo clippy -p jackin-diagnostics --all-targets --locked -- -D warnings` | exit 0 |

## Scope

**In scope** (the only files you should modify):
- `crates/jackin-diagnostics/src/redact.rs` (the one regex line)
- `crates/jackin-diagnostics/src/redact/tests.rs` (add canary tests)

**Out of scope** (do NOT touch):
- `crates/jackin-diagnostics/src/secret_scrub.rs` — fully unifying the two
  redactors onto one implementation is a larger, separately-tracked follow-up;
  do not merge them here.
- `crates/jackin-diagnostics/src/observability.rs` — the call site is correct;
  only the pattern is wrong.
- The value-shape patterns (lines 43-50) — do not weaken or reorder them.

## Git workflow

- Branch: the operator's active branch, or `fix/redactor-compound-key`.
- One commit, conventional style, signed (`git commit -s`). Example:
  `fix(diagnostics): redact compound credential env names (GH_TOKEN, *_API_KEY)`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Relax the key-name prefix so compound identifiers match

In `crates/jackin-diagnostics/src/redact.rs:42`, change the leading `\b(?:` to
`\b[A-Za-z0-9_-]*(?:` so the pattern allows an identifier prefix before the
keyword. The line becomes exactly:

```rust
r"(?i)\b[A-Za-z0-9_-]*(?:authorization|bearer|token|secret|password|passwd|credential|api[_-]?key|access[_-]?key|private[_-]?key)\b\s*[:=]\s*['\x22]?[^\s,'\x22}\]]+",
```

Why this is correct and bounded: `\b` now anchors at the *start* of the whole
identifier (`GH_TOKEN` → boundary before `G`); `[A-Za-z0-9_-]*` consumes `GH_`;
the alternation matches `TOKEN`; the trailing `\b` before `\s*[:=]` still holds
(`N` → `=`). `[A-Za-z0-9_-]*` only consumes identifier characters, so it cannot
cross whitespace and swallow a preceding word. A bare `token=x` still matches
(prefix consumes empty). `author=alice` still does NOT match (no curated keyword
is a suffix of `author`).

**Verify**: `cargo nextest run -p jackin-diagnostics -E 'test(redact)'` — the
existing redact tests still pass (the change is a superset of the old matches).

### Step 2: Add canary tests for the real forwarded env names

In `crates/jackin-diagnostics/src/redact/tests.rs`, add tests asserting each
credential env name jackin❯ forwards is redacted. Cover at minimum these keys,
each as `<KEY>=<fake-value>`:

- `GH_TOKEN`
- `GITHUB_TOKEN`
- `GH_ENTERPRISE_TOKEN`
- `ANTHROPIC_API_KEY`
- `XAI_API_KEY` (with an `xai-`-prefixed value — the shape the value patterns miss)
- `AMP_API_KEY`
- `CLAUDE_CODE_OAUTH_TOKEN`

Use obviously-fake values (e.g. `xai-EXAMPLENOTAREALKEY0000`) — never a real
credential. Follow the existing `assert_eq!` shape. Example:

```rust
#[test]
fn redacts_compound_credential_env_names() {
    for key in [
        "GH_TOKEN", "GITHUB_TOKEN", "GH_ENTERPRISE_TOKEN",
        "ANTHROPIC_API_KEY", "XAI_API_KEY", "AMP_API_KEY",
        "CLAUDE_CODE_OAUTH_TOKEN",
    ] {
        let input = format!("{key}=xai-EXAMPLENOTAREALKEY000000 rest");
        let redacted = super::redact_text(&input);
        assert_eq!(redacted, "<redacted> rest", "key {key} not redacted");
    }
}
```

Also add one negative test proving no over-redaction of benign prose:

```rust
#[test]
fn leaves_author_field_alone() {
    let input = "author=alice committed the change";
    assert_eq!(super::redact_text(input), input);
}
```

**Verify**: `cargo nextest run -p jackin-diagnostics -E 'test(redact)'` — all
pass, including the two new tests.

### Step 3: Full crate check

**Verify**: `cargo nextest run -p jackin-diagnostics` all pass; `cargo clippy -p
jackin-diagnostics --all-targets --locked -- -D warnings` exits 0.

## Test plan

- New tests in `crates/jackin-diagnostics/src/redact/tests.rs`:
  `redacts_compound_credential_env_names` (the 7 canary keys) and
  `leaves_author_field_alone` (negative). Model them after the existing
  `redacts_named_secret_values` test in the same file.
- Verification: `cargo nextest run -p jackin-diagnostics` → all pass including
  the 2 new tests.

## Done criteria

Machine-checkable. ALL must hold:

- [ ] `redact.rs:42` contains `\b[A-Za-z0-9_-]*(?:` (verify: `grep -n 'A-Za-z0-9_-\]\*(?:authorization' crates/jackin-diagnostics/src/redact.rs`)
- [ ] `cargo nextest run -p jackin-diagnostics` exits 0; the two new tests exist and pass
- [ ] `cargo clippy -p jackin-diagnostics --all-targets --locked -- -D warnings` exits 0
- [ ] No files outside the in-scope list modified (`git status`)
- [ ] `security-review/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

- `redact.rs:42` no longer matches the "Current state" excerpt (redactor was
  already refactored — the fix may already be in, or the shape changed).
- Adding the compound-key match breaks an existing redact test in a way that
  isn't a pure superset (e.g. a test asserted a compound key was left alone —
  unlikely, but if so, report it; that assertion would itself be the bug).
- You find the redactor now lives in a different module or is generated.

## Maintenance notes

- **Deferred follow-up (not this plan):** fully unify `redact_text` and
  `secret_scrub` onto one implementation so the diagnostics/OTLP sink and the
  key/value scrubber can never disagree again, and add value-shape patterns for
  provider-specific token prefixes. Tracked in `security-review/README.md`.
- **Deferred follow-up:** the Phase-3 "telemetry redaction" policy test that
  pushes canary credentials through the real launch/auth code paths (not just
  the redactor unit) belongs with the credential-flow work in plan 006, not
  here.
- Reviewer should confirm the regex change is a strict superset of the old
  matches and that every new test value is obviously synthetic.
