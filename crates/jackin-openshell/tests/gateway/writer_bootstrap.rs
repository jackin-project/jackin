// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};

use tempfile::TempDir;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

const RELATIVE: &str = ".codex/auth.json";

struct Tools {
    helper: PathBuf,
    #[cfg(target_os = "linux")]
    shim: PathBuf,
    driver: PathBuf,
}

async fn compile(root: &TempDir) -> Tools {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let writer = manifest.join("../jackin-image/images/construct/write-file.c");
    let shim_source = manifest.join("tests/gateway/writer_bootstrap_shim.c");
    let driver_source = manifest.join("tests/gateway/writer_bootstrap_driver.c");
    let sandbox = root.path().join("sandbox");
    let helper = root.path().join("write-file");
    let shim = root.path().join("writer-shim");
    let driver = root.path().join("writer-driver");
    let sandbox_define = format!("-DJACKIN_SANDBOX_ROOT=\"{}\"", sandbox.display());
    let writer_build = Command::new("cc")
        .args(["-std=c11", "-O2", "-Wall", "-Wextra", "-Werror"])
        .arg(sandbox_define)
        .arg(writer)
        .arg("-o")
        .arg(&helper)
        .output()
        .await
        .unwrap();
    assert!(
        writer_build.status.success(),
        "writer compile failed: {}",
        String::from_utf8_lossy(&writer_build.stderr)
    );
    let mut shim_build = Command::new("cc");
    shim_build.args(["-std=c11", "-O2", "-Wall", "-Wextra", "-Werror"]);
    if cfg!(target_os = "macos") {
        shim_build.arg("-dynamiclib");
    } else {
        shim_build.args(["-fPIC", "-shared"]);
    }
    let shim_build = shim_build
        .arg(shim_source)
        .arg("-o")
        .arg(&shim)
        .output()
        .await
        .unwrap();
    assert!(
        shim_build.status.success(),
        "shim compile failed: {}",
        String::from_utf8_lossy(&shim_build.stderr)
    );
    let driver_build = Command::new("cc")
        .args(["-std=c11", "-O2", "-Wall", "-Wextra", "-Werror", "-pthread"])
        .arg(driver_source)
        .arg("-o")
        .arg(&driver)
        .output()
        .await
        .unwrap();
    assert!(
        driver_build.status.success(),
        "driver compile failed: {}",
        String::from_utf8_lossy(&driver_build.stderr)
    );
    Tools {
        helper,
        #[cfg(target_os = "linux")]
        shim,
        driver,
    }
}

fn hash_path(path: &str) -> u64 {
    let mut hash = 14_695_981_039_346_656_037_u64;
    for byte in path.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(1_099_511_628_211);
    }
    hash
}

fn namespace(root: &Path) -> PathBuf {
    root.join("sandbox/.codex")
        .join(format!(".jackin-write-{:016x}", hash_path(RELATIVE)))
}

fn target(root: &Path) -> PathBuf {
    root.join("sandbox").join(RELATIVE)
}

async fn run_writer(helper: &Path, relative: &str, payload: &[u8]) -> Output {
    let mut child = Command::new(helper)
        .args([relative, "600", &payload.len().to_string()])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    if let Err(error) = stdin.write_all(payload).await {
        assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
    }
    drop(stdin);
    child.wait_with_output().await.unwrap()
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn concurrent_first_use_serializes_identity_publication_and_writes() {
    let root = TempDir::new().unwrap();
    std::fs::create_dir_all(root.path().join("sandbox/.codex")).unwrap();
    let tools = compile(&root).await;
    let namespace = namespace(root.path());
    assert!(!namespace.exists());
    let output = Command::new(&tools.driver)
        .args([
            "concurrent",
            tools.helper.to_str().unwrap(),
            tools.shim.to_str().unwrap(),
            root.path().to_str().unwrap(),
            namespace.to_str().unwrap(),
            RELATIVE,
            "first-complete-payload",
            "second-complete-payload",
        ])
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "driver status {:?}, stdout {}, stderr {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let transcript = String::from_utf8_lossy(&output.stdout);
    assert!(transcript.contains("status_a=0 status_b=0"), "{transcript}");
    let bytes = std::fs::read(target(root.path())).unwrap();
    assert_eq!(bytes, b"second-complete-payload");
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(target(root.path()))
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
    let identity = std::fs::read(namespace.join("identity")).unwrap();
    assert_eq!(identity, target(root.path()).to_str().unwrap().as_bytes());
    let entries = std::fs::read_dir(&namespace)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 2, "{entries:?}");
    assert!(entries.iter().any(|name| name == "identity"), "{entries:?}");
    assert!(entries.iter().any(|name| name == "lock"), "{entries:?}");
}

#[cfg(unix)]
#[tokio::test]
async fn a_sigxfsz_during_identity_bootstrap_preserves_old_target_and_retry_recovers() {
    let root = TempDir::new().unwrap();
    std::fs::create_dir_all(root.path().join("sandbox/.codex")).unwrap();
    let target = target(root.path());
    std::fs::write(&target, b"old-target-bytes").unwrap();
    let tools = compile(&root).await;
    let output = Command::new(&tools.driver)
        .args([
            "limit",
            tools.helper.to_str().unwrap(),
            RELATIVE,
            "replacement-payload",
        ])
        .output()
        .await
        .unwrap();
    assert!(
        output.status.success(),
        "driver status {:?}, stdout {}, stderr {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"termination=SIGXFSZ\n");
    assert_eq!(std::fs::read(&target).unwrap(), b"old-target-bytes");
    let namespace = namespace(root.path());
    assert!(!namespace.join("identity").exists());
    let names = std::fs::read_dir(&namespace)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let temporary = names
        .iter()
        .find(|name| name.starts_with("identity."))
        .unwrap();
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(names.iter().any(|name| name == "lock"), "{names:?}");
    let temporary = namespace.join(temporary);
    assert_eq!(std::fs::read(&temporary).unwrap().len(), 1);
    assert_eq!(
        run_writer(&tools.helper, RELATIVE, b"replacement-payload")
            .await
            .status
            .code(),
        Some(0)
    );
    assert_eq!(std::fs::read(&target).unwrap(), b"replacement-payload");
    assert_eq!(
        std::fs::read(namespace.join("identity")).unwrap(),
        target.to_str().unwrap().as_bytes()
    );
    assert!(!temporary.exists());
    let names = std::fs::read_dir(&namespace)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(names.len(), 2, "{names:?}");
}

#[cfg(unix)]
#[tokio::test]
async fn an_unrecognized_entry_or_invalid_identity_temp_prevents_missing_identity_repair() {
    use std::os::unix::fs::{PermissionsExt, symlink};

    for entry_name in ["operator-note", "identity.not-hex"] {
        let root = TempDir::new().unwrap();
        std::fs::create_dir_all(root.path().join("sandbox/.codex")).unwrap();
        let namespace = namespace(root.path());
        std::fs::create_dir(&namespace).unwrap();
        std::fs::set_permissions(&namespace, std::fs::Permissions::from_mode(0o700)).unwrap();
        let preserved = namespace.join(entry_name);
        std::fs::write(&preserved, b"preserve exactly").unwrap();
        std::fs::set_permissions(&preserved, std::fs::Permissions::from_mode(0o600)).unwrap();
        let tools = compile(&root).await;
        let result = run_writer(&tools.helper, RELATIVE, b"must-not-write").await;
        assert!(!result.status.success(), "{entry_name}");
        assert!(!target(root.path()).exists());
        assert_eq!(std::fs::read(&preserved).unwrap(), b"preserve exactly");
        let names = std::fs::read_dir(&namespace)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(names, [entry_name.to_owned()]);
    }

    let root = TempDir::new().unwrap();
    std::fs::create_dir_all(root.path().join("sandbox/.codex")).unwrap();
    let namespace = namespace(root.path());
    std::fs::create_dir(&namespace).unwrap();
    std::fs::set_permissions(&namespace, std::fs::Permissions::from_mode(0o700)).unwrap();
    let victim = root.path().join("victim");
    std::fs::write(&victim, b"untouched").unwrap();
    let temporary = namespace.join("identity.0000000000000000");
    symlink(&victim, &temporary).unwrap();
    let tools = compile(&root).await;
    let result = run_writer(&tools.helper, RELATIVE, b"must-not-write").await;
    assert!(!result.status.success());
    assert!(!target(root.path()).exists());
    assert_eq!(std::fs::read(&victim).unwrap(), b"untouched");
    assert!(
        std::fs::symlink_metadata(&temporary)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        std::fs::read_dir(&namespace)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
        ["identity.0000000000000000".to_owned()]
    );
}
