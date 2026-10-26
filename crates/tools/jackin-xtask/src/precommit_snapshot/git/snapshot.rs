// SPDX-FileCopyrightText: 2026 The jackin❯ Authors
// SPDX-License-Identifier: Apache-2.0

use super::{
    GateCompletion, LiveRepository, OwnedSnapshot, checked, configure_isolated_git, index_inputs,
    private_spool, validate_link, write_private_new,
};
use crate::precommit_snapshot::{
    process::{
        CommandBudget, EnvironmentMode, SetupControls, SpoolOutput, capture_spooled, capture_strict,
    },
    toolchain::QualifiedTools,
};
use anyhow::{Context, Result, anyhow, bail, ensure};
use jackin_process::{ExecRequest, SpoolRecovery, StdioMode, exec_spooled_sync_cancellable};
use nix::{
    fcntl::{OFlag, open},
    sys::stat::Mode,
};
use sha2::{Digest, Sha256};
use std::{
    env,
    ffi::{OsStr, OsString},
    fs,
    io::{BufReader, Read, Write},
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    process::Command,
};

mod admission;

const STAGED_INDEX_OUTPUT_LIMIT: usize = 1024 * 1024 * 1024;
const STAGED_INDEX_STDERR_LIMIT: usize = 32 * 1024 * 1024;
const STAGED_INDEX_HEADER_LIMIT: usize = 256;
const STAGED_SYMLINK_PATH_LIMIT: usize = 1024 * 1024;

impl OwnedSnapshot {
    pub(in crate::precommit_snapshot) fn create(live: &LiveRepository) -> Result<Self> {
        ensure!(live.root.is_dir(), "physical repository root disappeared");
        ensure!(
            live.common_git_dir.is_dir(),
            "Git common directory disappeared"
        );
        let alternate_records = live_alternate_records(live)?;
        let temp_parent = super::super::process::canonical_temporary_parent(&env::temp_dir())?;
        let directory = tempfile::Builder::new()
            .prefix("jackin-pre-commit-")
            .tempdir_in(temp_parent)
            .context("creating private pre-commit snapshot")?;
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700))?;
        super::super::process::validate_confidential_directory(directory.path())
            .context("validating private snapshot scratch directory")?;
        let root = directory.path().join("worktree");
        let scratch_root = directory.path().to_path_buf();
        fs::create_dir(&root).context("creating private snapshot worktree")?;
        let git_dir = root.join(".git");
        let mut init_command = Command::new("git");
        init_command.args([
            OsStr::new("init"),
            OsStr::new("--quiet"),
            OsStr::new("--template="),
        ]);
        init_command
            .arg(format!(
                "--object-format={}",
                live.object_format.to_string_lossy()
            ))
            .arg(&root);
        init_command
            .current_dir(&root)
            .env_clear()
            .env("PATH", "/usr/bin:/bin:/opt/homebrew/bin:/usr/local/bin")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null");
        let init = capture_strict(
            &mut init_command,
            &live.controls,
            EnvironmentMode::Clear,
            super::super::process::METADATA_OUTPUT_LIMIT,
            1024 * 1024,
            CommandBudget::Probe,
        )
        .context("initializing private Git repository")?;
        checked(init, "initializing private Git repository")?;
        let index = git_dir.join("index");
        let objects = git_dir.join("objects");
        let snapshot = Self {
            directory: Some(directory),
            scratch_root,
            root,
            git_dir,
            index,
            objects,
            invocation_dir: live.invocation_dir.clone(),
            extra_alternates: live.alternates.clone(),
            controls: live.controls.clone(),
            #[cfg(test)]
            test_git_program: None,
        };
        snapshot.write_alternates(&live.objects, &alternate_records)?;
        if let Some(path) = &live.shallow {
            fs::copy(path, snapshot.git_dir.join("shallow")).context("copying shallow boundary")?;
        }
        snapshot.git(&["config", "core.sparseCheckout", "false"])?;
        snapshot.git(&["config", "core.sparseCheckoutCone", "false"])?;
        snapshot.copy_index(live)?;
        Ok(snapshot)
    }

    pub(in crate::precommit_snapshot) fn capture_staged_tree(&self) -> Result<OsString> {
        if self.index.exists() {
            self.git(&["write-tree"])
        } else {
            self.git(&["read-tree", "--empty"])?;
            self.git(&["write-tree"])
        }
    }

    pub(in crate::precommit_snapshot) fn materialize(
        &self,
        tree: &OsStr,
        live: &LiveRepository,
    ) -> Result<()> {
        self.materialize_with_staged_content_limit(tree, live, admission::STAGED_TREE_CONTENT_LIMIT)
    }

    pub(super) fn materialize_with_staged_content_limit(
        &self,
        tree: &OsStr,
        live: &LiveRepository,
        staged_content_limit: u64,
    ) -> Result<()> {
        match fs::remove_file(&self.index) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error).context("removing private snapshot index"),
        }
        self.git_os([OsString::from("read-tree"), tree.to_os_string()])?;
        if let Some(head) = &live.head {
            self.git_os([
                OsString::from("update-ref"),
                OsString::from("refs/heads/jackin-pre-commit"),
                head.clone(),
            ])?;
        }
        self.git(&["symbolic-ref", "HEAD", "refs/heads/jackin-pre-commit"])?;
        for filename in ["MERGE_HEAD", "MERGE_MSG", "MERGE_MODE"] {
            let source = live.worktree_git_dir.join(filename);
            if source.is_file() {
                fs::copy(source, self.git_dir.join(filename))
                    .with_context(|| format!("copying {filename}"))?;
            }
        }
        self.git(&["config", "core.fileMode", "true"])?;
        self.git(&["config", "core.symlinks", "true"])?;
        self.check_materialized_tree_size_with_limit(tree, staged_content_limit)?;
        self.git(&["checkout-index", "--all", "--force"])?;
        Ok(())
    }

    pub(in crate::precommit_snapshot) fn admit(&self, tree: &OsStr) -> Result<()> {
        self.admit_with_inventory_limit(tree, STAGED_INDEX_OUTPUT_LIMIT)
    }

    pub(in crate::precommit_snapshot) fn admit_with_inventory_limit(
        &self,
        _tree: &OsStr,
        inventory_limit: usize,
    ) -> Result<()> {
        let mut entries = self.git_spooled(
            &["ls-files", "--stage", "-z"],
            inventory_limit,
            STAGED_INDEX_STDERR_LIMIT,
            CommandBudget::PhaseRemainder,
        )?;
        ensure!(
            entries.status().success(),
            "running isolated Git command failed with {}",
            entries.status()
        );
        admit_staged_index_entries(&self.root, entries.stdout_mut())
    }

    pub(in crate::precommit_snapshot) fn retain(&mut self) -> Option<PathBuf> {
        if let Some(directory) = self.directory.take() {
            return Some(directory.keep());
        }
        None
    }

    #[cfg(test)]
    pub(in crate::precommit_snapshot) fn set_test_git_program(
        &mut self,
        program: &Path,
    ) -> Result<()> {
        ensure!(
            program.is_absolute(),
            "test Git executable must be absolute"
        );
        ensure!(program.is_file(), "test Git executable is missing");
        self.test_git_program = Some(program.to_path_buf());
        Ok(())
    }

    pub(in crate::precommit_snapshot) fn spool_failure_error(
        &mut self,
        detail: &str,
        recovery: Option<SpoolRecovery>,
    ) -> anyhow::Error {
        let retained_path = recovery.as_ref().and_then(|_| self.retain());
        drop(recovery);
        match retained_path {
            Some(path) => anyhow!(
                "native pre-commit gate failed: {detail}; recovery snapshot retained at {}",
                path.display()
            ),
            None => anyhow!("native pre-commit gate failed: {detail}"),
        }
    }

    pub(in crate::precommit_snapshot) fn report_gate_output(&self) {
        for (label, path) in [
            ("stdout", self.scratch_root.join("stdout.log")),
            ("stderr", self.scratch_root.join("stderr.log")),
        ] {
            let Ok(bytes) = fs::read(path) else {
                continue;
            };
            if bytes.is_empty() {
                continue;
            }
            const LIMIT: usize = 64 * 1024;
            let truncated = bytes.len() > LIMIT;
            let display = if truncated {
                &bytes[bytes.len() - LIMIT..]
            } else {
                &bytes
            };
            let mut stderr = std::io::stderr().lock();
            if writeln!(
                stderr,
                "native pre-commit {label}{}:",
                if truncated { " (last 64 KiB)" } else { "" }
            )
            .is_err()
            {
                return;
            }
            if write!(stderr, "{}", String::from_utf8_lossy(display)).is_err() {
                return;
            }
            if !display.ends_with(b"\n") && writeln!(stderr).is_err() {
                return;
            }
        }
    }

    pub(in crate::precommit_snapshot) fn run_gate(
        &mut self,
        tools: &QualifiedTools,
        hook_arguments: &[OsString],
        controls: &SetupControls,
    ) -> Result<GateCompletion> {
        let stdout = private_spool(&self.scratch_root.join("stdout.log"))?;
        let stderr = private_spool(&self.scratch_root.join("stderr.log"))?;
        let mut args = vec![
            OsString::from("run"),
            OsString::from("pre-commit"),
            OsString::from("--from-hook"),
        ];
        args.extend(hook_arguments.iter().cloned());
        let mut request = ExecRequest::new(&tools.gate, args);
        request.cwd = Some(self.root.clone());
        request.env_clear = true;
        request.env = tools
            .environment
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        request.stdin_mode = StdioMode::Null;
        request.stdout_mode = StdioMode::Capture;
        request.stderr_mode = StdioMode::Capture;
        request.timeout = Some(controls.child_timeout(CommandBudget::PhaseRemainder)?);
        request.stdout_limit = Some(32 * 1024 * 1024);
        request.stderr_limit = Some(32 * 1024 * 1024);
        request.retry.max_retries = 0;
        match exec_spooled_sync_cancellable(&request, stdout, stderr, controls.cancellation()) {
            Ok(result) => Ok(GateCompletion {
                success: result.success,
                code: result.code,
                signal: result.signal,
                execution: result.execution,
                capture: result.capture,
                failures: result.failures,
                cleanup: result.cleanup,
            }),
            Err(error) => {
                let detail = format!("{error:#}");
                match error.downcast::<jackin_process::SpoolError>() {
                    Ok(spool) => {
                        let (_, _, recovery) = spool.into_parts();
                        Err(self.spool_failure_error(&detail, recovery))
                    }
                    Err(error) => Err(error).context("running the native pre-commit gate"),
                }
            }
        }
    }

    pub(in crate::precommit_snapshot) fn collect_fixes(
        &self,
        tree: &OsStr,
        controls: &SetupControls,
    ) -> Result<Vec<u8>> {
        const PATH_LIST_LIMIT: usize = 64 * 1024 * 1024;
        const PATCH_LIMIT: usize = 256 * 1024 * 1024;
        let paths = self.scratch_root.join("new-paths");
        let listed =
            self.git_bytes_limited(&["ls-files", "--others", "-z"], PATH_LIST_LIMIT, controls)?;
        write_private_new(&paths, &listed)?;
        if !listed.is_empty() {
            let mut pathspec = OsString::from("--pathspec-from-file=");
            pathspec.push(paths.as_os_str());
            self.git_os_with_controls(
                [
                    OsString::from("--literal-pathspecs"),
                    OsString::from("add"),
                    OsString::from("--force"),
                    OsString::from("--intent-to-add"),
                    pathspec,
                    OsString::from("--pathspec-file-nul"),
                ],
                controls,
            )?;
        }
        let args = vec![
            OsString::from("--literal-pathspecs"),
            OsString::from("diff"),
            OsString::from("--binary"),
            tree.to_os_string(),
            OsString::from("--"),
        ];
        // The tree hash is an object identifier from write-tree, never user input.
        let output = self.git_bytes_limited(&args, PATCH_LIMIT, controls)?;
        Ok(output)
    }

    fn git(&self, args: &[impl AsRef<OsStr>]) -> Result<OsString> {
        let output = capture_strict(
            &mut self.git_command(args),
            &self.controls,
            EnvironmentMode::Clear,
            super::super::process::METADATA_OUTPUT_LIMIT,
            1024 * 1024,
            CommandBudget::Probe,
        )
        .context("running isolated Git command")?;
        let bytes = checked(output, "running isolated Git command")?;
        let text = String::from_utf8(bytes).context("decoding isolated Git output")?;
        Ok(OsString::from(text.trim_end_matches(['\r', '\n'])))
    }

    fn git_os(&self, args: impl IntoIterator<Item = OsString>) -> Result<OsString> {
        self.git_os_with_controls(args, &self.controls)
    }

    fn git_os_with_controls(
        &self,
        args: impl IntoIterator<Item = OsString>,
        controls: &SetupControls,
    ) -> Result<OsString> {
        let args = args.into_iter().collect::<Vec<_>>();
        let output = capture_strict(
            &mut self.git_command(&args),
            controls,
            EnvironmentMode::Clear,
            super::super::process::METADATA_OUTPUT_LIMIT,
            1024 * 1024,
            CommandBudget::Probe,
        )
        .context("running isolated Git command")?;
        let text = String::from_utf8(checked(output, "running isolated Git command")?)
            .context("decoding isolated Git output")?;
        Ok(OsString::from(text.trim_end_matches(['\r', '\n'])))
    }

    fn git_bytes_limited(
        &self,
        args: &[impl AsRef<OsStr>],
        limit: usize,
        controls: &SetupControls,
    ) -> Result<Vec<u8>> {
        let output = capture_strict(
            &mut self.git_command(args),
            controls,
            EnvironmentMode::Clear,
            limit,
            1024 * 1024,
            CommandBudget::PhaseRemainder,
        )
        .context("capturing bounded isolated Git output")?;
        checked(output, "running isolated Git command")
    }

    fn git_spooled(
        &self,
        args: &[impl AsRef<OsStr>],
        stdout_limit: usize,
        stderr_limit: usize,
        budget: CommandBudget,
    ) -> Result<SpoolOutput> {
        capture_spooled(
            &mut self.git_command(args),
            &self.controls,
            EnvironmentMode::Clear,
            stdout_limit,
            stderr_limit,
            budget,
        )
        .context("capturing descriptor-backed isolated Git output")
    }

    fn git_command(&self, args: &[impl AsRef<OsStr>]) -> Command {
        let mut command = Command::new("git");
        #[cfg(test)]
        if let Some(program) = self.test_git_program.as_deref() {
            command = Command::new(program);
        }
        command.args(args.iter().map(AsRef::as_ref));
        configure_isolated_git(
            &mut command,
            &self.invocation_dir,
            &self.root,
            &self.git_dir,
            &self.index,
            &self.objects,
            self.extra_alternates.as_deref(),
        );
        command
    }

    fn copy_index(&self, live: &LiveRepository) -> Result<()> {
        ensure!(
            index_inputs(&live.index, live.shared_index.as_deref(), &live.controls)?
                == live.index_inputs,
            "live Git index changed after snapshot admission began"
        );
        if let Some(active_index) = live
            .index_inputs
            .iter()
            .find(|input| input.admitted_path() == live.index)
        {
            copy_and_verify_index(
                active_index,
                &self.index,
                "active Git index",
                &live.controls,
            )?;
        } else {
            self.git(&["read-tree", "--empty"])?;
        }
        if let Some(shared_index) = live.shared_index.as_deref()
            && let Some(shared_input) = live
                .index_inputs
                .iter()
                .find(|input| input.admitted_path() == shared_index)
        {
            let name = shared_index
                .file_name()
                .context("active shared index has no filename")?;
            copy_and_verify_index(
                shared_input,
                &self.git_dir.join(name),
                "active split-index backing file",
                &live.controls,
            )?;
        }
        ensure!(
            index_inputs(&live.index, live.shared_index.as_deref(), &live.controls)?
                == live.index_inputs,
            "live Git index changed while its private snapshot was copied"
        );
        Ok(())
    }

    pub(in crate::precommit_snapshot) fn verify_live_index(
        live: &LiveRepository,
        controls: &SetupControls,
    ) -> Result<()> {
        ensure!(
            index_inputs(&live.index, live.shared_index.as_deref(), controls)? == live.index_inputs,
            "live Git index or split-index backing file changed while the native gate ran"
        );
        Ok(())
    }

    fn write_alternates(&self, objects: &Path, alternate_records: &[u8]) -> Result<()> {
        let file = self.objects.join("info/alternates");
        let mut encoded =
            Vec::with_capacity(objects.as_os_str().as_bytes().len() + alternate_records.len() + 4);
        encoded.push(b'"');
        for byte in objects.as_os_str().as_bytes() {
            match byte {
                b'\\' => encoded.extend_from_slice(b"\\\\"),
                b'"' => encoded.extend_from_slice(b"\\\""),
                b'\n' => encoded.extend_from_slice(b"\\n"),
                b'\t' => encoded.extend_from_slice(b"\\t"),
                b'\r' => encoded.extend_from_slice(b"\\r"),
                0x20..=0x7e => encoded.push(*byte),
                _ => encoded.extend_from_slice(format!("\\{byte:03o}").as_bytes()),
            }
        }
        encoded.extend_from_slice(b"\"\n");
        encoded.extend_from_slice(alternate_records);
        fs::write(file, encoded).context("writing Git object alternates")
    }
}

fn copy_and_verify_index(
    input: &super::capture::IndexInput,
    destination: &Path,
    label: &str,
    controls: &SetupControls,
) -> Result<()> {
    controls.check_start()?;
    fs::copy(input.admitted_path(), destination).with_context(|| format!("copying {label}"))?;
    verify_private_index_copy(input, destination, label, controls)
}

pub(in crate::precommit_snapshot) fn verify_private_index_copy(
    input: &super::capture::IndexInput,
    destination: &Path,
    label: &str,
    controls: &SetupControls,
) -> Result<()> {
    controls.check_start()?;
    let metadata =
        fs::symlink_metadata(destination).with_context(|| format!("inspecting private {label}"))?;
    ensure!(
        metadata.file_type().is_file() && metadata.len() == input.admitted_length(),
        "private {label} differs in type or length from admitted input"
    );
    controls.check_start()?;
    let mut file = fs::File::from(
        open(
            destination,
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .with_context(|| format!("opening private {label}"))?,
    );
    let opened = file
        .metadata()
        .with_context(|| format!("restating private {label}"))?;
    ensure!(
        opened.file_type().is_file()
            && opened.len() == input.admitted_length()
            && opened.dev() == metadata.dev()
            && opened.ino() == metadata.ino(),
        "private {label} changed while opening"
    );
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    let mut total = 0_u64;
    loop {
        controls.check_start()?;
        let count = file
            .read(&mut buffer)
            .with_context(|| format!("hashing private {label}"))?;
        if count == 0 {
            break;
        }
        let count_u64 = u64::try_from(count).context("converting private Git index read length")?;
        total = total
            .checked_add(count_u64)
            .context("counting private Git index bytes")?;
        ensure!(
            total <= input.admitted_length(),
            "private {label} grew while it was hashed"
        );
        hasher.update(&buffer[..count]);
    }
    controls.check_start()?;
    let digest: [u8; 32] = hasher.finalize().into();
    ensure!(
        total == input.admitted_length() && digest == *input.admitted_digest(),
        "private {label} digest differs from admitted input"
    );
    Ok(())
}

fn live_alternate_records(live: &LiveRepository) -> Result<Vec<u8>> {
    let alternates_file = live.objects.join("info/alternates");
    let has_alternates_file = match fs::symlink_metadata(&alternates_file) {
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error).context("inspecting live object alternates file"),
    };
    let has_environment_alternates = live
        .alternates
        .as_ref()
        .is_some_and(|alternates| !alternates.is_empty());
    if !has_alternates_file && !has_environment_alternates {
        return Ok(Vec::new());
    }

    let mut command = Command::new("git");
    command
        .args(["count-objects", "-v"])
        .current_dir(&live.invocation_dir);
    let output = capture_strict(
        &mut command,
        &live.controls,
        EnvironmentMode::Inherit,
        super::super::process::METADATA_OUTPUT_LIMIT,
        1024 * 1024,
        CommandBudget::PhaseRemainder,
    )
    .context("enumerating Git object alternates")?;
    let stdout = checked(output, "enumerating Git object alternates")?;
    ensure!(
        stdout.last() == Some(&b'\n'),
        "Git alternate listing is missing its final record terminator"
    );

    let mut records = Vec::new();
    for line in stdout.split(|byte| *byte == b'\n') {
        let Some(path_record) = line.strip_prefix(b"alternate: ") else {
            continue;
        };
        ensure!(
            !path_record.is_empty(),
            "Git returned an empty alternate path"
        );
        ensure!(
            !path_record.contains(&0) && !path_record.contains(&b'\n'),
            "Git returned a malformed alternate path record"
        );
        records.extend_from_slice(path_record);
        records.push(b'\n');
    }
    Ok(records)
}

fn admit_staged_index_entries(root: &Path, stdout: &mut fs::File) -> Result<()> {
    let mut reader = BufReader::with_capacity(64 * 1024, stdout);
    let mut chunk = vec![0; 64 * 1024].into_boxed_slice();
    let mut header = Vec::with_capacity(32);
    let mut path = Vec::new();
    let mut after_tab = false;
    let mut path_is_symlink = false;
    let mut path_has_bytes = false;

    loop {
        let count = reader
            .read(&mut chunk)
            .context("reading staged index spool")?;
        if count == 0 {
            break;
        }
        for byte in &chunk[..count] {
            if !after_tab {
                match *byte {
                    0 => {
                        bail!("parsing staged index entry without a path separator");
                    }
                    b'\t' => {
                        ensure!(!header.is_empty(), "parsing empty staged index metadata");
                        ensure!(
                            !header.starts_with(b"160000 "),
                            "staged submodules require an isolated source checkout"
                        );
                        path_is_symlink = header.starts_with(b"120000 ");
                        after_tab = true;
                    }
                    _ => {
                        ensure!(
                            header.len() < STAGED_INDEX_HEADER_LIMIT,
                            "staged index metadata record exceeds its limit"
                        );
                        header.push(*byte);
                    }
                }
                continue;
            }

            if *byte == 0 {
                ensure!(
                    path_has_bytes,
                    "parsing staged index entry with an empty path"
                );
                if path_is_symlink {
                    let link_path = root.join(OsStr::from_bytes(&path));
                    validate_link(root, &link_path)?;
                }
                header.clear();
                path.clear();
                after_tab = false;
                path_is_symlink = false;
                path_has_bytes = false;
            } else {
                path_has_bytes = true;
                if path_is_symlink {
                    ensure!(
                        path.len() < STAGED_SYMLINK_PATH_LIMIT,
                        "staged symlink path exceeds its resolution limit"
                    );
                    path.push(*byte);
                }
            }
        }
    }

    ensure!(
        !after_tab && header.is_empty(),
        "staged index spool ended with an incomplete record"
    );
    Ok(())
}
