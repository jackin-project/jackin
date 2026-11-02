// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::error::Error;
use std::sync::Arc;

use jackin_core::{CommitId, ProcessRunner, SandboxName};
use jackin_session::{BundleRefusal, ExportError, ExportReport, LaunchLimits, Session};
use jackin_testkit::{Rig, SandboxHandle, TestResult, default_limits, object_store_files};
use tokio_util::sync::CancellationToken;

use super::heads::crafted;
use super::pack::header_len;
use super::support::{
    Sees, branch_with_commit, bundles_left, exports, ref_of, snapshot, staging_left,
};
use super::tap::Tap;

pub(crate) const IMPORT: [&str; 1] = ["fetch"];
const BANNED: [&str; 7] = [
    "index-pack",
    "update-ref",
    "fetch",
    "bundle",
    "unbundle",
    "verify",
    "list-heads",
];

pub(crate) type Check = Box<dyn Fn(&ExportError) -> bool>;

pub(crate) fn refusal(wanted: BundleRefusal) -> Check {
    Box::new(move |error| matches!(error, ExportError::BundleRefused(found) if *found == wanted))
}

pub(crate) fn heads() -> Check {
    Box::new(|error| matches!(error, ExportError::BundleHeads { .. }))
}

pub(crate) fn family() -> Check {
    Box::new(|error| matches!(error, ExportError::BundleRefused(_)))
}

pub(crate) struct Lab {
    pub(crate) rig: Rig,
    pub(crate) handle: SandboxHandle,
    pub(crate) name: SandboxName,
    pub(crate) tip: CommitId,
    pub(crate) sees: Arc<Sees>,
    pub(crate) tap: Arc<Tap>,
    pub(crate) session: Session,
    default_session: Session,
    allowed: BTreeSet<String>,
}

impl Lab {
    pub(crate) async fn over(rig: Rig) -> Result<Lab, Box<dyn Error>> {
        rig.launch(1).await?;
        let handle = rig.handle(1).await?;
        let name = handle.name();
        let tip = branch_with_commit(&handle, "work", "w.txt").await?;
        let tap = Tap::new(&rig);
        let sees = Arc::new(Sees::new(&rig));
        let runner: Arc<dyn ProcessRunner> = Arc::<Sees>::clone(&sees);
        let session = tap.session_over(&rig, default_limits(), Arc::clone(&runner));
        let default_session = tap.session_over(&rig, default_limits(), runner);
        Ok(Lab {
            rig,
            handle,
            name,
            tip,
            sees,
            tap,
            session,
            default_session,
            allowed: BTreeSet::new(),
        })
    }

    pub(crate) async fn new() -> Result<Lab, Box<dyn Error>> {
        Lab::over(Rig::new().await?).await
    }

    pub(crate) async fn armed() -> Result<Lab, Box<dyn Error>> {
        Lab::armed_over(Rig::new().await?).await
    }

    pub(crate) async fn armed_over(rig: Rig) -> Result<Lab, Box<dyn Error>> {
        let mut lab = Lab::over(rig).await?;
        lab.control().await?;
        Ok(lab)
    }

    pub(crate) fn limits(&mut self, limits: LaunchLimits) {
        let runner: Arc<dyn ProcessRunner> = Arc::<Sees>::clone(&self.sees);
        self.session = self.tap.session_over(&self.rig, limits, runner);
    }

    pub(crate) async fn advance(&mut self) -> TestResult {
        self.tip = self
            .handle
            .commit_in_project("advance.txt", self.tip.as_str().as_bytes(), "advance")
            .await?;
        Ok(())
    }

    pub(crate) async fn honest(&self) -> Result<Vec<u8>, Box<dyn Error>> {
        crafted(&self.handle, &["refs/heads/work"]).await
    }

    pub(crate) async fn export(&self) -> Result<ExportReport, ExportError> {
        self.session
            .export(&self.rig.config, &self.name, CancellationToken::new())
            .await
    }

    pub(crate) async fn control(&mut self) -> TestResult {
        let mark = self.sees.mark();
        let report = self.export().await.map_err(|error| format!("{error:?}"))?;
        assert_eq!(report.branches.len(), 1);
        let calls = self.sees.since(mark);
        for word in IMPORT {
            assert!(
                calls.iter().any(|call| call.subcommand() == Some(word)),
                "the control export ran no host {word}: the instrument sees nothing"
            );
        }
        self.allowed = calls
            .iter()
            .filter_map(|call| call.subcommand())
            .filter(|word| !IMPORT.contains(word))
            .map(str::to_owned)
            .collect();
        self.forget_export().await
    }

    pub(crate) async fn forget_export(&self) -> TestResult {
        self.rig
            .repo
            .git(&["update-ref", "-d", &ref_of(&self.name, "work")])
            .await?;
        Ok(())
    }

    pub(crate) async fn passes_the_walker(
        &mut self,
        label: &str,
        build: impl FnOnce(&[u8]) -> Vec<u8>,
    ) -> TestResult {
        self.advance().await?;
        let honest = self.honest().await?;
        self.tap
            .replace_next_download(build(&honest[..header_len(&honest)]));
        let result = self.export().await;
        assert!(
            matches!(result, Err(ExportError::HostGit { .. })),
            "{label}: {result:?}"
        );
        let after = snapshot(&self.rig).await?;
        assert!(exports(&after).is_empty(), "{label}: a ref was written");
        Ok(())
    }

    pub(crate) async fn refuses(&self, label: &str, bytes: Vec<u8>, wanted: &Check) -> TestResult {
        assert!(
            !self.allowed.is_empty(),
            "{label}: the lab has no control run"
        );
        let before = snapshot(&self.rig).await?;
        let objects = object_store_files(self.rig.repo.root()).await?;
        let mark = self.sees.mark();
        self.tap.replace_next_download(bytes);
        let result = self.export().await;
        assert!(result.as_ref().is_err_and(wanted), "{label}: {result:?}");
        for call in self.sees.since(mark) {
            for word in BANNED {
                assert!(
                    !call.args.iter().any(|arg| arg == word),
                    "{label}: the host ran {word}: {:?}",
                    call.args
                );
            }
            let subcommand = call.subcommand().unwrap_or_default();
            assert!(
                self.allowed.contains(subcommand),
                "{label}: the host ran {subcommand}, which the control export never ran"
            );
        }
        let after = snapshot(&self.rig).await?;
        assert!(exports(&after).is_empty(), "{label}: a ref was written");
        assert_eq!(after.rest, before.rest, "{label}: the host changed");
        assert_eq!(
            object_store_files(self.rig.repo.root()).await?,
            objects,
            "{label}: the host object store changed"
        );
        assert_eq!(staging_left(&self.rig)?, Vec::<String>::new(), "{label}");
        assert_eq!(bundles_left(&self.handle), Vec::<String>::new(), "{label}");
        let landed = self
            .default_session
            .export(&self.rig.config, &self.name, CancellationToken::new())
            .await
            .map_err(|error| format!("{label}: {error:?}"))?;
        assert_eq!(landed.branches.len(), 1, "{label}");
        self.forget_export().await
    }
}
