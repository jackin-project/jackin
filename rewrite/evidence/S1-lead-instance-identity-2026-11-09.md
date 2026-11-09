# S1 lead instance identity correction

Base: `26bfd4605d24fc491f9dce163e03ce706f4ef605`. This unit fixes hosted P1 `4509243325`.

The canonical representation keeps the operator-selected lead first and sorts only secondaries. Multi-instance hashes include `lead-first-v1:` before the comma-joined canonical items; single-instance hashes retain their prior recipe. The shared manifest reader validates typed instances, catalog model constraints and the current managed label before returning canonical names to attach, integration, listing and stop. Direct attach without an agent also validates the manifest.

Independent evidence retained in `attach-instance-identity-20261109-v1` under the project verification archive:

- Original source exposed the lead-order failure. Six isolated v2 controls failed semantically for missing domain separation, lead-insensitive identity, wrong attach routing, wrong integration selection, missing model validation and missing hash binding. Their transfer to this source is limited to unchanged canonical-input production paths.
- The subsequent canonical-reader regression failed with `[omp, grok, codex]` instead of `[omp, codex, grok]`, then passed after reuse of the same canonical sequence. T03 was parsed fixture metadata only.
- Independent final publication source: four core and ten runtime tests passed; manifest hashes matched before and after. Receipt SHA-256: `27af9095aff09557e90b5ec069939a4fc3732b75a83c3be0379266eb96c9c56b`.
- The final split preserves the complete earlier candidate and its secondary Codex work. The publication excludes that configuration change because its materialized default model conflicts with prior live ChatGPT evidence. P2 remains active.

Synthetic external gateway metadata alignment isolates retry differences to the agents label; it is not an authentic historic sandbox. These hermetic checks prove the specified state transitions, not live Codex startup, model availability, atomic sandbox incarnation protection, final G-FULL, or whole-goal acceptance. Root publication gates are recorded separately.

Production file SHA-256:

- `crates/jackin-core/src/instance.rs`: `14c3b0cecaa7903a0d750d0b3d7217d2690468f0e9b1c7c326ac90f2c042d21e`
- `crates/jackin-session/src/attach.rs`: `f114d383021106035621f6bd0390949a31d8c8094dcee878c5190063465f4765`
- `crates/jackin-session/src/multi_session.rs`: `e4f0db45c58b77851b685fabaf601676c23426c1e417e9570f5f67235ea9a1ba`
