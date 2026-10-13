// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0
//! `Z.AI` quota fetch, team scope, and endpoint resolution.

use jackin_usage_provider_core::{
    env_value, normalize_url_or_host, provider_http_client, provider_request,
};

use super::ZaiQuotaResponse;

/// CN team scope: `?type=2` on the quota path plus the
/// `Bigmodel-Organization` / `Bigmodel-Project` headers.
#[derive(Debug, Clone, Default)]
pub struct ZaiTeamScope {
    pub quota_type: Option<String>,
    pub organization: Option<String>,
    pub project: Option<String>,
}

pub fn resolve_zai_team_scope() -> ZaiTeamScope {
    zai_team_scope_from(
        env_value("ZAI_QUOTA_TYPE").as_deref(),
        env_value("BIGMODEL_ORGANIZATION")
            .or_else(|| env_value("ZAI_TEAM_ORG"))
            .as_deref(),
        env_value("BIGMODEL_PROJECT")
            .or_else(|| env_value("ZAI_TEAM_PROJECT"))
            .as_deref(),
    )
}

pub(crate) fn zai_team_scope_from(
    quota_type: Option<&str>,
    organization: Option<&str>,
    project: Option<&str>,
) -> ZaiTeamScope {
    let clean = |value: Option<&str>| {
        value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    ZaiTeamScope {
        quota_type: clean(quota_type),
        organization: clean(organization),
        project: clean(project),
    }
}

impl ZaiTeamScope {
    pub fn active(&self) -> bool {
        self.quota_type.is_some() || self.organization.is_some() || self.project.is_some()
    }

    pub fn query(&self) -> Option<&str> {
        self.quota_type.as_deref()
    }
}

pub fn fetch_zai_usage(token: &str) -> Result<ZaiQuotaResponse, String> {
    let mut url = resolve_zai_quota_url();
    let scope = resolve_zai_team_scope();
    if let Some(quota_type) = scope.query() {
        url = format!("{url}?type={quota_type}");
    }
    let quota: ZaiQuotaResponse = provider_request(
        jackin_telemetry::schema::enums::ProviderName::Zai,
        "GET",
        "/api/monitor/usage/quota/limit",
        || {
            let client = provider_http_client()?;
            let mut request = client
                .get(&url)
                .bearer_auth(token)
                .header(reqwest::header::ACCEPT, "application/json");
            if let Some(organization) = scope.organization.as_deref() {
                request = request.header("Bigmodel-Organization", organization);
            }
            if let Some(project) = scope.project.as_deref() {
                request = request.header("Bigmodel-Project", project);
            }
            let response = request
                .send()
                .map_err(|err| format!("Z.AI quota request failed: {err}"))?;
            let status = response.status();
            if !status.is_success() {
                return Err(format!("Z.AI quota HTTP {status}"));
            }
            response
                .json::<ZaiQuotaResponse>()
                .map_err(|err| format!("Z.AI quota decode failed: {err}"))
        },
    )?;
    // HTTP success with `success: false` is a valid key without a GLM Coding
    // Plan — surfaced distinctly so it is never mistaken for key presence.
    if quota.success == Some(false) {
        let detail = quota.msg.unwrap_or_else(|| "quota rejected".to_owned());
        return Err(format!("Z.AI key has no GLM Coding Plan ({detail})"));
    }
    if quota.code.is_some_and(|code| code != 200) {
        let detail = quota.msg.unwrap_or_else(|| "unknown error".to_owned());
        return Err(format!("Z.AI quota rejected response: {detail}"));
    }
    // HTTP success with no windows: a team key missing its selectors, or no
    // plan entitlement — never rendered as empty-but-fresh quota.
    let empty = quota
        .data
        .as_ref()
        .is_none_or(|data| data.limits.is_empty());
    if empty {
        return Err(if scope.active() {
            "Z.AI quota returned no usage windows for team scope; verify organization/project selectors".to_owned()
        } else {
            "Z.AI quota returned no usage windows; verify Coding Plan entitlement".to_owned()
        });
    }
    Ok(quota)
}

pub fn resolve_zai_quota_url() -> String {
    let override_url = env_value("ZAI_QUOTA_URL").or_else(|| env_value("Z_AI_QUOTA_URL"));
    let host = env_value("ZAI_API_HOST")
        .or_else(|| env_value("Z_AI_API_HOST"))
        .unwrap_or_else(|| "https://api.z.ai".to_owned());
    resolve_zai_quota_url_from(override_url.as_deref(), Some(&host))
}

pub fn resolve_zai_quota_url_from(override_url: Option<&str>, host: Option<&str>) -> String {
    if let Some(url) = override_url {
        return normalize_url_or_host(url, "");
    }
    let host = host.unwrap_or("https://api.z.ai");
    normalize_url_or_host(&zai_quota_host(host), "api/monitor/usage/quota/limit")
}

pub fn zai_quota_host(value: &str) -> String {
    let normalized = normalize_url_or_host(value, "");
    let Ok(mut url) = url::Url::parse(&normalized) else {
        return normalized;
    };
    url.set_path("");
    url.set_query(None);
    url.set_fragment(None);
    url.to_string().trim_end_matches('/').to_owned()
}
