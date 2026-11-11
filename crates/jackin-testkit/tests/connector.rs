// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use jackin_core::{Config, GatewayName};
use jackin_openshell::GatewayError;
use jackin_session::{ConnectError, GatewayConnector};
use jackin_testkit::{FakeGateway, ScriptedConnector};
use tokio_util::sync::CancellationToken;

type Fallible<T> = Result<T, Box<dyn std::error::Error>>;

const CONFIG: &str = r#"[gateways.alpha]
openshell_workspace = "default"
builder = "b"
platform = "linux/arm64"

[gateways.beta]
openshell_workspace = "default"
builder = "b"
platform = "linux/arm64"
"#;

fn fake(name: &str) -> Fallible<Arc<FakeGateway>> {
    Ok(Arc::new(FakeGateway::new(
        GatewayName::parse(name)?,
        "default".parse()?,
    )))
}

fn down() -> GatewayError {
    GatewayError::Unreachable {
        endpoint: "alpha".to_owned(),
        message: "refused".to_owned(),
    }
}

#[tokio::test]
async fn a_scripted_gateway_fails_as_scripted_then_connects_and_keeps_connecting() -> Fallible<()> {
    let config = Config::parse(CONFIG)?;
    let (alpha, beta) = (GatewayName::parse("alpha")?, GatewayName::parse("beta")?);
    let connector = ScriptedConnector::new()
        .gateway(fake("alpha")?, vec![Some(down()), None])
        .gateway(fake("beta")?, Vec::new());
    let cancel = CancellationToken::new();
    let entry = |name: &GatewayName| config.gateway(name).ok_or("no gateway entry");

    let first = connector.connect(&alpha, entry(&alpha)?, &cancel).await;
    assert!(
        matches!(
            &first,
            Err(ConnectError::Gateway(GatewayError::Unreachable { .. }))
        ),
        "{first:?}"
    );
    let second = connector.connect(&alpha, entry(&alpha)?, &cancel).await?;
    assert_eq!(second.name(), &alpha);
    let third = connector.connect(&alpha, entry(&alpha)?, &cancel).await?;
    assert_eq!(third.name(), &alpha);
    assert_eq!(connector.calls(&alpha), 3);

    let other = connector.connect(&beta, entry(&beta)?, &cancel).await?;
    assert_eq!(other.name(), &beta);
    assert_eq!(connector.calls(&beta), 1);
    Ok(())
}

#[tokio::test]
async fn an_unknown_gateway_and_a_cancelled_token_are_connection_errors() -> Fallible<()> {
    let config = Config::parse(CONFIG)?;
    let (alpha, beta) = (GatewayName::parse("alpha")?, GatewayName::parse("beta")?);
    let connector = ScriptedConnector::new().gateway(fake("alpha")?, Vec::new());
    let cancel = CancellationToken::new();
    let unknown = connector
        .connect(&beta, config.gateway(&beta).ok_or("no beta")?, &cancel)
        .await;
    assert!(
        matches!(
            &unknown,
            Err(ConnectError::Gateway(GatewayError::Unreachable { .. }))
        ),
        "{unknown:?}"
    );
    cancel.cancel();
    let cancelled = connector
        .connect(&alpha, config.gateway(&alpha).ok_or("no alpha")?, &cancel)
        .await;
    assert!(
        matches!(&cancelled, Err(error) if error.is_cancelled()),
        "{cancelled:?}"
    );
    assert_eq!(connector.calls(&alpha), 0);
    Ok(())
}
