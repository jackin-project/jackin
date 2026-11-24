// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-FileCopyrightText: 2026 Robin Ebers
// SPDX-License-Identifier: Apache-2.0 AND MIT

use std::net::Ipv4Addr;
use std::time::Duration;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use jackin_core::Secret;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use url::Url;

pub(crate) const CLAUDE_CLIENT: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
pub(crate) const CHATGPT_CLIENT: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub(crate) const CLAUDE_SCOPES: &str = "user:profile user:inference";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserProvider {
    Claude,
    ChatGpt,
}

#[derive(Debug, Error)]
pub enum BrowserError {
    #[error("could not bind the local callback listener")]
    Bind,
    #[error("could not generate a secure sign-in challenge")]
    Random,
    #[error("invalid authorization URL")]
    Url,
    #[error("the sign-in response did not match this request")]
    State,
    #[error("the authorization code was missing or invalid")]
    Code,
    #[error("authorization was refused")]
    Refused,
    #[error("sign-in cancelled")]
    Cancelled,
    #[error("the local callback listener failed")]
    Listener,
}

pub struct BrowserLogin {
    listener: TcpListener,
    authorize: Url,
    redirect: String,
    state: Secret,
    verifier: Secret,
    path: &'static str,
}

#[derive(Debug)]
pub struct BrowserGrant {
    pub code: Secret,
    pub verifier: Secret,
    pub state: Secret,
    pub redirect: String,
}

impl std::fmt::Debug for BrowserLogin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BrowserLogin")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl BrowserLogin {
    pub async fn start(provider: BrowserProvider) -> Result<Self, BrowserError> {
        let (port, path, endpoint, client_id, scope) = match provider {
            BrowserProvider::Claude => (
                0,
                "/callback",
                "https://claude.ai/oauth/authorize",
                CLAUDE_CLIENT,
                CLAUDE_SCOPES,
            ),
            BrowserProvider::ChatGpt => (
                1455,
                "/auth/callback",
                "https://auth.openai.com/oauth/authorize",
                CHATGPT_CLIENT,
                "openid profile email offline_access",
            ),
        };
        let mut login = Self::listen(port, path, "localhost", endpoint, client_id, scope).await?;
        match provider {
            BrowserProvider::Claude => {
                login
                    .authorize
                    .query_pairs_mut()
                    .append_pair("code", "true");
            }
            BrowserProvider::ChatGpt => {
                login.authorize.query_pairs_mut().extend_pairs([
                    ("id_token_add_organizations", "true"),
                    ("codex_cli_simplified_flow", "true"),
                    ("originator", "pi"),
                ]);
            }
        }
        Ok(login)
    }

    pub(crate) async fn listen(
        port: u16,
        path: &'static str,
        host: &'static str,
        endpoint: &str,
        client_id: &str,
        scope: &str,
    ) -> Result<Self, BrowserError> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
            .await
            .map_err(|_| BrowserError::Bind)?;
        let port = listener
            .local_addr()
            .map_err(|_| BrowserError::Listener)?
            .port();
        let redirect = format!("http://{host}:{port}{path}");
        let state = random_secret()?;
        let verifier = random_secret()?;
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.expose().as_bytes()));
        let mut authorize = Url::parse(endpoint).map_err(|_| BrowserError::Url)?;
        authorize.query_pairs_mut().extend_pairs([
            ("client_id", client_id),
            ("response_type", "code"),
            ("redirect_uri", &redirect),
            ("scope", scope),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
            ("state", state.expose()),
        ]);
        Ok(Self {
            listener,
            authorize,
            redirect,
            state,
            verifier,
            path,
        })
    }

    pub(crate) fn add_parameters(&mut self, pairs: &[(&str, &str)]) {
        self.authorize.query_pairs_mut().extend_pairs(pairs);
    }

    pub fn authorization_url(&self) -> &Url {
        &self.authorize
    }

    pub async fn wait(
        self,
        paste: oneshot::Receiver<Secret>,
        cancel: &CancellationToken,
    ) -> Result<BrowserGrant, BrowserError> {
        let code = tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(BrowserError::Cancelled),
            value = paste => parse_paste(value.map_err(|_| BrowserError::Cancelled)?.expose(), self.state.expose())?,
            result = self.callback() => result?,
        };
        Ok(self.grant(code))
    }

    pub async fn wait_callback(
        self,
        cancel: &CancellationToken,
    ) -> Result<BrowserGrant, BrowserError> {
        let code = tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(BrowserError::Cancelled),
            result = self.callback() => result?,
        };
        Ok(self.grant(code))
    }

    fn grant(self, code: Secret) -> BrowserGrant {
        BrowserGrant {
            code,
            verifier: self.verifier,
            state: self.state,
            redirect: self.redirect,
        }
    }

    async fn callback(&self) -> Result<Secret, BrowserError> {
        loop {
            let (mut stream, _) = self
                .listener
                .accept()
                .await
                .map_err(|_| BrowserError::Listener)?;
            if let Ok(Some(result)) =
                timeout(Duration::from_secs(3), self.receive(&mut stream)).await
            {
                return result;
            }
        }
    }

    async fn receive(&self, stream: &mut TcpStream) -> Option<Result<Secret, BrowserError>> {
        let mut header = Vec::new();
        let mut chunk = [0; 1024];
        while !header.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = stream.read(&mut chunk).await.ok()?;
            if n == 0 || header.len() + n > 16 * 1024 {
                return None;
            }
            header.extend_from_slice(&chunk[..n]);
        }
        let mut words = std::str::from_utf8(&header)
            .ok()?
            .lines()
            .next()?
            .split_ascii_whitespace();
        let method = words.next()?;
        let target = words.next()?;
        let url = Url::parse(&format!("http://localhost{target}")).ok()?;
        let (status, message, result) = if method != "GET" {
            ("405 Method Not Allowed", "Use GET.", None)
        } else if url.path() != self.path {
            ("404 Not Found", "Callback not found.", None)
        } else {
            match query_code(&url, self.state.expose()) {
                Ok(code) => (
                    "200 OK",
                    "Authorization received. Return to jackin.",
                    Some(Ok(code)),
                ),
                Err(BrowserError::State) => ("400 Bad Request", "Invalid sign-in state.", None),
                Err(error) => ("400 Bad Request", "Authorization failed.", Some(Err(error))),
            }
        };
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: text/plain\r\nCache-Control: no-store\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{message}",
            message.len()
        );
        let _write_result = stream.write_all(response.as_bytes()).await;
        result
    }
}

pub(crate) fn random_secret() -> Result<Secret, BrowserError> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(|_| BrowserError::Random)?;
    Ok(Secret::new(URL_SAFE_NO_PAD.encode(bytes)))
}

fn parse_paste(value: &str, state: &str) -> Result<Secret, BrowserError> {
    let trimmed = value.trim();
    if let Ok(url) = Url::parse(trimmed)
        && url.has_host()
    {
        return query_code(&url, state);
    }
    let (code, candidate) = trimmed.split_once('#').ok_or(BrowserError::State)?;
    if candidate != state {
        return Err(BrowserError::State);
    }
    checked_code(code)
}

fn query_code(url: &Url, state: &str) -> Result<Secret, BrowserError> {
    let field = |name| url.query_pairs().find(|(k, _)| k == name).map(|(_, v)| v);
    if field("state").as_deref() != Some(state) {
        return Err(BrowserError::State);
    }
    if field("error").is_some() {
        return Err(BrowserError::Refused);
    }
    checked_code(&field("code").ok_or(BrowserError::Code)?)
}

fn checked_code(code: &str) -> Result<Secret, BrowserError> {
    if code.is_empty() {
        return Err(BrowserError::Code);
    }
    Ok(Secret::new(code.to_owned()))
}
