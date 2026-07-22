//! jackin❯ Desktop (native macOS usage menu bar) assembly and verification.
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
const BUNDLE_NAME: &str = "Jackin Desktop";
const MIN_OS: &str = "14.0";
const FRAMEWORK_NAME: &str = "JackinUsageFFI";
const MODULE_NAME: &str = "jackin_usage_ffiFFI";
const STATIC_LIB: &str = "libjackin_usage_ffi.a";
const HOST_TARGET: &str = "aarch64-apple-darwin";
const ARCH: &str = "arm64";

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
    /// Generate `UniFFI` Swift bindings into `native/Generated`.
    Bindings(BindingsArgs),
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
    /// Developer ID sign + notarize + staple + final release ZIP.
    SignNotarize(sign_notarize::SignNotarizeArgs),
    /// Independent publication state (`KEY=value` lines for `GITHUB_OUTPUT`).
    ReleaseState(release_state::ReleaseStateArgs),
    /// Bootstrap GitHub env `release-macos` Apple secrets (never prints values).
    BootstrapSecrets(Box<bootstrap::BootstrapSecretsArgs>),
}

#[derive(Args)]
pub(crate) struct BindingsArgs {
    /// Cargo profile used to build the library for uniffi-bindgen.
    #[arg(long, default_value = "release")]
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
        DesktopCommand::Xcframework => build_xcframework(&docs::repo_root()?),
        DesktopCommand::Build(args) => {
            let (version, build) = resolve_version_build(args.version, args.build)?;
            build_app(&docs::repo_root()?, &version, &build)
        }
        DesktopCommand::Test => run_desktop_tests(&docs::repo_root()?),
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
    let xcf = root.join("target/xcframework/JackinUsageFFI.xcframework");
    if !xcf.is_dir() {
        progress("==> XCFramework missing — building");
        build_xcframework(root)?;
    }

    let native = root.join("native");
    for (name, product) in [
        ("StatusItemChipHarness", "StatusItemChipHarness"),
        ("DesktopArchitectureLint", "DesktopArchitectureLint"),
        ("DesktopParityMatrixHarness", "DesktopParityMatrixHarness"),
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
    progress("│ jackin❯ Desktop — tests OK");
    progress("│   host nextest + StatusItemChipHarness");
    progress("│   DesktopArchitectureLint + DesktopParityMatrixHarness");
    progress("│   (full Xcode: cd native && swift test -c release)");
    progress("└─────────────────────────────────────────────────────────────");
    Ok(())
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
    progress("│ jackin❯ Desktop — launching");
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
    progress("│ jackin❯ Desktop — build complete");
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
    require_macos("desktop bindings")?;
    let profile = profile.trim();
    if profile != "release" && profile != "debug" {
        bail!("profile must be release or debug (got {profile})");
    }

    progress(format!("==> building jackin-usage-ffi ({profile})"));
    let mut cargo = cmd::command("cargo");
    cargo
        .current_dir(root)
        .args(["build", "-p", "jackin-usage-ffi", &format!("--{profile}")]);
    cmd::run_streaming(&mut cargo)?;

    let lib = root.join(format!("target/{profile}/libjackin_usage_ffi.dylib"));
    if !lib.is_file() {
        bail!("expected library at {}", lib.display());
    }

    let bindgen = which("uniffi-bindgen").context(
        "uniffi-bindgen not on PATH; install via mise (`mise install`) — see mise.toml cargo:uniffi",
    )?;

    let out_dir = root.join("native/Generated");
    fs::create_dir_all(&out_dir)?;
    progress(format!(
        "==> generating Swift bindings into {}",
        out_dir.display()
    ));
    let mut bindgen_cmd = cmd::command(&bindgen);
    bindgen_cmd.current_dir(root).args([
        "generate",
        "--library",
        lib.to_str().context("lib path utf-8")?,
        "--language",
        "swift",
        "--out-dir",
        out_dir.to_str().context("out_dir utf-8")?,
    ]);
    cmd::run_streaming(&mut bindgen_cmd)?;

    let sources = root.join("native/Sources/JackinUsageBridge");
    fs::create_dir_all(&sources)?;
    let generated_swift = out_dir.join("jackin_usage_ffi.swift");
    if generated_swift.is_file() {
        fs::copy(&generated_swift, sources.join("jackin_usage_ffi.swift"))?;
    }
    let modulemap = out_dir.join("module.modulemap");
    if out_dir.join("jackin_usage_ffiFFI.modulemap").is_file() && !modulemap.is_file() {
        fs::write(
            &modulemap,
            "module jackin_usage_ffiFFI {\n    header \"jackin_usage_ffiFFI.h\"\n    export *\n}\n",
        )?;
    }

    progress(format!(
        "==> generated bindings under {}",
        out_dir.display()
    ));
    Ok(())
}

fn build_xcframework(root: &Path) -> Result<()> {
    require_macos("desktop xcframework")?;

    progress(format!(
        "==> building staticlib for {HOST_TARGET} (macOS 14 floor)"
    ));
    let mut rustup = cmd::command("rustup");
    rustup.args(["target", "add", HOST_TARGET]);
    // Already-installed target is fine; surface other rustup failures below if cargo fails.
    drop(cmd::run(&mut rustup));

    let mut cargo = cmd::command("cargo");
    cargo
        .current_dir(root)
        .env("MACOSX_DEPLOYMENT_TARGET", MIN_OS)
        .args([
            "build",
            "-p",
            "jackin-usage-ffi",
            "--release",
            "--target",
            HOST_TARGET,
        ]);
    cmd::run_streaming(&mut cargo)?;

    let arm_lib = root.join(format!("target/{HOST_TARGET}/release/{STATIC_LIB}"));
    if !arm_lib.is_file() {
        bail!("missing {}", arm_lib.display());
    }

    generate_bindings(root, "release")?;

    let header = find_generated_header(root)?;
    let out_dir = root.join("target/xcframework");
    let xcframework = out_dir.join(format!("{FRAMEWORK_NAME}.xcframework"));
    if out_dir.exists() {
        fs::remove_dir_all(&out_dir)?;
    }
    fs::create_dir_all(&xcframework)?;

    progress(format!(
        "==> assembling static XCFramework ({MODULE_NAME}, arm64 only)"
    ));
    install_slice(&xcframework, ARCH, &arm_lib, &header)?;

    let info_plist = xcframework.join("Info.plist");
    fs::write(&info_plist, XCFRAMEWORK_INFO_PLIST)?;
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

    progress(format!("==> XCFramework ready: {}", xcframework.display()));
    Ok(())
}

fn install_slice(xcframework: &Path, arch: &str, lib: &Path, header: &Path) -> Result<()> {
    let id = format!("macos-{arch}");
    let slice = xcframework.join(&id);
    let headers = slice.join("Headers");
    fs::create_dir_all(&headers)?;
    fs::copy(lib, slice.join(STATIC_LIB))?;
    fs::copy(header, headers.join("jackin_usage_ffiFFI.h"))?;
    fs::write(
        headers.join("module.modulemap"),
        format!("module {MODULE_NAME} {{\n  header \"jackin_usage_ffiFFI.h\"\n  export *\n}}\n"),
    )?;

    let archs = lipo_archs(&slice.join(STATIC_LIB))?;
    progress(format!("  slice {id}: {archs}"));
    if !archs.split_whitespace().any(|a| a == arch) {
        bail!("{id} library missing {arch} (got {archs})");
    }
    Ok(())
}

fn find_generated_header(root: &Path) -> Result<PathBuf> {
    let preferred = root.join("native/Generated/jackin_usage_ffiFFI.h");
    if preferred.is_file() {
        return Ok(preferred);
    }
    let generated = root.join("native/Generated");
    find_files_with_ext(&generated, "h")?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("no generated header under native/Generated"))
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
    progress(format!("==> swift build ({ARCH})"));
    let mut swift = cmd::command("swift");
    swift.current_dir(&native).args([
        "build",
        "-c",
        "release",
        "--product",
        APP_EXECUTABLE,
        "--arch",
        ARCH,
        "-Xswiftc",
        "-target",
        "-Xswiftc",
        &format!("{ARCH}-apple-macosx{MIN_OS}"),
    ]);
    cmd::run_streaming(&mut swift)?;

    let bin_dir = swift_bin_path(&native, ARCH)?;
    let mut bin = bin_dir.join(APP_EXECUTABLE);
    if !bin.is_file() {
        let fallback = swift_bin_path(&native, "")?;
        bin = fallback.join(APP_EXECUTABLE);
    }
    if !bin.is_file() {
        bail!("missing Swift product for {ARCH}");
    }

    let got = lipo_archs(&bin)?;
    if !got.split_whitespace().any(|a| a == ARCH) {
        bail!("expected {ARCH} in {}, got: {got}", bin.display());
    }
    if got.split_whitespace().any(|a| a == "x86_64") {
        bail!("unexpected x86_64 slice in arm64-only build: {got}");
    }

    if dist.exists() {
        fs::remove_dir_all(&dist)?;
    }
    fs::create_dir_all(dist.join("Contents/MacOS"))?;
    fs::create_dir_all(dist.join("Contents/Resources"))?;
    let app_bin = dist.join(format!("Contents/MacOS/{APP_EXECUTABLE}"));
    fs::copy(&bin, &app_bin)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&app_bin)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&app_bin, perms)?;
    }

    let resource_bundle = find_resource_bundle(&bin_dir)?;
    copy_dir_all(
        &resource_bundle,
        &dist
            .join("Contents/Resources")
            .join(resource_bundle.file_name().context("bundle name")?),
    )?;

    let archs = lipo_archs(&app_bin)?;
    progress(format!("  executable archs: {archs}"));
    if !archs.split_whitespace().any(|a| a == ARCH) {
        bail!("final app missing arm64 (got {archs})");
    }
    if archs.split_whitespace().any(|a| a == "x86_64") {
        bail!("final app must be arm64-only (got {archs})");
    }

    let plist = dist.join("Contents/Info.plist");
    fs::write(&plist, app_info_plist(version, build))?;

    assert_no_embedded_libs(&dist)?;
    assert_no_absolute_ffi_link(&app_bin)?;

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
    let resource_bundle = app.join("Contents/Resources/JackinDesktop_JackinDesktop.bundle");

    if !bin.is_file() {
        bail!("missing executable {}", bin.display());
    }
    if !plist.is_file() {
        bail!("missing {}", plist.display());
    }
    if !resource_bundle.is_dir() {
        bail!(
            "missing SwiftPM resource bundle {}",
            resource_bundle.display()
        );
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
    progress("│ jackin❯ Desktop — verify OK");
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
        if minos.is_empty() || minos == "14.0" || minos == "14.0.0" {
            continue;
        }
        if minos_newer_than_14(minos) {
            bail!("slice arm64 minos {minos} newer than 14.0");
        }
    }
    Ok(())
}

fn minos_newer_than_14(minos: &str) -> bool {
    let mut parts = minos.split('.');
    let major: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    major > 14 || (major == 14 && minor > 0)
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

fn swift_bin_path(native: &Path, arch: &str) -> Result<PathBuf> {
    let mut swift = cmd::command("swift");
    swift
        .current_dir(native)
        .args(["build", "-c", "release", "--show-bin-path"]);
    if !arch.is_empty() {
        swift.args(["--arch", arch]);
    }
    let path = cmd::output_string(&mut swift)?.trim().to_owned();
    Ok(PathBuf::from(path))
}

fn find_resource_bundle(bin_dir: &Path) -> Result<PathBuf> {
    for name in ["JackinDesktop_JackinDesktop.bundle", "JackinDesktop.bundle"] {
        let candidate = bin_dir.join(name);
        if candidate.is_dir() {
            return Ok(candidate);
        }
    }
    for path in walk_dirs(bin_dir)? {
        if path.file_name().and_then(|s| s.to_str()) == Some("JackinDesktop_JackinDesktop.bundle") {
            // Prefer shallow matches under bin_dir (maxdepth-ish: path components).
            if path
                .strip_prefix(bin_dir)
                .ok()
                .is_some_and(|rel| rel.components().count() <= 3)
            {
                return Ok(path);
            }
        }
    }
    bail!(
        "missing SwiftPM resource bundle JackinDesktop_JackinDesktop.bundle under {}",
        bin_dir.display()
    )
}

fn app_info_plist(version: &str, build: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>{APP_EXECUTABLE}</string>
  <key>CFBundleIdentifier</key>
  <string>{BUNDLE_ID}</string>
  <key>CFBundleName</key>
  <string>{BUNDLE_NAME}</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>{version}</string>
  <key>CFBundleVersion</key>
  <string>{build}</string>
  <key>LSUIElement</key>
  <true/>
  <key>LSMinimumSystemVersion</key>
  <string>{MIN_OS}</string>
</dict>
</plist>
"#
    )
}

const XCFRAMEWORK_INFO_PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>AvailableLibraries</key>
  <array>
    <dict>
      <key>LibraryIdentifier</key>
      <string>macos-arm64</string>
      <key>LibraryPath</key>
      <string>libjackin_usage_ffi.a</string>
      <key>HeadersPath</key>
      <string>Headers</string>
      <key>SupportedArchitectures</key>
      <array>
        <string>arm64</string>
      </array>
      <key>SupportedPlatform</key>
      <string>macos</string>
    </dict>
  </array>
  <key>CFBundlePackageType</key>
  <string>XFWK</string>
  <key>XCFrameworkFormatVersion</key>
  <string>1.0</string>
</dict>
</plist>
"#;

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

fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in crate::fs_util::read_dir_sorted(src)? {
        let ty = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &to)?;
        } else if ty.is_file() {
            fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
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
