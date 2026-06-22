//! Run-level diagnostics — re-exported from `jackin-diagnostics`.

pub(crate) use jackin_diagnostics::{
    RunDiagnostics, active_run, configured_endpoint_summary, prune_old_runs,
    unsupported_otlp_protocol,
};
