// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use jackin_core::Secret;
use jackin_usage::browser::{BrowserError, BrowserLogin, BrowserProvider};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use url::Url;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn parameter(login: &BrowserLogin, key: &str) -> Result<String> {
    Ok(login
        .authorization_url()
        .query_pairs()
        .find(|(name, _)| name == key)
        .ok_or("missing authorization parameter")?
        .1
        .into_owned())
}

#[tokio::test]
async fn pasted_authorization_is_bound_to_the_original_challenge() -> Result<()> {
    for callback_url in [false, true] {
        let login = BrowserLogin::start(BrowserProvider::Claude).await?;
        let state = parameter(&login, "state")?;
        let redirect = parameter(&login, "redirect_uri")?;
        let challenge = parameter(&login, "code_challenge")?;
        let input = if callback_url {
            format!("{redirect}?code=example&state={state}")
        } else {
            format!("example#{state}")
        };
        let (sender, receiver) = oneshot::channel();
        sender
            .send(Secret::new(input))
            .map_err(|_| "paste closed")?;
        let grant = login.wait(receiver, &CancellationToken::new()).await?;
        assert_eq!(grant.code.expose(), "example");
        assert_eq!(grant.state.expose(), state);
        assert_eq!(grant.redirect, redirect);
        assert_eq!(
            challenge,
            URL_SAFE_NO_PAD.encode(Sha256::digest(grant.verifier.expose()))
        );
    }
    for input in ["example", "example#wrong"] {
        let login = BrowserLogin::start(BrowserProvider::Claude).await?;
        let (sender, receiver) = oneshot::channel();
        sender
            .send(Secret::new(input.to_owned()))
            .map_err(|_| "paste closed")?;
        assert!(matches!(
            login.wait(receiver, &CancellationToken::new()).await,
            Err(BrowserError::State)
        ));
    }
    Ok(())
}

async fn get(port: u16, path: &str) -> Result<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await?;
    stream
        .write_all(format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").as_bytes())
        .await?;
    let mut response = String::new();
    stream.read_to_string(&mut response).await?;
    Ok(response)
}

#[tokio::test]
async fn unrelated_callbacks_leave_the_listener_available() -> Result<()> {
    let login = BrowserLogin::start(BrowserProvider::Claude).await?;
    let redirect = Url::parse(&parameter(&login, "redirect_uri")?)?;
    assert_eq!(redirect.host_str(), Some("localhost"));
    assert_eq!(redirect.path(), "/callback");
    let port = redirect.port().ok_or("missing port")?;
    let state = parameter(&login, "state")?;
    let (_sender, receiver) = oneshot::channel();
    let pending =
        tokio::spawn(async move { login.wait(receiver, &CancellationToken::new()).await });
    assert!(get(port, "/elsewhere").await?.starts_with("HTTP/1.1 404"));
    assert!(
        get(port, "/callback?code=a&state=wrong")
            .await?
            .starts_with("HTTP/1.1 400")
    );
    assert!(
        get(port, &format!("/callback?code=authorized&state={state}"))
            .await?
            .starts_with("HTTP/1.1 200")
    );
    assert_eq!(pending.await??.code.expose(), "authorized");
    assert!(TcpStream::connect(("127.0.0.1", port)).await.is_err());
    Ok(())
}

#[tokio::test]
async fn cancellation_of_chatgpt_sign_in_releases_its_fixed_port() -> Result<()> {
    let login = BrowserLogin::start(BrowserProvider::ChatGpt).await?;
    assert_eq!(
        parameter(&login, "redirect_uri")?,
        "http://localhost:1455/auth/callback"
    );
    let (_sender, receiver) = oneshot::channel();
    let cancel = CancellationToken::new();
    let pending = tokio::spawn({
        let cancel = cancel.clone();
        async move { login.wait(receiver, &cancel).await }
    });
    cancel.cancel();
    assert!(matches!(pending.await?, Err(BrowserError::Cancelled)));
    let _next = BrowserLogin::start(BrowserProvider::ChatGpt).await?;
    Ok(())
}
