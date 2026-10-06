# Goal: multi-account settings, usage, and containers

Implement complete multi-account support on `feat/multi-account-support`, PR #1002
only. Completion means: Settings registers/selects/monitors every AI account
(several per provider); first-run bootstrap + Settings Scan import evidenced logins;
workspace authorization/defaults/launch admission are distinct; one container runs
repeated agent instances (2×Claude + Codex tracer bullet); Usage is an async
broker-driven periodic monitor with stable IDs and typed provider detail; every
requested client/service is in the capability matrix with verified sources or exact
unsupported reasons.

## Operator protocol

1. Branch is `feat/multi-account-support`, PR #1002. Never create another branch/PR.
2. Spine order: S1 catalog expansion → S2 config schema + resolver → S3 credential
   transport → parallel lanes (discovery, Settings, launch/provisioning, Capsule,
   broker/usage, providers A–E, runtimes, tracer bullet) → gates → live.
3. One owner per file set at a time; contracts in `001-t02-domain-contracts.md` are
   frozen — change them only via an explicit orchestrator decision recorded there.
4. Commit with DCO signoff; push every in-scope change to the branch.
5. Mark ledger rows only with recorded command/test evidence. Never report an unrun
   check as passed. Live credentials stay on this Mac, redacted from all output.

## Final gates

```sh gates
test "$(git branch --show-current)" = "feat/multi-account-support"
mise install
cargo nextest run -p jackin-core -p jackin-instance -p jackin-config -p jackin-env -p jackin-protocol
cargo nextest run -p jackin-usage -p jackin-usage-ffi -p jackin-runtime -p jackin-console -p jackin-capsule
cargo xtask ci --fast
cargo xtask ci
cargo xtask ci --e2e
cargo xtask roadmap audit
cargo xtask docs repo-links
cargo xtask research check
mise run desktop-ci
mise run desktop-merge
```

Every manual `jackin` invocation includes `--debug`.
