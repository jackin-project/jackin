// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-FileCopyrightText: 2026 Robin Ebers
// SPDX-License-Identifier: Apache-2.0 AND MIT

use super::{Auth, Http, JsonReply, ProviderError, json_reply, with_authorization};
use reqwest::{
    Method, Url,
    header::{CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue},
};
use serde_json::{Value, json};
use std::{fmt, net::IpAddr, sync::Arc};
use zeroize::Zeroizing;

#[derive(Debug, Clone)]
pub struct HubAddress(Url);

impl HubAddress {
    pub fn parse(input: &str) -> Result<Self, ProviderError> {
        let invalid =
            ProviderError::Unsupported("enter an HTTPS hub address, or a local HTTP address");
        let input = input.trim();
        let address = if input.contains("://") {
            input.to_owned()
        } else {
            format!("https://{input}")
        };
        let mut url = Url::parse(&address).map_err(|_| invalid.clone())?;
        let host = url.host_str().ok_or(invalid.clone())?;
        if !(url.scheme() == "https" || url.scheme() == "http" && local(host)) {
            return Err(invalid);
        }
        url.set_query(None);
        url.set_fragment(None);
        url.set_username("").map_err(|()| invalid.clone())?;
        url.set_password(None).map_err(|()| invalid)?;
        let mut path = url.path().trim_end_matches('/').to_owned();
        for suffix in [
            "/management.html",
            "/v0/management",
            "/v8/management",
            "/v1",
        ] {
            while path.to_ascii_lowercase().ends_with(suffix) {
                path.truncate(path.len() - suffix.len());
            }
        }
        url.set_path(&path);
        Ok(Self(url))
    }
    pub fn as_str(&self) -> &str {
        self.0.as_str().trim_end_matches('/')
    }
    pub fn logins_url(&self) -> String {
        format!("{}/v0/management/auth-files", self.as_str())
    }
}

fn local(host: &str) -> bool {
    if host == "localhost"
        || host
            .rsplit_once('.')
            .is_some_and(|(_, suffix)| suffix.eq_ignore_ascii_case("local"))
        || host.ends_with(".ts.net")
    {
        return true;
    }
    match host.trim_matches(['[', ']']).parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => {
            ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || matches!(ip.octets(), [100, 64..=127, _, _])
        }
        Ok(IpAddr::V6(ip)) => {
            ip.is_loopback() || ip.is_unicast_link_local() || ip.is_unique_local()
        }
        _ => false,
    }
}

#[derive(Clone, Copy, Debug)]
pub enum HubProvider {
    Claude,
    ChatGpt,
    Grok,
}

impl HubProvider {
    fn allows(self, url: &str) -> bool {
        let Ok(url) = Url::parse(url) else {
            return false;
        };
        if url.scheme() != "https"
            || url.port().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return false;
        }
        match self {
            Self::Claude => {
                url.host_str() == Some("api.anthropic.com")
                    && matches!(url.path(), "/api/oauth/profile" | "/api/oauth/usage")
            }
            Self::Grok => {
                url.host_str() == Some("cli-chat-proxy.grok.com")
                    && matches!(url.path(), "/v1/settings" | "/v1/billing")
            }
            Self::ChatGpt => {
                url.host_str() == Some("chatgpt.com")
                    && matches!(
                        url.path(),
                        "/backend-api/wham/usage"
                            | "/backend-api/wham/rate-limit-reset-credits"
                            | "/backend-api/wham/analytics/daily-workspace-usage-counts"
                            | "/backend-api/wham/usage/daily-workspace-user-token-usage-breakdown"
                            | "/backend-api/wham/usage/daily-workspace-user-credit-usage"
                    )
            }
        }
    }
}

pub(super) struct Relay {
    address: HubAddress,
    key: Zeroizing<String>,
    auth_index: String,
    provider: HubProvider,
}

impl fmt::Debug for Relay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Relay")
            .field("provider", &self.provider)
            .finish_non_exhaustive()
    }
}

impl Http {
    #[must_use]
    pub fn routed_via_hub(
        &self,
        address: HubAddress,
        key: String,
        auth_index: String,
        provider: HubProvider,
    ) -> Self {
        let mut http = self.clone();
        http.hub = Some(Arc::new(Relay {
            address,
            key: Zeroizing::new(key),
            auth_index,
            provider,
        }));
        http
    }

    pub(super) async fn hub_request(
        &self,
        method: Method,
        url: &str,
        headers: &[(&str, &str)],
        body: Option<&Value>,
    ) -> Result<JsonReply, ProviderError> {
        let relay = self
            .hub
            .as_ref()
            .ok_or(ProviderError::Decode("missing usage hub"))?;
        let allowed = if method == Method::GET {
            body.is_none() && relay.provider.allows(url)
        } else {
            method == Method::POST && body.is_some_and(|v| reset_request(relay.provider, url, v))
        };
        if !allowed {
            return Err(ProviderError::Unsupported(
                "the hub login cannot make this usage request",
            ));
        }
        let mut forwarded = serde_json::Map::new();
        for &(name, value) in headers {
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "authorization" | "proxy-authorization" | "host" | "cookie"
            ) {
                return Err(ProviderError::Unsupported("invalid hub request header"));
            }
            forwarded.insert(name.to_owned(), Value::String(value.to_owned()));
        }
        forwarded.insert(
            "Authorization".into(),
            Value::String("Bearer $TOKEN$".into()),
        );
        let mut payload = json!({"auth_index":relay.auth_index,"method":method.as_str(),"url":url,"header":forwarded});
        if let Some(body) = body {
            payload["header"]["Content-Type"] = Value::String("application/json".into());
            payload["data"] = Value::String(body.to_string());
        }
        let request = self.request(
            Method::POST,
            &format!("{}/v0/management/api-call", relay.address.as_str()),
        )?;
        let request = with_authorization(request, Auth::Bearer(&relay.key))?
            .header(CONTENT_TYPE, "application/json")
            .body(payload.to_string());
        let outer = json_reply(request).await?;
        if matches!(outer.status, 401 | 403) {
            return Err(ProviderError::HubRefused);
        }
        outer.ensure_success()?;
        let invalid = ProviderError::Decode("invalid hub usage response");
        let status = outer.body["status_code"]
            .as_u64()
            .and_then(|n| u16::try_from(n).ok())
            .ok_or(invalid.clone())?;
        let headers = unwrap_headers(&outer.body["header"]);
        let retry_after_seconds = headers
            .get("retry-after")
            .and_then(|value| value.to_str().ok())
            .and_then(|s| super::retry_after_text(s, chrono::Utc::now()));
        let body = outer.body["body"]
            .as_str()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(Value::Null);
        Ok(JsonReply {
            status,
            body,
            retry_after_seconds,
            headers,
        })
    }
}

fn reset_request(provider: HubProvider, target: &str, body: &Value) -> bool {
    let Ok(url) = Url::parse(target) else {
        return false;
    };
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return false;
    }
    let Some(fields) = body.as_object() else {
        return false;
    };
    let uuid = |v: Option<&str>| v.is_some_and(|v| uuid::Uuid::parse_str(v).is_ok());
    match provider {
        HubProvider::Claude => {
            let org = url
                .path()
                .strip_prefix("/api/organizations/")
                .and_then(|p| p.strip_suffix("/reset_rate_limits"));
            url.host_str() == Some("api.anthropic.com")
                && uuid(org)
                && fields.len() == 3
                && body["program"] == "cedar_ember"
                && uuid(body["request_id"].as_str())
                && body["grant_id"].as_str().is_some_and(|s| {
                    (1..=40).contains(&s.len())
                        && s.bytes().all(|b| {
                            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-')
                        })
                })
        }
        HubProvider::ChatGpt => {
            url.host_str() == Some("chatgpt.com")
                && url.path() == "/backend-api/wham/rate-limit-reset-credits/consume"
                && fields.len() == 2
                && uuid(body["redeem_request_id"].as_str())
                && body["credit_id"].as_str().is_some_and(|s| !s.is_empty())
        }
        HubProvider::Grok => false,
    }
}

fn unwrap_headers(fields: &Value) -> HeaderMap {
    let mut out = HeaderMap::new();
    for (name, values) in fields.as_object().into_iter().flatten() {
        let Ok(name) = HeaderName::from_bytes(name.as_bytes()) else {
            continue;
        };
        for value in values
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .filter_map(|s| HeaderValue::from_str(s).ok())
        {
            out.append(name.clone(), value);
        }
    }
    out
}
