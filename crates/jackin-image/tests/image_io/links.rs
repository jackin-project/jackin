// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_testkit::guard;
use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::Path;
use std::sync::Arc;

use jackin_core::{DockerSubcommand, FileSystem, ImageId, RepoPath, StdFileSystem};
use jackin_image::{
    BuildStage, ContextEntry, ContextProblem, ContextSource, EngineError, ImageEngine, ImagePlan,
    PlannedBuild,
};
use jackin_testkit::CapturedFile;
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::support::{IidFile, Script, Stack, TestResult, tag_id};

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
        let mut entries = Vec::new();
        for name in names {
            let path = self.root().join(name);
            let repo = RepoPath::parse(name)?;
            if fs::symlink_metadata(&path)?.is_symlink() {
                entries.push(ContextEntry::Link {
                    path: repo,
                    target: fs::read_link(&path)?.to_string_lossy().into_owned(),
                });
            } else {
                entries.push(ContextEntry::File {
                    path: repo,
                    bytes: fs::read(&path)?,
                    executable: fs::metadata(&path)?.permissions().mode() & 0o111 != 0,
                });
            }
        }
        Ok(PlannedBuild {
            context: ContextSource::Entries(entries),
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
    guard(async {
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
    guard(async {
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

#[tokio::test]
async fn buildx_checks_each_listed_link_on_its_own_text_and_never_resolves_chains() -> TestResult {
    guard(async {
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
