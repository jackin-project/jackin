// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn ignoring_resolvers() -> PrepareResolvers<'static> {
    PrepareResolvers {
        auth_modes: &|_| AuthForwardMode::Ignore,
        sync_source_dirs: &|_| None,
    }
}

pub(super) fn simple_manifest(temp: &tempfile::TempDir) -> RoleManifest {
    std::fs::write(
        temp.path().join("jackin.role.toml"),
        r#"version = "v1alpha3"
dockerfile = "Dockerfile"

[claude]
plugins = []
"#,
    )
    .unwrap();
    std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM projectjackin/construct:0.1-trixie\n",
    )
    .unwrap();
    load_role_manifest(temp.path()).unwrap()
}

pub(super) fn amp_binding_with_cache(key: &str, cache: PathBuf) -> InstanceAuthBinding {
    let mut binding =
        InstanceAuthBinding::new(key, jackin_core::Agent::Amp, AuthForwardMode::Ignore, None);
    binding.xdg_roots = Some(jackin_config::XdgRoots {
        data: cache.join("data"),
        config: cache.join("config"),
        cache,
    });
    binding
}
