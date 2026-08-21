# Unified usage contract fixtures

These secret-free fixtures freeze Plan 001 vocabulary before production V1 types and
surfaces land.

- `usage-projection-v1-current.json`: valid canonical V1 shape.
- `usage-projection-v1-invalid.json`: unknown-major, missing-field, and percent-
  invariant failures.
- `provider-call-allowlist.json`: every currently classified production provider call.
  `legacy_bypass` is debt for its owning migration plan, not approval to add another.
- `surface-matrix.json`: required state/dimension families for later golden suites.

## Executable baseline

```sh
rtk cargo test -p jackin-usage contract_baseline -- --test-threads=1
rtk cargo test -p jackin-usage coordinator::tests -- --test-threads=1
rtk cargo test -p jackin-usage host::broker::tests -- --test-threads=1
rtk cargo test -p jackin-runtime usage_relay::tests -- --test-threads=1
rtk cargo test -p jackin-capsule usage -- --test-threads=1
rtk cargo test -p jackin cli::usage -- --test-threads=1
rtk cargo test -p jackin-usage-ffi bridge::tests -- --test-threads=1
rtk cargo xtask research check
rtk cargo xtask roadmap audit
```

## Reserved behavioral targets

The owning plan adds each target atomically with passing implementation. Zero matching
tests fail that plan; Plan 001 does not commit known-failing tests.

| Target | Owner |
|---|---|
| `jackin-usage canonical_projection` | Plan 002 |
| `jackin-runtime broker_service_lifecycle` | Plan 003 |
| provider-specific classifier/source fixtures | Plan 004 |
| `jackin cli::usage::canonical_overview` | Plan 005 |
| `jackin-console usage` | Plan 005 |
| `jackin-runtime usage_relay::resolved_launch_inventory` | Plan 006 |
| `jackin-capsule usage_projection` | Plan 006 |
| `jackin-usage-ffi canonical_projection` | Plan 007 |
| cross-surface and release proof | Plan 008 |

Update the fixture, validator, and owning specification together. Never include a real
credential, credential-derived identifier, raw provider error, or host path.
