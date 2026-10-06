//! jackin❯ desktop (native macOS usage menu bar) assembly and verification.
//!
//! Rust owns desktop build and verification logic; `native/mise.toml` composes
//! the local format, lint, test, and cadence commands. The UI test driver stays
//! in `native/Scripts/run-ui-tests.sh`.
//!
//! ```sh
//! cargo xtask desktop build --version 0.6.0 --build 1
//! cargo xtask desktop verify native/dist/JackinDesktop.app
//! # or use the local native cadence: mise -C native run ci
//! ```

mod bootstrap;
mod release_state;
mod sign_notarize;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Args, Subcommand};
use quick_xml::{
    Decoder, XmlVersion,
    events::{BytesDecl, BytesRef, BytesStart, Event},
    reader::Reader,
};

use crate::cmd;
use crate::docs;

const APP_EXECUTABLE: &str = "JackinDesktop";
const BROKER_EXECUTABLE: &str = "jackin-usage-broker";
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
const FFI_CRATE: &str = "jackin-usage-ffi";
/// Symbol-rich release lane for the desktop static library (see workspace
/// `[profile.desktop-release]`); release CI archives its unstripped bytes.
const DESKTOP_PROFILE: &str = "desktop-release";
const DEFAULT_SWIFT_JOBS: usize = 2;
const MAX_SWIFT_JOBS: usize = 8;

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
    TestSwift(SwiftTestArgs),
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

#[derive(Args)]
pub(crate) struct SwiftTestArgs {
    /// Maximum `SwiftPM` build jobs and parallel test workers (1–8).
    #[arg(long, default_value_t = DEFAULT_SWIFT_JOBS, value_parser = parse_swift_jobs)]
    jobs: usize,
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
        DesktopCommand::TestSwift(args) => run_swift_unit_tests(&docs::repo_root()?, args.jobs),
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
            "app not found at {}\n  build first: cargo xtask desktop build --version 0.6.0 --build 1",
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

/// `SwiftPM` unit tests with a count proof. Parallel `XCTest` writes the
/// aggregate xUnit report at `swift-unit-tests.xml`; Swift Testing writes its
/// separate `swift-unit-tests-swift-testing.xml` report. Both reports must be
/// present and nonzero — a mistyped selector, crashed runner, or missing
/// report can never look green.
fn run_swift_unit_tests(root: &Path, jobs: usize) -> Result<()> {
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

    progress(format!("==> swift build -c release --jobs {jobs}"));
    let mut build = cmd::command("swift");
    build.current_dir(&native).args(swift_build_args(jobs)?);
    cmd::run_streaming(&mut build)?;

    progress(format!(
        "==> swift test -c release --jobs {jobs} --parallel (counted)"
    ));
    let mut swift = cmd::command("swift");
    swift
        .current_dir(&native)
        .args(swift_test_args(jobs, &xunit_base)?);
    let run = cmd::run_stdout_file(&mut swift, &log);
    if run.is_err() {
        let tail = fs::read_to_string(&log)
            .map(|text| text.lines().rev().take(20).collect::<Vec<_>>().join("\n"))
            .unwrap_or_default();
        progress(format!("swift test failed; log tail:\n{tail}"));
        run?;
    }

    let xctest = read_xunit_totals(&xunit_base, "XCTest")?;
    validate_test_totals("XCTest", &xctest)?;

    let swift_testing = read_xunit_totals(&swift_testing_report, "Swift Testing")?;
    validate_test_totals("Swift Testing", &swift_testing)?;

    progress(format!(
        "==> swift unit tests OK: {} XCTest + {} Swift Testing executed, 0 failures",
        xctest.tests, swift_testing.tests
    ));
    Ok(())
}

fn parse_swift_jobs(value: &str) -> std::result::Result<usize, String> {
    let jobs = value
        .parse::<usize>()
        .map_err(|_| "jobs must be an integer from 1 through 8".to_owned())?;
    if !(1..=MAX_SWIFT_JOBS).contains(&jobs) {
        return Err("jobs must be from 1 through 8".to_owned());
    }
    Ok(jobs)
}

fn swift_build_args(jobs: usize) -> Result<Vec<String>> {
    let jobs = parse_swift_jobs(&jobs.to_string()).map_err(anyhow::Error::msg)?;
    Ok(vec![
        "build".to_owned(),
        "-c".to_owned(),
        "release".to_owned(),
        "--jobs".to_owned(),
        jobs.to_string(),
    ])
}

fn swift_test_args(jobs: usize, xunit_report: &Path) -> Result<Vec<String>> {
    let jobs = parse_swift_jobs(&jobs.to_string()).map_err(anyhow::Error::msg)?;
    Ok(vec![
        "test".to_owned(),
        "-c".to_owned(),
        "release".to_owned(),
        "--jobs".to_owned(),
        jobs.to_string(),
        "--parallel".to_owned(),
        "--num-workers".to_owned(),
        jobs.to_string(),
        "--experimental-maximum-parallelization-width".to_owned(),
        jobs.to_string(),
        "--xunit-output".to_owned(),
        xunit_report
            .to_str()
            .context("xunit path utf-8")?
            .to_owned(),
    ])
}

#[derive(Debug, PartialEq, Eq)]
struct XunitTotals {
    tests: u64,
    failures: u64,
    errors: u64,
}

/// Validate an opening (`Start`/`Empty`) element's name and document position.
/// An empty root both opens and closes the document; `Start` elements advance
/// depth at the call site.
fn check_xunit_open_position(
    name: &[u8],
    depth: usize,
    root_seen: &mut bool,
    root_closed: &mut bool,
    empty: bool,
) -> Result<()> {
    anyhow::ensure!(is_xml_name(name), "corrupt xUnit: invalid XML element name");
    if depth == 0 {
        anyhow::ensure!(
            !*root_seen && !*root_closed && name == b"testsuites",
            "corrupt xUnit: expected one testsuites document root"
        );
        *root_seen = true;
        if empty {
            *root_closed = true;
        }
    } else if depth == 1 {
        anyhow::ensure!(
            name == b"testsuite",
            "corrupt xUnit: testsuites may contain only testsuite elements"
        );
    } else {
        anyhow::ensure!(
            name != b"testsuites" && name != b"testsuite",
            "corrupt xUnit: nested test suite element"
        );
    }
    Ok(())
}

/// Sum `tests`/`failures`/`errors` across direct `<testsuite>` children.
/// The token reader is state-checked as an XML document; unsupported DTDs,
/// malformed prologs, content outside the root, and missing reports fail closed.
/// Missing elements or attributes are corruption, never zero.
fn parse_xunit_totals(source: &str) -> Result<XunitTotals> {
    anyhow::ensure!(
        source.chars().all(is_xml_char),
        "corrupt xUnit: input contains a character forbidden by XML 1.0"
    );
    let mut reader = Reader::from_str(source);
    reader.config_mut().check_end_names = true;
    reader.config_mut().check_comments = true;
    let mut totals = XunitTotals {
        tests: 0,
        failures: 0,
        errors: 0,
    };
    let mut suites = 0_u64;
    let mut depth = 0_usize;
    let mut root_seen = false;
    let mut root_closed = false;
    let mut declaration_seen = false;
    let mut prolog_started = false;
    loop {
        match reader.read_event() {
            Ok(Event::Start(element)) => {
                check_xunit_open_position(
                    element.name().as_ref(),
                    depth,
                    &mut root_seen,
                    &mut root_closed,
                    false,
                )?;
                validate_xunit_attributes(&element, reader.decoder())?;
                if depth == 0 {
                    prolog_started = true;
                }
                if element.name().as_ref() == b"testsuite" {
                    add_xunit_suite(&element, reader.decoder(), &mut totals)?;
                    suites = suites
                        .checked_add(1)
                        .context("corrupt xUnit: testsuite count overflow")?;
                }
                depth = depth
                    .checked_add(1)
                    .context("corrupt xUnit: element nesting overflow")?;
            }
            Ok(Event::Empty(element)) => {
                check_xunit_open_position(
                    element.name().as_ref(),
                    depth,
                    &mut root_seen,
                    &mut root_closed,
                    true,
                )?;
                validate_xunit_attributes(&element, reader.decoder())?;
                if depth == 0 {
                    prolog_started = true;
                }
                if element.name().as_ref() == b"testsuite" {
                    add_xunit_suite(&element, reader.decoder(), &mut totals)?;
                    suites = suites
                        .checked_add(1)
                        .context("corrupt xUnit: testsuite count overflow")?;
                }
            }
            Ok(Event::End(element)) => {
                anyhow::ensure!(
                    depth > 0,
                    "corrupt xUnit: closing tag without an open element"
                );
                depth -= 1;
                if depth == 0 {
                    anyhow::ensure!(
                        element.name().as_ref() == b"testsuites",
                        "corrupt xUnit: testsuites document root closed unexpectedly"
                    );
                    root_closed = true;
                }
            }
            Ok(Event::Eof) => {
                anyhow::ensure!(
                    root_seen && root_closed && depth == 0,
                    "corrupt xUnit: truncated document or missing testsuites root"
                );
                break;
            }
            Ok(Event::Text(text)) if depth == 0 => {
                anyhow::ensure!(
                    text.iter().copied().all(is_xml_whitespace),
                    "corrupt xUnit: character data outside document root"
                );
                if !root_seen {
                    prolog_started = true;
                }
            }
            Ok(Event::Text(_)) => {}
            Ok(Event::CData(_)) if depth > 0 => {}
            Ok(Event::CData(_)) => bail!("corrupt xUnit: CDATA outside document root"),
            Ok(Event::GeneralRef(reference)) if depth > 0 => {
                validate_xml_reference(&reference)?;
            }
            Ok(Event::GeneralRef(_)) => {
                bail!("corrupt xUnit: entity reference outside document root");
            }
            Ok(Event::Comment(_)) => {
                if !root_seen {
                    prolog_started = true;
                }
            }
            Ok(Event::PI(instruction)) => {
                anyhow::ensure!(
                    is_xml_name(instruction.target())
                        && !instruction.target().eq_ignore_ascii_case(b"xml"),
                    "corrupt xUnit: invalid or reserved processing-instruction target"
                );
                if !root_seen {
                    prolog_started = true;
                }
            }
            Ok(Event::Decl(declaration)) => {
                anyhow::ensure!(
                    !declaration_seen && !prolog_started && !root_seen && depth == 0,
                    "corrupt xUnit: XML declaration must be the first document item"
                );
                validate_xunit_declaration(&declaration)?;
                declaration_seen = true;
                prolog_started = true;
            }
            Ok(Event::DocType(_)) => {
                bail!("corrupt xUnit: document type declarations are unsupported");
            }
            Err(error) => bail!("corrupt xUnit at byte {}: {error}", reader.error_position()),
        }
    }
    if suites == 0 {
        bail!("corrupt xUnit: no testsuite elements");
    }
    Ok(totals)
}

fn is_xml_char(character: char) -> bool {
    matches!(
        character as u32,
        0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF
    )
}

fn is_xml_name(name: &[u8]) -> bool {
    let Ok(name) = std::str::from_utf8(name) else {
        return false;
    };
    let mut characters = name.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    is_xml_name_start(first) && characters.all(is_xml_name_char)
}

fn is_xml_name_start(character: char) -> bool {
    matches!(
        character as u32,
        0x3A
            | 0x41..=0x5A
            | 0x5F
            | 0x61..=0x7A
            | 0xC0..=0xD6
            | 0xD8..=0xF6
            | 0xF8..=0x2FF
            | 0x370..=0x37D
            | 0x37F..=0x1FFF
            | 0x200C..=0x200D
            | 0x2070..=0x218F
            | 0x2C00..=0x2FEF
            | 0x3001..=0xD7FF
            | 0xF900..=0xFDCF
            | 0xFDF0..=0xFFFD
            | 0x10000..=0xEFFFF
    )
}

fn is_xml_name_char(character: char) -> bool {
    is_xml_name_start(character)
        || matches!(
            character as u32,
            0x2D | 0x2E | 0x30..=0x39 | 0xB7 | 0x300..=0x36F | 0x203F..=0x2040
        )
}

fn is_xml_whitespace(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r')
}

fn validate_xml_reference(reference: &BytesRef<'_>) -> Result<()> {
    if let Some(character) = reference
        .resolve_char_ref()
        .context("decoding xUnit character reference")?
    {
        anyhow::ensure!(
            is_xml_char(character),
            "corrupt xUnit: character reference is forbidden by XML 1.0"
        );
        return Ok(());
    }
    let entity: &[u8] = reference.as_ref();
    anyhow::ensure!(
        entity == b"amp"
            || entity == b"lt"
            || entity == b"gt"
            || entity == b"apos"
            || entity == b"quot",
        "corrupt xUnit: undeclared entity reference"
    );
    Ok(())
}

fn validate_xunit_declaration(declaration: &BytesDecl<'_>) -> Result<()> {
    let content =
        std::str::from_utf8(declaration.as_ref()).context("decoding xUnit XML declaration")?;
    let element = BytesStart::from_content(content, b"xml".len());
    let mut attributes = element.attributes();
    attributes.with_checks(true);
    let mut position = 0;
    for attribute in attributes {
        let attribute = attribute.context("parsing xUnit XML declaration attribute")?;
        match attribute.key.as_ref() {
            b"version" => {
                anyhow::ensure!(
                    position == 0 && attribute.value.as_ref() == b"1.0",
                    "corrupt xUnit: declaration must begin with XML version 1.0"
                );
            }
            b"encoding" => {
                anyhow::ensure!(
                    position == 1 && attribute.value.as_ref().eq_ignore_ascii_case(b"utf-8"),
                    "corrupt xUnit: only UTF-8 XML declarations are supported"
                );
            }
            b"standalone" => {
                anyhow::ensure!(
                    (position == 1 || position == 2)
                        && (attribute.value.as_ref() == b"yes"
                            || attribute.value.as_ref() == b"no"),
                    "corrupt xUnit: standalone must follow version/encoding and be yes or no"
                );
            }
            _ => bail!("corrupt xUnit: unknown XML declaration attribute"),
        }
        position += 1;
    }
    anyhow::ensure!(position > 0, "corrupt xUnit: declaration lacks a version");
    Ok(())
}

fn validate_xunit_attributes(element: &BytesStart<'_>, decoder: Decoder) -> Result<()> {
    let mut attributes = element.attributes();
    attributes.with_checks(true);
    for attribute in attributes {
        let attribute = attribute.context("parsing xUnit element attribute")?;
        anyhow::ensure!(
            is_xml_name(attribute.key.as_ref()),
            "corrupt xUnit: invalid XML attribute name"
        );
        let value = attribute
            .decoded_and_normalized_value(XmlVersion::Implicit1_0, decoder)
            .context("decoding xUnit attribute value")?;
        anyhow::ensure!(
            value.chars().all(is_xml_char),
            "corrupt xUnit: attribute contains a character forbidden by XML 1.0"
        );
    }
    Ok(())
}

fn add_xunit_suite(
    element: &BytesStart<'_>,
    decoder: Decoder,
    totals: &mut XunitTotals,
) -> Result<()> {
    let mut suite = XunitTotals {
        tests: 0,
        failures: 0,
        errors: 0,
    };
    let mut found_tests = false;
    let mut found_failures = false;
    let mut found_errors = false;
    for attribute in element.attributes() {
        let attribute = attribute.context("parsing xUnit testsuite attribute")?;
        let value = attribute
            .decoded_and_normalized_value(XmlVersion::Implicit1_0, decoder)
            .context("decoding xUnit testsuite attribute")?;
        let (slot, found, name) = match attribute.key.as_ref() {
            b"tests" => (&mut suite.tests, &mut found_tests, "tests"),
            b"failures" => (&mut suite.failures, &mut found_failures, "failures"),
            b"errors" => (&mut suite.errors, &mut found_errors, "errors"),
            _ => continue,
        };
        *slot = value
            .parse()
            .with_context(|| format!("corrupt xUnit: non-numeric {name} attribute"))?;
        *found = true;
    }
    anyhow::ensure!(
        found_tests && found_failures && found_errors,
        "corrupt xUnit: testsuite must have tests, failures, and errors attributes"
    );
    totals.tests = totals
        .tests
        .checked_add(suite.tests)
        .context("corrupt xUnit: tests total overflow")?;
    totals.failures = totals
        .failures
        .checked_add(suite.failures)
        .context("corrupt xUnit: failures total overflow")?;
    totals.errors = totals
        .errors
        .checked_add(suite.errors)
        .context("corrupt xUnit: errors total overflow")?;
    Ok(())
}

fn read_xunit_totals(path: &Path, framework: &str) -> Result<XunitTotals> {
    let source = fs::read_to_string(path)
        .with_context(|| format!("missing {framework} xUnit report {}", path.display()))?;
    parse_xunit_totals(&source)
        .with_context(|| format!("invalid {framework} xUnit report {}", path.display()))
}

fn validate_test_totals(framework: &str, totals: &XunitTotals) -> Result<()> {
    if totals.tests == 0 {
        bail!("{framework} executed zero tests — refusing the false-green selection trap");
    }
    if totals.failures > 0 || totals.errors > 0 {
        bail!(
            "{framework} reported {} failures and {} errors across {} tests",
            totals.failures,
            totals.errors,
            totals.tests
        );
    }
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
    progress("│   verify:  cargo xtask desktop verify");
    progress("│   run:     cargo xtask desktop run");
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
/// `cargo xtask desktop verify` reads version metadata from the app bundle.
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
        "committed boltffi bindings are stale; run `cargo xtask desktop bindings` and commit:",
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
        "boltffi not on PATH; install via mise (`mise install`) — see mise.toml github:boltffi/boltffi",
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
    bindings_check(root, DESKTOP_PROFILE)?;
    require_macos("desktop xcframework")?;

    progress(format!(
        "==> packing staticlib for {HOST_TARGET} (macOS {MIN_OS} floor)"
    ));
    let mut rustup = cmd::command("rustup");
    rustup.args(["target", "add", HOST_TARGET]);
    // Already-installed target is fine; surface other rustup failures below if cargo fails.
    drop(cmd::run(&mut rustup));

    // Stream the long staticlib build directly: boltffi captures cargo's
    // output and only summarizes, leaving CI silent for 400s+ on a healthy
    // full build — indistinguishable from a stall to the 600s watchdog.
    // Same profile/target/env as boltffi's own invocation, so its cargo
    // call below goes incremental and fast; mismatch degrades to a slow
    // but correct rebuild, never wrong output.
    progress("==> building staticlib (streaming cargo progress)");
    let mut prebuild = cmd::command("cargo");
    prebuild
        .current_dir(root.join(FFI_CRATE_DIR))
        .env("MACOSX_DEPLOYMENT_TARGET", MIN_OS)
        .args([
            "build",
            "--profile",
            DESKTOP_PROFILE,
            "--target",
            HOST_TARGET,
            "-p",
            FFI_CRATE,
        ]);
    cmd::run_streaming(&mut prebuild)?;

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

    // boltffi's clean pack needs both its generated header and XCFramework
    // staging inputs. It also regenerates Swift, so normalize those outputs
    // after packing to keep the committed bindings deterministic.
    let boltffi = which("boltffi").context(
        "boltffi not on PATH; install via mise (`mise install`) — see mise.toml github:boltffi/boltffi",
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

    // The FFI resolves its broker beside the running app executable. Build
    // that process for the app's native target, regardless of CLI release lanes.
    progress("==> building native usage broker");
    let mut broker_build = cmd::command("cargo");
    broker_build
        .current_dir(root)
        .env("MACOSX_DEPLOYMENT_TARGET", MIN_OS)
        .env("JACKIN_VERSION_OVERRIDE", version)
        .args([
            "build",
            "--profile",
            DESKTOP_PROFILE,
            "--target",
            HOST_TARGET,
            "-p",
            "jackin",
            "--bin",
            BROKER_EXECUTABLE,
        ])
        .arg("--target-dir")
        .arg(root.join("target"));
    cmd::run_streaming(&mut broker_build)?;
    let built_broker = root.join(format!(
        "target/{HOST_TARGET}/{DESKTOP_PROFILE}/{BROKER_EXECUTABLE}"
    ));
    verify_broker(&built_broker, version)?;

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

    let broker = broker_path(&dist);
    fs::copy(&built_broker, &broker)
        .with_context(|| format!("copying usage broker into {}", broker.display()))?;
    verify_broker(&broker, version)?;

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
    sign_broker(&dist, "-", false)?;
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
    let broker = broker_path(app);
    verify_broker(&broker, version)?;
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
    verify_broker_signature(app)?;

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
    progress("│   run:     cargo xtask desktop run");
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

pub(super) fn broker_path(app: &Path) -> PathBuf {
    app.join("Contents/MacOS").join(BROKER_EXECUTABLE)
}

fn assert_native_broker_archs(archs: &str) -> Result<()> {
    if archs.split_whitespace().collect::<Vec<_>>() != [ARCH] {
        bail!("usage broker must be {ARCH}-only (got {archs})");
    }
    Ok(())
}

fn assert_broker_version(output: &str, version: &str) -> Result<()> {
    if output.trim() != format!("{BROKER_EXECUTABLE} {version}") {
        bail!(
            "usage broker version mismatch (got {}, expected {version})",
            output.trim()
        );
    }
    Ok(())
}

fn assert_executable_file(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("missing usage broker {}", path.display()))?;
    if !metadata.is_file() {
        bail!(
            "usage broker must be a regular executable file: {}",
            path.display()
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            bail!("usage broker is not executable: {}", path.display());
        }
    }
    Ok(())
}

fn verify_broker(path: &Path, version: &str) -> Result<()> {
    assert_executable_file(path)?;
    assert_native_broker_archs(&lipo_archs(path)?)?;
    check_vtool_minos(path)?;
    assert_no_absolute_ffi_link(path)?;
    let mut probe = cmd::command(path);
    probe.arg("--version");
    assert_broker_version(&cmd::output_string(&mut probe)?, version)
}

/// Sign nested code explicitly before sealing the enclosing bundle.
pub(super) fn sign_broker(app: &Path, identity: &str, release: bool) -> Result<()> {
    let broker = broker_path(app);
    assert_executable_file(&broker)?;
    let mut codesign = cmd::command("codesign");
    codesign.arg("--force");
    if release {
        codesign.args(["--options", "runtime", "--timestamp"]);
    } else {
        codesign.arg("--timestamp=none");
    }
    codesign.args(["--sign", identity]).arg(&broker);
    cmd::run_streaming(&mut codesign)?;
    verify_broker_signature(app)
}

pub(super) fn verify_broker_signature(app: &Path) -> Result<()> {
    let mut codesign = cmd::command("codesign");
    codesign
        .args(["--verify", "--strict"])
        .arg(broker_path(app));
    cmd::run(&mut codesign).context("usage broker codesign verify failed")
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
