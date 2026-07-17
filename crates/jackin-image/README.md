# jackin-image

Image generation and binary-artifact management for jackin❯. Builds the derived role image, acquires/caches the agent and capsule binaries, and decides the image-build vs reuse path.

## What this crate owns

- Derived-image Dockerfile generation (`derived_image`, `image_recipe`) and the build pipeline (`image_build`).
- Agent + capsule binary acquisition and caching (`agent_binary`, `capsule_binary`, `binary_artifact`).
- The image *decision* — build vs reuse vs published (`image_decision`, `version_check`) and image naming (`naming`).

## Architecture tier and allowed dependencies

**L1 application (image subsystem).** Allowed workspace dependencies: `jackin-core`, `jackin-manifest`, `jackin-docker`, `jackin-diagnostics`, `jackin-build-meta`. No presentation or runtime dependencies — image materialization is a domain/app concern consumed by `jackin-runtime`.

## Structure

| Module | Owns | Tests |
|---|---|---|
| [`lib.rs`](src/lib.rs) | crate root, re-exports | — |
| [`derived_image.rs`](src/derived_image.rs) · [`derived_image/`](src/derived_image) | derived-image Dockerfile generation | [`tests.rs`](src/derived_image/tests.rs) |
| [`image_recipe.rs`](src/image_recipe.rs) · [`image_recipe/`](src/image_recipe) | Dockerfile recipe | [`tests.rs`](src/image_recipe/tests.rs) |
| [`image_build.rs`](src/image_build.rs) · [`image_build/`](src/image_build) | build pipeline | [`tests.rs`](src/image_build/tests.rs) |
| [`agent_binary.rs`](src/agent_binary.rs) · [`agent_binary/`](src/agent_binary) | agent binary acquisition + cache | [`tests.rs`](src/agent_binary/tests.rs) |
| [`capsule_binary.rs`](src/capsule_binary.rs) · [`capsule_binary/`](src/capsule_binary) | capsule binary acquisition + cache | [`tests.rs`](src/capsule_binary/tests.rs) |
| [`binary_artifact.rs`](src/binary_artifact.rs) · [`binary_artifact/`](src/binary_artifact) | shared artifact helpers | [`tests.rs`](src/binary_artifact/tests.rs) |
| [`image_decision.rs`](src/image_decision.rs) · [`image_decision/`](src/image_decision) | build-vs-reuse decision | [`tests.rs`](src/image_decision/tests.rs) |
| [`version_check.rs`](src/version_check.rs) · [`version_check/`](src/version_check) | version check | [`tests.rs`](src/version_check/tests.rs) |
| [`process_telemetry.rs`](src/process_telemetry.rs) · [`process_telemetry/`](src/process_telemetry) | bounded image subprocess telemetry ownership | [`tests.rs`](src/process_telemetry/tests.rs) |
| [`telemetry_boundary.rs`](src/telemetry_boundary.rs) | governed download/cache/retry boundaries | — |
| [`naming.rs`](src/naming.rs) | image naming | — |

## Public API

The image-build decision and materialization entry points consumed by `jackin-runtime`. The `construct` Dockerfile (operator-built base) lives under `docker/construct/`; this crate generates *derived* images on top of it.

## How to verify

```sh
cargo nextest run -p jackin-image
cargo clippy -p jackin-image --all-targets -- -D warnings
```
