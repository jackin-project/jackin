# jackin-lints

- **Never make this a workspace member.** Root `Cargo.toml` must keep
  `exclude = ["crates/jackin-lints"]`. Main PR lanes must not compile it.
- **Nightly pin is the API contract.** Bump `rust-toolchain` only when dylint /
  rustc-private APIs require it; treat lane reds on nightly churn as chores,
  never PR blockers (advisory posture).
- First lint: `render_thread_purity`. Follow-ups (foundational Debug/sealed,
  config field consistency, telemetry discipline) are separate small plans.
