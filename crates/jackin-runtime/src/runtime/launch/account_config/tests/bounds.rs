// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

#![expect(
    clippy::disallowed_methods,
    reason = "isolated synchronous filesystem fixtures run on test threads, never render/runtime threads"
)]

use super::*;
use std::fs::{File, OpenOptions};
use std::io::{BufRead as _, Read as _, Write as _};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const LOCK_CHILD_ROOT: &str = "JACKIN_TEST_PRIVATE_CONFIG_LOCK_ROOT";

#[test]
fn private_config_bounds_reject_sparse_oversize_before_reading() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.toml");
    let mut file = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    file.set_len(8 * 1024 * 1024).unwrap();
    let error = private_config_bounds::read(&mut file, "config.toml", 1024).unwrap_err();
    assert!(format!("{error:#}").contains("exceeds 1024 byte limit"));
    use std::io::Seek as _;
    assert_eq!(
        file.stream_position().unwrap(),
        0,
        "oversize file must not be consumed"
    );
}

#[test]
fn private_config_bounds_reject_growth_after_stat() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.toml");
    std::fs::write(&path, b"original").unwrap();
    let mut reader = File::open(&path).unwrap();
    let error = private_config_bounds::read_with_hook(&mut reader, "config.toml", 32, || {
        OpenOptions::new()
            .append(true)
            .open(&path)?
            .write_all(&[b'x'; 128])?;
        Ok(())
    })
    .unwrap_err();
    assert!(format!("{error:#}").contains("exceeds 32 byte limit"));
    use std::io::Seek as _;
    assert_eq!(
        reader.stream_position().unwrap(),
        33,
        "reader must stop at one detection byte"
    );
}

#[test]
fn private_config_bounds_accept_exact_limit_and_reject_one_extra_byte() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.toml");
    std::fs::write(&path, b"0123456789abcdef").unwrap();
    let mut reader = File::open(&path).unwrap();
    assert_eq!(
        private_config_bounds::read(&mut reader, "config.toml", 16).unwrap(),
        b"0123456789abcdef"
    );
    OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(b"x")
        .unwrap();
    let mut reader = File::open(&path).unwrap();
    private_config_bounds::read(&mut reader, "config.toml", 16)
        .expect_err("one extra input byte must be rejected");
}

#[test]
fn oversized_catalog_preserves_last_good_config_and_referenced_catalog() {
    let temp = tempfile::tempdir().unwrap();
    let (config, instances) = codex_moonshot_fixture();
    configure_for_test(temp.path(), &config, &instances).unwrap();
    let directory = temp.path().join("home/.codex");
    let previous_config = std::fs::read(directory.join("config.toml")).unwrap();
    let previous: toml::Value =
        toml::from_str(std::str::from_utf8(&previous_config).unwrap()).unwrap();
    let previous_catalog = Path::new(previous["model_catalog_json"].as_str().unwrap())
        .file_name()
        .unwrap();
    let previous_catalog_bytes = std::fs::read(directory.join(previous_catalog)).unwrap();
    let next_catalog =
        serde_json::to_vec_pretty(&model_catalog(AiProvider::Moonshot, "k3").unwrap()).unwrap();
    let next_name = codex_catalog_filename(&next_catalog);
    let next_path = directory.join(next_name);
    let sparse = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&next_path)
        .unwrap();
    sparse.set_len(8 * 1024 * 1024).unwrap();
    let next = [instance(
        "codex-work",
        Agent::Codex,
        "work",
        Some("k3"),
        None,
    )];
    let error = configure_for_test(temp.path(), &config, &next).unwrap_err();
    assert!(format!("{error:#}").contains("byte limit"));
    assert_eq!(
        std::fs::read(directory.join("config.toml")).unwrap(),
        previous_config
    );
    assert_eq!(
        std::fs::read(directory.join(previous_catalog)).unwrap(),
        previous_catalog_bytes
    );
    assert_eq!(std::fs::metadata(next_path).unwrap().len(), 8 * 1024 * 1024);
}

#[test]
fn oversized_config_preserves_input_and_last_good_catalog_without_quarantine() {
    let temp = tempfile::tempdir().unwrap();
    let (config, instances) = codex_moonshot_fixture();
    configure_for_test(temp.path(), &config, &instances).unwrap();
    let directory = temp.path().join("home/.codex");
    let config_path = directory.join("config.toml");
    let mut config_before = std::fs::read(&config_path).unwrap();
    let previous: toml::Value =
        toml::from_str(std::str::from_utf8(&config_before).unwrap()).unwrap();
    let catalog = Path::new(previous["model_catalog_json"].as_str().unwrap())
        .file_name()
        .unwrap();
    let catalog_before = std::fs::read(directory.join(catalog)).unwrap();
    config_before.extend_from_slice(b"\n#");
    config_before.resize(1024 * 1024 + 1, b'x');
    std::fs::write(&config_path, &config_before).unwrap();
    let error = configure_for_test(temp.path(), &config, &instances).unwrap_err();
    assert!(format!("{error:#}").contains("byte limit"));
    assert_eq!(std::fs::read(config_path).unwrap(), config_before);
    assert_eq!(
        std::fs::read(directory.join(catalog)).unwrap(),
        catalog_before
    );
    assert!(std::fs::read_dir(directory).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".corrupt-")
    }));
}

#[test]
fn near_limit_config_expansion_is_rejected_before_publishing_new_catalog() {
    let temp = tempfile::tempdir().unwrap();
    let (config, instances) = codex_moonshot_fixture();
    configure_for_test(temp.path(), &config, &instances).unwrap();
    let directory = temp.path().join("home/.codex");
    let config_path = directory.join("config.toml");
    let previous: toml::Value =
        toml::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
    let catalog_reference = previous["model_catalog_json"].as_str().unwrap();
    let catalog = Path::new(catalog_reference).file_name().unwrap();
    let catalog_before = std::fs::read(directory.join(catalog)).unwrap();
    // Valid operator settings just below the input contract. Adding the
    // selected provider fields must not emit an unreadable next generation.
    let template = format!("preserved = \"\"\nmodel_catalog_json = \"{catalog_reference}\"\n");
    let padding = "x".repeat(1024 * 1024 - 64 - template.len());
    let config_before =
        format!("preserved = \"{padding}\"\nmodel_catalog_json = \"{catalog_reference}\"\n");
    assert_eq!(config_before.len(), 1024 * 1024 - 64);
    toml::from_str::<toml::Table>(&config_before).unwrap();
    std::fs::write(&config_path, &config_before).unwrap();
    let next_catalog =
        serde_json::to_vec_pretty(&model_catalog(AiProvider::Moonshot, "k3").unwrap()).unwrap();
    let next_path = directory.join(codex_catalog_filename(&next_catalog));
    assert!(!next_path.exists());
    let next = [instance(
        "codex-work",
        Agent::Codex,
        "work",
        Some("k3"),
        None,
    )];
    let error = configure_for_test(temp.path(), &config, &next).unwrap_err();
    assert!(format!("{error:#}").contains("byte limit"));
    assert_eq!(std::fs::read_to_string(config_path).unwrap(), config_before);
    assert_eq!(
        std::fs::read(directory.join(catalog)).unwrap(),
        catalog_before
    );
    assert!(
        !next_path.exists(),
        "output rejection must precede catalog publication"
    );
}

#[test]
fn oversized_atomic_output_preserves_existing_codex_and_opencode_files() {
    let temp = tempfile::tempdir().unwrap();
    let directory = private_config_fs::open_directory(temp.path(), Path::new(".codex")).unwrap();
    let contents = vec![b'x'; 1024 * 1024 + 1];
    for (name, artifact) in [
        ("config.toml", private_config_fs::Artifact::CodexConfig),
        ("opencode.json", private_config_fs::Artifact::OpenCodeConfig),
    ] {
        let path = temp.path().join("home/.codex").join(name);
        std::fs::write(&path, b"previous complete config").unwrap();
        let mut publication_started = false;
        let error =
            private_config_fs::publish_atomic(&directory, name, &contents, artifact, |_| {
                publication_started = true;
                Ok(())
            })
            .unwrap_err();
        assert!(format!("{error:#}").contains("byte limit"));
        assert!(
            !publication_started,
            "oversize output must not enter publication"
        );
        assert_eq!(std::fs::read(path).unwrap(), b"previous complete config");
    }
}

struct LockHolder(Child);

impl Drop for LockHolder {
    fn drop(&mut self) {
        drop(self.0.kill());
        drop(self.0.wait());
    }
}

fn lock_holder(root: &Path) -> anyhow::Result<LockHolder> {
    let child = Command::new(std::env::current_exe().context("locate lock fixture executable")?)
        .args([
            "--exact",
            "runtime::launch::account_config::tests::bounds::private_config_lock_holder_child",
            "--nocapture",
        ])
        .env(LOCK_CHILD_ROOT, root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .context("spawn lock fixture child")?;
    let mut holder = LockHolder(child);
    let output = holder.0.stdout.take().context("lock fixture stdout pipe")?;
    let (ready, observed) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::BufReader::new(output).lines() {
            let Ok(line) = line else { return };
            // libtest can print its test-name prefix on this same line.
            if line.trim().ends_with("PRIVATE_CONFIG_LOCK_HELD") {
                ready.send(()).unwrap_or(());
                return;
            }
        }
    });
    observed
        .recv_timeout(Duration::from_secs(10))
        .context("lock helper must acknowledge ownership before the deadline")?;
    Ok(holder)
}

/// Child-process fixture only; the parent scenarios are the acceptance tests.
#[test]
fn private_config_lock_holder_child() {
    let Some(root) = std::env::var_os(LOCK_CHILD_ROOT) else {
        return;
    };
    let directory =
        private_config_fs::open_directory(Path::new(&root), Path::new(".codex")).unwrap();
    let _lock = private_config_fs::lock(&directory).unwrap();
    println!("PRIVATE_CONFIG_LOCK_HELD");
    std::io::stdout().flush().unwrap();
    let mut release = [0];
    drop(std::io::stdin().read_exact(&mut release));
}

#[test]
fn held_private_config_lock_times_out_and_preserves_last_good_pair() {
    let temp = tempfile::tempdir().unwrap();
    let (config, instances) = codex_moonshot_fixture();
    configure_for_test(temp.path(), &config, &instances).unwrap();
    let directory = temp.path().join("home/.codex");
    let config_before = std::fs::read(directory.join("config.toml")).unwrap();
    let previous: toml::Value =
        toml::from_str(std::str::from_utf8(&config_before).unwrap()).unwrap();
    let catalog = Path::new(previous["model_catalog_json"].as_str().unwrap())
        .file_name()
        .unwrap();
    let catalog_before = std::fs::read(directory.join(catalog)).unwrap();
    let holder = lock_holder(temp.path()).unwrap();
    let started = Instant::now();
    let error = configure_for_test(temp.path(), &config, &instances).unwrap_err();
    assert!(format!("{error:#}").contains("timed out locking"));
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "launch must not wait for lock owner indefinitely"
    );
    assert_eq!(
        std::fs::read(directory.join("config.toml")).unwrap(),
        config_before
    );
    assert_eq!(
        std::fs::read(directory.join(catalog)).unwrap(),
        catalog_before
    );
    drop(holder);
}

#[test]
fn private_config_lock_deadline_shortens_poll_and_recovers_after_owner_exit() {
    let temp = tempfile::tempdir().unwrap();
    let holder = lock_holder(temp.path()).unwrap();
    let lock = File::options()
        .read(true)
        .write(true)
        .open(
            temp.path()
                .join("home/.codex/.jackin-private-provider-config.lock"),
        )
        .unwrap();
    let started = Instant::now();
    private_config_bounds::lock_with_timeout(&lock, Duration::from_millis(1))
        .expect_err("a held lock must not be acquired after the deadline");
    assert!(started.elapsed() < Duration::from_secs(1));
    drop(holder);
    private_config_bounds::lock_with_timeout(&lock, Duration::from_secs(1)).unwrap();
    lock.unlock().unwrap();
}
