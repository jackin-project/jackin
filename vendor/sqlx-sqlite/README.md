# sqlx-sqlite compatibility patch

This directory vendors the published `sqlx-sqlite` 0.9.0 crate with its
`libsqlite3-sys` upper bound updated to `<0.40.0`. No Rust source is changed.
The normalized `Cargo.toml` has the backported bound. The upstream crate's
standalone `Cargo.lock` is omitted because the workspace lockfile owns
resolution for this path dependency; retaining a nested lock would make
repository tooling resolve the vendored crate as an independent project.
`Cargo.toml.orig` and all crate source files remain as published.

The source archive is `sqlx-sqlite-0.9.0.crate`, SHA-256
`488e99c397a62007e4229aec669a179816339afc6d2620ca6fa420dbee2e982c`. The
original registry lockfile recorded this archive checksum before the crate
was vendored. Its upstream VCS revision is
`003b698e99e024f3621b8043a2426fde5b741171`.

The bound backports two merged SQLx manifest changes:

- [PR 4402](https://github.com/transact-rs/sqlx/pull/4402), merge commit
  `94aafe3`, which changed the upper bound from `<0.38.0` to `<0.39.0`.
- [PR 4502](https://github.com/transact-rs/sqlx/pull/4502), merge commit
  `10462fa4414c6c7f1ed1d39bb31678e3b726878c`, which changed it from
  `<0.39.0` to `<0.40.0`.

This lets pinned OpenShell SQLx 0.9.0 resolve with rusqlite 0.41.1 and
`libsqlite3-sys` 0.39.x. Remove the workspace patch and this directory when a
published SQLx release includes the `<0.40.0` bound and the pinned OpenShell
version accepts it.
