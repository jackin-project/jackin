// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::os::unix::fs::symlink;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

use async_trait::async_trait;
use jackin_core::{
    CREDENTIAL_FILE_MAX_BYTES, CredentialFileProblem, DirEntry, FileIdentity, FileKind, FileLock,
    FileMode, FileSystem, FsError, HostPath, LinkProblem, RegularFile, RegularFileProblem,
    RepoPath, StdFileSystem, read_credential_file,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[derive(Debug, Default)]
struct Counting {
    inner: StdFileSystem,
    reads: Mutex<Vec<(usize, usize)>>,
}

impl Counting {
    fn reads(&self) -> Vec<(usize, usize)> {
        self.reads
            .lock()
            .map(|reads| reads.clone())
            .unwrap_or_default()
    }
}

#[async_trait]
impl FileSystem for Counting {
    async fn read(&self, p: &HostPath) -> Result<Option<Vec<u8>>, FsError> {
        self.inner.read(p).await
    }
    async fn stat(&self, p: &HostPath) -> Result<Option<FileKind>, FsError> {
        self.inner.stat(p).await
    }
    async fn stat_following(&self, p: &HostPath) -> Result<Option<FileKind>, FsError> {
        self.inner.stat_following(p).await
    }
    async fn canonicalize(&self, p: &HostPath) -> Result<Option<HostPath>, FsError> {
        self.inner.canonicalize(p).await
    }
    async fn file_len(&self, p: &HostPath) -> Result<Option<u64>, FsError> {
        self.inner.file_len(p).await
    }
    async fn identity(&self, p: &HostPath) -> Result<Option<FileIdentity>, FsError> {
        self.inner.identity(p).await
    }
    async fn list(&self, d: &HostPath) -> Result<Vec<DirEntry>, FsError> {
        self.inner.list(d).await
    }
    async fn create_dir_all(&self, d: &HostPath) -> Result<(), FsError> {
        self.inner.create_dir_all(d).await
    }
    async fn create_private_dir_all(&self, d: &HostPath) -> Result<(), FsError> {
        self.inner.create_private_dir_all(d).await
    }
    async fn write_new(&self, p: &HostPath, b: &[u8], m: FileMode) -> Result<(), FsError> {
        self.inner.write_new(p, b, m).await
    }
    async fn rename(&self, f: &HostPath, t: &HostPath) -> Result<(), FsError> {
        self.inner.rename(f, t).await
    }
    async fn remove_file(&self, p: &HostPath) -> Result<(), FsError> {
        self.inner.remove_file(p).await
    }
    async fn remove_dir_all(&self, p: &HostPath) -> Result<(), FsError> {
        self.inner.remove_dir_all(p).await
    }
    async fn mode(&self, p: &HostPath) -> Result<Option<FileMode>, FsError> {
        self.inner.mode(p).await
    }
    async fn sync_dir(&self, d: &HostPath) -> Result<(), FsError> {
        self.inner.sync_dir(d).await
    }
    async fn lock(&self, p: &HostPath) -> Result<FileLock, FsError> {
        self.inner.lock(p).await
    }
    async fn read_regular(
        &self,
        r: &HostPath,
        f: &RepoPath,
        max_bytes: usize,
    ) -> Result<Result<RegularFile, RegularFileProblem>, FsError> {
        let found = self.inner.read_regular(r, f, max_bytes).await?;
        if let Ok(mut reads) = self.reads.lock() {
            reads.push((max_bytes, found.as_ref().map_or(0, |file| file.bytes.len())));
        }
        Ok(found)
    }
    async fn read_link(
        &self,
        r: &HostPath,
        f: &RepoPath,
    ) -> Result<Result<String, LinkProblem>, FsError> {
        self.inner.read_link(r, f).await
    }
    async fn write_symlink(&self, p: &HostPath, target: &str) -> Result<(), FsError> {
        self.inner.write_symlink(p, target).await
    }
    async fn read_at(&self, p: &HostPath, offset: u64, len: usize) -> Result<Vec<u8>, FsError> {
        self.inner.read_at(p, offset, len).await
    }
}

fn host(path: &Path) -> Result<HostPath, Box<dyn std::error::Error>> {
    Ok(HostPath::parse(path)?)
}

async fn read(
    fs: &dyn FileSystem,
    path: &Path,
) -> Result<Result<Vec<u8>, CredentialFileProblem>, Box<dyn std::error::Error>> {
    Ok(read_credential_file(fs, &host(path)?).await?)
}

#[tokio::test]
async fn a_credential_file_over_the_cap_is_refused_after_reading_at_most_one_byte_past_it()
-> TestResult {
    let dir = tempfile::tempdir()?;
    let large = dir.path().join("large");
    std::fs::write(&large, vec![b'x'; CREDENTIAL_FILE_MAX_BYTES + 4096])?;
    let fs = Counting::default();
    assert_eq!(
        read(&fs, &large).await?,
        Err(CredentialFileProblem::TooLarge)
    );
    assert_eq!(
        fs.reads(),
        vec![(CREDENTIAL_FILE_MAX_BYTES, CREDENTIAL_FILE_MAX_BYTES + 1)]
    );
    Ok(())
}

#[tokio::test]
async fn a_credential_file_follows_the_operator_link_and_refuses_everything_but_a_small_regular_file()
-> TestResult {
    let dir = tempfile::tempdir()?;
    let base = std::fs::canonicalize(dir.path())?;
    let fs = StdFileSystem::default();
    std::fs::write(base.join("good"), b"token")?;
    std::fs::write(base.join("exact"), vec![b'x'; CREDENTIAL_FILE_MAX_BYTES])?;
    std::fs::write(base.join("over"), vec![b'x'; CREDENTIAL_FILE_MAX_BYTES + 1])?;
    std::fs::create_dir(base.join("folder"))?;
    assert!(
        Command::new("mkfifo")
            .arg(base.join("pipe"))
            .status()?
            .success()
    );
    symlink(base.join("good"), base.join("link"))?;
    symlink(base.join("link"), base.join("chain"))?;
    symlink(&base, base.join("dir-link"))?;
    symlink(base.join("exact"), base.join("exact-link"))?;
    symlink(base.join("over"), base.join("over-link"))?;
    symlink(base.join("folder"), base.join("folder-link"))?;
    symlink(base.join("pipe"), base.join("pipe-link"))?;
    symlink(base.join("gone"), base.join("dangling"))?;

    assert_eq!(read(&fs, &base.join("good")).await?, Ok(b"token".to_vec()));
    assert_eq!(read(&fs, &base.join("link")).await?, Ok(b"token".to_vec()));
    assert_eq!(read(&fs, &base.join("chain")).await?, Ok(b"token".to_vec()));
    assert_eq!(
        read(&fs, &base.join("dir-link/good")).await?,
        Ok(b"token".to_vec())
    );
    assert_eq!(
        read(&fs, &base.join("exact-link"))
            .await?
            .map(|bytes| bytes.len()),
        Ok(CREDENTIAL_FILE_MAX_BYTES)
    );
    for (name, problem) in [
        ("over", CredentialFileProblem::TooLarge),
        ("over-link", CredentialFileProblem::TooLarge),
        ("folder", CredentialFileProblem::NotARegularFile),
        ("folder-link", CredentialFileProblem::NotARegularFile),
        ("pipe", CredentialFileProblem::NotARegularFile),
        ("pipe-link", CredentialFileProblem::NotARegularFile),
        ("gone", CredentialFileProblem::Missing),
        ("dangling", CredentialFileProblem::Missing),
    ] {
        assert_eq!(read(&fs, &base.join(name)).await?, Err(problem), "{name}");
    }
    Ok(())
}
