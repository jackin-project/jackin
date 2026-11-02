// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use jackin_core::{
    Agent, BuilderName, CommitId, DockerLayer, HostPath, ImageId, Platform, RepoPath, RoleName,
};
use jackin_image::{
    ContextSource, HookFiles, ImagePlan, ImageRequest, Rebuild, RoleRevision, SourceFile,
};
use tempfile::TempDir;

type TestResult = Result<(), Box<dyn Error>>;

const CONSTRUCT_DOCKERFILE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/images/construct/Dockerfile"
));
const START: &str = "/usr/local/bin/jackin-start";
const CODEX: &str = "/usr/local/bin/codex";
const CODEX_HOST: &str = "/usr/local/bin/codex-code-mode-host";
const SETUP: &str = "/usr/local/lib/jackin/hooks/setup";
const PREFLIGHT: &str = "/usr/local/lib/jackin/hooks/preflight";
const DIRECTORIES: [&str; 6] = [
    "usr",
    "usr/local",
    "usr/local/bin",
    "usr/local/lib",
    "usr/local/lib/jackin",
    "usr/local/lib/jackin/hooks",
];
const LEAVES: [&str; 5] = [START, CODEX, CODEX_HOST, SETUP, PREFLIGHT];
const NO_GETCAP: &str = "exit 0\n";

fn scan_script() -> Result<Vec<u8>, Box<dyn Error>> {
    let request = ImageRequest {
        role: RoleName::parse("sentinel")?,
        context: HostPath::parse(Path::new("/work/role"))?,
        context_files: vec![RepoPath::parse("Dockerfile")?],
        dockerfile: SourceFile {
            path: RepoPath::parse("Dockerfile")?,
            bytes: b"FROM construct AS role\n".to_vec(),
        },
        hooks: HookFiles {
            setup: None,
            preflight: None,
        },
        revision: RoleRevision::Clean {
            commit: CommitId::parse(&"1".repeat(40))?,
        },
        agent: Agent::Codex,
        platform: Platform::LinuxArm64,
        builder: BuilderName::parse("orbstack")?,
        docker: DockerLayer::Absent,
        rebuild: Rebuild::No,
    };
    let id = ImageId::parse(&format!("sha256:{}", "1".repeat(64)))?;
    let built = ImagePlan::new(&request)?.final_builds(&id);
    let ContextSource::Materialized(files) = built.derived.context else {
        return Err("derived context is not materialized".into());
    };
    files
        .into_iter()
        .find(|file| file.name == "scan")
        .map(|file| file.bytes)
        .ok_or_else(|| "the derived context holds no scan".into())
}

struct Outcome {
    ok: bool,
    stdout: String,
    stderr: String,
}

impl Outcome {
    fn lines(&self) -> Vec<&str> {
        self.stderr.lines().collect()
    }
}

struct Fixture {
    _dir: TempDir,
    top: PathBuf,
    uid: u32,
    gid: u32,
}

impl Fixture {
    fn new() -> Result<Fixture, Box<dyn Error>> {
        let dir = TempDir::new()?;
        let top = dir.path().canonicalize()?;
        fs::write(top.join("scan"), scan_script()?)?;
        for sub in ["image", "tools", "trusted", "sandbox"] {
            fs::create_dir(top.join(sub))?;
        }
        let meta = fs::metadata(top.join("image"))?;
        let fx = Fixture {
            _dir: dir,
            top,
            uid: meta.uid(),
            gid: meta.gid(),
        };
        for name in ["find", "tr", "cmp"] {
            fx.tool(name)?;
        }
        fx.stub("getcap", NO_GETCAP)?;
        for directory in DIRECTORIES {
            fx.dir(directory, 0o755)?;
        }
        for leaf in LEAVES {
            fx.file(leaf, b"leaf", 0o755)?;
        }
        let big = vec![7u8; 256 * 1024];
        fs::write(fx.top.join("trusted/codex"), &big)?;
        fs::write(fx.top.join("trusted/codex-code-mode-host"), &big)?;
        fx.file(CODEX, &big, 0o755)?;
        fx.file(CODEX_HOST, &big, 0o755)?;
        Ok(fx)
    }

    fn image(&self, path: &str) -> PathBuf {
        self.top.join("image").join(path.trim_start_matches('/'))
    }

    fn dir(&self, path: &str, mode: u32) -> TestResult {
        let target = self.image(path);
        fs::create_dir_all(&target)?;
        fs::set_permissions(&target, fs::Permissions::from_mode(mode))?;
        Ok(())
    }

    fn file(&self, path: &str, bytes: &[u8], mode: u32) -> TestResult {
        let target = self.image(path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&target, bytes)?;
        fs::set_permissions(&target, fs::Permissions::from_mode(mode))?;
        Ok(())
    }

    fn stub(&self, name: &str, body: &str) -> TestResult {
        let path = self.top.join("tools").join(name);
        match fs::remove_file(&path) {
            Ok(()) | Err(_) => {}
        }
        fs::write(&path, format!("#!/bin/sh\n{body}"))?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
        Ok(())
    }

    fn tool(&self, name: &str) -> TestResult {
        let real = ["/usr/bin", "/bin"]
            .iter()
            .map(|dir| Path::new(dir).join(name))
            .find(|path| path.exists())
            .ok_or_else(|| format!("no real {name}"))?;
        symlink(real, self.top.join("tools").join(name))?;
        Ok(())
    }

    fn drop_tool(&self, name: &str) -> TestResult {
        fs::remove_file(self.top.join("tools").join(name))?;
        Ok(())
    }

    fn arguments(&self) -> Vec<String> {
        let trusted = self.top.join("trusted");
        let mut args = vec![
            "--owner".to_owned(),
            format!("{}:{}", self.uid, self.gid),
            "--same-as".to_owned(),
            format!("{CODEX}={}/codex", trusted.display()),
            "--same-as".to_owned(),
            format!("{CODEX_HOST}={}/codex-code-mode-host", trusted.display()),
            self.top.join("image").display().to_string(),
        ];
        args.extend(LEAVES.iter().map(|leaf| (*leaf).to_owned()));
        args
    }

    fn run(&self, args: &[String]) -> Result<Outcome, Box<dyn Error>> {
        let output = Command::new("/bin/sh")
            .arg(self.top.join("scan"))
            .args(args)
            .current_dir(&self.top)
            .env_clear()
            .env("PATH", self.top.join("tools"))
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?
            .wait_with_output()?;
        Ok(Outcome {
            ok: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    fn scan(&self) -> Result<Outcome, Box<dyn Error>> {
        self.run(&self.arguments())
    }

    fn move_aside(&self, path: &str) -> Result<PathBuf, Box<dyn Error>> {
        let moved = self
            .top
            .join("sandbox")
            .join(path.trim_start_matches('/').replace('/', "_"));
        fs::rename(self.image(path), &moved)?;
        Ok(moved)
    }

    fn relink(&self, path: &str, absolute: bool) -> TestResult {
        let moved = self.move_aside(path)?;
        let target = if absolute {
            moved
        } else {
            let depth = path.trim_start_matches('/').matches('/').count();
            let up = "../".repeat(depth + 1);
            PathBuf::from(format!(
                "{up}sandbox/{}",
                moved.file_name().and_then(|n| n.to_str()).unwrap_or("")
            ))
        };
        symlink(target, self.image(path))?;
        Ok(())
    }
}

#[test]
fn scan_refuses_setuid_and_setgid_files() -> TestResult {
    let fx = Fixture::new()?;
    let clean = fx.scan()?;
    assert!(clean.ok, "a clean tree must pass: {}", clean.stderr);
    assert_eq!(clean.stdout, "jackin-image-scan 1\n");
    assert_eq!(clean.stderr, "");
    for (name, mode) in [("suid", 0o4755), ("sgid", 0o2755)] {
        let fx = Fixture::new()?;
        let path = format!("/usr/local/bin/{name}");
        fx.file(&path, b"x", mode)?;
        let out = fx.scan()?;
        assert!(!out.ok, "{name} must fail the scan");
        assert_eq!(
            out.lines(),
            [format!("setuid or setgid file in role image: {path}")],
            "{name}"
        );
    }
    Ok(())
}

#[test]
fn scan_never_executes_a_file_under_its_root_argument() -> TestResult {
    let fx = Fixture::new()?;
    for shadow in [
        "usr/bin/find",
        "usr/bin/getcap",
        "bin/sh",
        "usr/bin/cmp",
        "usr/bin/tr",
    ] {
        let mark = fx.top.join(format!("ran-{}", shadow.replace('/', "_")));
        fx.file(
            shadow,
            format!("#!/bin/sh\n: > {}\n", mark.display()).as_bytes(),
            0o755,
        )?;
    }
    fx.file("/usr/local/bin/capsuid", b"x", 0o4755)?;
    fx.file("/usr/local/bin/capbin", b"x", 0o755)?;
    fx.stub(
        "getcap",
        "if [ -e \"$2/usr/local/bin/capbin\" ]; then printf '%s\\n' \"$2/usr/local/bin/capbin cap_net_raw=ep\"; fi\n",
    )?;
    let out = fx.scan()?;
    assert!(!out.ok, "{}", out.stderr);
    assert!(
        out.lines()
            .contains(&"setuid or setgid file in role image: /usr/local/bin/capsuid"),
        "{}",
        out.stderr
    );
    assert!(
        out.lines()
            .contains(&"file capability in role image: /usr/local/bin/capbin cap_net_raw=ep"),
        "{}",
        out.stderr
    );
    let marks: Vec<String> = fs::read_dir(&fx.top)?
        .map(|entry| entry.map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect::<Result<_, _>>()?;
    let ran: Vec<&String> = marks.iter().filter(|n| n.starts_with("ran-")).collect();
    assert!(ran.is_empty(), "the scan ran a file from the tree: {ran:?}");
    Ok(())
}

#[test]
fn scan_fails_closed_when_getcap_or_find_is_missing_or_fails() -> TestResult {
    let hit = "printf '%s\\n' \"$2/usr/local/bin/capbin cap_net_raw=ep\"\n";
    let passes = [
        ("quiet", "exit 0\n"),
        (
            "noisy",
            "echo 'Failed to get capabilities of file /proc/1/x' >&2\nexit 0\n",
        ),
    ];
    for (label, body) in passes {
        let fx = Fixture::new()?;
        fx.stub("getcap", body)?;
        let out = fx.scan()?;
        assert!(out.ok, "{label}: {}", out.stderr);
    }
    let fx = Fixture::new()?;
    fx.stub("getcap", hit)?;
    let out = fx.scan()?;
    assert!(!out.ok);
    assert_eq!(
        out.lines(),
        ["file capability in role image: /usr/local/bin/capbin cap_net_raw=ep"]
    );
    let fx = Fixture::new()?;
    fx.stub(
        "getcap",
        "echo 'getcap: cannot read the tree' >&2\nexit 2\n",
    )?;
    let out = fx.scan()?;
    assert!(!out.ok, "a failing getcap must fail the scan");
    assert!(
        out.stderr.contains("getcap failed in role image"),
        "{}",
        out.stderr
    );
    let fx = Fixture::new()?;
    fx.drop_tool("getcap")?;
    let out = fx.scan()?;
    assert!(!out.ok, "a missing getcap must fail the scan");
    assert!(out.stderr.contains("getcap is missing"), "{}", out.stderr);
    let fx = Fixture::new()?;
    fx.drop_tool("find")?;
    fx.stub("find", "exit 1\n")?;
    let out = fx.scan()?;
    assert!(!out.ok, "a failing find must fail the scan");
    assert!(
        out.stderr
            .contains("find failed while looking for setuid or setgid files"),
        "{}",
        out.stderr
    );
    Ok(())
}

fn construct_tools() -> Result<Vec<String>, Box<dyn Error>> {
    let line = CONSTRUCT_DOCKERFILE
        .lines()
        .find_map(|line| line.trim().strip_prefix("RUN for tool in "))
        .ok_or("no forbidden-tool loop in the construct Dockerfile")?;
    let (tools, _) = line.split_once(';').ok_or("unreadable tool list")?;
    Ok(tools.split_whitespace().map(str::to_owned).collect())
}

fn script_tools() -> Result<Vec<String>, Box<dyn Error>> {
    let script = String::from_utf8(scan_script()?)?;
    let line = script
        .lines()
        .find_map(|line| line.strip_prefix("tools=\""))
        .and_then(|rest| rest.strip_suffix('"'))
        .ok_or("no tools= line in the scan")?;
    Ok(line.split_whitespace().map(str::to_owned).collect())
}

#[test]
fn scan_refuses_forbidden_tools_by_name_anywhere_in_the_tree() -> TestResult {
    let tools = construct_tools()?;
    assert_eq!(tools.len(), 13, "{tools:?}");
    let mut from_script = script_tools()?;
    from_script.sort();
    let mut from_construct = tools.clone();
    from_construct.sort();
    assert_eq!(from_script, from_construct, "scan list against construct");
    for tool in &tools {
        let fx = Fixture::new()?;
        fx.file(&format!("/usr/bin/{tool}"), b"x", 0o755)?;
        fx.file(&format!("/usr/local/sbin/{tool}"), b"x", 0o755)?;
        fs::create_dir_all(fx.image("/opt/x"))?;
        symlink("/bin/true", fx.image(&format!("/opt/x/{tool}")))?;
        let out = fx.scan()?;
        assert!(!out.ok, "{tool} must fail the scan");
        let mut found: Vec<&str> = out.lines();
        found.sort_unstable();
        let mut wanted = vec![
            format!("forbidden tool in role image: /opt/x/{tool}"),
            format!("forbidden tool in role image: /usr/bin/{tool}"),
            format!("forbidden tool in role image: /usr/local/sbin/{tool}"),
        ];
        wanted.sort();
        assert_eq!(found, wanted, "{tool}");
    }
    let fx = Fixture::new()?;
    fx.file("/usr/share/completions/docker", b"x", 0o644)?;
    fx.file("/usr/share/doc/docker/README", b"x", 0o644)?;
    fx.file("/etc/sudoers", b"x", 0o440)?;
    fx.file("/usr/lib/x/docker/docker-compose", b"x", 0o755)?;
    let out = fx.scan()?;
    assert!(out.ok, "names that are not tools must pass: {}", out.stderr);
    Ok(())
}

fn expect_refused(fx: &Fixture, path: &str, reason: &str, label: &str) -> TestResult {
    let out = fx.scan()?;
    assert!(!out.ok, "{label}: the scan passed");
    let line = format!("protected path is not a real root-owned path: {path} ({reason})");
    assert!(
        out.lines().contains(&line.as_str()),
        "{label}: {line:?} not in {}",
        out.stderr
    );
    Ok(())
}

#[test]
fn scan_refuses_a_protected_path_that_is_not_a_real_root_owned_path() -> TestResult {
    let fx = Fixture::new()?;
    let out = fx.scan()?;
    assert!(out.ok, "real owned paths must pass: {}", out.stderr);
    for (path, absolute) in [
        ("/usr/local/lib/jackin", true),
        ("/usr/local/lib/jackin", false),
        ("/usr/local", false),
        ("/usr/local/lib", false),
        ("/usr/local/bin", false),
        ("/usr/local/lib/jackin/hooks", false),
    ] {
        let fx = Fixture::new()?;
        fx.relink(path, absolute)?;
        expect_refused(&fx, path, "symlink", &format!("{path} absolute={absolute}"))?;
    }
    for leaf in LEAVES {
        let fx = Fixture::new()?;
        fx.relink(leaf, false)?;
        expect_refused(&fx, leaf, "symlink", &format!("{leaf} as a symlink"))?;
        let fx = Fixture::new()?;
        fs::remove_file(fx.image(leaf))?;
        expect_refused(&fx, leaf, "missing", &format!("{leaf} missing"))?;
        let fx = Fixture::new()?;
        fs::remove_file(fx.image(leaf))?;
        fx.dir(leaf, 0o755)?;
        expect_refused(&fx, leaf, "wrong type", &format!("{leaf} as a directory"))?;
    }
    for (path, mode) in [
        (CODEX, 0o775),
        (SETUP, 0o757),
        ("/usr/local/lib/jackin", 0o775),
        ("/usr/local/bin", 0o757),
    ] {
        let fx = Fixture::new()?;
        fs::set_permissions(fx.image(path), fs::Permissions::from_mode(mode))?;
        expect_refused(
            &fx,
            path,
            "writable by group or others",
            &format!("{path} {mode:o}"),
        )?;
    }
    let fx = Fixture::new()?;
    let mut args = fx.arguments();
    args[1] = format!("{}:{}", fx.uid + 1, fx.gid);
    let out = fx.run(&args)?;
    assert!(
        !out.ok && out.stderr.contains("not owned by"),
        "{}",
        out.stderr
    );
    args[1] = format!("{}:{}", fx.uid, fx.gid + 1);
    let out = fx.run(&args)?;
    assert!(
        !out.ok && out.stderr.contains("not owned by"),
        "{}",
        out.stderr
    );
    Ok(())
}

#[test]
fn scan_refuses_a_codex_file_that_differs_from_the_trusted_copy() -> TestResult {
    let fx = Fixture::new()?;
    assert!(fx.scan()?.ok, "identical bytes must pass");
    let wanted = |path: &str| format!("codex file differs from the trusted copy: {path}");
    let mut last = vec![7u8; 256 * 1024];
    last[256 * 1024 - 1] = 8;
    for (label, path) in [("codex", CODEX), ("host", CODEX_HOST)] {
        let fx = Fixture::new()?;
        fx.file(path, &last, 0o755)?;
        let out = fx.scan()?;
        assert!(
            !out.ok && out.lines() == [wanted(path)],
            "{label} last byte: {}",
            out.stderr
        );
        let fx = Fixture::new()?;
        fx.file(path, b"", 0o755)?;
        let out = fx.scan()?;
        assert!(
            !out.ok && out.lines() == [wanted(path)],
            "{label} empty: {}",
            out.stderr
        );
        let fx = Fixture::new()?;
        fs::remove_file(fx.image(path))?;
        let out = fx.scan()?;
        assert!(
            out.stderr.contains(&wanted(path)),
            "{label} missing: {}",
            out.stderr
        );
        let fx = Fixture::new()?;
        fx.relink(path, false)?;
        let out = fx.scan()?;
        assert!(
            out.stderr.contains(&wanted(path)),
            "{label} symlink: {}",
            out.stderr
        );
        let fx = Fixture::new()?;
        let name = path.rsplit('/').next().unwrap_or(path);
        fs::remove_file(fx.top.join("trusted").join(name))?;
        let out = fx.scan()?;
        assert!(
            out.stderr.contains(&wanted(path)),
            "{label} trusted copy missing: {}",
            out.stderr
        );
        let fx = Fixture::new()?;
        fs::remove_file(fx.image(path))?;
        fx.dir(path, 0o755)?;
        let out = fx.scan()?;
        assert!(
            out.stderr.contains(&wanted(path)),
            "{label} directory: {}",
            out.stderr
        );
    }
    Ok(())
}
