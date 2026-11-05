// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use jackin_core::{ProcessRunner, Program};
use jackin_testkit::{Rig, TestResult, guard};
use tokio_util::sync::CancellationToken;

use super::support::{Call, Sees, branch_with_commit};

fn before(call: &Call, word: &str, pin: &str) -> bool {
    let at = call.args.iter().position(|arg| arg == word);
    let pinned = call.args.iter().position(|arg| arg == pin);
    matches!((at, pinned), (Some(at), Some(pinned)) if pinned < at)
}

#[tokio::test]
async fn every_host_git_call_is_hardened_and_the_import_carries_its_memory_pins() -> TestResult {
    guard(async {
        let rig = Rig::new().await?;
        let sees = Arc::new(Sees::new(&rig));
        let runner: Arc<dyn ProcessRunner> = Arc::<Sees>::clone(&sees);
        let session = rig.session_over(runner);
        let request = rig.request(1)?;
        rig.run_on(&session, request, CancellationToken::new())
            .await?
            .result
            .map_err(|failure| format!("{failure:?}"))?;
        let handle = rig.handle(1).await?;
        branch_with_commit(&handle, "work", "w.txt").await?;
        session
            .export(&rig.config, &handle.name(), CancellationToken::new())
            .await
            .map_err(|error| format!("{error:?}"))?;

        let calls = sees.since(0);
        for call in calls
            .iter()
            .filter(|call| call.program == Program::Git && call.args != ["--version"])
        {
            assert!(
                call.env
                    .contains(&("GIT_NO_LAZY_FETCH".to_owned(), "1".to_owned())),
                "no GIT_NO_LAZY_FETCH on {:?}",
                call.args
            );
            assert!(
                call.args.starts_with(
                    &[
                        "-c",
                        "core.fsmonitor=false",
                        "-c",
                        "core.hooksPath=/dev/null"
                    ]
                    .map(str::to_owned)
                ),
                "no hardening on {:?}",
                call.args
            );
        }
        let root = rig.repo.root().as_path();
        let home = format!("--git-dir={}", root.join(".git").display());
        let tree = format!("--work-tree={}", root.display());
        let is_discovery = |call: &Call| {
            call.args
                .iter()
                .any(|arg| arg == "--git-common-dir" || arg == "--absolute-git-dir")
        };
        let discovery: Vec<&Call> = calls
            .iter()
            .filter(|call| call.program == Program::Git && is_discovery(call))
            .collect();
        let flags = |call: &&Call| {
            call.args
                .iter()
                .find(|arg| {
                    arg.starts_with("--git-common-dir") || arg.starts_with("--absolute-git-dir")
                })
                .cloned()
        };
        let named: Vec<Option<String>> = discovery.iter().map(flags).collect();
        assert!(!named.is_empty() && named.len() % 2 == 0, "{named:?}");
        for pair in named.chunks(2) {
            assert_eq!(
                pair,
                [
                    Some("--git-common-dir".to_owned()),
                    Some("--absolute-git-dir".to_owned())
                ]
            );
        }
        let scoped: Vec<&Call> = calls
            .iter()
            .filter(|call| {
                call.program == Program::Git && call.args != ["--version"] && !is_discovery(call)
            })
            .collect();
        assert!(!scoped.is_empty());
        for call in scoped {
            assert!(call.args.contains(&tree), "no {tree} on {:?}", call.args);
            let dirs: Vec<&String> = call
                .args
                .iter()
                .filter(|arg| arg.starts_with("--git-dir="))
                .collect();
            assert!(
                matches!(dirs.as_slice(), [dir] if **dir == home || dir.ends_with("/shadow")),
                "git directory not pinned on {:?}",
                call.args
            );
        }
        let import: Vec<&Call> = calls
            .iter()
            .filter(|call| call.args.iter().any(|arg| arg == "index-pack"))
            .collect();
        assert_eq!(import.len(), 1);
        let call = import[0];
        let after = call
            .args
            .iter()
            .position(|arg| arg == "index-pack")
            .unwrap_or(0);
        assert!(
            call.args[after..].iter().any(|arg| arg == "--threads=1"),
            "--threads=1 must follow index-pack: {:?}",
            call.args
        );
        assert!(
            before(call, "index-pack", "core.bigFileThreshold=64m"),
            "{:?}",
            call.args
        );
        assert!(
            before(call, "index-pack", "core.deltaBaseCacheLimit=96m"),
            "{:?}",
            call.args
        );
        Ok(())
    })
    .await
}
