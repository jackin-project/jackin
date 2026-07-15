# jackin-process

Shared subprocess transport (capture / timeout / retry / status).

## Rules

- Bytes, timing, exit status, timeout, and retry only.
- **Never** add redaction, secret classification, env-map policy, or logging/telemetry here — callers own those (see sensitive-boundary ownership and `ShellRunner` spans).
- Preserve per-call semantics via explicit `ExecRequest` options; do not invent shared default timeouts that change call sites silently.
- Prefer this crate over ad-hoc `Command::new` in jackin-xtask, jackin-capsule, and jackin-runtime.
