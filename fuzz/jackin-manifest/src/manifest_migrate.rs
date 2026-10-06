//! Fuzz: jackin.role.toml migration — no panic; Ok path is idempotent.
#![no_main]
use libfuzzer_sys::fuzz_target;
use std::fs;
use std::io::Write;

fuzz_target!(|data: &[u8]| {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("jackin.role.toml");
    {
        let mut f = fs::File::create(&path).expect("create");
        f.write_all(data).expect("write");
    }
    if jackin_manifest::migrations::migrate_manifest_file(&path).is_ok() {
        let first = fs::read(&path).unwrap_or_default();
        let _ = jackin_manifest::migrations::migrate_manifest_file(&path);
        let second = fs::read(&path).unwrap_or_default();
        assert_eq!(first, second, "manifest migration must be idempotent");
    }
});
