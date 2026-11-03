// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use std::path::Path;

use jackin_core::{
    Agent, HookKind, HostPath, LaunchId, PathError, ProjectRoot, RepoPath, SandboxPath,
    StdFileSystem, sandbox_layout,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn sandbox_path_normalizes_and_rejects_non_absolute_or_parent_paths() -> TestResult {
    for (input, expected) in [
        ("/sandbox/project", "/sandbox/project"),
        ("/sandbox/./project", "/sandbox/project"),
        ("/sandbox/project/", "/sandbox/project"),
    ] {
        assert_eq!(SandboxPath::parse(input)?.as_str(), expected, "{input}");
    }
    assert!(matches!(
        SandboxPath::parse(""),
        Err(PathError::NotAbsolute(_))
    ));
    assert!(matches!(
        SandboxPath::parse("sandbox/project"),
        Err(PathError::NotAbsolute(_))
    ));
    assert!(matches!(
        SandboxPath::parse("/sandbox/../etc"),
        Err(PathError::ParentSegment(_))
    ));
    assert!(matches!(
        SandboxPath::parse("/sandbox//project"),
        Err(PathError::EmptySegment(_))
    ));
    let base = SandboxPath::parse("/sandbox/project")?;
    assert_eq!(base.join("x")?.as_str(), "/sandbox/project/x");
    assert!(matches!(base.join(".."), Err(PathError::ParentSegment(_))));
    assert!(matches!(base.join(""), Err(PathError::EmptySegment(_))));
    Ok(())
}

#[test]
fn host_path_rejects_parent_segments_and_folds_dots_and_slashes() -> TestResult {
    for text in ["/a/../b", "/..", "/a/b/..", "/a/link/../b"] {
        assert!(
            matches!(
                HostPath::parse(Path::new(text)),
                Err(PathError::ParentSegment(_))
            ),
            "{text}"
        );
    }
    for text in ["/a/..b", "/a/b..", "/a/..."] {
        assert_eq!(HostPath::parse(Path::new(text))?.as_path(), Path::new(text));
    }
    let cwd = HostPath::parse(Path::new("/w"))?;
    for text in ["../x", "x/../y"] {
        assert!(
            matches!(
                HostPath::resolve(Path::new(text), &cwd),
                Err(PathError::ParentSegment(_))
            ),
            "{text}"
        );
    }
    for (text, expected) in [("x", "/w/x"), ("./x", "/w/x"), ("/abs", "/abs")] {
        assert_eq!(
            HostPath::resolve(Path::new(text), &cwd)?.as_path(),
            Path::new(expected),
            "{text}"
        );
    }
    for text in ["/a/./b", "/a//b", "/a/b/"] {
        assert_eq!(
            HostPath::parse(Path::new(text))?.as_path(),
            Path::new("/a/b"),
            "{text}"
        );
    }
    Ok(())
}

#[test]
fn repo_path_is_relative_without_dot_or_empty_segments() -> TestResult {
    assert_eq!(RepoPath::parse("src/main.rs")?.as_str(), "src/main.rs");
    assert_eq!(RepoPath::parse("a")?.as_str(), "a");
    assert!(matches!(
        RepoPath::parse("/abs"),
        Err(PathError::NotRelative(_))
    ));
    for text in ["../x", "a/../b"] {
        assert!(
            matches!(RepoPath::parse(text), Err(PathError::ParentSegment(_))),
            "{text}"
        );
    }
    for text in ["a//b", "a/./b", "a/", ""] {
        assert!(
            matches!(RepoPath::parse(text), Err(PathError::EmptySegment(_))),
            "{text}"
        );
    }
    Ok(())
}

#[test]
fn sandbox_layout_matches_the_documented_literals() -> TestResult {
    assert_eq!(sandbox_layout::PROJECT.as_str(), "/sandbox/project");
    assert_eq!(sandbox_layout::JACKIN_DIR.as_str(), "/sandbox/.jackin");
    assert_eq!(
        sandbox_layout::RUN_STATE.as_str(),
        "/sandbox/.jackin/run/state"
    );
    assert_eq!(
        sandbox_layout::RUN_START.as_str(),
        "/sandbox/.jackin/run/start"
    );
    assert_eq!(
        sandbox_layout::HOOKS_DIR.as_str(),
        "/usr/local/lib/jackin/hooks"
    );
    assert_eq!(sandbox_layout::GIT.as_str(), "/usr/bin/git");
    assert_eq!(sandbox_layout::PRINTENV.as_str(), "/usr/bin/printenv");
    assert_eq!(sandbox_layout::SHA256SUM.as_str(), "/usr/bin/sha256sum");
    assert_eq!(sandbox_layout::STAT.as_str(), "/usr/bin/stat");
    let launch = LaunchId::parse("L")?;
    assert_eq!(
        sandbox_layout::env_file(&launch).as_str(),
        "/sandbox/.jackin/env-L"
    );
    assert_eq!(
        sandbox_layout::uploaded_marker(&launch).as_str(),
        "/sandbox/.jackin/uploaded-L"
    );
    assert_eq!(
        sandbox_layout::setup_done_marker(&launch).as_str(),
        "/sandbox/.jackin/setup-done-L"
    );
    assert_eq!(
        sandbox_layout::hook_path(HookKind::Setup).as_str(),
        "/usr/local/lib/jackin/hooks/setup"
    );
    assert_eq!(
        sandbox_layout::hook_path(HookKind::Preflight).as_str(),
        "/usr/local/lib/jackin/hooks/preflight"
    );
    let fixed = sandbox_layout::fixed();
    assert_eq!(fixed.len(), 11);
    let distinct: HashSet<&str> = fixed.iter().map(|path| path.as_str()).collect();
    assert_eq!(distinct.len(), 11);
    for entry in fixed {
        assert_eq!(&SandboxPath::parse(entry.as_str())?, entry);
    }
    for entry in Agent::Codex.spec().paths() {
        assert_eq!(&SandboxPath::parse(entry.as_str())?, entry);
    }
    Ok(())
}

#[tokio::test]
async fn project_root_open_requires_a_directory_with_a_git_entry() -> TestResult {
    let dir = tempfile::tempdir()?;
    let fs = StdFileSystem::default();
    let at = |name: &str| HostPath::parse(&dir.path().join(name));

    std::fs::create_dir_all(dir.path().join("plain/.git"))?;
    ProjectRoot::open(&fs, &at("plain")?).await?;

    std::fs::create_dir_all(dir.path().join("worktree"))?;
    std::fs::write(dir.path().join("worktree/.git"), "gitdir: /elsewhere\n")?;
    ProjectRoot::open(&fs, &at("worktree")?).await?;

    std::fs::create_dir_all(dir.path().join("linked"))?;
    std::os::unix::fs::symlink(
        dir.path().join("plain/.git"),
        dir.path().join("linked/.git"),
    )?;
    assert!(matches!(
        ProjectRoot::open(&fs, &at("linked")?).await,
        Err(PathError::UnsupportedGitEntry(_))
    ));

    std::fs::create_dir_all(dir.path().join("bare"))?;
    assert!(matches!(
        ProjectRoot::open(&fs, &at("bare")?).await,
        Err(PathError::NotAGitWorkTree(_))
    ));

    std::fs::write(dir.path().join("file"), "x")?;
    assert!(matches!(
        ProjectRoot::open(&fs, &at("file")?).await,
        Err(PathError::NotADirectory(_))
    ));
    assert!(matches!(
        ProjectRoot::open(&fs, &at("missing")?).await,
        Err(PathError::NotADirectory(_))
    ));
    Ok(())
}

#[test]
fn path_newtypes_print_their_value_as_plain_text() -> TestResult {
    let host = |text: &str| HostPath::parse(Path::new(text));
    assert_eq!(host("/a/b c/\u{fc}")?.to_string(), "/a/b c/\u{fc}");
    assert_eq!(host("/q\"d\\e")?.to_string(), "/q\"d\\e");
    assert_eq!(host("/a/b\nc\u{1b}")?.to_string(), "/a/b\\nc\\u{1b}");
    assert_eq!(
        SandboxPath::parse("/sandbox/project/")?.to_string(),
        "/sandbox/project"
    );
    assert_eq!(RepoPath::parse("src/a.rs")?.to_string(), "src/a.rs");
    assert_eq!(
        SandboxPath::parse("/a\nb\u{1b}c")?.to_string(),
        "/a\\nb\\u{1b}c"
    );
    assert_eq!(RepoPath::parse("a\nb\u{1b}c")?.to_string(), "a\\nb\\u{1b}c");
    Ok(())
}

#[tokio::test]
async fn project_root_prints_its_host_path() -> TestResult {
    let dir = tempfile::tempdir()?;
    std::fs::create_dir_all(dir.path().join("plain/.git"))?;
    let path = HostPath::parse(&dir.path().join("plain"))?;
    let root = ProjectRoot::open(&StdFileSystem::default(), &path).await?;
    assert_eq!(root.to_string(), path.to_string());
    assert_eq!(
        root.to_string(),
        dir.path().join("plain").display().to_string()
    );
    Ok(())
}

#[test]
fn path_errors_show_the_rejected_text_as_plain_text() {
    let host = |text: &str| {
        HostPath::parse(Path::new(text))
            .err()
            .map(|e| e.to_string())
    };
    let sandbox = |text: &str| SandboxPath::parse(text).err().map(|e| e.to_string());
    let repo = |text: &str| RepoPath::parse(text).err().map(|e| e.to_string());
    let shown = [
        (host("rel/x"), "path is not absolute: rel/x"),
        (host("rel\u{1b}x"), "path is not absolute: rel\\u{1b}x"),
        (
            host("/a\n/../b"),
            "path contains a parent segment: /a\\n/../b",
        ),
        (sandbox("rel\nx"), "path is not absolute: rel\\nx"),
        (
            sandbox("/a\u{1b}//b"),
            "path contains an empty segment: /a\\u{1b}//b",
        ),
        (repo("/a\u{202e}"), "path is not relative: /a\\u{202e}"),
        (repo("a\n/.."), "path contains a parent segment: a\\n/.."),
        (repo("a\n//b"), "path contains an empty segment: a\\n//b"),
    ];
    for (message, expected) in shown {
        assert_eq!(message.as_deref(), Some(expected));
    }
}
