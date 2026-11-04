// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroU16;

use jackin_console::domain::docker::{EngineEndpoint, EngineState};
use jackin_console::domain::vocabulary::Vocabulary;
use jackin_console::domain::workspace::RoleSource;
use jackin_console::world::{DaemonHealth, Snapshot};
use jackin_core::Config;
use jackin_tui::{Facts, Fleet, GatewayReading, IdTable, ProjectInput, RoleDocs, project};

const CONFIG_PATH: &str = "/home/dev/.config/jackin/config.toml";

const GATEWAY: &str = "default_gateway = \"g\"\n\n[gateways.g]\nopenshell_workspace = \"default\"\nbuilder = \"b\"\nplatform = \"linux/arm64\"\n";

const ENGINE: &str =
    "\n[gateways.g.docker_engine]\nport = 2376\nclient_certs = \"/home/dev/certs\"\n";

const ROLES: &str = "\n[roles.pinned]\nsource = { git = \"https://github.com/acme/roles\", rev = \"main\" }\n\n[roles.floating]\nsource = { git = \"https://github.com/acme/other\" }\n";

fn projected(text: &str, fleet: &Fleet) -> Result<Snapshot, Box<dyn std::error::Error>> {
    let config = Config::parse(text)?;
    Ok(project(
        &ProjectInput {
            config: &config,
            fleet,
            facts: &Facts {
                version: "1.0.0".to_owned(),
                config_path: CONFIG_PATH.to_owned(),
                home: "/home/dev".to_owned(),
                now_secs: 0,
            },
            roles: &RoleDocs::new(),
        },
        &mut IdTable::new(),
    ))
}

#[test]
fn about_rows_use_the_vocabulary_labels() -> Result<(), Box<dyn std::error::Error>> {
    let words = Vocabulary::openshell();
    let snapshot = projected(GATEWAY, &Fleet::default())?;
    assert_eq!(
        snapshot.surfaces.about,
        [
            (words.about_product.clone(), "1.0.0".to_owned()),
            (words.about_gateway.clone(), "g".to_owned()),
            (words.about_config.clone(), CONFIG_PATH.to_owned()),
        ]
    );
    Ok(())
}

#[test]
fn about_has_no_gateway_row_without_a_gateway() -> Result<(), Box<dyn std::error::Error>> {
    let words = Vocabulary::openshell();
    let snapshot = projected("", &Fleet::default())?;
    assert_eq!(
        snapshot.surfaces.about,
        [
            (words.about_product.clone(), "1.0.0".to_owned()),
            (words.about_config.clone(), CONFIG_PATH.to_owned()),
        ]
    );
    Ok(())
}

#[test]
fn engine_is_a_tls_port() -> Result<(), Box<dyn std::error::Error>> {
    let snapshot = projected(&format!("{GATEWAY}{ENGINE}"), &Fleet::default())?;
    let docker = snapshot.surfaces.docker.ok_or("no docker setting")?;
    let port = NonZeroU16::new(2376).ok_or("zero port")?;
    assert_eq!(
        docker.engine,
        EngineState::Configured {
            endpoint: EngineEndpoint::TlsPort(port)
        }
    );
    Ok(())
}

#[test]
fn a_role_without_a_rev_has_no_branch() -> Result<(), Box<dyn std::error::Error>> {
    let snapshot = projected(&format!("{GATEWAY}{ROLES}"), &Fleet::default())?;
    let branch_of = |name: &str| {
        snapshot
            .roles
            .iter()
            .find(|role| role.name == name)
            .map(|role| role.source.clone())
    };
    assert_eq!(
        branch_of("pinned"),
        Some(RoleSource::Git {
            url: "https://github.com/acme/roles".to_owned(),
            branch: Some("main".to_owned()),
        })
    );
    assert_eq!(
        branch_of("floating"),
        Some(RoleSource::Git {
            url: "https://github.com/acme/other".to_owned(),
            branch: None,
        })
    );
    Ok(())
}

#[test]
fn health_follows_the_gateways() -> Result<(), Box<dyn std::error::Error>> {
    let reading = |failure: Option<&str>| -> Result<GatewayReading, Box<dyn std::error::Error>> {
        Ok(GatewayReading {
            name: jackin_core::GatewayName::parse("g")?,
            failure: failure.map(str::to_owned),
        })
    };
    let fleet = |readings: Vec<GatewayReading>| Fleet {
        gateways: readings,
        ..Fleet::default()
    };
    let healthy = projected(GATEWAY, &fleet(vec![reading(None)?]))?;
    assert_eq!(healthy.daemon_health, DaemonHealth::Healthy);
    let failed = projected(
        GATEWAY,
        &fleet(vec![reading(None)?, reading(Some("down"))?]),
    )?;
    assert_eq!(failed.daemon_health, DaemonHealth::Stale);
    assert_eq!(failed.index_error, None);
    Ok(())
}
