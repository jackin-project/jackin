//! jackin-usage-provider-cursor: `Cursor` usage snapshot collection.
//!
//! **Architecture Invariant:** T3.
//! Entry point: [`cursor_snapshot`] — `Cursor` usage snapshot.
//!
//! `Cursor` personal allowance vs Enterprise Admin reporting.
//!
//! Two scopes, never mixed (see `ref-contracts-C.md` §2):
//!
//! * Personal (selected account token): `DashboardService` Connect RPC on
//!   `api2.cursor.sh` (`GetCurrentPeriodUsage`, `GetPlanInfo`,
//!   `GetCreditGrantsBalance`, `GetSandUsageStatus`) plus `cursor.com` session
//!   REST (`/api/usage?user=`, `/api/usage-summary`, `/api/auth/stripe`).
//!   A personal key never implies Enterprise reporting access.
//! * Enterprise Admin (`api.cursor.com/teams/spend|filtered-usage-events`):
//!   separate [`CursorEnterpriseScope`] with explicit admin auth; the personal
//!   snapshot never touches admin hosts. The events API is hourly aggregated
//!   and is never polled at the overview interval.
//!
//! Money discipline: actual charged amounts, estimated model cost, included
//! quota, and credit grants are separate buckets. Estimates never back a
//! Spend-slot headline.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "provider-adapter fixtures remain testable while production dispatch is broker-only"
    )
)]

mod auth;
mod events;
mod period;
mod plan;
mod requests;
mod rest;
mod sand;
mod snapshot;
mod summary;
mod teams;

#[cfg(test)]
pub(crate) use auth::CURSOR_DEFAULT_DASHBOARD_BASE;
pub use auth::CURSOR_SESSION_BASE;
pub use auth::{
    CursorAuth, cursor_auth_from_value, cursor_auth_path, cursor_cli_identity_from_value,
    cursor_dashboard_base, cursor_dashboard_url_with_base, cursor_default_base,
    cursor_identity_from_cli_config, cursor_user_id_from_token, load_cursor_auth,
    load_cursor_cli_identity,
};
pub use events::{
    CursorUsageEvents, cursor_events_buckets, fetch_cursor_usage_events, parse_cursor_usage_events,
};
pub use period::{
    CursorPeriodUsage, cursor_dashboard_post, cursor_needs_request_fallback, cursor_period_buckets,
    fetch_cursor_period_usage, parse_cursor_period_usage,
};
pub use plan::{
    cursor_credits_bucket, fetch_cursor_credit_grants, fetch_cursor_plan_info,
    parse_cursor_credit_grants, parse_cursor_plan_info,
};
pub use requests::{
    CursorRequestUsage, cursor_request_bucket, fetch_cursor_request_usage,
    parse_cursor_request_usage,
};
pub use rest::{cursor_rest_get, cursor_session_cookie};
pub use sand::{
    CursorSandUsage, cursor_sand_bucket, fetch_cursor_sand_usage, parse_cursor_sand_usage,
};
pub use snapshot::{
    cursor_enterprise_snapshot, cursor_profile_snapshot, cursor_snapshot, cursor_snapshot_with_auth,
};
pub use summary::{
    CursorUsageSummary, cursor_summary_buckets, fetch_cursor_stripe_balance,
    fetch_cursor_usage_summary, parse_cursor_stripe_balance, parse_cursor_usage_summary,
};
pub use teams::{
    CursorEnterpriseScope, CursorMemberSpend, CursorTeamSpend, cursor_team_spend_buckets,
    cursor_teams_events_url, cursor_teams_spend_url, fetch_cursor_team_spend,
    parse_cursor_team_spend,
};

#[cfg(test)]
mod tests;
