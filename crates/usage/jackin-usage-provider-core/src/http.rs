// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderName, HeaderValue, RETRY_AFTER};
use reqwest::redirect::Policy;
use reqwest::{Client, Method, RequestBuilder, StatusCode, Url};
use serde_json::Value;

use crate::error::ProviderError;

const AGENT: &str = concat!("jackin/", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Debug)]
pub struct Http {
    client: Client,
    base: Option<Url>,
}

impl Http {
    pub fn new(timeout: Duration) -> Result<Http, ProviderError> {
        let client = Client::builder()
            .timeout(timeout)
            .redirect(Policy::none())
            .build()
            .map_err(|_| ProviderError::Network("the HTTP client could not be built"))?;
        Ok(Http { client, base: None })
    }

    pub fn with_base(&self, base: &str) -> Result<Http, ProviderError> {
        let parsed =
            Url::parse(base).map_err(|_| ProviderError::Network("the base URL is not valid"))?;
        Ok(Http {
            client: self.client.clone(),
            base: Some(parsed),
        })
    }

    pub async fn get(
        &self,
        url: &str,
        token: &str,
        headers: &[(&str, &str)],
    ) -> Result<Value, ProviderError> {
        self.send(Method::GET, url, token, headers, None).await
    }

    pub async fn post(
        &self,
        url: &str,
        token: &str,
        headers: &[(&str, &str)],
        body: &Value,
    ) -> Result<Value, ProviderError> {
        self.send(Method::POST, url, token, headers, Some(body))
            .await
    }

    async fn send(
        &self,
        method: Method,
        url: &str,
        token: &str,
        headers: &[(&str, &str)],
        body: Option<&Value>,
    ) -> Result<Value, ProviderError> {
        let target = self.target(url)?;
        let mut request = self
            .client
            .request(method, target)
            .header(ACCEPT, "application/json")
            .header(reqwest::header::USER_AGENT, AGENT);
        request = with_authorization(request, token)?;
        for (name, value) in headers {
            request = with_header(request, name, value)?;
        }
        if let Some(body) = body {
            let bytes = serde_json::to_vec(body)
                .map_err(|_| ProviderError::Decode("the request body could not be encoded"))?;
            request = request.header(CONTENT_TYPE, "application/json").body(bytes);
        }
        let response = request.send().await.map_err(transport)?;
        let status = response.status();
        let retry_after = retry_after_seconds(&response);
        match status {
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                return Err(ProviderError::Unauthorized);
            }
            StatusCode::TOO_MANY_REQUESTS => {
                return Err(ProviderError::RateLimited {
                    retry_after_seconds: retry_after,
                });
            }
            other if !other.is_success() => return Err(ProviderError::Status(other.as_u16())),
            _ => {}
        }
        let bytes = response.bytes().await.map_err(transport)?;
        serde_json::from_slice(&bytes)
            .map_err(|_| ProviderError::Decode("the response is not valid JSON"))
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

fn with_authorization(
    request: RequestBuilder,
    token: &str,
) -> Result<RequestBuilder, ProviderError> {
    let mut value = HeaderValue::from_str(&format!("Bearer {token}"))
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
    response
        .headers()
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|text| text.trim().parse().ok())
}

fn transport(error: reqwest::Error) -> ProviderError {
    if error.is_timeout() {
        ProviderError::Network("the request timed out")
    } else if error.is_builder() {
        ProviderError::Network("the request could not be built")
    } else {
        ProviderError::Network("the connection failed")
    }
}
