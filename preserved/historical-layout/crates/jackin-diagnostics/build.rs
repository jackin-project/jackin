// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

fn main() {
    let version = rustc_version::version_meta().map_or_else(
        |_| "rustc unknown".to_owned(),
        |metadata| format!("rustc {}", metadata.semver),
    );
    println!("cargo:rustc-env=RUSTC_VERSION={version}");
}
