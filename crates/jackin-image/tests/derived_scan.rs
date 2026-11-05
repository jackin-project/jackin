// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::Sha256;
use jackin_image::ContextEntry;
use std::error::Error;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use jackin_core::{
    Agent, BuilderName, CommitId, DockerLayer, ImageId, Platform, RepoPath, RoleName,
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
const CONFIG: &str = "/etc/codex/config.toml";
const CONFIG_BYTES: &[u8] = b"check_for_update_on_startup = false\n";
const VERDICT: &str = "/usr/local/lib/jackin/image-scan";
const CODEX_SYSTEM_LAYER: [(&str, bool); 6] = [
    ("/etc/codex/requirements.toml", false),
    ("/etc/codex/managed_config.toml", false),
    ("/etc/codex/hooks.json", false),
    ("/etc/codex/rules", true),
    ("/etc/codex/agents", true),
    ("/etc/codex/skills", true),
];
const DOCKER: &str = "/usr/local/bin/docker";
const BUILDX: &str = "/usr/local/libexec/docker/cli-plugins/docker-buildx";
const COMPOSE: &str = "/usr/local/libexec/docker/cli-plugins/docker-compose";
const CLIENT: [&str; 3] = [DOCKER, BUILDX, COMPOSE];
const CLIENT_DIRECTORIES: [&str; 3] = [
    "usr/local/libexec",
    "usr/local/libexec/docker",
    "usr/local/libexec/docker/cli-plugins",
];
const NO_GETCAP: &str = "exit 0\n";
const PASSWD: &str = "root:x:0:0:root:/root:/bin/bash\ndaemon:x:1:1:daemon:/usr/sbin:/usr/sbin/nologin\nsandbox:x:1000:1000::/sandbox:/bin/bash\nnobody:x:65534:65534:nobody:/nonexistent:/usr/sbin/nologin\n";
const GROUP: &str = "root:x:0:\ndaemon:x:1:\nsandbox:x:1000:\nnogroup:x:65534:\n";
const SANDBOX_PASSWD: &str = "sandbox:x:1000:1000::/sandbox:/bin/bash\n";
const SANDBOX_GROUP: &str = "sandbox:x:1000:\n";
const MISSING_PASSWD: &str = "sandbox account is missing or duplicated in /etc/passwd";
const WRONG_PASSWD: &str = "sandbox account is not 1000:1000 in /etc/passwd";
const MISSING_GROUP: &str = "sandbox group is missing or duplicated in /etc/group";
const WRONG_GROUP: &str = "sandbox group is not gid 1000 in /etc/group";

fn entry(path: &str) -> Result<ContextEntry, Box<dyn Error>> {
    Ok(ContextEntry::File {
        path: RepoPath::parse(path)?,
        bytes: path.as_bytes().to_vec(),
        executable: false,
    })
}

fn leaf_bytes(leaf: &str) -> Vec<u8> {
    if leaf == CODEX || leaf == CODEX_HOST {
        vec![7u8; 256 * 1024]
    } else {
        format!("#!/bin/sh\n# {leaf}\n").into_bytes()
    }
}

fn trusted_name(leaf: &str) -> &str {
    leaf.rsplit('/').next().unwrap_or(leaf)
}

fn derived_file(name: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    let request = ImageRequest {
        role: RoleName::parse("sentinel")?,
        context_files: vec![entry("Dockerfile")?],
        dockerfile: SourceFile {
            path: RepoPath::parse("Dockerfile")?,
            bytes: b"FROM construct AS role\n".to_vec(),
        },
        hooks: HookFiles {
            setup: None,
            preflight: None,
        },
        revision: RoleRevision {
            commit: CommitId::parse(&"1".repeat(40))?,
            digest: Sha256::of(b"clean"),
        },
        agent: Agent::Codex,
        platform: Platform::LinuxArm64,
        builder: BuilderName::parse("orbstack")?,
        docker: DockerLayer::Absent,
        read_write: Vec::new(),
        rebuild: Rebuild::No,
    };
    let id = ImageId::parse(&format!("sha256:{}", "1".repeat(64)))?;
    let built = ImagePlan::new(&request)?.final_builds(&id);
    let ContextSource::Materialized(files) = built.derived.context else {
        return Err("derived context is not materialized".into());
    };
    files
        .into_iter()
        .find(|file| file.name == name)
        .map(|file| file.bytes)
        .ok_or_else(|| format!("the derived context holds no {name}").into())
}

fn scan_script() -> Result<Vec<u8>, Box<dyn Error>> {
    derived_file("scan")
}

fn scan_absent_paths() -> Result<Vec<String>, Box<dyn Error>> {
    let dockerfile = String::from_utf8(derived_file("Dockerfile")?)?;
    let (_, scan_stage) = dockerfile
        .split_once("AS scan\n")
        .ok_or("the derived Dockerfile has no scan stage")?;
    let words: Vec<&str> = scan_stage.split_whitespace().collect();
    Ok(words
        .windows(2)
        .filter(|pair| pair[0] == "--absent")
        .map(|pair| pair[1].to_owned())
        .collect())
}

struct Outcome {
    ok: bool,
    code: Option<i32>,
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
    absent: Vec<String>,
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
            absent: scan_absent_paths()?,
        };
        for name in ["find", "tr", "cmp"] {
            fx.tool(name)?;
        }
        fx.stub("getcap", NO_GETCAP)?;
        for directory in DIRECTORIES {
            fx.dir(directory, 0o755)?;
        }
        for leaf in LEAVES {
            fx.file(leaf, &leaf_bytes(leaf), 0o755)?;
            fs::write(fx.trusted(leaf), leaf_bytes(leaf))?;
        }
        fx.dir("/etc", 0o755)?;
        fx.dir("/etc/codex", 0o755)?;
        fx.file(CONFIG, CONFIG_BYTES, 0o644)?;
        fs::write(fx.trusted(CONFIG), CONFIG_BYTES)?;
        fx.file("/etc/passwd", PASSWD.as_bytes(), 0o644)?;
        fx.file("/etc/group", GROUP.as_bytes(), 0o644)?;
        Ok(fx)
    }

    fn trusted(&self, leaf: &str) -> PathBuf {
        self.top.join("trusted").join(trusted_name(leaf))
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
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
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
        let mut args = vec!["--owner".to_owned(), format!("{}:{}", self.uid, self.gid)];
        for leaf in LEAVES {
            args.push("--same-as".to_owned());
            args.push(format!("{leaf}={}", self.trusted(leaf).display()));
        }
        args.extend(self.data_arguments());
        args.push(self.top.join("image").display().to_string());
        args.extend(LEAVES.iter().map(|leaf| (*leaf).to_owned()));
        args
    }

    fn data_arguments(&self) -> Vec<String> {
        let mut args = vec![
            "--same-as".to_owned(),
            format!("{CONFIG}={}", self.trusted(CONFIG).display()),
            "--data".to_owned(),
            CONFIG.to_owned(),
        ];
        for path in &self.absent {
            args.push("--absent".to_owned());
            args.push(path.clone());
        }
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
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    fn client(&self) -> TestResult {
        for directory in CLIENT_DIRECTORIES {
            self.dir(directory, 0o755)?;
        }
        for leaf in CLIENT {
            let bytes = format!("client {leaf}\n").into_bytes();
            self.file(leaf, &bytes, 0o755)?;
            fs::write(self.trusted(leaf), bytes)?;
        }
        Ok(())
    }

    fn client_arguments(&self) -> Vec<String> {
        let mut args = vec!["--owner".to_owned(), format!("{}:{}", self.uid, self.gid)];
        for leaf in LEAVES.iter().chain(&CLIENT) {
            args.push("--same-as".to_owned());
            args.push(format!("{leaf}={}", self.trusted(leaf).display()));
        }
        for leaf in CLIENT {
            args.push("--allow-tool".to_owned());
            args.push(leaf.to_owned());
        }
        args.extend(self.data_arguments());
        args.push(self.top.join("image").display().to_string());
        args.extend(LEAVES.iter().chain(&CLIENT).map(|leaf| (*leaf).to_owned()));
        args
    }

    fn scan_client(&self) -> Result<Outcome, Box<dyn Error>> {
        self.run(&self.client_arguments())
    }

    fn scan(&self) -> Result<Outcome, Box<dyn Error>> {
        self.run(&self.arguments())
    }

    fn scan_read_write(&self, paths: &[&str]) -> Result<Outcome, Box<dyn Error>> {
        let mut args = self.arguments();
        let image = self.top.join("image").display().to_string();
        let at = args
            .iter()
            .position(|arg| *arg == image)
            .ok_or("the arguments hold no image root")?;
        let flags: Vec<String> = paths
            .iter()
            .flat_map(|path| ["--read-write".to_owned(), (*path).to_owned()])
            .collect();
        args.splice(at..at, flags);
        self.run(&args)
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
    fx.stub(
        "find",
        "n=0\n[ ! -f \"$0.count\" ] || read n < \"$0.count\"\necho $((n + 1)) > \"$0.count\"\nif [ \"$n\" -eq 1 ]; then exit 1; fi\nexec /usr/bin/find \"$@\"\n",
    )?;
    let out = fx.scan()?;
    assert!(!out.ok, "a failing forbidden-tool find must fail the scan");
    assert!(
        out.stderr
            .contains("find failed while looking for forbidden tools"),
        "{}",
        out.stderr
    );
    let fx = Fixture::new()?;
    fx.drop_tool("find")?;
    fx.stub(
        "find",
        "n=0\n[ ! -f \"$0.count\" ] || read n < \"$0.count\"\necho $((n + 1)) > \"$0.count\"\nif [ \"$n\" -eq 3 ]; then exit 1; fi\nexec /usr/bin/find \"$@\"\n",
    )?;
    let out = fx.scan()?;
    assert!(
        !out.ok,
        "a failing control-character find must fail the scan"
    );
    assert!(
        out.stderr
            .contains("find failed while looking for control characters in names"),
        "{}",
        out.stderr
    );
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

fn script_list(name: &str) -> Result<Vec<String>, Box<dyn Error>> {
    let script = String::from_utf8(scan_script()?)?;
    let prefix = format!("{name}=\"");
    let line = script
        .lines()
        .find_map(|line| line.strip_prefix(prefix.as_str()))
        .and_then(|rest| rest.strip_suffix('"'))
        .ok_or_else(|| format!("no {name}= line in the scan"))?;
    Ok(line.split_whitespace().map(str::to_owned).collect())
}

const CLIENT_NAMES: [&str; 4] = ["docker-compose", "docker-buildx", "buildctl", "ctr"];
const DAEMON_NAMES: [&str; 2] = ["docker-proxy", "dockerd-rootless.sh"];

#[test]
fn scan_refuses_forbidden_tools_by_name_anywhere_in_the_tree() -> TestResult {
    let tools = construct_tools()?;
    assert_eq!(tools.len(), 25, "{tools:?}");
    for engine in [
        "dockerd",
        "containerd",
        "containerd-shim",
        "containerd-shim-runc-v2",
        "runc",
        "crun",
        "buildkitd",
        "rootlesskit",
    ]
    .into_iter()
    .chain(DAEMON_NAMES)
    .chain(CLIENT_NAMES)
    {
        assert!(
            tools.iter().any(|tool| tool == engine),
            "{engine} is not forbidden"
        );
    }
    let mut from_script = script_list("tools")?;
    from_script.extend(script_list("client_tools")?);
    from_script.sort();
    let mut from_construct = tools.clone();
    from_construct.sort();
    assert_eq!(from_script, from_construct, "scan list against construct");
    assert_eq!(script_list("client_tools")?, CLIENT_NAMES);
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
    fx.file("/usr/lib/x/docker/compose-notes.txt", b"x", 0o755)?;
    for name in ["ctr", "docker-compose", "docker-buildx", "buildctl"] {
        fx.file(&format!("/usr/share/{name}"), b"x", 0o644)?;
    }
    let out = fx.scan()?;
    assert!(out.ok, "names that are not tools must pass: {}", out.stderr);
    Ok(())
}

fn other_group(current: u32) -> Result<Option<u32>, Box<dyn Error>> {
    let output = Command::new("id")
        .arg("-G")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()?
        .wait_with_output()?;
    Ok(String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .filter_map(|id| id.parse::<u32>().ok())
        .find(|id| *id != current))
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
        ("/usr", false),
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
    for leaf in LEAVES {
        let fx = Fixture::new()?;
        fs::set_permissions(fx.image(leaf), fs::Permissions::from_mode(0o644))?;
        expect_refused(&fx, leaf, "not executable", &format!("{leaf} 644"))?;
    }
    if let Some(gid) = other_group(fx.gid)? {
        for leaf in LEAVES {
            let fx = Fixture::new()?;
            std::os::unix::fs::chown(fx.image(leaf), None, Some(gid))?;
            let want = format!("not owned by {}:{}", fx.uid, fx.gid);
            expect_refused(&fx, leaf, &want, &format!("{leaf} in group {gid}"))?;
        }
    }
    if fx.uid == 0 {
        for leaf in LEAVES {
            let fx = Fixture::new()?;
            std::os::unix::fs::chown(fx.image(leaf), Some(1), None)?;
            let want = format!("not owned by {}:{}", fx.uid, fx.gid);
            expect_refused(&fx, leaf, &want, &format!("{leaf} owned by uid 1"))?;
        }
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
fn scan_refuses_a_protected_file_that_differs_from_the_trusted_copy() -> TestResult {
    let fx = Fixture::new()?;
    assert!(fx.scan()?.ok, "identical bytes must pass");
    let wanted = |path: &str| format!("protected file differs from the trusted copy: {path}");
    for path in LEAVES {
        let trusted = leaf_bytes(path);
        let mut last = trusted.clone();
        if let Some(byte) = last.last_mut() {
            *byte ^= 1;
        }
        let mut appended = trusted.clone();
        appended.push(b'x');
        for (label, bytes) in [
            ("last byte", last),
            ("same size", vec![b'H'; trusted.len()]),
            ("empty", Vec::new()),
            ("appended", appended),
        ] {
            let fx = Fixture::new()?;
            fx.file(path, &bytes, 0o755)?;
            let out = fx.scan()?;
            assert!(
                !out.ok && out.lines() == [wanted(path)],
                "{path} {label}: {}",
                out.stderr
            );
        }
        let fx = Fixture::new()?;
        fs::remove_file(fx.image(path))?;
        let out = fx.scan()?;
        assert!(
            !out.ok && out.stderr.contains(&wanted(path)),
            "{path} missing: {}",
            out.stderr
        );
        let fx = Fixture::new()?;
        fx.relink(path, false)?;
        let out = fx.scan()?;
        assert!(
            out.stderr.contains(&wanted(path)),
            "{path} symlink: {}",
            out.stderr
        );
        let fx = Fixture::new()?;
        fs::remove_file(fx.trusted(path))?;
        let out = fx.scan()?;
        assert!(
            !out.ok && out.stderr.contains(&wanted(path)),
            "{path} trusted copy missing: {}",
            out.stderr
        );
        let fx = Fixture::new()?;
        fs::remove_file(fx.image(path))?;
        fx.dir(path, 0o755)?;
        let out = fx.scan()?;
        assert!(
            out.stderr.contains(&wanted(path)),
            "{path} directory: {}",
            out.stderr
        );
    }
    Ok(())
}

fn expect_lines(fx: &Fixture, label: &str, wanted: &[&str]) -> TestResult {
    let out = fx.scan()?;
    assert!(!out.ok, "{label}: the scan passed");
    assert_eq!(out.lines(), wanted, "{label}");
    Ok(())
}

#[test]
fn scan_requires_the_sandbox_account_at_1000_and_no_other_uid_0_account() -> TestResult {
    let fx = Fixture::new()?;
    fx.file("/etc/passwd", PASSWD.trim_end().as_bytes(), 0o644)?;
    fx.file("/etc/group", GROUP.trim_end().as_bytes(), 0o644)?;
    let out = fx.scan()?;
    assert!(out.ok, "no trailing newline must pass: {}", out.stderr);
    let fx = Fixture::new()?;
    fx.file(
        "/etc/passwd",
        format!("{PASSWD}ops:x:1:0::/:/bin/sh\n").as_bytes(),
        0o644,
    )?;
    assert!(fx.scan()?.ok, "a non-zero uid in group 0 passes");
    let m3 = "malformed line 3 in /etc/passwd";
    let m5 = "malformed line 5 in /etc/passwd";
    let sandbox: [(&str, &str, Vec<&str>); 8] = [
        (
            "uid 0 gid 0",
            "sandbox:x:0:0::/sandbox:/bin/bash\n",
            vec![WRONG_PASSWD],
        ),
        (
            "gid 0",
            "sandbox:x:1000:0::/sandbox:/bin/bash\n",
            vec![WRONG_PASSWD],
        ),
        (
            "uid 0",
            "sandbox:x:0:1000::/sandbox:/bin/bash\n",
            vec![WRONG_PASSWD],
        ),
        (
            "uid 1001",
            "sandbox:x:1001:1001::/sandbox:/bin/bash\n",
            vec![WRONG_PASSWD],
        ),
        (
            "leading zeros",
            "sandbox:x:01000:01000::/sandbox:/bin/bash\n",
            vec![m3, MISSING_PASSWD],
        ),
        ("absent", "", vec![MISSING_PASSWD]),
        (
            "short line",
            "sandbox:x:1000:1000::/sandbox\n",
            vec![m3, MISSING_PASSWD],
        ),
        (
            "second line at uid 0",
            "sandbox:x:1000:1000::/sandbox:/bin/bash\nsandbox:x:0:0::/sandbox:/bin/bash\n",
            vec![MISSING_PASSWD, WRONG_PASSWD],
        ),
    ];
    for (label, line, wanted) in sandbox {
        let fx = Fixture::new()?;
        fx.file(
            "/etc/passwd",
            PASSWD.replace(SANDBOX_PASSWD, line).as_bytes(),
            0o644,
        )?;
        expect_lines(&fx, label, &wanted)?;
    }
    let uid0 = "uid 0 account other than root in /etc/passwd: ";
    let extra: [(&str, Vec<String>); 15] = [
        ("toor:x:0:0::/root:/bin/sh", vec![format!("{uid0}toor")]),
        ("Root:x:0:0::/root:/bin/sh", vec![format!("{uid0}Root")]),
        ("toor:x:00:0::/root:/bin/sh", vec![m5.to_owned()]),
        ("toor:x:+0:0::/root:/bin/sh", vec![m5.to_owned()]),
        ("toor:x: 0:0::/root:/bin/sh", vec![m5.to_owned()]),
        ("toor:x:0x0:0::/root:/bin/sh", vec![m5.to_owned()]),
        ("toor:x:4294967296:0::/root:/bin/sh", vec![m5.to_owned()]),
        ("toor:x:0:00::/root:/bin/sh", vec![m5.to_owned()]),
        ("toor:x:0:+0::/root:/bin/sh", vec![m5.to_owned()]),
        ("toor:x:0:0", vec![m5.to_owned()]),
        ("# sandbox:x:0:0", vec![m5.to_owned()]),
        (
            "# sandbox:x:0:0::/:/bin/sh",
            vec![format!("{uid0}# sandbox")],
        ),
        ("+::::::", vec![m5.to_owned()]),
        ("", vec![m5.to_owned()]),
        ("toor:x:0:0::/root:/bin/sh:extra", vec![m5.to_owned()]),
    ];
    for (line, wanted) in extra {
        let fx = Fixture::new()?;
        fx.file("/etc/passwd", format!("{PASSWD}{line}\n").as_bytes(), 0o644)?;
        let wanted: Vec<&str> = wanted.iter().map(String::as_str).collect();
        expect_lines(&fx, line, &wanted)?;
    }
    let g3 = "malformed line 3 in /etc/group";
    let groups: [(&str, &str, Vec<&str>); 8] = [
        ("gid 0", "sandbox:x:0:\n", vec![WRONG_GROUP]),
        ("gid 1001", "sandbox:x:1001:\n", vec![WRONG_GROUP]),
        (
            "leading zero",
            "sandbox:x:01000:\n",
            vec![g3, MISSING_GROUP],
        ),
        ("plus sign", "sandbox:x:+1000:\n", vec![g3, MISSING_GROUP]),
        ("absent", "", vec![MISSING_GROUP]),
        (
            "twice",
            "sandbox:x:1000:\nsandbox:x:1000:\n",
            vec![MISSING_GROUP],
        ),
        ("two fields", "sandbox:x:1000\n", vec![g3, MISSING_GROUP]),
        (
            "five fields",
            "sandbox:x:1000:a:b\n",
            vec![g3, MISSING_GROUP],
        ),
    ];
    for (label, line, wanted) in groups {
        let fx = Fixture::new()?;
        fx.file(
            "/etc/group",
            GROUP.replace(SANDBOX_GROUP, line).as_bytes(),
            0o644,
        )?;
        expect_lines(&fx, label, &wanted)?;
    }
    let fx = Fixture::new()?;
    fx.file(
        "/etc/group",
        format!("{GROUP}big:x:4294967296:\n").as_bytes(),
        0o644,
    )?;
    expect_lines(&fx, "ten digit gid", &["malformed line 5 in /etc/group"])?;
    Ok(())
}

#[test]
fn scan_reads_an_unterminated_final_line_of_each_account_file() -> TestResult {
    let rows: [(&str, &str, &str, Vec<&str>); 6] = [
        (
            "/etc/passwd",
            PASSWD,
            "toor:x:0:0::/:/bin/sh",
            vec!["uid 0 account other than root in /etc/passwd: toor"],
        ),
        (
            "/etc/passwd",
            &PASSWD.replace(SANDBOX_PASSWD, ""),
            "sandbox:x:1001:1001::/sandbox:/bin/bash",
            vec![WRONG_PASSWD],
        ),
        (
            "/etc/passwd",
            PASSWD,
            "toor:x:00:0::/:/bin/sh",
            vec!["malformed line 5 in /etc/passwd"],
        ),
        ("/etc/group", GROUP, "sandbox:x:1000:", vec![MISSING_GROUP]),
        (
            "/etc/group",
            GROUP,
            "sandbox:x:0:",
            vec![MISSING_GROUP, WRONG_GROUP],
        ),
        (
            "/etc/group",
            &GROUP.replace(SANDBOX_GROUP, ""),
            "sandbox:x:1001:",
            vec![WRONG_GROUP],
        ),
    ];
    for (path, head, last, wanted) in rows {
        let fx = Fixture::new()?;
        fx.file(path, format!("{head}{last}").as_bytes(), 0o644)?;
        expect_lines(&fx, &format!("{path} ends with {last}"), &wanted)?;
    }
    Ok(())
}

#[test]
fn scan_refuses_an_account_file_that_holds_a_nul_byte() -> TestResult {
    for (path, text) in [("/etc/passwd", PASSWD), ("/etc/group", GROUP)] {
        let name = path.trim_start_matches("/etc/");
        let mut bytes = text.as_bytes().to_vec();
        let at = text.find("sandbox").map_or(0, |index| index + 3);
        bytes.insert(at, 0);
        let fx = Fixture::new()?;
        fx.file(path, &bytes, 0o644)?;
        let wanted = format!("account file holds a NUL byte or cannot be read: /etc/{name}");
        expect_lines(&fx, &format!("{path} with a NUL"), &[&wanted])?;
    }
    Ok(())
}

#[test]
fn scan_refuses_account_files_that_are_not_real_root_owned_files() -> TestResult {
    let reason_dir = |reason: &str| {
        format!("account directory is not a real root-owned directory: /etc ({reason})")
    };
    let reason_file = |name: &str, reason: &str| {
        format!("account file is not a real root-owned file: /etc/{name} ({reason})")
    };
    for name in ["passwd", "group"] {
        let text = if name == "passwd" { PASSWD } else { GROUP };
        let path = format!("/etc/{name}");
        let fx = Fixture::new()?;
        fs::write(fx.top.join("sandbox/valid"), text)?;
        fs::remove_file(fx.image(&path))?;
        symlink(fx.top.join("sandbox/valid"), fx.image(&path))?;
        expect_lines(&fx, "absolute symlink", &[&reason_file(name, "symlink")])?;
        let fx = Fixture::new()?;
        fs::rename(fx.image(&path), fx.image(&format!("{path}-")))?;
        symlink(format!("{name}-"), fx.image(&path))?;
        expect_lines(&fx, "relative symlink", &[&reason_file(name, "symlink")])?;
        let fx = Fixture::new()?;
        fs::remove_file(fx.image(&path))?;
        expect_lines(&fx, "missing", &[&reason_file(name, "missing")])?;
        fx.dir(&path, 0o755)?;
        expect_lines(&fx, "directory", &[&reason_file(name, "wrong type")])?;
        for mode in [0o666, 0o664, 0o646] {
            let fx = Fixture::new()?;
            fs::set_permissions(fx.image(&path), fs::Permissions::from_mode(mode))?;
            let wanted = reason_file(name, "writable by group or others");
            expect_lines(&fx, &format!("mode {mode:o}"), &[&wanted])?;
        }
        let fx = Fixture::new()?;
        fx.file(&path, b"", 0o644)?;
        let missing = if name == "passwd" {
            MISSING_PASSWD
        } else {
            MISSING_GROUP
        };
        expect_lines(&fx, "empty", &[missing])?;
        if let Some(gid) = other_group(fx.gid)? {
            let fx = Fixture::new()?;
            std::os::unix::fs::chown(fx.image(&path), None, Some(gid))?;
            let wanted = reason_file(name, &format!("not owned by {}:{}", fx.uid, fx.gid));
            expect_lines(&fx, "other group", &[&wanted])?;
        }
    }
    let fx = Fixture::new()?;
    fs::create_dir(fx.top.join("sandbox/etc"))?;
    fs::write(fx.top.join("sandbox/etc/passwd"), PASSWD)?;
    fs::write(fx.top.join("sandbox/etc/group"), GROUP)?;
    fs::remove_dir_all(fx.image("/etc"))?;
    symlink(fx.top.join("sandbox/etc"), fx.image("/etc"))?;
    let protected_etc =
        |reason: &str| format!("protected path is not a real root-owned path: /etc ({reason})");
    let below_etc = [
        "protected path is not a real root-owned path: /etc/codex (missing)",
        "protected path is not a real root-owned path: /etc/codex/config.toml (missing)",
        "protected file differs from the trusted copy: /etc/codex/config.toml",
    ];
    let mut symlinked = vec![protected_etc("symlink")];
    symlinked.extend(below_etc.map(str::to_owned));
    symlinked.push(reason_dir("symlink"));
    let wanted: Vec<&str> = symlinked.iter().map(String::as_str).collect();
    expect_lines(&fx, "/etc absolute symlink", &wanted)?;
    let fx = Fixture::new()?;
    fs::set_permissions(fx.image("/etc"), fs::Permissions::from_mode(0o775))?;
    expect_lines(
        &fx,
        "/etc 775",
        &[
            &protected_etc("writable by group or others"),
            &reason_dir("writable by group or others"),
        ],
    )?;
    let fx = Fixture::new()?;
    fs::remove_dir_all(fx.image("/etc"))?;
    let mut missing = vec![protected_etc("missing")];
    missing.extend(below_etc.map(str::to_owned));
    missing.push(reason_dir("missing"));
    let wanted: Vec<&str> = missing.iter().map(String::as_str).collect();
    expect_lines(&fx, "/etc missing", &wanted)?;
    Ok(())
}

#[test]
fn scan_never_passes_when_one_of_its_tools_is_missing() -> TestResult {
    for tool in ["find", "tr", "cmp", "getcap"] {
        let fx = Fixture::new()?;
        fx.drop_tool(tool)?;
        let out = fx.scan()?;
        assert!(!out.ok, "{tool} missing: the scan passed");
        let fx = Fixture::new()?;
        fx.file(
            "/etc/passwd",
            PASSWD
                .replace(SANDBOX_PASSWD, "sandbox:x:0:0::/:/bin/sh\n")
                .as_bytes(),
            0o644,
        )?;
        fx.drop_tool(tool)?;
        let out = fx.scan()?;
        assert!(!out.ok, "{tool} missing and uid 0 sandbox: the scan passed");
    }
    Ok(())
}

const DEVICE_FIND: &str = "n=$#\nwhile [ \"$n\" -gt 0 ]; do\n  a=$1; shift; n=$((n - 1))\n  if [ \"$a\" = -type ] && [ \"$n\" -gt 0 ]; then\n    t=$1; shift; n=$((n - 1))\n    case $t in\n      b) set -- \"$@\" -name '*.blk' ;;\n      c) set -- \"$@\" -name '*.chr' ;;\n      *) set -- \"$@\" -type \"$t\" ;;\n    esac\n  else\n    set -- \"$@\" \"$a\"\n  fi\ndone\nexec /usr/bin/find \"$@\"\n";

#[test]
fn scan_refuses_block_and_character_device_nodes_anywhere_in_the_tree() -> TestResult {
    let fx = Fixture::new()?;
    fx.drop_tool("find")?;
    fx.stub("find", DEVICE_FIND)?;
    let out = fx.scan()?;
    assert!(out.ok, "a clean tree must pass: {}", out.stderr);
    fx.file("/sandbox/rawdisk", b"x", 0o644)?;
    symlink("/dev/null", fx.image("/sandbox/rawmem"))?;
    fx.dir("/sandbox/dev", 0o755)?;
    let out = fx.scan()?;
    assert!(out.ok, "files, links and directories pass: {}", out.stderr);
    let mut fx = Fixture::new()?;
    fx.drop_tool("find")?;
    fx.stub("find", DEVICE_FIND)?;
    fx.file("/sandbox/rawdisk.blk", b"x", 0o644)?;
    fx.file("/sandbox/rawmem.chr", b"x", 0o644)?;
    fx.file("/usr/lib/x y/tty.chr", b"x", 0o644)?;
    fx.file("/usr/lib/t\tab.blk", b"x", 0o644)?;
    let out = fx.scan()?;
    assert!(!out.ok, "device nodes must fail the scan");
    let mut found = out.lines();
    found.sort_unstable();
    assert_eq!(
        found,
        [
            "device node in role image: /sandbox/rawdisk.blk",
            "device node in role image: /sandbox/rawmem.chr",
            "device node in role image: /usr/lib/t?ab.blk",
            "device node in role image: /usr/lib/x y/tty.chr",
            "path with a control character in role image: /usr/lib/t?ab.blk",
        ]
    );
    fx = Fixture::new()?;
    fx.drop_tool("find")?;
    fx.stub(
        "find",
        "n=0\n[ ! -f \"$0.count\" ] || read n < \"$0.count\"\necho $((n + 1)) > \"$0.count\"\nif [ \"$n\" -eq 2 ]; then exit 1; fi\nexec /usr/bin/find \"$@\"\n",
    )?;
    let out = fx.scan()?;
    assert!(!out.ok, "a failing device find must fail the scan");
    assert!(
        out.stderr
            .contains("find failed while looking for device nodes"),
        "{}",
        out.stderr
    );
    Ok(())
}

fn sorted_lines(out: &Outcome) -> Vec<&str> {
    let mut lines = out.lines();
    lines.sort_unstable();
    lines
}

fn forbidden(path: &str) -> String {
    format!("forbidden tool in role image: {path}")
}

fn differs(path: &str) -> String {
    format!("protected file differs from the trusted copy: {path}")
}

#[test]
fn scan_exempts_only_the_exact_client_paths_that_hold_the_trusted_bytes() -> TestResult {
    let fx = Fixture::new()?;
    fx.client()?;
    let clean = fx.scan_client()?;
    assert!(clean.ok, "{}", clean.stderr);
    assert_eq!(clean.stdout, "jackin-image-scan 1\n");
    assert_eq!(clean.stderr, "");

    let fx = Fixture::new()?;
    fx.client()?;
    fx.file("/opt/x/docker", &fs::read(fx.trusted(DOCKER))?, 0o755)?;
    let out = fx.scan_client()?;
    assert!(!out.ok);
    assert_eq!(out.lines(), [forbidden("/opt/x/docker")]);

    for leaf in CLIENT {
        let fx = Fixture::new()?;
        fx.client()?;
        fx.file(leaf, b"role bytes", 0o755)?;
        let out = fx.scan_client()?;
        assert!(!out.ok, "{leaf}");
        let mut wanted = vec![differs(leaf)];
        if leaf == DOCKER {
            wanted.push(forbidden(leaf));
        }
        wanted.sort();
        assert_eq!(sorted_lines(&out), wanted, "{leaf}");
    }

    let fx = Fixture::new()?;
    fx.client()?;
    fx.relink(DOCKER, true)?;
    let out = fx.scan_client()?;
    assert!(!out.ok);
    let mut wanted = vec![
        forbidden(DOCKER),
        differs(DOCKER),
        format!("protected path is not a real root-owned path: {DOCKER} (symlink)"),
    ];
    wanted.sort();
    assert_eq!(sorted_lines(&out), wanted);
    Ok(())
}

#[test]
fn scan_refuses_every_other_tool_and_a_missing_pair_in_the_client_form() -> TestResult {
    let fx = Fixture::new()?;
    fx.client()?;
    let mut args = fx.client_arguments();
    args.insert(2, "--allow-tool".to_owned());
    args.insert(3, "/usr/local/bin/other".to_owned());
    let out = fx.run(&args)?;
    assert_eq!(out.code, Some(64), "{}", out.stderr);

    let fx = Fixture::new()?;
    fx.client()?;
    fx.file(DOCKER, b"x", 0o755)?;
    let absent = fx.scan()?;
    assert!(!absent.ok);
    let mut wanted: Vec<String> = CLIENT.iter().map(|leaf| forbidden(leaf)).collect();
    wanted.sort();
    assert_eq!(sorted_lines(&absent), wanted);

    for tool in [
        "dockerd",
        "containerd",
        "containerd-shim",
        "containerd-shim-runc-v2",
        "runc",
        "crun",
        "buildkitd",
        "rootlesskit",
        "podman",
        "nerdctl",
        "docker-proxy",
        "dockerd-rootless.sh",
    ] {
        let fx = Fixture::new()?;
        fx.client()?;
        let path = format!("/usr/local/bin/{tool}");
        fx.file(&path, &fs::read(fx.trusted(DOCKER))?, 0o755)?;
        let out = fx.scan_client()?;
        assert!(!out.ok, "{tool}");
        assert_eq!(out.lines(), [forbidden(&path)], "{tool}");
    }
    Ok(())
}

const CONTROL: &str = "path with a control character in role image: ";

fn hidden_below_a_newline_directory(fx: &Fixture) -> TestResult {
    let nested = format!(
        "/usr/local/bin/docker\n{}/usr/local/bin/docker",
        fx.top.join("image").display()
    );
    fx.file(&nested, b"role bytes", 0o755)
}

#[test]
fn scan_refuses_a_path_with_a_control_character_instead_of_splitting_it_into_lines() -> TestResult {
    let fx = Fixture::new()?;
    fx.client()?;
    let clean = fx.scan_client()?;
    assert!(clean.ok, "{}", clean.stderr);
    hidden_below_a_newline_directory(&fx)?;
    let out = fx.scan_client()?;
    assert!(!out.ok, "a newline in a directory name hid a docker binary");
    assert_eq!(out.lines(), [format!("{CONTROL}/usr/local/bin/docker?")]);

    let fx = Fixture::new()?;
    hidden_below_a_newline_directory(&fx)?;
    let out = fx.scan()?;
    assert!(!out.ok);
    assert!(out.stderr.contains(CONTROL), "{}", out.stderr);

    for (name, shown) in [("x\ty", "x?y"), ("\u{1b}[2J", "?[2J"), ("a\rb", "a?b")] {
        let fx = Fixture::new()?;
        fx.file(&format!("/opt/{name}"), b"x", 0o644)?;
        let out = fx.scan()?;
        assert!(!out.ok, "{name:?}");
        assert_eq!(out.lines(), [format!("{CONTROL}/opt/{shown}")], "{name:?}");
    }
    Ok(())
}

#[test]
fn scan_accepts_non_ascii_names_that_hold_no_control_character() -> TestResult {
    let fx = Fixture::new()?;
    fx.client()?;
    fx.file("/usr/share/caf\u{e9}/\u{65e5}\u{672c}.txt", b"x", 0o644)?;
    fx.file("/opt/with space/and'quote", b"x", 0o644)?;
    let out = fx.scan_client()?;
    assert!(out.ok, "{}", out.stderr);
    assert_eq!(out.stdout, "jackin-image-scan 1\n");
    let fx = Fixture::new()?;
    fx.file("/usr/share/caf\u{e9}/\u{65e5}\u{672c}.txt", b"x", 0o644)?;
    let out = fx.scan()?;
    assert!(out.ok, "{}", out.stderr);
    Ok(())
}

#[test]
fn scan_leaves_the_client_names_alone_when_the_capability_is_on() -> TestResult {
    let fx = Fixture::new()?;
    fx.client()?;
    for name in CLIENT_NAMES {
        fx.file(&format!("/opt/x/{name}"), b"role bytes", 0o755)?;
        symlink("/bin/true", fx.image(&format!("/opt/y-{name}")))?;
    }
    let out = fx.scan_client()?;
    assert!(out.ok, "{}", out.stderr);
    Ok(())
}

#[test]
fn scan_checks_the_data_file_like_a_protected_path_without_demanding_the_executable_bit()
-> TestResult {
    let fx = Fixture::new()?;
    assert!(fx.scan()?.ok, "a 0644 root-owned config must pass");
    let wanted = |path: &str| format!("protected file differs from the trusted copy: {path}");
    let refused = |path: &str, reason: &str| {
        format!("protected path is not a real root-owned path: {path} ({reason})")
    };

    let fx = Fixture::new()?;
    fx.file(CONFIG, b"check_for_update_on_startup = true\n", 0o644)?;
    let out = fx.scan()?;
    assert!(!out.ok && out.lines() == [wanted(CONFIG)], "{}", out.stderr);

    let fx = Fixture::new()?;
    fs::remove_file(fx.image(CONFIG))?;
    let out = fx.scan()?;
    let lines = out.lines();
    assert!(
        !out.ok && lines.contains(&refused(CONFIG, "missing").as_str()),
        "{}",
        out.stderr
    );

    let fx = Fixture::new()?;
    fx.relink(CONFIG, false)?;
    let out = fx.scan()?;
    let lines = out.lines();
    assert!(
        !out.ok && lines.contains(&refused(CONFIG, "symlink").as_str()),
        "{}",
        out.stderr
    );

    let fx = Fixture::new()?;
    fs::set_permissions(fx.image(CONFIG), fs::Permissions::from_mode(0o666))?;
    let out = fx.scan()?;
    let lines = out.lines();
    assert!(
        !out.ok && lines.contains(&refused(CONFIG, "writable by group or others").as_str()),
        "{}",
        out.stderr
    );

    let fx = Fixture::new()?;
    fx.relink("/etc/codex", true)?;
    let out = fx.scan()?;
    let lines = out.lines();
    assert!(
        !out.ok && lines.contains(&refused("/etc/codex", "symlink").as_str()),
        "{}",
        out.stderr
    );

    let fx = Fixture::new()?;
    fs::remove_file(fx.trusted(CONFIG))?;
    let out = fx.scan()?;
    assert!(
        !out.ok && out.stderr.contains(&wanted(CONFIG)),
        "{}",
        out.stderr
    );
    Ok(())
}

#[test]
fn scan_refuses_a_role_image_that_already_holds_the_verdict_path() -> TestResult {
    let present = |path: &str| format!("path must be absent from the role image: {path}");
    let fx = Fixture::new()?;
    assert!(fx.scan()?.ok, "an absent verdict path must pass");

    let fx = Fixture::new()?;
    fx.file(VERDICT, b"forged\n", 0o644)?;
    let out = fx.scan()?;
    assert!(
        !out.ok && out.lines() == [present(VERDICT)],
        "{}",
        out.stderr
    );

    let fx = Fixture::new()?;
    symlink("/etc/passwd", fx.image(VERDICT))?;
    let out = fx.scan()?;
    assert!(
        !out.ok && out.lines() == [present(VERDICT)],
        "{}",
        out.stderr
    );

    let fx = Fixture::new()?;
    symlink("/nonexistent/target", fx.image(VERDICT))?;
    let out = fx.scan()?;
    assert!(
        !out.ok && out.lines() == [present(VERDICT)],
        "a dangling link counts: {}",
        out.stderr
    );

    let fx = Fixture::new()?;
    fx.dir(VERDICT, 0o755)?;
    let out = fx.scan()?;
    assert!(
        !out.ok && out.lines() == [present(VERDICT)],
        "{}",
        out.stderr
    );
    Ok(())
}

#[test]
fn scan_refuses_every_codex_system_layer_path_the_role_image_holds() -> TestResult {
    let present = |path: &str| format!("path must be absent from the role image: {path}");
    assert!(
        Fixture::new()?.scan()?.ok,
        "a clean image with only the trusted config must pass"
    );
    for (path, directory) in CODEX_SYSTEM_LAYER {
        let fx = Fixture::new()?;
        if directory {
            fx.dir(path, 0o755)?;
        } else {
            fx.file(path, b"sandbox = \"danger-full-access\"\n", 0o644)?;
        }
        let out = fx.scan()?;
        assert!(
            !out.ok && out.lines() == [present(path)],
            "{path}: {}",
            out.stderr
        );
    }
    Ok(())
}

#[test]
fn scan_refuses_a_read_write_path_that_is_or_crosses_a_symlink() -> TestResult {
    let crossing = |path: &str, at: &str| {
        format!("read_write path of the role policy is or crosses a symlink: {path} (at {at})")
    };
    let fx = Fixture::new()?;
    fx.dir("/sandbox/work", 0o755)?;
    fx.dir("/tmp", 0o1777)?;
    let out = fx.scan_read_write(&["/sandbox/work", "/tmp", "/sandbox/not-created-yet"])?;
    assert!(
        out.ok,
        "real directories and a missing path pass: {}",
        out.stderr
    );

    let fx = Fixture::new()?;
    fx.dir("/usr/local/bin", 0o755)?;
    fx.dir("/sandbox", 0o755)?;
    symlink("/usr/local/bin", fx.image("/sandbox/tools"))?;
    let out = fx.scan_read_write(&["/sandbox/tools"])?;
    assert!(
        !out.ok && out.lines() == [crossing("/sandbox/tools", "/sandbox/tools")],
        "{}",
        out.stderr
    );

    let fx = Fixture::new()?;
    fx.dir("/usr/share/data", 0o755)?;
    symlink("/usr/share", fx.image("/srv"))?;
    let out = fx.scan_read_write(&["/srv/data"])?;
    assert!(
        !out.ok && out.lines() == [crossing("/srv/data", "/srv")],
        "{}",
        out.stderr
    );

    let fx = Fixture::new()?;
    symlink("/nonexistent", fx.image("/dangling"))?;
    let out = fx.scan_read_write(&["/dangling"])?;
    assert!(
        !out.ok && out.lines() == [crossing("/dangling", "/dangling")],
        "a dangling link counts: {}",
        out.stderr
    );
    Ok(())
}
