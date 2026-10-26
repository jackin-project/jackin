use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, PartialEq, Eq)]
enum MatchKind {
    Exact(Vec<String>),
    Prefix(Vec<String>),
}

#[derive(Debug, Clone)]
struct Entry {
    kind: MatchKind,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    exit_code: i32,
}

#[derive(Debug, Clone, Default)]
pub struct ProcessScript {
    entries: Vec<Entry>,
    default_stdout: Vec<u8>,
    default_stderr: Vec<u8>,
    default_exit_code: i32,
}

impl ProcessScript {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn on_exact<const N: usize>(
        mut self,
        args: [&str; N],
        stdout: &str,
        stderr: &str,
        exit_code: i32,
    ) -> Self {
        self.entries.push(Entry {
            kind: MatchKind::Exact(args.iter().map(ToString::to_string).collect()),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
            exit_code,
        });
        self
    }

    #[must_use]
    pub fn on_prefix<const N: usize>(
        mut self,
        args: [&str; N],
        stdout: &str,
        stderr: &str,
        exit_code: i32,
    ) -> Self {
        self.entries.push(Entry {
            kind: MatchKind::Prefix(args.iter().map(ToString::to_string).collect()),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
            exit_code,
        });
        self
    }

    #[must_use]
    pub fn with_default(mut self, stdout: &str, stderr: &str, exit_code: i32) -> Self {
        self.default_stdout = stdout.as_bytes().to_vec();
        self.default_stderr = stderr.as_bytes().to_vec();
        self.default_exit_code = exit_code;
        self
    }

    #[must_use]
    pub fn version_stub(name: &str, version: &str) -> Self {
        let out = format!("{name} {version}\n");
        Self::new()
            .on_exact(["--version"], &out, "", 0)
            .on_exact(["version"], &out, "", 0)
            .on_exact(["-V"], &out, "", 0)
    }

    #[must_use]
    pub fn help_stub(usage: &str) -> Self {
        Self::new()
            .on_exact(["--help"], usage, "", 0)
            .on_exact(["help"], usage, "", 0)
            .on_exact(["-h"], usage, "", 0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
}

impl Invocation {
    #[must_use]
    pub fn args(&self) -> &[String] {
        self.argv.get(1..).unwrap_or_default()
    }

    #[must_use]
    pub fn env_get(&self, key: &str) -> Option<&str> {
        self.env
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }

    pub fn assert_no_secrets(&self, forbidden: &[&str]) {
        for (index, secret) in forbidden.iter().enumerate() {
            if secret.is_empty() {
                continue;
            }
            for arg in &self.argv {
                assert!(
                    !arg.contains(secret),
                    "spawned argv leaked forbidden value #{index} (redacted)"
                );
            }
            for (name, value) in &self.env {
                assert!(
                    !value.contains(secret),
                    "spawned env leaked forbidden value #{index} in {name} (redacted)"
                );
            }
        }
    }
}

#[derive(Debug)]
pub struct FakeProcessHarness {
    dir: PathBuf,
}

impl FakeProcessHarness {
    pub fn new() -> std::io::Result<Self> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("jackin-fakeproc-{}-{unique}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn binary(&self, name: &str, script: &ProcessScript) -> std::io::Result<FakeBinary> {
        assert!(
            !name.is_empty() && !name.contains('/') && !name.contains('\\'),
            "fake binary name must be a plain file name: {name}"
        );
        let path = self.dir.join(format!("{name}.sh"));
        let log = self.dir.join(format!("{name}.log"));
        drop(std::fs::remove_file(&log));
        let source = render_script(&log, script);
        std::fs::write(&path, source)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&path, perms)?;
        }
        Ok(FakeBinary { path, log })
    }
}

impl Drop for FakeProcessHarness {
    fn drop(&mut self) {
        drop(std::fs::remove_dir_all(&self.dir));
    }
}

#[derive(Debug)]
pub struct FakeBinary {
    path: PathBuf,
    log: PathBuf,
}

impl FakeBinary {
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn command(&self) -> Command {
        if cfg!(unix) {
            Command::new(&self.path)
        } else {
            let mut cmd = Command::new("sh");
            cmd.arg(&self.path);
            cmd
        }
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "test-only log read on the calling test thread; never a render/runtime path"
    )]
    pub fn invocations(&self) -> std::io::Result<Vec<Invocation>> {
        let file = match std::fs::File::open(&self.log) {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(err),
        };
        parse_log(BufReader::new(file))
    }
}

fn shell_quote(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('\'');
    for chunk in text.split('\'') {
        if out.len() > 1 {
            out.push_str("'\\''");
        }
        out.push_str(chunk);
    }
    out.push('\'');
    out
}

fn emit(bytes: &[u8], stream: &str) -> String {
    let body = String::from_utf8_lossy(bytes).into_owned();
    let redirect = if stream == "stdout" { "" } else { " >&2" };
    format!("printf '%s' {}{redirect}", shell_quote(&body))
}

fn condition(kind: &MatchKind) -> String {
    let (args, op) = match kind {
        MatchKind::Exact(args) => (args, "-eq"),
        MatchKind::Prefix(args) => (args, "-ge"),
    };
    let mut out = format!("[ \"$#\" {op} {} ]", args.len());
    for (index, arg) in args.iter().enumerate() {
        out.push_str(&format!(
            " && [ \"${{{}}}\" = {} ]",
            index + 1,
            shell_quote(arg)
        ));
    }
    out
}

fn render_script(log: &Path, script: &ProcessScript) -> String {
    let mut out = String::from("#!/bin/sh\n");
    out.push_str(&format!(
        "_JACKIN_LOG={}\n",
        shell_quote(&log.to_string_lossy())
    ));
    out.push_str(
        "{\necho \"A:$0\"\nfor _a in \"$@\"; do echo \"A:$_a\"; done\n\
         env | sed 's/^/E:/'\necho '---'\n} >> \"$_JACKIN_LOG\"\n",
    );
    for (index, entry) in script.entries.iter().enumerate() {
        let keyword = if index == 0 { "if" } else { "elif" };
        out.push_str(&format!(
            "{keyword} {}; then {}; {}; exit {};\n",
            condition(&entry.kind),
            emit(&entry.stdout, "stdout"),
            emit(&entry.stderr, "stderr"),
            entry.exit_code
        ));
    }
    if script.entries.is_empty() {
        out.push_str(&format!(
            "{}; {}; exit {};\n",
            emit(&script.default_stdout, "stdout"),
            emit(&script.default_stderr, "stderr"),
            script.default_exit_code
        ));
    } else {
        out.push_str(&format!(
            "else {}; {}; exit {};\nfi\n",
            emit(&script.default_stdout, "stdout"),
            emit(&script.default_stderr, "stderr"),
            script.default_exit_code
        ));
    }
    out
}

fn parse_log(reader: BufReader<std::fs::File>) -> std::io::Result<Vec<Invocation>> {
    let mut invocations = Vec::new();
    let mut argv = Vec::new();
    let mut env = Vec::new();
    for line in reader.lines() {
        let line = line?;
        if line == "---" {
            invocations.push(Invocation {
                argv: std::mem::take(&mut argv),
                env: std::mem::take(&mut env),
            });
        } else if let Some(arg) = line.strip_prefix("A:") {
            argv.push(arg.to_owned());
        } else if let Some((key, value)) = line
            .strip_prefix("E:")
            .and_then(|pair| pair.split_once('='))
        {
            env.push((key.to_owned(), value.to_owned()));
        }
    }
    Ok(invocations)
}
