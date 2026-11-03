// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use jackin_core::{
    DirEntry, DockerSubcommand, FileKind, FileLock, FileMode, FileSystem, FsError, HostPath,
    ImageId, LinkProblem, RegularFile, RegularFileProblem, RepoPath, StdFileSystem,
};
use jackin_image::{
    BuildStage, ContextProblem, ContextSource, EngineError, ImageEngine, ImagePlan, PlannedBuild,
};
use jackin_testkit::CapturedFile;
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::support::{IidFile, Script, Stack, TestResult, guarded, tag_id};

type Tree = BTreeMap<String, String>;

fn tree(dir: &Path) -> Result<Tree, Box<dyn Error>> {
    let mut rows = Tree::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        for entry in fs::read_dir(&current)? {
            let path = entry?.path();
            let name = path.strip_prefix(dir)?.to_string_lossy().into_owned();
            let meta = fs::symlink_metadata(&path)?;
            let shown = if meta.is_dir() {
                pending.push(path);
                "dir".to_owned()
            } else if meta.is_symlink() {
                format!("link {}", fs::read_link(&path)?.display())
            } else {
                format!("file {:?}", fs::read(&path)?)
            };
            rows.insert(name, shown);
        }
    }
    Ok(rows)
}

struct Lab {
    stack: Stack,
    outside: TempDir,
}

impl Lab {
    fn new(fs: Arc<dyn FileSystem>) -> Result<Lab, Box<dyn Error>> {
        let lab = Lab {
            stack: Stack::with_fs(fs)?,
            outside: tempfile::tempdir()?,
        };
        fs::write(lab.outside.path().join("secret.txt"), b"outside secret")?;
        Ok(lab)
    }

    fn root(&self) -> &Path {
        self.stack.fixture.role.as_path()
    }

    fn file(&self, name: &str, bytes: &[u8], mode: u32) -> TestResult {
        let path = self.root().join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, bytes)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(mode))?;
        Ok(())
    }

    fn link(&self, name: &str, target: impl AsRef<Path>) -> TestResult {
        let path = self.root().join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        symlink(target, path)?;
        Ok(())
    }

    fn build(&self, names: &[&str]) -> Result<PlannedBuild, Box<dyn Error>> {
        let plan = ImagePlan::new(&self.stack.fixture.request()?)?;
        let finals = plan.final_builds(&ImageId::parse(&tag_id("construct"))?);
        let files = names
            .iter()
            .map(|name| RepoPath::parse(name))
            .collect::<Result<_, _>>()?;
        Ok(PlannedBuild {
            context: ContextSource::Listed {
                root: self.stack.fixture.role.clone(),
                files,
            },
            ..finals.role
        })
    }

    async fn refuses(&self, names: &[&str], path: &str, problem: ContextProblem) -> TestResult {
        let before = (tree(self.root())?, tree(self.outside.path())?);
        let (tx, _rx) = mpsc::channel(8);
        let built = self
            .stack
            .engine
            .build(&self.build(names)?, &tx, &CancellationToken::new())
            .await;
        match built {
            Err(EngineError::ContextEntry {
                path: found,
                problem: kind,
            }) => {
                assert_eq!(found, path, "refused path for {names:?}");
                assert_eq!(kind, problem, "problem for {names:?}");
            }
            other => return Err(format!("{names:?}: expected ContextEntry, got {other:?}").into()),
        }
        assert!(
            self.stack.calls(DockerSubcommand::BuildxBuild).is_empty(),
            "docker ran for {names:?}"
        );
        assert!(
            self.stack.fixture.work_entries().is_empty(),
            "work dir for {names:?}"
        );
        assert_eq!(
            (tree(self.root())?, tree(self.outside.path())?),
            before,
            "source trees for {names:?}"
        );
        Ok(())
    }
}

fn std_lab() -> Result<Lab, Box<dyn Error>> {
    Lab::new(Arc::new(StdFileSystem::default()))
}

#[tokio::test]
async fn buildx_stages_listed_links_as_links_with_their_own_target_text() -> TestResult {
    guarded(async {
        let lab = std_lab()?;
        lab.stack.model.script(
            BuildStage::Role,
            Script {
                iid: IidFile::Text(format!("sha256:{}\n", "2".repeat(64))),
                ..Script::default()
            },
        );
        for (name, mode) in [
            ("AGENTS.md", 0o644),
            ("notes.txt", 0o644),
            ("real.txt", 0o644),
            ("run.sh", 0o755),
        ] {
            lab.file(name, format!("body of {name}").as_bytes(), mode)?;
        }
        fs::create_dir_all(lab.root().join("d/g"))?;
        let links = [
            ("CLAUDE.md", "AGENTS.md"),
            ("docs/x", "../AGENTS.md"),
            ("inner", "real.txt"),
            ("broken", "nowhere.txt"),
            ("chain_a", "docs/x"),
            ("shape_a", "b/c"),
            ("d/e", "../b"),
            ("d/g/f", "../../h"),
        ];
        for (name, target) in links {
            lab.link(name, target)?;
        }
        let mut names = vec!["AGENTS.md", "notes.txt", "real.txt", "run.sh"];
        names.extend(links.iter().map(|(name, _)| *name));
        let (tx, _rx) = mpsc::channel(8);
        lab.stack
            .engine
            .build(&lab.build(&names)?, &tx, &CancellationToken::new())
            .await?;
        let mut expected: BTreeMap<String, CapturedFile> = BTreeMap::new();
        for (name, mode) in [
            ("AGENTS.md", 0o644),
            ("notes.txt", 0o644),
            ("real.txt", 0o644),
            ("run.sh", 0o755),
        ] {
            let bytes = format!("body of {name}");
            expected.insert(name.to_owned(), CapturedFile::file(bytes.as_bytes(), mode));
        }
        for (name, target) in links {
            expected.insert(name.to_owned(), CapturedFile::link(target));
        }
        let captured = lab.stack.runner.captured_contexts();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].files, expected);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn buildx_refuses_a_link_whose_text_can_leave_the_role_root() -> TestResult {
    guarded(async {
        let lab = std_lab()?;
        let inside = format!("{}/real.txt", lab.root().display());
        fs::create_dir_all(lab.root().join("r/s"))?;
        let rows = [
            ("link", "/etc/hosts"),
            ("abs_in", inside.as_str()),
            ("up", "../x"),
            ("p/q", "../../outside"),
            ("r/s/t", "../../../x"),
            ("dp1", "b/../c"),
            ("dp2", "a/b/../../outside"),
            ("dp3", "./b/../c"),
            ("dot0", "."),
            ("dot1", "./b"),
            ("trail", "b/"),
            ("dbl", "b//c"),
            ("r/up0", ".."),
        ];
        for (name, target) in rows {
            lab.link(name, target)?;
        }
        for (name, _) in rows {
            lab.refuses(&[name], name, ContextProblem::Escapes).await?;
        }
        Ok(())
    })
    .await
}

#[derive(Debug)]
struct ProblemFs {
    inner: StdFileSystem,
    problem: LinkProblem,
}

#[async_trait]
impl FileSystem for ProblemFs {
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
        root: &HostPath,
        file: &RepoPath,
        max_bytes: usize,
    ) -> Result<Result<RegularFile, RegularFileProblem>, FsError> {
        self.inner.read_regular(root, file, max_bytes).await
    }
    async fn read_link(
        &self,
        root: &HostPath,
        file: &RepoPath,
    ) -> Result<Result<String, LinkProblem>, FsError> {
        match self.inner.read_link(root, file).await? {
            Ok(_) => Ok(Err(self.problem)),
            Err(problem) => Ok(Err(problem)),
        }
    }
    async fn write_symlink(&self, p: &HostPath, target: &str) -> Result<(), FsError> {
        self.inner.write_symlink(p, target).await
    }
    async fn read_at(&self, p: &HostPath, offset: u64, len: usize) -> Result<Vec<u8>, FsError> {
        self.inner.read_at(p, offset, len).await
    }
}

#[tokio::test]
async fn buildx_checks_each_listed_link_on_its_own_text_and_never_resolves_chains() -> TestResult {
    guarded(async {
        let chain = std_lab()?;
        chain.link("chain_ok", "ch_up")?;
        chain.link("ch_up", "../x")?;
        chain
            .refuses(&["chain_ok", "ch_up"], "ch_up", ContextProblem::Escapes)
            .await?;

        let pair = std_lab()?;
        pair.link("d1/up", "..")?;
        pair.link("pair_x", "d1/up/../../outside")?;
        pair.refuses(&["d1/up", "pair_x"], "d1/up", ContextProblem::Escapes)
            .await?;
        Ok(())
    })
    .await
}

#[tokio::test]
async fn buildx_maps_each_problem_found_while_reading_a_link_to_its_context_problem() -> TestResult
{
    guarded(async {
        let rows = [
            (LinkProblem::NonUtf8, ContextProblem::Escapes),
            (LinkProblem::NotALink, ContextProblem::NotARegularFile),
            (LinkProblem::Missing, ContextProblem::Missing),
        ];
        for (found, expected) in rows {
            let lab = Lab::new(Arc::new(ProblemFs {
                inner: StdFileSystem::default(),
                problem: found,
            }))?;
            lab.link("bad", "target.txt")?;
            lab.refuses(&["bad"], "bad", expected).await?;
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn buildx_never_follows_a_linked_directory_to_read_a_listed_link() -> TestResult {
    guarded(async {
        let through = std_lab()?;
        symlink("secret.txt", through.outside.path().join("x"))?;
        through.link("dir", through.outside.path())?;
        through
            .refuses(&["dir/x"], "dir/x", ContextProblem::Symlink)
            .await
    })
    .await
}

#[tokio::test]
async fn buildx_refuses_a_listed_path_below_a_linked_directory_in_either_order() -> TestResult {
    guarded(async {
        for names in [["d", "d/f"], ["d/f", "d"]] {
            let lab = std_lab()?;
            lab.file("real_dir/f", b"inside", 0o644)?;
            lab.link("d", "real_dir")?;
            lab.refuses(&names, "d/f", ContextProblem::Symlink).await?;
        }
        Ok(())
    })
    .await
}
