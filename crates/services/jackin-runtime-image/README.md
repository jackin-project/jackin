# jackin-runtime-image

Role image build,
prewarm, refresh,
and staleness
pipeline.

## What this crate owns

- Build (`image`):
  `build_agent_image`,
  `decide_role_image`,
  `local_image_build_args`,
  cache-bust +
  base-tag helpers.
- Prewarm (`image`):
  `prewarm_role_images`,
  `ImagePrewarmStatus`,
  sibling prewarm
  spawn, staleness
  sentinels.
- Refresh (`image`):
  `spawn_selected_image_refresh`,
  `published_image_is_stale`,
  validated-repo
  rebuilds.
- Test hooks
  (`image`,
  `test-support`):
  `image_recipe`
  re-exports for
  launch suites.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | re-exports | — |
| [`image.rs`](src/image.rs) | pipeline root | `image/tests/` |
| [`image/`](src/image/) | build/prewarm/refresh | `image/tests/` |

## Public API

`image::prewarm_role_images`,
`image::ImagePrewarmStatus`,
`image::RoleImagePrewarmRow`,
`image::decide_role_image`,
`image::build_agent_image`.

## How to verify

```sh
cargo nextest run -p jackin-runtime-image
cargo clippy -p jackin-runtime-image --all-targets -- -D warnings
```
