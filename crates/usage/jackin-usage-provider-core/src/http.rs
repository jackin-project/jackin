// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

mod relay;
pub use relay::{HubAddress, HubProvider};
use std::fmt;
use std::sync::OnceLock;
use std::time::Duration;

use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderName, HeaderValue, RETRY_AFTER};
use reqwest::redirect::Policy;
use reqwest::{Client, Method, RequestBuilder, Url};
use serde_json::Value;
use time::{
    OffsetDateTime, UtcOffset, format_description::well_known::Rfc2822, macros::format_description,
};

use crate::error::ProviderError;

const MAX_BODY_BYTES: u64 = 1 << 20;
const AGENT: &str = concat!("jackin/", env!("CARGO_PKG_VERSION"));

static SECURE: OnceLock<Option<Client>> = OnceLock::new();
static PLAIN: OnceLock<Option<Client>> = OnceLock::new();

#[derive(Clone)]
pub struct Http {
    timeout: Duration,
    base: Option<Url>,
    proxied: Option<Client>,
    hub: Option<std::sync::Arc<relay::Relay>>,
    claude_budget: Option<std::sync::Arc<dyn ClaudeRequestBudget>>,
}

pub trait ClaudeRequestBudget: Send + Sync {
    fn before(&self) -> Result<(String, i64), ProviderError>;
    fn after(
        &self,
        ticket: (String, i64),
        profile: bool,
        response: &Result<Value, ProviderError>,
    ) -> Result<(), ProviderError>;
}

#[derive(Clone, Copy)]
pub enum Auth<'a> {
    Bearer(&'a str),
    Raw(&'a str),
}

pub enum PostFields<'a> {
    Json(&'a Value),
    Form(&'a [(&'a str, &'a str)]),
}

pub struct JsonReply {
    pub status: u16,
    pub body: Value,
    pub retry_after_seconds: Option<u64>,
    headers: reqwest::header::HeaderMap,
}

impl fmt::Debug for Auth<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Auth(<redacted>)")
    }
}

impl fmt::Debug for PostFields<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PostFields(<redacted>)")
    }
}

impl fmt::Debug for JsonReply {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JsonReply")
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

impl JsonReply {
    pub fn header(&self, key: &str) -> Option<&str> {
        self.headers.get(key)?.to_str().ok()
    }

    pub fn ensure_success(&self) -> Result<(), ProviderError> {
        check_status(self.status, self.retry_after_seconds)
    }
}

fn build(roots: bool) -> Option<Client> {
    install_crypto_provider();
    let builder = Client::builder().redirect(Policy::none()).user_agent(AGENT);
    let builder = if roots {
        builder
    } else {
        builder.tls_certs_only([])
    };
    builder.build().ok()
}

fn client_for(url: &Url) -> Result<&'static Client, ProviderError> {
    let secure = url.scheme() == "https";
    let cell = if secure { &SECURE } else { &PLAIN };
    cell.get_or_init(|| build(secure))
        .as_ref()
        .ok_or(ProviderError::Network("the HTTP client could not be built"))
}

impl fmt::Debug for Http {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Http")
            .field("timeout", &self.timeout)
            .field("proxied", &self.proxied.is_some())
            .finish_non_exhaustive()
    }
}
impl Http {
    pub fn new(timeout: Duration) -> Http {
        Http {
            timeout,
            base: None,
            proxied: None,
            hub: None,
            claude_budget: None,
        }
    }

    #[must_use]
    pub fn with_claude_budget(mut self, budget: std::sync::Arc<dyn ClaudeRequestBudget>) -> Self {
        self.claude_budget = Some(budget);
        self
    }

    pub fn with_proxy(mut self, address: &str) -> Result<Self, ProviderError> {
        let invalid =
            ProviderError::Unsupported("use an http, https or socks5 proxy URL with a host");
        let mut url = Url::parse(address).map_err(|_| invalid.clone())?;
        if !matches!(url.scheme(), "http" | "https" | "socks5")
            || url.host_str().is_none()
            || !matches!(url.path(), "" | "/")
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(invalid);
        }
        if url.scheme() == "socks5" && url.port().is_none() {
            url.set_port(Some(1080)).map_err(|()| invalid.clone())?;
        }
        let proxy = reqwest::Proxy::all(url).map_err(|_| invalid.clone())?;
        install_crypto_provider();
        self.proxied = Some(
            Client::builder()
                .redirect(Policy::none())
                .user_agent(AGENT)
                .proxy(proxy)
                .build()
                .map_err(|_| invalid)?,
        );
        Ok(self)
    }

    pub fn with_base(&self, base: &str) -> Result<Http, ProviderError> {
        let parsed =
            Url::parse(base).map_err(|_| ProviderError::Network("the base URL is not valid"))?;
        Ok(Http {
            timeout: self.timeout,
            base: Some(parsed),
            proxied: self.proxied.clone(),
            hub: self.hub.clone(),
            claude_budget: self.claude_budget.clone(),
        })
    }

    pub async fn get(
        &self,
        url: &str,
        auth: Auth<'_>,
        headers: &[(&str, &str)],
    ) -> Result<Value, ProviderError> {
        self.send(Method::GET, url, auth, headers, None).await
    }

    pub async fn post(
        &self,
        url: &str,
        auth: Auth<'_>,
        headers: &[(&str, &str)],
        body: &Value,
    ) -> Result<Value, ProviderError> {
        self.send(Method::POST, url, auth, headers, Some(body))
            .await
    }

    async fn send(
        &self,
        method: Method,
        url: &str,
        auth: Auth<'_>,
        headers: &[(&str, &str)],
        body: Option<&Value>,
    ) -> Result<Value, ProviderError> {
        let budget = self
            .claude_budget
            .as_ref()
            .filter(|_| url.starts_with("https://api.anthropic.com/"));
        let ticket = budget.map(|b| b.before()).transpose()?;
        let result = self.authorized_json(method, url, auth, headers, body).await;
        if let (Some(budget), Some(ticket)) = (budget, ticket) {
            budget.after(
                ticket,
                url == "https://api.anthropic.com/api/oauth/profile",
                &result,
            )?;
        }
        result
    }

    async fn authorized_json(
        &self,
        method: Method,
        url: &str,
        auth: Auth<'_>,
        headers: &[(&str, &str)],
        body: Option<&Value>,
    ) -> Result<Value, ProviderError> {
        if self.hub.is_some() {
            let reply = self.hub_request(method, url, headers, body).await?;
            reply.ensure_success()?;
            return Ok(reply.body);
        }
        let mut request = self.request(method, url)?;
        request = with_authorization(request, auth)?;
        for (name, value) in headers {
            request = with_header(request, name, value)?;
        }
        if let Some(body) = body {
            let bytes = serde_json::to_vec(body)
                .map_err(|_| ProviderError::Decode("the request body could not be encoded"))?;
            request = request.header(CONTENT_TYPE, "application/json").body(bytes);
        }
        let response = request.send().await.map_err(transport)?;
        check_status(response.status().as_u16(), retry_after_seconds(&response))?;
        serde_json::from_slice(&read_bytes(response).await?)
            .map_err(|_| ProviderError::Decode("the response is not valid JSON"))
    }

    fn request(&self, method: Method, url: &str) -> Result<RequestBuilder, ProviderError> {
        let target = self.target(url)?;
        let client = match &self.proxied {
            Some(client) => client,
            None => client_for(&target)?,
        };
        Ok(client
            .request(method, target)
            .timeout(self.timeout)
            .header(ACCEPT, "application/json"))
    }

    pub async fn post_fields(
        &self,
        url: &str,
        fields: PostFields<'_>,
    ) -> Result<JsonReply, ProviderError> {
        let request = self.request(Method::POST, url)?;
        let request = match fields {
            PostFields::Json(value) => request.header(CONTENT_TYPE, "application/json").body(
                serde_json::to_vec(value)
                    .map_err(|_| ProviderError::Decode("invalid request body"))?,
            ),
            PostFields::Form(pairs) => request.form(pairs),
        };
        json_reply(request).await
    }

    pub fn rewrites_origin(&self) -> bool {
        self.base.is_some()
    }

    pub async fn post_reply(
        &self,
        url: &str,
        auth: Auth<'_>,
        headers: &[(&str, &str)],
        body: &Value,
    ) -> Result<JsonReply, ProviderError> {
        if self.hub.is_some() {
            return self
                .hub_request(Method::POST, url, headers, Some(body))
                .await;
        }
        let bytes = serde_json::to_vec(body)
            .map_err(|_| ProviderError::Decode("the request body could not be encoded"))?;
        let mut request = with_authorization(self.request(Method::POST, url)?, auth)?;
        for (name, value) in headers {
            request = with_header(request, name, value)?;
        }
        json_reply(request.header(CONTENT_TYPE, "application/json").body(bytes)).await
    }

    pub async fn get_reply(
        &self,
        url: &str,
        auth: Auth<'_>,
        headers: &[(&str, &str)],
    ) -> Result<JsonReply, ProviderError> {
        if self.hub.is_some() {
            return self.hub_request(Method::GET, url, headers, None).await;
        }
        let mut request = with_authorization(self.request(Method::GET, url)?, auth)?;
        for (name, value) in headers {
            request = with_header(request, name, value)?;
        }
        json_reply(request).await
    }

    fn target(&self, url: &str) -> Result<Url, ProviderError> {
        let mut parsed =
            Url::parse(url).map_err(|_| ProviderError::Network("the request URL is not valid"))?;
        let Some(base) = &self.base else {
            return Ok(parsed);
        };
        let invalid = || ProviderError::Network("the base URL cannot replace the origin");
        parsed.set_scheme(base.scheme()).map_err(|()| invalid())?;
        parsed.set_host(base.host_str()).map_err(|_| invalid())?;
        parsed.set_port(base.port()).map_err(|()| invalid())?;
        let prefix = base.path().trim_end_matches('/');
        if !prefix.is_empty() {
            let joined = format!("{prefix}{}", parsed.path());
            parsed.set_path(&joined);
        }
        Ok(parsed)
    }
}

async fn json_reply(request: RequestBuilder) -> Result<JsonReply, ProviderError> {
    let response = request.send().await.map_err(transport)?;
    let status = response.status();
    let retry_after_seconds = retry_after_seconds(&response);
    let headers = response.headers().clone();
    let bytes = read_bytes(response).await?;
    let body = match serde_json::from_slice(&bytes) {
        Ok(body) => body,
        Err(_) if !status.is_success() => Value::Null,
        Err(_) => return Err(ProviderError::Decode("the response is not valid JSON")),
    };
    Ok(JsonReply {
        status: status.as_u16(),
        body,
        headers,
        retry_after_seconds,
    })
}

fn check_status(status: u16, retry_after_seconds: Option<u64>) -> Result<(), ProviderError> {
    match status {
        200..=299 => Ok(()),
        401 => Err(ProviderError::Unauthorized),
        403 => Err(ProviderError::Forbidden),
        429 => Err(ProviderError::RateLimited {
            retry_after_seconds,
        }),
        other => Err(ProviderError::Status(other)),
    }
}

async fn read_bytes(mut response: reqwest::Response) -> Result<Vec<u8>, ProviderError> {
    let too_large = ProviderError::Decode("the response is larger than 1 MiB");
    if response
        .content_length()
        .is_some_and(|length| length > MAX_BODY_BYTES)
    {
        return Err(too_large);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(transport)? {
        if u64::try_from(bytes.len().saturating_add(chunk.len()))
            .map_or(true, |total| total > MAX_BODY_BYTES)
        {
            return Err(too_large);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn install_crypto_provider() {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _installed = rustls::crypto::aws_lc_rs::default_provider().install_default();
    }
}

fn with_authorization(
    request: RequestBuilder,
    auth: Auth<'_>,
) -> Result<RequestBuilder, ProviderError> {
    let text = match auth {
        Auth::Bearer(token) => format!("Bearer {token}"),
        Auth::Raw(token) => token.to_owned(),
    };
    let mut value = HeaderValue::from_str(&text)
        .map_err(|_| ProviderError::Decode("the credential cannot be sent in a header"))?;
    value.set_sensitive(true);
    Ok(request.header(AUTHORIZATION, value))
}

fn with_header(
    request: RequestBuilder,
    name: &str,
    value: &str,
) -> Result<RequestBuilder, ProviderError> {
    let name = HeaderName::from_bytes(name.as_bytes())
        .map_err(|_| ProviderError::Decode("a request header name is not valid"))?;
    let value = HeaderValue::from_str(value)
        .map_err(|_| ProviderError::Decode("a request header value is not valid"))?;
    Ok(request.header(name, value))
}

fn retry_after_seconds(response: &reqwest::Response) -> Option<u64> {
    let text = response.headers().get(RETRY_AFTER)?.to_str().ok()?.trim();
    retry_after_text(text, OffsetDateTime::now_utc())
}

pub fn parse_http_date(text: &str) -> Option<OffsetDateTime> {
    let text = text.trim();
    let at = OffsetDateTime::parse(text, &Rfc2822).ok()?;
    let fields = if let Some((prefix, _)) = text.split_once('(') {
        OffsetDateTime::parse(prefix.trim_end(), &Rfc2822).ok()?;
        prefix
    } else {
        text
    };
    if let Some((weekday, _)) = fields.split_once(',') {
        let expected = at
            .format(format_description!("[weekday repr:short]"))
            .ok()?;
        if !weekday.eq_ignore_ascii_case(&expected) {
            return None;
        }
    }
    let zone = fields.split_ascii_whitespace().next_back()?;
    if zone.starts_with(['-', '+']) {
        let offset = UtcOffset::parse(
            zone,
            format_description!("[offset_hour sign:mandatory][offset_minute]"),
        )
        .ok()?;
        if offset.whole_hours().unsigned_abs() >= 24 {
            return None;
        }
        return Some(at.replace_offset(offset));
    }
    Some(at)
}

pub fn retry_after_text(text: &str, now: OffsetDateTime) -> Option<u64> {
    let text = text.trim();
    if let Ok(seconds) = text.parse::<u64>() {
        return Some(seconds);
    }
    let at = parse_http_date(text)?;
    let delta = at - now;
    if delta.is_negative() {
        return None;
    }
    u64::try_from(delta.whole_seconds()).ok()
}

fn transport(error: reqwest::Error) -> ProviderError {
    use std::error::Error;
    let mut source = error.source();
    while let Some(cause) = source {
        if let Some(io) = cause.downcast_ref::<std::io::Error>()
            && matches!(
                io.kind(),
                std::io::ErrorKind::NetworkUnreachable | std::io::ErrorKind::NetworkDown
            )
        {
            return ProviderError::Network("no network path is available");
        }
        source = cause.source();
    }
    if error.is_timeout() {
        ProviderError::Network("the request timed out")
    } else if error.is_builder() {
        ProviderError::Network("the request could not be built")
    } else {
        ProviderError::Network("the connection failed")
    }
}
