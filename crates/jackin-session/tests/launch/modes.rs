// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use jackin_core::{
    LineSink, ProcessError, ProcessOutput, ProcessRunner, ProcessSpec, SecretOutput,
};
use jackin_testkit::{Rig, TestRunner};
use tokio_util::sync::CancellationToken;

use super::support::{TestResult, guard, ready};

#[derive(Debug)]
struct WatchesStaging {
    inner: Arc<TestRunner>,
    modes: Mutex<Vec<u32>>,
}

#[async_trait]
impl ProcessRunner for WatchesStaging {
    async fn run(
        &self,
        spec: &ProcessSpec,
        lines: Option<&LineSink>,
        cancel: &CancellationToken,
    ) -> Result<ProcessOutput, ProcessError> {
        let bundle = spec.args.iter().any(|arg| arg == "bundle");
        let target = spec.args.iter().find(|arg| arg.ends_with("upload.bundle"));
        if let (true, Some(target)) = (bundle, target)
            && let Some(parent) = Path::new(target).parent()
            && let Ok(meta) = std::fs::metadata(parent)
            && let Ok(mut modes) = self.modes.lock()
        {
            modes.push(meta.permissions().mode() & 0o777);
        }
        self.inner.run(spec, lines, cancel).await
    }

    async fn run_secret(
        &self,
        spec: &ProcessSpec,
        cancel: &CancellationToken,
    ) -> Result<SecretOutput, ProcessError> {
        self.inner.run_secret(spec, cancel).await
    }
}

fn directories(root: &Path) -> Result<Vec<(String, u32)>, Box<dyn std::error::Error>> {
    let mut found = vec![(root.display().to_string(), mode_of(root)?)];
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                found.push((entry.path().display().to_string(), mode_of(&entry.path())?));
                pending.push(entry.path());
            }
        }
    }
    Ok(found)
}

fn mode_of(path: &Path) -> Result<u32, std::io::Error> {
    Ok(std::fs::metadata(path)?.permissions().mode() & 0o777)
}

#[tokio::test]
async fn the_host_staging_directory_and_every_cache_directory_are_owner_only() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let watcher = Arc::new(WatchesStaging {
            inner: Arc::clone(&rig.world.runner),
            modes: Mutex::new(Vec::new()),
        });
        let session = rig.session_over(Arc::<WatchesStaging>::clone(&watcher));
        let outcome = rig
            .run_on(&session, rig.request(1)?, CancellationToken::new())
            .await?;
        ready(outcome)?;
        let seen = watcher.modes.lock().map_err(|_| "poisoned")?.clone();
        assert!(!seen.is_empty());
        assert!(seen.iter().all(|mode| *mode == 0o700), "{seen:?}");
        let cache = directories(rig.world.cache_dir().as_path())?;
        assert!(cache.iter().all(|(_, mode)| *mode == 0o700), "{cache:?}");
        Ok(())
    })
    .await
}
