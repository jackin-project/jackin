//! jackin❯ desktop (native macOS usage menu bar) assembly and verification.
//!
//! Canonical local/CI path — Rust owns orchestration; mise tasks thin-wrap
//! these subcommands. No shell scripts.
//!
//! ```sh
//! cargo xtask desktop build --version 0.6.0 --build 1
//! cargo xtask desktop verify native/dist/JackinDesktop.app
//! # or: mise run desktop-build -- 0.6.0 1
//! ```

mod bootstrap;
mod release_state;
mod sign_notarize;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};

use crate::cmd;
use crate::docs;

const APP_EXECUTABLE: &str = "JackinDesktop";
const BUNDLE_ID: &str = "com.jackin-project.desktop";
const BUNDLE_NAME: &str = "jackin❯ desktop";
const MIN_OS: &str = "26.0";
/// XC framework artifact name; the boltffi FFI module is always `{name}FFI`.
const FRAMEWORK_NAME: &str = "JackinUsage";
const MODULE_NAME: &str = "JackinUsageFFI";
const STATIC_LIB: &str = "libjackin_usage_ffi.a";
const HOST_TARGET: &str = "aarch64-apple-darwin";
const ARCH: &str = "arm64";
/// Crate holding `boltffi.toml`; boltffi resolves the crate from its cwd.
const FFI_CRATE_DIR: &str = "crates/jackin-usage-ffi";
/// Symbol-rich release lane for the desktop static library (see workspace
/// `[profile.desktop-release]`); release CI archives its unstripped bytes.
const DESKTOP_PROFILE: &str = "desktop-release";

pub(super) fn progress(msg: impl AsRef<str>) {
    #[expect(
        clippy::print_stderr,
        reason = "jackin-xtask desktop CLI progress is user-facing"
    )]
    {
        eprintln!("{}", msg.as_ref());
    }
}

#[derive(Subcommand)]
pub(crate) enum DesktopCommand {
    /// Generate boltffi Swift bindings into `native/Sources/JackinUsageBindings`.
    Bindings(BindingsArgs),
    /// Nonmutating drift gate: regenerate into staging and byte-compare.
    BindingsCheck(BindingsArgs),
    /// Build the static arm64 `XCFramework` for `jackin-usage-ffi`.
    Xcframework,
    /// Assemble arm64 static `JackinDesktop.app` under `native/dist/`.
    Build(BuildArgs),
    /// Fail-closed validation for a `JackinDesktop.app` (and optional ZIP).
    Verify(VerifyArgs),
    /// Launch a built `JackinDesktop.app` (menu-bar / `LSUIElement` — no Dock icon).
    Run(RunArgs),
    /// Run host + pure Swift parity harnesses (OpenUsage/CodexBar limits-only matrix).
    Test,
    /// Counted `SwiftPM` unit tests: parse the xUnit report, reject zero/corrupt results.
    TestSwift,
    /// Developer ID sign + notarize + staple + final release ZIP.
    SignNotarize(sign_notarize::SignNotarizeArgs),
    /// Independent publication state (`KEY=value` lines for `GITHUB_OUTPUT`).
    ReleaseState(release_state::ReleaseStateArgs),
    /// Bootstrap GitHub env `release-macos` Apple secrets (never prints values).
    BootstrapSecrets(Box<bootstrap::BootstrapSecretsArgs>),
}

#[derive(Args)]
pub(crate) struct BindingsArgs {
    /// Cargo profile passed to boltffi's cargo invocation.
    #[arg(long, default_value = "desktop-release")]
    profile: String,
}

#[derive(Args)]
pub(crate) struct BuildArgs {
    /// `CFBundleShortVersionString` (or env `JACKIN_APP_VERSION`).
    #[arg(long)]
    version: Option<String>,
    /// `CFBundleVersion` numeric build (or env `JACKIN_APP_BUILD`).
    #[arg(long)]
    build: Option<String>,
}

#[derive(Args)]
pub(crate) struct VerifyArgs {
    /// Path to `JackinDesktop.app` (default `native/dist/JackinDesktop.app`).
    #[arg(default_value = "native/dist/JackinDesktop.app")]
    app: PathBuf,
    /// Optional ZIP for archive round-trip verification.
    zip: Option<PathBuf>,
    /// Require Developer ID + notarization (Gatekeeper + stapler).
    #[arg(long)]
    release: bool,
    /// Expected short version (or env `JACKIN_APP_VERSION`).
    #[arg(long)]
    version: Option<String>,
    /// Expected build number (or env `JACKIN_APP_BUILD`).
    #[arg(long)]
    build: Option<String>,
}

#[derive(Args)]
pub(crate) struct RunArgs {
    /// Path to `JackinDesktop.app` (default `native/dist/JackinDesktop.app`).
    #[arg(default_value = "native/dist/JackinDesktop.app")]
    app: PathBuf,
    /// Fail-closed verify the bundle before launching.
    #[arg(long)]
    verify: bool,
}

pub(crate) fn run(command: DesktopCommand) -> Result<()> {
    match command {
        DesktopCommand::Bindings(args) => generate_bindings(&docs::repo_root()?, &args.profile),
        DesktopCommand::BindingsCheck(args) => bindings_check(&docs::repo_root()?, &args.profile),
        DesktopCommand::Xcframework => build_xcframework(&docs::repo_root()?),
        DesktopCommand::Build(args) => {
            let (version, build) = resolve_version_build(args.version, args.build)?;
            build_app(&docs::repo_root()?, &version, &build)
        }
        DesktopCommand::Test => run_desktop_tests(&docs::repo_root()?),
        DesktopCommand::TestSwift => run_swift_unit_tests(&docs::repo_root()?),
        DesktopCommand::Verify(args) => {
            let release = args.release || env_truthy("RELEASE_MODE");
            let app = resolve_app_path(&args.app)?;
            let (version, build) =
                resolve_version_build_for_verify(&app, args.version, args.build)?;
            verify_app(&app, args.zip.as_deref(), &version, &build, release)
        }
        DesktopCommand::Run(args) => run_app(&args),
        DesktopCommand::SignNotarize(args) => sign_notarize::run(args),
        DesktopCommand::ReleaseState(args) => release_state::run(args),
        DesktopCommand::BootstrapSecrets(args) => bootstrap::run(*args),
    }
}

/// Resolve a relative app path against the repo root and return an absolute path.
pub(super) fn resolve_app_path(app: &Path) -> Result<PathBuf> {
    let root = docs::repo_root()?;
    let path = if app.is_absolute() {
        app.to_path_buf()
    } else {
        root.join(app)
    };
    if !path.exists() {
        bail!(
            "app not found at {}\n  build first: mise run desktop-build\n  or:         cargo xtask desktop build --version 0.6.0 --build 1",
            path.display()
        );
    }
    Ok(fs::canonicalize(&path).unwrap_or(path))
}

/// Host unit tests + pure Swift harnesses (OpenUsage/CodexBar limits-only matrix).
///
/// Does not require full Xcode `XCTest` — uses CLT-safe `swift run` harnesses.
fn run_desktop_tests(root: &Path) -> Result<()> {
    require_macos("desktop test")?;
    progress("==> jackin-usage + jackin-usage-ffi nextest");
    let mut nextest = cmd::command("cargo");
    nextest.current_dir(root).args([
        "nextest",
        "run",
        "-p",
        "jackin-usage",
        "-p",
        "jackin-usage-ffi",
        "--lib",
    ]);
    cmd::run_streaming(&mut nextest)?;

    // Ensure XCFramework exists for SwiftPM binary target.
    let xcf = root.join(format!("target/xcframework/{FRAMEWORK_NAME}.xcframework"));
    if !xcf.is_dir() {
        progress("==> XCFramework missing — building");
        build_xcframework(root)?;
    }

    let native = root.join("native");
    for (name, product) in [
        ("StatusItemChipHarness", "StatusItemChipHarness"),
        ("DesktopArchitectureLint", "DesktopArchitectureLint"),
        ("DesktopParityMatrixHarness", "DesktopParityMatrixHarness"),
        ("DesktopSoTParityHarness", "DesktopSoTParityHarness"),
        ("ProviderMarksHarness", "ProviderMarksHarness"),
    ] {
        progress(format!("==> swift run -c release {name}"));
        let mut swift = cmd::command("swift");
        swift
            .current_dir(&native)
            .args(["run", "-c", "release", product]);
        cmd::run_streaming(&mut swift)?;
    }

    progress("");
    progress("┌─────────────────────────────────────────────────────────────");
    progress("│ jackin❯ desktop — tests OK");
    progress("│   host nextest + all five pure Swift harnesses");
    progress("│   (counted SwiftPM unit tests: cargo xtask desktop test-swift)");
    progress("└─────────────────────────────────────────────────────────────");
    Ok(())
}

/// `SwiftPM` unit tests with a count proof. `SwiftPM` writes xUnit only for
/// Swift Testing tests (`<name>-swift-testing.xml`); `XCTest` totals come from
/// the runner's `All tests` summary line in the captured log. Both halves
/// must be present and nonzero — a mistyped selector, crashed runner, or
/// missing report can never look green.
fn run_swift_unit_tests(root: &Path) -> Result<()> {
    require_macos("desktop test-swift")?;
    let native = root.join("native");
    let log = native.join(".build/swift-unit-tests.log");
    let xunit_base = native.join(".build/swift-unit-tests.xml");
    let swift_testing_report = native.join(".build/swift-unit-tests-swift-testing.xml");
    for stale in [&log, &xunit_base, &swift_testing_report] {
        if stale.exists() {
            fs::remove_file(stale)
                .with_context(|| format!("removing stale report {}", stale.display()))?;
        }
    }
    progress("==> swift test -c release (counted)");
    let mut swift = cmd::command("swift");
    swift.current_dir(&native).args([
        "test",
        "-c",
        "release",
        "--xunit-output",
        xunit_base.to_str().context("xunit path utf-8")?,
    ]);
    let run = cmd::run_stdout_file(&mut swift, &log);
    if run.is_err() {
        let tail = fs::read_to_string(&log)
            .map(|text| text.lines().rev().take(20).collect::<Vec<_>>().join("\n"))
            .unwrap_or_default();
        progress(format!("swift test failed; log tail:\n{tail}"));
        run?;
    }

    let log_text = fs::read_to_string(&log)
        .with_context(|| format!("missing captured swift test log {}", log.display()))?;
    let xctest = parse_xctest_summary(&log_text)?;
    if xctest.tests == 0 {
        bail!("XCTest executed zero tests — refusing the false-green selection trap");
    }
    if xctest.failures > 0 {
        bail!(
            "XCTest reported {} failures across {} tests",
            xctest.failures,
            xctest.tests
        );
    }

    let swift_testing_text = fs::read_to_string(&swift_testing_report).with_context(|| {
        format!(
            "missing Swift Testing xUnit report {}; the package declares swift-testing tests, so its absence is corruption",
            swift_testing_report.display()
        )
    })?;
    let swift_testing = parse_xunit_totals(&swift_testing_text)?;
    if swift_testing.tests == 0 {
        bail!("Swift Testing executed zero tests — refusing the false-green selection trap");
    }
    if swift_testing.failures > 0 || swift_testing.errors > 0 {
        bail!(
            "Swift Testing reported {} failures and {} errors across {} tests",
            swift_testing.failures,
            swift_testing.errors,
            swift_testing.tests
        );
    }

    progress(format!(
        "==> swift unit tests OK: {} XCTest + {} Swift Testing executed, 0 failures",
        xctest.tests, swift_testing.tests
    ));
    Ok(())
}

/// Extract the final `XCTest` `All tests` summary from captured runner output.
/// Missing or malformed summary lines are corruption, never zero.
fn parse_xctest_summary(log: &str) -> Result<XunitTotals> {
    let mut totals: Option<XunitTotals> = None;
    let mut in_all_tests = false;
    for line in log.lines() {
        if line.contains("Test Suite 'All tests'") {
            in_all_tests = true;
            continue;
        }
        if in_all_tests && line.contains("Executed ") {
            let numbers: Vec<u64> = line
                .split(|c: char| !c.is_ascii_digit())
                .filter(|part| !part.is_empty())
                .filter_map(|part| part.parse().ok())
                .collect();
            // `Executed N tests, with M failures (K unexpected) in X (Y) seconds`
            let (Some(&tests), Some(&failures)) = (numbers.first(), numbers.get(1)) else {
                bail!("corrupt XCTest summary line: {line}");
            };
            totals = Some(XunitTotals {
                tests,
                failures,
                errors: 0,
            });
            in_all_tests = false;
        }
    }
    totals.context("corrupt swift test log: no 'All tests' XCTest summary found")
}

#[derive(Debug, PartialEq, Eq)]
struct XunitTotals {
    tests: u64,
    failures: u64,
    errors: u64,
}

/// Sum `tests`/`failures`/`errors` across every `<testsuite>` element.
/// Missing elements or attributes are corruption, never zero.
fn parse_xunit_totals(source: &str) -> Result<XunitTotals> {
    fn attr_u64(tag: &str, name: &str) -> Result<u64> {
        let needle = format!("{name}=\"");
        let start = tag
            .find(&needle)
            .with_context(|| format!("corrupt xUnit: testsuite missing {name} attribute"))?
            + needle.len();
        let rest = &tag[start..];
        let end = rest
            .find('"')
            .context("corrupt xUnit: unterminated attribute")?;
        rest[..end]
            .parse()
            .with_context(|| format!("corrupt xUnit: non-numeric {name} attribute"))
    }

    let mut totals = XunitTotals {
        tests: 0,
        failures: 0,
        errors: 0,
    };
    let mut suites = 0_u64;
    let mut rest = source;
    while let Some(index) = rest.find("<testsuite ") {
        rest = &rest[index + "<testsuite ".len()..];
        let end = rest
            .find('>')
            .context("corrupt xUnit: unterminated testsuite tag")?;
        let tag = &rest[..end];
        totals.tests += attr_u64(tag, "tests")?;
        totals.failures += attr_u64(tag, "failures")?;
        totals.errors += attr_u64(tag, "errors")?;
        suites += 1;
        rest = &rest[end..];
    }
    if suites == 0 {
        bail!("corrupt xUnit: no testsuite elements");
    }
    Ok(totals)
}

fn run_app(args: &RunArgs) -> Result<()> {
    require_macos("desktop run")?;
    let app = resolve_app_path(&args.app)?;
    if args.verify {
        let (version, build) = resolve_version_build_for_verify(&app, None, None)?;
        verify_app(&app, None, &version, &build, false)?;
    }
    let bin = app.join(format!("Contents/MacOS/{APP_EXECUTABLE}"));
    if !bin.is_file() {
        bail!("missing executable {}", bin.display());
    }

    // WHY: reusing a stale agent process (open without -n) can leave a PID alive
    // with no MenuBarExtra after a bad first launch. Always restart cleanly.
    {
        let mut pkill = cmd::command("pkill");
        pkill.args(["-x", APP_EXECUTABLE]);
        drop(cmd::run(&mut pkill));
    }

    // Clear quarantine bits from local builds so LaunchServices will map UI.
    {
        let mut xattr = cmd::command("xattr");
        xattr.args(["-cr", app.to_str().context("app utf-8")?]);
        drop(cmd::run(&mut xattr));
    }

    progress("");
    progress("┌─────────────────────────────────────────────────────────────");
    progress("│ jackin❯ desktop — launching");
    progress(format!("│   app:  {}", app.display()));
    progress(format!("│   bin:  {}", bin.display()));
    progress("│   note: LSUIElement — no Dock icon; look at the menu bar");
    progress("│         (right side near Control Center / clock)");
    progress("│   look: per-provider chips (e.g. Cl 100%/79% remaining) or Cl 37%");
    progress("│   quit: osascript -e 'quit app \"Jackin Desktop\"'");
    progress("│         or: pkill -x JackinDesktop");
    progress("└─────────────────────────────────────────────────────────────");
    progress("");

    // -n forces a new instance after pkill; absolute path avoids PATH ambiguity.
    let mut open = cmd::command("open");
    open.args(["-n", app.to_str().context("app utf-8")?]);
    cmd::run(&mut open).with_context(|| format!("opening {}", app.display()))?;

    // Poll briefly for a live process (no thread::sleep — short bash wait).
    let mut seen = String::new();
    for _ in 0..20 {
        let mut pgrep = cmd::command("pgrep");
        pgrep.args(["-x", APP_EXECUTABLE]);
        if let Ok(out) = cmd::output_string(&mut pgrep) {
            let trimmed = out.trim();
            if !trimmed.is_empty() {
                seen = trimmed.to_owned();
                break;
            }
        }
        let mut nap = cmd::command("/bin/bash");
        nap.args(["-c", "read -t 0.05 || true"]);
        drop(cmd::run(&mut nap));
    }
    if seen.is_empty() {
        bail!(
            "JackinDesktop did not stay running after open. \
Try: open -n {}  and check Console.app for crash reports.",
            app.display()
        );
    }
    progress(format!("OK: process running (pid {seen})"));
    progress("If no menu-bar icon: System Settings → Control Center → Menu Bar Only");
    progress("  and ensure menu bar icons are not hidden (fullscreen / Stage Manager).");
    Ok(())
}

fn print_app_ready_banner(app: &Path, version: &str, build: &str) {
    let abs = fs::canonicalize(app).unwrap_or_else(|_| app.to_path_buf());
    let rel = PathBuf::from("native/dist/JackinDesktop.app");
    progress("");
    progress("┌─────────────────────────────────────────────────────────────");
    progress("│ jackin❯ desktop — build complete");
    progress(format!("│   version: {version}  (CFBundleVersion {build})"));
    progress(format!("│   app:     {}", abs.display()));
    progress(format!("│   rel:     {}", rel.display()));
    progress("│");
    progress("│   verify:  mise run desktop-verify");
    progress("│            cargo xtask desktop verify");
    progress("│   run:     mise run desktop-run");
    progress("│            cargo xtask desktop run");
    progress(format!("│   open:    open {}", abs.display()));
    progress("│");
    progress("│   (menu bar only — no Dock icon; LSUIElement)");
    progress("└─────────────────────────────────────────────────────────────");
    progress("");
    // Machine-readable line for scripts / CI grepping.
    progress(format!("DESKTOP_APP={}", abs.display()));
}

pub(super) fn resolve_version_build(
    version: Option<String>,
    build: Option<String>,
) -> Result<(String, String)> {
    let version = version
        .or_else(|| env::var("JACKIN_APP_VERSION").ok())
        .context("version required: pass --version or set JACKIN_APP_VERSION")?;
    let build = build
        .or_else(|| env::var("JACKIN_APP_BUILD").ok())
        .context("build required: pass --build or set JACKIN_APP_BUILD")?;
    validate_version(&version)?;
    validate_build(&build)?;
    Ok((version, build))
}

/// Prefer flags/env; otherwise read identity from the app plist so
/// `mise run desktop-verify` works without re-stating the version.
fn resolve_version_build_for_verify(
    app: &Path,
    version: Option<String>,
    build: Option<String>,
) -> Result<(String, String)> {
    let version = version
        .or_else(|| env::var("JACKIN_APP_VERSION").ok())
        .or_else(|| {
            let plist = app.join("Contents/Info.plist");
            plist_buddy_print(&plist, "CFBundleShortVersionString").ok()
        })
        .context(
            "version required: pass --version, set JACKIN_APP_VERSION, or point at a built app",
        )?;
    let build = build
        .or_else(|| env::var("JACKIN_APP_BUILD").ok())
        .or_else(|| {
            let plist = app.join("Contents/Info.plist");
            plist_buddy_print(&plist, "CFBundleVersion").ok()
        })
        .context("build required: pass --build, set JACKIN_APP_BUILD, or point at a built app")?;
    validate_version(&version)?;
    validate_build(&build)?;
    Ok((version, build))
}

fn validate_version(version: &str) -> Result<()> {
    let ok = !version.is_empty()
        && version
            .split('.')
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()));
    if ok {
        Ok(())
    } else {
        bail!("JACKIN_APP_VERSION must be numeric dotted (got {version})")
    }
}

fn validate_build(build: &str) -> Result<()> {
    if !build.is_empty() && build.chars().all(|c| c.is_ascii_digit()) {
        Ok(())
    } else {
        bail!("JACKIN_APP_BUILD must be numeric (got {build})")
    }
}

fn env_truthy(key: &str) -> bool {
    matches!(
        env::var(key).ok().as_deref(),
        Some("1" | "true" | "TRUE" | "yes" | "YES")
    )
}

pub(super) fn require_macos(action: &str) -> Result<()> {
    if cfg!(target_os = "macos") {
        Ok(())
    } else {
        bail!("{action} requires macOS (Apple Silicon)")
    }
}

fn generate_bindings(root: &Path, profile: &str) -> Result<()> {
    let sources = root.join("native/Sources/JackinUsageBindings");
    generate_bindings_into(root, profile, &sources, None)
}

fn bindings_check(root: &Path, profile: &str) -> Result<()> {
    let staging = root.join("native/.build/bindings-check");
    if staging.exists() {
        fs::remove_dir_all(&staging).with_context(|| format!("clearing {}", staging.display()))?;
    }
    let staging_sources = staging.join("Sources/JackinUsageBindings");
    // Redirect boltffi's Swift output into staging via an overlay config so the
    // committed tree is never touched by the drift gate.
    let overlay = staging.join("boltffi.overlay.toml");
    fs::create_dir_all(&staging)?;
    fs::write(
        &overlay,
        format!(
            "[targets.apple.swift]\noutput = \"{}\"\n",
            staging_sources.display()
        ),
    )?;
    generate_bindings_into(root, profile, &staging_sources, Some(&overlay))?;

    let differences = tree_differences(
        &root.join("native/Sources/JackinUsageBindings"),
        &staging_sources,
        "native/Sources/JackinUsageBindings",
    )?;
    if differences.is_empty() {
        progress("==> bindings-check: committed bindings match regeneration");
        return Ok(());
    }
    let mut report = String::from(
        "committed boltffi bindings are stale; run `mise run desktop-bindings` and commit:",
    );
    for difference in &differences {
        report.push_str("\n  ");
        report.push_str(difference);
    }
    bail!(report)
}

/// Byte-compare two directory trees; each entry names a missing, extra, or
/// changed committed-relative path. `label` prefixes entries for reporting.
fn tree_differences(expected: &Path, actual: &Path, label: &str) -> Result<Vec<String>> {
    fn collect(root: &Path) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            if !dir.is_dir() {
                continue;
            }
            for entry in crate::fs_util::read_dir_sorted(&dir)? {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    files.push(path.strip_prefix(root)?.to_path_buf());
                }
            }
        }
        files.sort();
        Ok(files)
    }

    let expected_files = collect(expected)?;
    let actual_files = collect(actual)?;
    let mut differences = Vec::new();
    for relative in &expected_files {
        if !actual_files.contains(relative) {
            differences.push(format!(
                "{label}/{}: missing after regeneration",
                relative.display()
            ));
        }
    }
    for relative in &actual_files {
        if !expected_files.contains(relative) {
            differences.push(format!("{label}/{}: not committed", relative.display()));
        }
    }
    for relative in expected_files.iter().filter(|r| actual_files.contains(r)) {
        let committed = fs::read(expected.join(relative))?;
        let regenerated = fs::read(actual.join(relative))?;
        if committed != regenerated {
            differences.push(format!("{label}/{}: content drift", relative.display()));
        }
    }
    Ok(differences)
}

fn generate_bindings_into(
    root: &Path,
    profile: &str,
    sources: &Path,
    overlay: Option<&Path>,
) -> Result<()> {
    require_macos("desktop bindings")?;
    let profile = profile.trim();
    if !["release", "debug", DESKTOP_PROFILE].contains(&profile) {
        bail!("profile must be release, debug, or {DESKTOP_PROFILE} (got {profile})");
    }

    let boltffi = which("boltffi").context(
        "boltffi not on PATH; install via mise (`mise install`) — see mise.toml cargo:boltffi_cli",
    )?;

    progress(format!(
        "==> generating Swift bindings into {}",
        sources.display()
    ));
    let mut generate = cmd::command(&boltffi);
    generate
        .current_dir(root.join(FFI_CRATE_DIR))
        .env("MACOSX_DEPLOYMENT_TARGET", MIN_OS)
        .arg("--cargo-arg=--profile")
        .arg(format!("--cargo-arg={profile}"));
    if let Some(overlay) = overlay {
        generate.arg("--overlay").arg(overlay);
    }
    generate.args(["generate", "swift"]);
    cmd::run_streaming(&mut generate)?;

    // `boltffi generate` also drops the C header beside the Swift; only the
    // xcframework consumes headers, so the committed tree stays pure Swift.
    let stray_header = sources.join("BoltFFI/boltffi.h");
    if stray_header.is_file() {
        fs::remove_file(&stray_header)?;
    }
    for generated in find_files_with_ext(sources, "swift")? {
        normalize_generated_file(&generated)?;
    }

    progress(format!(
        "==> generated bindings under {}",
        sources.display()
    ));
    Ok(())
}

fn normalize_generated_file(path: &Path) -> Result<()> {
    if !path.is_file() {
        return Ok(());
    }
    let source = fs::read_to_string(path)
        .with_context(|| format!("reading generated binding {}", path.display()))?;
    let normalized = normalize_generated_text(&source);
    if normalized != source {
        fs::write(path, normalized)
            .with_context(|| format!("normalizing generated binding {}", path.display()))?;
    }
    Ok(())
}

fn normalize_generated_text(source: &str) -> String {
    let mut lines = source.lines().map(str::trim_end).collect::<Vec<_>>();
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    }
}

fn build_xcframework(root: &Path) -> Result<()> {
    require_macos("desktop xcframework")?;

    progress(format!(
        "==> packing staticlib for {HOST_TARGET} (macOS {MIN_OS} floor)"
    ));
    let mut rustup = cmd::command("rustup");
    rustup.args(["target", "add", HOST_TARGET]);
    // Already-installed target is fine; surface other rustup failures below if cargo fails.
    drop(cmd::run(&mut rustup));

    let out_dir = root.join("target/xcframework");
    let xcframework = out_dir.join(format!("{FRAMEWORK_NAME}.xcframework"));
    // boltffi merges into an existing output directory; wipe for a clean slice set.
    if xcframework.exists() {
        fs::remove_dir_all(&xcframework)?;
    }
    let zip = out_dir.join(format!("{FRAMEWORK_NAME}.xcframework.zip"));
    if zip.exists() {
        fs::remove_file(&zip)?;
    }

    // boltffi drives cargo itself; the deployment-target floor propagates
    // through the inherited environment (slice advertises minos 26.0).
    let boltffi = which("boltffi").context(
        "boltffi not on PATH; install via mise (`mise install`) — see mise.toml cargo:boltffi_cli",
    )?;
    let mut pack = cmd::command(&boltffi);
    pack.current_dir(root.join(FFI_CRATE_DIR))
        .env("MACOSX_DEPLOYMENT_TARGET", MIN_OS)
        .args([
            "--cargo-arg=--profile".to_owned(),
            format!("--cargo-arg={DESKTOP_PROFILE}"),
        ])
        .args(["pack", "apple"]);
    cmd::run_streaming(&mut pack)?;

    // `boltffi pack` regenerates the Swift module beside the committed native
    // sources. Keep its whitespace normalization identical to the binding
    // command/check so a build cannot create drift that the next CI step sees.
    let generated_sources = root.join("native/Sources/JackinUsageBindings");
    for generated in find_files_with_ext(&generated_sources, "swift")? {
        normalize_generated_file(&generated)?;
    }

    if !xcframework.is_dir() {
        bail!("missing {}", xcframework.display());
    }

    let modulemap = xcframework.join(format!("macos-{ARCH}/Headers/module.modulemap"));
    let modulemap_text = fs::read_to_string(&modulemap)
        .with_context(|| format!("reading {}", modulemap.display()))?;
    if !modulemap_text.contains(&format!("module {MODULE_NAME} ")) {
        bail!("xcframework modulemap must declare `module {MODULE_NAME}`, got:\n{modulemap_text}");
    }

    let info_plist = xcframework.join("Info.plist");
    if which("plutil").is_ok() {
        let mut plutil = cmd::command("plutil");
        plutil.args(["-lint", info_plist.to_str().context("plist utf-8")?]);
        cmd::run(&mut plutil)?;
    }

    let libs = find_files_named(&xcframework, STATIC_LIB)?;
    if libs.len() != 1 {
        bail!(
            "expected exactly one arm64 static library inside XCFramework, found {}",
            libs.len()
        );
    }
    let archs = lipo_archs(&libs[0])?;
    progress(format!("  slice macos-{ARCH}: {archs}"));
    if !archs.split_whitespace().any(|a| a == ARCH) {
        bail!("xcframework library missing {ARCH} (got {archs})");
    }

    progress(format!("==> XCFramework ready: {}", xcframework.display()));
    Ok(())
}

fn build_app(root: &Path, version: &str, build: &str) -> Result<()> {
    require_macos("desktop build")?;

    let dist = root.join("native/dist/JackinDesktop.app");
    let xcframework = root.join(format!("target/xcframework/{FRAMEWORK_NAME}.xcframework"));

    progress("==> XCFramework (static arm64)");
    build_xcframework(root)?;
    if !xcframework.is_dir() {
        bail!("missing {}", xcframework.display());
    }

    let native = root.join("native");
    let manifest = native.join("project.yml");
    if !manifest.is_file() {
        bail!("missing XcodeGen manifest {}", manifest.display());
    }

    let xcodegen = which("xcodegen")
        .context("xcodegen not on PATH; install pinned tools via `mise install`")?;
    progress("==> xcodegen generate");
    let mut generate = cmd::command(&xcodegen);
    generate
        .current_dir(&native)
        .args(["generate", "--spec", "project.yml"]);
    cmd::run_streaming(&mut generate)?;

    let derived_data = native.join("DerivedData");
    progress(format!("==> xcodebuild Release ({ARCH}, macOS {MIN_OS})"));
    let mut xcodebuild = cmd::command("xcodebuild");
    xcodebuild.current_dir(&native).args([
        "-project",
        "JackinDesktop.xcodeproj",
        "-scheme",
        APP_EXECUTABLE,
        "-configuration",
        "Release",
        "-destination",
        "platform=macOS,arch=arm64",
        "-derivedDataPath",
        derived_data.to_str().context("derived data path utf-8")?,
        &format!("MARKETING_VERSION={version}"),
        &format!("CURRENT_PROJECT_VERSION={build}"),
        "build",
    ]);
    cmd::run_streaming(&mut xcodebuild)?;

    let built_app = derived_data.join("Build/Products/Release/JackinDesktop.app");
    if !built_app.is_dir() {
        bail!("missing Xcode product {}", built_app.display());
    }
    if dist.exists() {
        fs::remove_dir_all(&dist)?;
    }
    fs::create_dir_all(dist.parent().context("desktop dist parent")?)?;
    let mut ditto = cmd::command("ditto");
    ditto.args([
        built_app.to_str().context("built app path utf-8")?,
        dist.to_str().context("dist app path utf-8")?,
    ]);
    cmd::run(&mut ditto)?;

    let app_bin = dist.join(format!("Contents/MacOS/{APP_EXECUTABLE}"));
    if !app_bin.is_file() {
        bail!("missing Xcode app executable {}", app_bin.display());
    }

    let archs = lipo_archs(&app_bin)?;
    progress(format!("  executable archs: {archs}"));
    if !archs.split_whitespace().any(|a| a == ARCH) {
        bail!("final app missing arm64 (got {archs})");
    }
    if archs.split_whitespace().any(|a| a == "x86_64") {
        bail!("final app must be arm64-only (got {archs})");
    }

    assert_no_embedded_libs(&dist)?;
    assert_no_absolute_ffi_link(&app_bin)?;

    let built_dsym = derived_data.join("Build/Products/Release/JackinDesktop.app.dSYM");
    if !built_dsym.is_dir() {
        bail!("missing Xcode dSYM {}", built_dsym.display());
    }
    let dist_dsym = root.join("native/dist/JackinDesktop.app.dSYM");
    if dist_dsym.exists() {
        fs::remove_dir_all(&dist_dsym)?;
    }
    let mut ditto_dsym = cmd::command("ditto");
    ditto_dsym.args([
        built_dsym.to_str().context("built dSYM path utf-8")?,
        dist_dsym.to_str().context("dist dSYM path utf-8")?,
    ]);
    cmd::run(&mut ditto_dsym)?;
    let dwarf = dist_dsym.join(format!("Contents/Resources/DWARF/{APP_EXECUTABLE}"));
    let app_uuid = dwarf_uuid(&app_bin)?;
    let dsym_uuid = dwarf_uuid(&dwarf)?;
    if app_uuid != dsym_uuid {
        bail!("dSYM UUID {dsym_uuid} does not correspond to app UUID {app_uuid}");
    }
    progress(format!("==> dSYM archived beside app (UUID {app_uuid})"));

    progress("==> ad-hoc codesign (local/PR shape)");
    let mut codesign = cmd::command("codesign");
    codesign.args([
        "--force",
        "--sign",
        "-",
        "--timestamp=none",
        dist.to_str().context("dist utf-8")?,
    ]);
    cmd::run(&mut codesign)?;

    print_app_ready_banner(&dist, version, build);
    Ok(())
}

pub(super) fn verify_app(
    app: &Path,
    zip: Option<&Path>,
    version: &str,
    build: &str,
    release_mode: bool,
) -> Result<()> {
    require_macos("desktop verify")?;

    if !app.is_dir() {
        bail!("usage: cargo xtask desktop verify <JackinDesktop.app> [archive.zip]");
    }

    let bin = app.join(format!("Contents/MacOS/{APP_EXECUTABLE}"));
    let plist = app.join("Contents/Info.plist");
    let brand_assets = app.join("Contents/Resources/Brand");
    let provider_marks = app.join("Contents/Resources/ProviderMarks");

    if !bin.is_file() {
        bail!("missing executable {}", bin.display());
    }
    if !plist.is_file() {
        bail!("missing {}", plist.display());
    }
    for name in [
        "JackinMonogramDark.svg",
        "JackinMonogramLight.svg",
        "JackinWordmarkDark.svg",
        "JackinWordmarkLight.svg",
    ] {
        let asset = brand_assets.join(name);
        if !asset.is_file() {
            bail!("missing generated brand asset {}", asset.display());
        }
    }
    if !provider_marks.is_dir() {
        bail!("missing provider marks {}", provider_marks.display());
    }

    assert_plist_string(&plist, "CFBundleIdentifier", BUNDLE_ID)?;
    assert_plist_string(&plist, "CFBundleExecutable", APP_EXECUTABLE)?;
    assert_plist_string(&plist, "CFBundleName", BUNDLE_NAME)?;
    assert_plist_string(&plist, "CFBundleShortVersionString", version)?;
    assert_plist_string(&plist, "CFBundleVersion", build)?;
    assert_plist_string(&plist, "LSMinimumSystemVersion", MIN_OS)?;
    assert_plist_bool_true(&plist, "LSUIElement")?;

    let archs = lipo_archs(&bin)?;
    if !archs.split_whitespace().any(|a| a == ARCH) {
        bail!("missing arm64 (got {archs})");
    }
    if archs.split_whitespace().any(|a| a == "x86_64") {
        bail!("x86_64 not in scope (got {archs}); arm64-only expected");
    }

    check_vtool_minos(&bin)?;
    assert_no_embedded_libs(app)?;
    assert_no_absolute_ffi_link(&bin)?;

    let mut codesign = cmd::command("codesign");
    codesign.args([
        "--verify",
        "--deep",
        "--strict",
        app.to_str().context("app utf-8")?,
    ]);
    cmd::run(&mut codesign).context("codesign verify failed")?;

    if release_mode {
        let mut spctl = cmd::command("spctl");
        spctl.args([
            "--assess",
            "--type",
            "execute",
            app.to_str().context("app utf-8")?,
        ]);
        cmd::run(&mut spctl).context("spctl assess failed")?;
        let mut stapler = cmd::command("xcrun");
        stapler.args(["stapler", "validate", app.to_str().context("app utf-8")?]);
        cmd::run(&mut stapler).context("stapler validate failed")?;
    }

    if let Some(zip) = zip {
        if !zip.is_file() {
            bail!("zip not found: {}", zip.display());
        }
        let tmp = tempfile_dir("jackin-desktop-verify")?;
        let mut unzip = cmd::command("unzip");
        unzip.args([
            "-q",
            zip.to_str().context("zip utf-8")?,
            "-d",
            tmp.to_str().context("tmp utf-8")?,
        ]);
        cmd::run(&mut unzip)?;
        let nested = find_dirs_named(&tmp, "JackinDesktop.app")?;
        if nested.len() != 1 {
            bail!(
                "archive must contain exactly one JackinDesktop.app (found {})",
                nested.len()
            );
        }
        verify_app(&nested[0], None, version, build, release_mode)?;
        drop(fs::remove_dir_all(&tmp));
    }

    let abs = fs::canonicalize(app).unwrap_or_else(|_| app.to_path_buf());
    progress("");
    progress("┌─────────────────────────────────────────────────────────────");
    progress("│ jackin❯ desktop — verify OK");
    progress(format!("│   app:     {}", abs.display()));
    progress(format!("│   version: {version}  (CFBundleVersion {build})"));
    progress(format!(
        "│   mode:    {}",
        if release_mode {
            "release (Gatekeeper + stapler)"
        } else {
            "ad-hoc / PR"
        }
    ));
    progress("│   run:     mise run desktop-run");
    progress("│            cargo xtask desktop run");
    progress("└─────────────────────────────────────────────────────────────");
    progress("");
    progress(format!("DESKTOP_APP={}", abs.display()));
    Ok(())
}

fn assert_plist_string(plist: &Path, key: &str, expected: &str) -> Result<()> {
    let got = plist_buddy_print(plist, key)?;
    if got != expected {
        bail!("{key} {got} (expected {expected})");
    }
    Ok(())
}

fn assert_plist_bool_true(plist: &Path, key: &str) -> Result<()> {
    let got = plist_buddy_print(plist, key)?;
    if got != "true" {
        bail!("{key} must be true (got {got})");
    }
    Ok(())
}

fn plist_buddy_print(plist: &Path, key: &str) -> Result<String> {
    let mut cmd = cmd::command("/usr/libexec/PlistBuddy");
    cmd.args([
        "-c",
        &format!("Print :{key}"),
        plist.to_str().context("plist utf-8")?,
    ]);
    Ok(cmd::output_string(&mut cmd)?.trim().to_owned())
}

fn lipo_archs(path: &Path) -> Result<String> {
    let mut lipo = cmd::command("lipo");
    lipo.args(["-archs", path.to_str().context("path utf-8")?]);
    Ok(cmd::output_string(&mut lipo)?.trim().to_owned())
}

/// arm64 UUID of a Mach-O binary or dSYM DWARF file, via `dwarfdump --uuid`.
fn dwarf_uuid(path: &Path) -> Result<String> {
    let mut dwarfdump = cmd::command("xcrun");
    dwarfdump.args(["dwarfdump", "--uuid", path.to_str().context("path utf-8")?]);
    let output = cmd::output_string(&mut dwarfdump)?;
    parse_dwarf_uuid(&output)
        .with_context(|| format!("parsing dwarfdump UUID for {}", path.display()))
}

fn parse_dwarf_uuid(output: &str) -> Option<String> {
    for line in output.lines() {
        let Some(rest) = line.trim().strip_prefix("UUID: ") else {
            continue;
        };
        let uuid = rest.split_whitespace().next()?;
        if rest.contains("(arm64)") {
            return Some(uuid.to_owned());
        }
    }
    None
}

fn check_vtool_minos(bin: &Path) -> Result<()> {
    if which("vtool").is_err() {
        return Ok(());
    }
    let mut vtool = cmd::command("vtool");
    vtool.args([
        "-arch",
        ARCH,
        "-show-build",
        bin.to_str().context("bin utf-8")?,
    ]);
    let Ok(info) = cmd::output_string(&mut vtool) else {
        return Ok(());
    };
    for line in info.lines() {
        let lower = line.to_ascii_lowercase();
        if !lower.contains("minos") {
            continue;
        }
        let minos = line.split_whitespace().last().unwrap_or("");
        if !minos_matches_target(minos, MIN_OS) {
            bail!("slice arm64 minos {minos} (expected {MIN_OS})");
        }
    }
    Ok(())
}

fn minos_matches_target(minos: &str, target: &str) -> bool {
    fn major_minor(version: &str) -> Option<(u32, u32)> {
        let mut parts = version.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next().unwrap_or("0").parse().ok()?;
        Some((major, minor))
    }

    major_minor(minos) == major_minor(target)
}

pub(super) fn assert_no_embedded_libs(app: &Path) -> Result<()> {
    for path in walk_files(app)? {
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        if ext.eq_ignore_ascii_case("dylib") || ext.eq_ignore_ascii_case("a") {
            bail!("app embeds dylib or static archive: {}", path.display());
        }
    }
    for path in walk_dirs(app)? {
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if name.ends_with(".framework") || name.ends_with(".xcframework") {
            bail!("app embeds framework or XCFramework: {}", path.display());
        }
    }
    Ok(())
}

fn assert_no_absolute_ffi_link(bin: &Path) -> Result<()> {
    let mut otool = cmd::command("otool");
    otool.args(["-L", bin.to_str().context("bin utf-8")?]);
    let out = cmd::output_string(&mut otool)?;
    for line in out.lines() {
        if !line.starts_with('\t') {
            continue;
        }
        if line.contains("libjackin_usage_ffi")
            || line.contains("/Users/")
            || line.contains("/home/")
            || line.contains("target/")
        {
            bail!("absolute or FFI dylib linkage remains:\n{out}");
        }
    }
    Ok(())
}

pub(super) fn which(program: &str) -> Result<PathBuf> {
    let mut cmd = cmd::command("which");
    cmd.arg(program);
    let out = cmd::output_string(&mut cmd).with_context(|| format!("looking up {program}"))?;
    let path = out.trim();
    if path.is_empty() {
        bail!("{program} not found");
    }
    Ok(PathBuf::from(path))
}

pub(super) fn tempfile_dir(prefix: &str) -> Result<PathBuf> {
    let base = env::temp_dir().join(format!("{prefix}-{}", std::process::id()));
    if base.exists() {
        fs::remove_dir_all(&base)?;
    }
    fs::create_dir_all(&base)?;
    Ok(base)
}

fn walk_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    walk_collect(root, &mut out, true, false)?;
    Ok(out)
}

fn walk_dirs(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    walk_collect(root, &mut out, false, true)?;
    Ok(out)
}

fn walk_collect(root: &Path, out: &mut Vec<PathBuf>, files: bool, dirs: bool) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }
    for entry in crate::fs_util::read_dir_sorted(root)? {
        let path = entry.path();
        let ty = entry.file_type()?;
        if ty.is_dir() {
            if dirs {
                out.push(path.clone());
            }
            walk_collect(&path, out, files, dirs)?;
        } else if ty.is_file() && files {
            out.push(path);
        }
    }
    Ok(())
}

fn find_files_named(root: &Path, name: &str) -> Result<Vec<PathBuf>> {
    Ok(walk_files(root)?
        .into_iter()
        .filter(|p| p.file_name().and_then(|s| s.to_str()) == Some(name))
        .collect())
}

fn find_dirs_named(root: &Path, name: &str) -> Result<Vec<PathBuf>> {
    Ok(walk_dirs(root)?
        .into_iter()
        .filter(|p| p.file_name().and_then(|s| s.to_str()) == Some(name))
        .collect())
}

fn find_files_with_ext(root: &Path, ext: &str) -> Result<Vec<PathBuf>> {
    Ok(walk_files(root)?
        .into_iter()
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some(ext))
        .collect())
}

#[cfg(test)]
mod tests;
