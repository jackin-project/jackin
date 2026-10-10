# Protocol checkpoint

2026-10-10, isolated main port based on `868ce53519234f879d26a8bd2dffaa30fae2d729`.

The protocol exposes durable observation and dispatch guards separately, with
explicit policy approval, canonical local source selection, and an experimental
collection opt-in that defaults off. Wire version is v7; durable monitor state
version is 3; normalized statusline input retains its independent version 2.
Interactive authentication is absent from monitor IPC.

Verified offline:

```sh
/Users/donbeave/.rustup/toolchains/1.97.1-aarch64-apple-darwin/bin/cargo test --offline --locked -p jackin-protocol --lib
```

Result: 129 passed, zero failures. Log: `/private/tmp/jackin-main-protocol-tests.log`.
Scoped `git diff --check` passed. This checkpoint proves protocol fixtures only;
broker migration, CLI integration, installation, and real-account readiness are
still pending. No native Keychain or provider access was performed by this check.
