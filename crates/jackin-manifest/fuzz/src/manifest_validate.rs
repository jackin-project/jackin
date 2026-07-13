//! Fuzz: parse-then-validate role manifests — no panic.
#![no_main]
use libfuzzer_sys::fuzz_target;
use jackin_manifest::RoleManifest;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else { return; };
    let Ok(manifest) = toml::from_str::<RoleManifest>(text) else { return; };
    let _ = jackin_manifest::validate_role_manifest(&manifest);
    let _ = jackin_manifest::validate_agent_consistency(&manifest);
});
