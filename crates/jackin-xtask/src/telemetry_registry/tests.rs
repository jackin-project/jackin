// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::RegistryFile;

#[test]
fn parses_registry_attributes() {
    let registry: RegistryFile =
        serde_yaml_ng::from_str("groups:\n  - attributes:\n      - id: app.mode\n")
            .expect("fixture must parse");
    assert_eq!(registry.groups[0].attributes[0].id, "app.mode");
}
