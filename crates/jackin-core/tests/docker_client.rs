// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use jackin_core::{Compatibility, DOCKER_CLIENT, DockerPart, Platform, Sha256};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn lock_text(section: &str, key: &str) -> Result<String, Box<dyn std::error::Error>> {
    let lock_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../compatibility.lock.toml");
    let lock: toml::Table = std::fs::read_to_string(lock_path)?.parse()?;
    Ok(lock
        .get(section)
        .and_then(|table| table.get(key))
        .and_then(toml::Value::as_str)
        .ok_or_else(|| format!("no {section}.{key}"))?
        .to_owned())
}

fn is_date(text: &str) -> bool {
    let parts: Vec<&str> = text.split('-').collect();
    matches!(parts.as_slice(), [y, m, d]
        if y.len() == 4 && m.len() == 2 && d.len() == 2
            && parts.iter().all(|p| p.bytes().all(|b| b.is_ascii_digit())))
}

#[test]
fn docker_client_row_matches_the_compatibility_lock_and_pins_six_distinct_checksums() -> TestResult
{
    let pinned = Compatibility::pinned()?;
    let parts: [(&DockerPart, &str, &str); 3] = [
        (&DOCKER_CLIENT.cli, "cli", &pinned.docker_cli_version),
        (
            &DOCKER_CLIENT.buildx,
            "buildx",
            &pinned.docker_buildx_version,
        ),
        (
            &DOCKER_CLIENT.compose,
            "compose",
            &pinned.docker_compose_version,
        ),
    ];
    let mut digests = BTreeSet::new();
    for (part, name, pinned_version) in parts {
        assert_eq!(
            part.version,
            lock_text("docker", &format!("{name}_version"))?
        );
        assert_eq!(part.version, pinned_version, "{name}");
        assert!(
            is_date(&lock_text("docker", &format!("{name}_date"))?),
            "{name}"
        );
        assert_eq!(part.targets.len(), 2, "{name}");
        for platform in [Platform::LinuxArm64, Platform::LinuxAmd64] {
            let target = part.target(platform).ok_or("missing platform")?;
            let sha = target.sha256;
            assert!(Sha256::parse(sha).is_ok(), "{name} {sha}");
            assert!(
                sha.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "{name} {sha}"
            );
            digests.insert(sha);
        }
        assert_eq!(part.url_template.matches("{triple}").count(), 1, "{name}");
        assert!(part.url_template.starts_with("https://"), "{name}");
    }
    assert_eq!(digests.len(), 6);
    assert_eq!(DOCKER_CLIENT.cli_members, ["docker/docker"]);
    Ok(())
}
