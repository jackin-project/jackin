#![expect(
    clippy::unwrap_used,
    reason = "integration fixtures fail fast with their temporary repository and command output"
)]

//! Native integration coverage for the immutable six-payload Homebrew preview producer.
//!
//! The task body is read from mise.toml and run verbatim. Its cargo xtask calls
//! reach the actual compiled binary. Cross-target compilation, Rust target
//! installation, Syft's catalog scan, OIDC-backed cosign, and translation of
//! GNU tar metadata switches for native bsdtar are local fixtures. Real tar and
//! gzip still create the archives; SHA calculation, SBOM binding, manifests,
//! and verification use production xtask implementations. Fixture bundles are
//! explicitly unsigned and prove no live signing or publication claim.

use flate2::read::GzDecoder;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Cursor, Read},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use tar::Archive;
use tempfile::TempDir;

const SOURCE_URL: &str = "https://github.com/jackin-project/jackin.git";
const SOURCE_REPOSITORY: &str = "jackin-project/jackin";
const SOURCE_REF: &str = "refs/heads/main";
const PACKAGE_RELATIVE: &str = "out/homebrew-preview";
const BASE_VERSION: &str = "0.6.4";
const RUN_ID: &str = "880008";

const PAYLOADS: [&str; 6] = [
    "jackin-aarch64-apple-darwin.tar.gz",
    "jackin-x86_64-apple-darwin.tar.gz",
    "jackin-aarch64-unknown-linux-gnu.tar.gz",
    "jackin-x86_64-unknown-linux-gnu.tar.gz",
    "jackin-capsule-aarch64-unknown-linux-gnu.tar.gz",
    "jackin-capsule-x86_64-unknown-linux-gnu.tar.gz",
];

const CLI_BINARIES: [&str; 2] = ["jackin", "jackin-role"];
const CAPSULE_BINARY: &str = "jackin-capsule";

struct Fixture {
    _temp: TempDir,
    source: PathBuf,
    remote: PathBuf,
    runner_temp: PathBuf,
    shims: PathBuf,
    probe_source: PathBuf,
    source_commit: String,
    version: String,
    host_target: &'static str,
}

impl Fixture {
    fn new(advance_remote_main: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let remote = temp.path().join("origin.git");
        let runner_temp = temp.path().join("runner-temp");
        let shims = temp.path().join("fixture-bin");
        let probe_source = temp.path().join("version-probe.rs");

        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&remote).unwrap();
        fs::create_dir_all(&runner_temp).unwrap();
        fs::create_dir_all(&shims).unwrap();

        // This empty marker is only a cross-build fixture; it is not an SDK.
        fs::create_dir_all(runner_temp.join("macos-sdk/MacOSX26.1.sdk")).unwrap();

        git(&source, &["init", "--quiet", "--initial-branch=main"]);
        git(
            &source,
            &["config", "user.name", "TASK-008 integration fixture"],
        );
        git(
            &source,
            &["config", "user.email", "task008-fixture@example.invalid"],
        );
        fs::write(
            source.join("Cargo.toml"),
            "[package]\nname = \"jackin-preview-fixture\"\nversion = \"0.6.4\"\nedition = \"2024\"\n",
        )
        .unwrap();
        fs::write(source.join(".gitignore"), "out/\n").unwrap();
        fs::write(source.join("README.md"), "admitted source\n").unwrap();
        git(&source, &["add", "Cargo.toml", ".gitignore", "README.md"]);
        git(&source, &["commit", "--quiet", "-m", "admit source"]);
        let source_commit = git(&source, &["rev-parse", "HEAD"]);

        git(&remote, &["init", "--bare", "--quiet"]);
        let local_remote = format!("file://{}", remote.display());
        git(&source, &["remote", "add", "origin", &local_remote]);
        git(
            &source,
            &["push", "--quiet", "origin", "HEAD:refs/heads/main"],
        );
        git(&source, &["remote", "set-url", "origin", SOURCE_URL]);
        if advance_remote_main {
            fs::write(source.join("README.md"), "newer main tip\n").unwrap();
            git(&source, &["add", "README.md"]);
            git(&source, &["commit", "--quiet", "-m", "advance main"]);
            git(&source, &["remote", "set-url", "origin", &local_remote]);
            git(
                &source,
                &["push", "--quiet", "origin", "HEAD:refs/heads/main"],
            );
            git(&source, &["remote", "set-url", "origin", SOURCE_URL]);
            git(&source, &["reset", "--quiet", "--hard", &source_commit]);
        }

        let commit_count = git(&source, &["rev-list", "--count", &source_commit]);
        let version = format!(
            "{BASE_VERSION}-preview.{commit_count}+{}",
            &source_commit[..7]
        );
        let host_target = host_target();
        let source = source.canonicalize().unwrap();
        let runner_temp = runner_temp.canonicalize().unwrap();

        // The cargo fixture compiles this real host executable with the
        // producer's JACKIN_VERSION_OVERRIDE. Cross-target members get only
        // format/architecture headers plus a fixture marker.
        fs::write(
            &probe_source,
            concat!(
                "fn main() {\n",
                "    let path = std::env::current_exe().expect(\"current executable\");\n",
                "    let name = path.file_name().expect(\"binary name\").to_string_lossy();\n",
                "    println!(\"{} {}\", name, env!(\"JACKIN_VERSION_OVERRIDE\"));\n",
                "}\n",
            ),
        )
        .unwrap();
        write_shims(&shims);

        Self {
            _temp: temp,
            source,
            remote,
            runner_temp,
            shims,
            probe_source,
            source_commit,
            version,
            host_target,
        }
    }

    fn package_dir(&self, relative: &str) -> PathBuf {
        self.source.join(relative)
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "integration harness synchronously captures the real producer subprocess"
    )]
    fn run_producer(
        &self,
        attempt: &str,
        relative: &str,
        tamper_on_sign: bool,
        tamper_on_final_verify: bool,
    ) -> Output {
        let package_dir = self.package_dir(relative);
        fs::create_dir_all(&package_dir).unwrap();
        assert!(fs::read_dir(&package_dir).unwrap().next().is_none());

        let scratch = self
            .runner_temp
            .join(format!("package-release-scratch-{RUN_ID}-{attempt}"));
        assert!(!scratch.exists(), "fixture scratch must start absent");

        let task = producer_script();
        Command::new("bash")
            .arg("-c")
            .arg(task)
            .current_dir(&self.source)
            .env("PATH", prepend_path(&self.shims))
            .env("JACKIN_XTASK_BIN", env!("CARGO_BIN_EXE_jackin-xtask"))
            .env("JACKIN_HOST_TARGET", self.host_target)
            .env("JACKIN_VERSION_PROBE_SOURCE", &self.probe_source)
            .env("VELNOR_SOURCE_COMMIT", &self.source_commit)
            .env("VELNOR_SOURCE_REF", SOURCE_REF)
            .env("VELNOR_VERIFIED_PACKAGE_DIR", &package_dir)
            .env("RUNNER_TEMP", &self.runner_temp)
            .env("GITHUB_RUN_ID", RUN_ID)
            .env("GITHUB_RUN_ATTEMPT", attempt)
            .env("GITHUB_WORKSPACE", &self.source)
            .env("PACKAGE_DIR", relative)
            .env("PACKAGE_RELEASE_SCRATCH_DIR", scratch)
            .env(
                "FIXTURE_TAMPER_ON_SIGN",
                if tamper_on_sign { "1" } else { "0" },
            )
            .env(
                "FIXTURE_TAMPER_HANDOFF_ON_FINAL_VERIFY",
                if tamper_on_final_verify { "1" } else { "0" },
            )
            .output()
            .unwrap()
    }

    #[expect(
        clippy::disallowed_methods,
        reason = "integration harness synchronously captures the real verifier subprocess"
    )]
    fn run_verifier(&self, package_dir: &Path) -> Output {
        Command::new(env!("CARGO_BIN_EXE_jackin-xtask"))
            .args(["release-verify-package"])
            .current_dir(&self.source)
            .env("PATH", prepend_path(&self.shims))
            .env("VELNOR_VERIFIED_PACKAGE_DIR", package_dir)
            .env("VELNOR_SOURCE_CHECKOUT_DIR", &self.source)
            .output()
            .unwrap()
    }

    fn scratch_dir(&self, attempt: &str) -> PathBuf {
        self.runner_temp
            .join(format!("package-release-scratch-{RUN_ID}-{attempt}"))
    }
}

#[test]
fn output_survives_task_exit() {
    let fixture = Fixture::new(false);
    let handoff = fixture.package_dir(PACKAGE_RELATIVE);
    let output = fixture.run_producer("1", PACKAGE_RELATIVE, false, false);

    assert_success(&output, "release-preview-package");
    assert!(
        handoff.is_dir(),
        "declared generator handoff must survive producer exit"
    );
    assert_eq!(
        file_names(&handoff),
        expected_package_names(),
        "handoff must contain exactly six payloads, 21 supporting assets, and two metadata files"
    );
    assert!(
        !fixture.scratch_dir("1").exists(),
        "only uniquely owned scratch must be removed on exit"
    );
    assert!(
        !handoff.starts_with(&fixture.runner_temp),
        "handoff must remain at the generator-declared path"
    );
}

#[test]
fn admitted_old_sha_packages_correctly() {
    let fixture = Fixture::new(true);
    let admitted = &fixture.source_commit;
    let remote = fixture.remote.to_string_lossy();
    let tip = git(&fixture.source, &["ls-remote", &remote, "refs/heads/main"])
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned();
    assert_ne!(admitted, &tip, "fixture must represent a queued older SHA");
    git(
        &fixture.source,
        &["merge-base", "--is-ancestor", admitted, &tip],
    );
    assert_eq!(git(&fixture.source, &["rev-parse", "HEAD"]), *admitted);
    assert!(
        git(&fixture.source, &["status", "--porcelain=v1"]).is_empty(),
        "old admitted checkout must remain clean"
    );

    let output = fixture.run_producer("1", PACKAGE_RELATIVE, false, false);
    assert_success(&output, "old admitted source SHA");

    let manifest = read_json(
        &fixture
            .package_dir(PACKAGE_RELATIVE)
            .join("release-manifest.json"),
    );
    assert_eq!(manifest["source_commit"], *admitted);
    assert_eq!(manifest["source_ref"], SOURCE_REF);
    assert_eq!(manifest["source_repository"], SOURCE_REPOSITORY);
    assert_eq!(manifest["version"], fixture.version);
}

#[test]
fn all_six_payloads_have_expected_members() {
    let fixture = Fixture::new(false);
    let output = fixture.run_producer("1", PACKAGE_RELATIVE, false, false);
    assert_success(&output, "release-preview-package");

    let package = fixture.package_dir(PACKAGE_RELATIVE);
    for name in PAYLOADS {
        let expected: &[&str] = if name.starts_with("jackin-capsule-") {
            &[CAPSULE_BINARY]
        } else {
            &CLI_BINARIES
        };
        let members = archive_members(&package.join(name));
        let actual = members
            .iter()
            .map(|member| member.name.as_str())
            .collect::<BTreeSet<_>>();
        let expected_names = expected.iter().copied().collect::<BTreeSet<_>>();
        assert_eq!(actual, expected_names, "{name} binary members");
        assert_eq!(
            members.len(),
            expected.len(),
            "{name} must not contain duplicate or extra members"
        );

        let target = target_for_payload(name);
        for member in members {
            assert!(
                member.is_file,
                "{name}: {} must be a regular file",
                member.name
            );
            assert_ne!(
                member.mode & 0o111,
                0,
                "{name}: {} must remain executable",
                member.name
            );
            assert_binary_target(&member.bytes, target, name);
        }
    }
}

#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "test executes the extracted host probe and captures its version output"
)]
fn versions_and_manifests_match_source() {
    let fixture = Fixture::new(false);
    let first = fixture.run_producer("1", PACKAGE_RELATIVE, false, false);
    assert_success(&first, "first source-derived package");

    let package = fixture.package_dir(PACKAGE_RELATIVE);
    let manifest = read_json(&package.join("release-manifest.json"));
    let identity = read_json(&package.join("identity.json"));
    assert_eq!(manifest["schema"], "velnor.package-release.v1");
    assert_eq!(manifest["source_commit"], fixture.source_commit);
    assert_eq!(manifest["source_ref"], SOURCE_REF);
    assert_eq!(manifest["source_repository"], SOURCE_REPOSITORY);
    assert_eq!(manifest["version"], fixture.version);
    assert_eq!(identity["source_digest"], fixture.source_commit);
    assert_eq!(identity["source_ref"], SOURCE_REF);
    assert_eq!(identity["source_repository"], SOURCE_REPOSITORY);
    assert_eq!(identity["manifest"], manifest);

    let payload_digests = assert_asset_digests(&package, &manifest["assets"], &PAYLOADS);
    let support_names = expected_support_names().into_iter().collect::<Vec<_>>();
    let support_name_refs = support_names.iter().map(String::as_str).collect::<Vec<_>>();
    let supporting_digests =
        assert_asset_digests(&package, &manifest["supporting_assets"], &support_name_refs);

    let sums = fs::read_to_string(package.join("SHA256SUMS")).unwrap();
    let sum_rows = sums
        .lines()
        .map(|line| {
            let mut fields = line.split_whitespace();
            (
                fields.next().unwrap().to_owned(),
                fields.next().unwrap().to_owned(),
                fields.next().is_none(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(sum_rows.len(), PAYLOADS.len());
    for (digest, name, has_no_extra_field) in sum_rows {
        assert!(has_no_extra_field, "SHA256SUMS row must have two fields");
        assert_eq!(digest, payload_digests[&name], "SHA256SUMS {name}");
    }

    let capsule_manifest = read_json(&package.join("capsule-manifest.json"));
    assert_eq!(capsule_manifest["version"], fixture.version);
    let capsule_targets = capsule_manifest["targets"].as_object().unwrap();
    assert_eq!(capsule_targets.len(), 2);
    assert_eq!(
        capsule_targets["aarch64-unknown-linux-gnu"],
        payload_digests["jackin-capsule-aarch64-unknown-linux-gnu.tar.gz"]
    );
    assert_eq!(
        capsule_targets["x86_64-unknown-linux-gnu"],
        payload_digests["jackin-capsule-x86_64-unknown-linux-gnu.tar.gz"]
    );
    assert!(supporting_digests.contains_key("SHA256SUMS"));
    assert!(supporting_digests.contains_key("capsule-manifest.json"));

    let host_payload = payload_for_target(fixture.host_target);
    let host_archive = archive_members(&package.join(host_payload));
    for binary in if host_payload.starts_with("jackin-capsule-") {
        vec![CAPSULE_BINARY]
    } else {
        CLI_BINARIES.to_vec()
    } {
        let member = host_archive
            .iter()
            .find(|member| member.name == binary)
            .unwrap();
        let executable = executable_fixture(package.join(binary), &member.bytes);
        let output = Command::new(executable).arg("--version").output().unwrap();
        assert_success(&output, "extracted host version probe");
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!("{binary} {}\n", fixture.version)
        );
    }

    // Attempt identity owns scratch only. Same admitted SHA must retain the
    // same package version and manifest identity on retry.
    let retry_path = "out/homebrew-preview-retry";
    let retry = fixture.run_producer("2", retry_path, false, false);
    assert_success(&retry, "same-SHA retry");
    let retry_manifest = read_json(
        &fixture
            .package_dir(retry_path)
            .join("release-manifest.json"),
    );
    assert_eq!(retry_manifest["version"], fixture.version);
    assert_eq!(retry_manifest["source_commit"], fixture.source_commit);
    assert!(
        !fixture.scratch_dir("2").exists(),
        "retry scratch must also be removed after task exit"
    );
}

#[test]
fn partial_or_tampered_package_fails() {
    // Corrupt a staged archive after the real xtask writes its SHA. The real
    // verifier must reject it before promotion; the declared handoff stays empty.
    let failed = Fixture::new(false);
    let failed_handoff = failed.package_dir(PACKAGE_RELATIVE);
    let staged_failure = failed.run_producer("1", PACKAGE_RELATIVE, true, false);
    assert!(
        !staged_failure.status.success(),
        "checksum-detected staging tamper must fail before promotion:\n{}",
        output_text(&staged_failure)
    );
    assert!(
        fs::read_dir(&failed_handoff).unwrap().next().is_none(),
        "failed staged output must not contaminate the declared handoff"
    );
    assert!(
        !failed.scratch_dir("1").exists(),
        "failed staged output must clean only its owned scratch"
    );

    // Corrupt the handoff immediately before its final package verification,
    // after promotion has copied files. The producer must roll those files
    // back and remove its owned scratch after that later failure.
    let failed_after_copy = Fixture::new(false);
    let failed_after_copy_handoff = failed_after_copy.package_dir(PACKAGE_RELATIVE);
    let after_copy_failure = failed_after_copy.run_producer("1", PACKAGE_RELATIVE, false, true);
    assert_failure_contains(&after_copy_failure, "payload checksum mismatch");
    assert!(
        fs::read_dir(&failed_after_copy_handoff)
            .unwrap()
            .next()
            .is_none(),
        "final verification failure must remove every file copied into the handoff"
    );
    assert!(
        !failed_after_copy.scratch_dir("1").exists(),
        "final verification failure must remove its owned scratch"
    );

    let fixture = Fixture::new(false);
    let produced = fixture.run_producer("1", PACKAGE_RELATIVE, false, false);
    assert_success(&produced, "complete package before negative controls");
    let source_package = fixture.package_dir(PACKAGE_RELATIVE);
    assert_success(
        &fixture.run_verifier(&source_package),
        "baseline package verifier",
    );

    let partial = fixture.package_dir("out/homebrew-preview-partial");
    copy_files(&source_package, &partial);
    fs::remove_file(partial.join("jackin-capsule-x86_64-unknown-linux-gnu.tar.gz.bundle")).unwrap();
    let missing_result = fixture.run_verifier(&partial);
    assert_failure_contains(&missing_result, "file set mismatch");

    let tampered = fixture.package_dir("out/homebrew-preview-tampered");
    copy_files(&source_package, &tampered);
    let payload = tampered.join("jackin-x86_64-unknown-linux-gnu.tar.gz");
    let mut payload_bytes = fs::read(&payload).unwrap();
    payload_bytes.extend_from_slice(b"tampered after manifest");
    fs::write(&payload, payload_bytes).unwrap();
    let tampered_result = fixture.run_verifier(&tampered);
    assert_failure_contains(&tampered_result, "payload checksum mismatch");
}

#[test]
fn desktop_release_steps_remain_defined() {
    let root = repo_root();
    let current_mise = fs::read_to_string(root.join("mise.toml")).unwrap();
    let current_tasks = desktop_tasks(&current_mise);

    let release = fs::read_to_string(root.join(".github/workflows/release.yml")).unwrap();
    assert_ordered(
        &release,
        &[
            "mise run desktop-release-tools",
            "mise run desktop-release-env",
            "mise run desktop-build",
            "mise run desktop-verify",
            "mise run desktop-release-state",
            "mise run desktop-sign-notarize",
        ],
    );
    assert!(
        current_tasks.contains_key("desktop-sign-notarize"),
        "Developer ID sign/notarize task must remain present"
    );
    assert!(
        current_tasks.contains_key("desktop-release-state"),
        "desktop publication state task must remain present"
    );
}

struct ArchiveMember {
    name: String,
    is_file: bool,
    mode: u32,
    bytes: Vec<u8>,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn producer_script() -> String {
    let text = fs::read_to_string(repo_root().join("mise.toml")).unwrap();
    let root: toml::Value = toml::from_str(&text).unwrap();
    let script = root
        .get("tasks")
        .and_then(|value| value.get("release-preview-package"))
        .and_then(|value| value.get("run"))
        .and_then(toml::Value::as_str)
        .unwrap_or_default();
    assert!(
        !script.is_empty(),
        "mise.toml must define the real release-preview-package run script"
    );
    script.to_owned()
}

fn write_shims(directory: &Path) {
    write_executable(
        &directory.join("cargo"),
        r#"#!/bin/bash
set -euo pipefail
if [[ "${1:-}" == "xtask" ]]; then
  shift
  if [[ "${1:-}" == "release-verify-package" &&
        "${FIXTURE_TAMPER_HANDOFF_ON_FINAL_VERIFY:-0}" == "1" &&
        "${VELNOR_VERIFIED_PACKAGE_DIR:-}" != */verified-package ]]; then
    printf 'TASK-008 deterministic post-copy fixture tamper\n' >> \
      "$VELNOR_VERIFIED_PACKAGE_DIR/jackin-aarch64-apple-darwin.tar.gz"
  fi
  exec "$JACKIN_XTASK_BIN" "$@"
fi
if [[ "${1:-}" != "zigbuild" ]]; then
  echo "unexpected cargo fixture invocation: $*" >&2
  exit 97
fi
shift
target=""
package="jackin"
while (($#)); do
  case "$1" in
    --target) target="$2"; shift 2 ;;
    -p|--package) package="$2"; shift 2 ;;
    *) shift ;;
  esac
done
[[ -n "$target" ]]
rust_target="${target%%.2.17}"
case "$package" in
  jackin) binaries=(jackin jackin-role) ;;
  jackin-capsule) binaries=(jackin-capsule) ;;
  *) echo "unexpected package in cargo fixture: $package" >&2; exit 98 ;;
esac
release_dir="$CARGO_TARGET_DIR/$rust_target/release"
mkdir -p "$release_dir"
for binary in "${binaries[@]}"; do
  output="$release_dir/$binary"
  if [[ "$rust_target" == "$JACKIN_HOST_TARGET" ]]; then
    env -u RUSTFLAGS -u CARGO_ENCODED_RUSTFLAGS -u CARGO_BUILD_TARGET \
      -u SDKROOT -u MACOSX_DEPLOYMENT_TARGET \
      rustc --edition=2024 --target "$JACKIN_HOST_TARGET" "$JACKIN_VERSION_PROBE_SOURCE" -o "$output"
  else
    case "$rust_target" in
      aarch64-apple-darwin) printf '\xCF\xFA\xED\xFE\x0C\x00\x00\x01' > "$output" ;;
      x86_64-apple-darwin) printf '\xCF\xFA\xED\xFE\x07\x00\x00\x01' > "$output" ;;
      aarch64-unknown-linux-gnu)
        printf '\x7FELF\x02\x01\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x02\x00\xB7\x00' > "$output" ;;
      x86_64-unknown-linux-gnu)
        printf '\x7FELF\x02\x01\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x02\x00\x3E\x00' > "$output" ;;
      *) echo "unexpected target in cargo fixture: $target" >&2; exit 99 ;;
    esac
    printf ' TASK-008 unsigned cross-build format fixture; version=%s\n' "$JACKIN_VERSION_OVERRIDE" >> "$output"
  fi
  chmod 0755 "$output"
done
"#,
    );

    // Target installation is external toolchain setup only; no target binary
    // is produced by this shim. The cargo build shim above writes explicit
    // non-runnable cross-target fixtures.
    write_executable(&directory.join("rustup"), "#!/bin/sh\nexit 0\n");

    // Production asks GNU tar to normalize archive metadata. macOS supplies
    // bsdtar instead; strip only GNU-specific reproducibility switches while
    // forwarding archive creation/listing to the host tar binary.
    write_executable(
        &directory.join("tar"),
        r#"#!/bin/bash
set -euo pipefail
export COPYFILE_DISABLE=1
args=()
for arg in "$@"; do
  case "$arg" in
    --sort=name|--mtime=@0|--owner=0|--group=0|--numeric-owner) ;;
    *) args+=("$arg") ;;
  esac
done
exec /usr/bin/tar "${args[@]}"
"#,
    );

    // Local unsigned fixture bundles let real checksum/SBOM/package verifiers
    // exercise their full paths without making an OIDC claim.
    write_executable(
        &directory.join("cosign"),
        r#"#!/bin/bash
set -euo pipefail
mode="${1:-}"
shift || true
bundle=""
archive=""
while (($#)); do
  case "$1" in
    --bundle) bundle="$2"; shift 2 ;;
    --yes) shift ;;
    *) archive="$1"; shift ;;
  esac
done
case "$mode" in
  sign-blob)
    [[ -n "$bundle" && -n "$archive" ]]
    printf 'TASK-008 UNSIGNED LOCAL COSIGN FIXTURE; not a signature or publication proof\n' > "$bundle"
    if [[ "${FIXTURE_TAMPER_ON_SIGN:-0}" == "1" && "$archive" == *jackin-aarch64-apple-darwin.tar.gz ]]; then
      printf 'tampered after real checksum generation\n' >> "$archive"
    fi
    ;;
  verify-blob)
    [[ -n "$bundle" && -s "$bundle" ]]
    grep -Fq 'TASK-008 UNSIGNED LOCAL COSIGN FIXTURE' "$bundle"
    ;;
  *) echo "unexpected local cosign fixture operation: $mode" >&2; exit 100 ;;
esac
"#,
    );

    // Syft supplies scanner input only. release_sbom binds and validates this
    // CycloneDX document against the actual archive bytes.
    write_executable(
        &directory.join("syft"),
        r#"#!/bin/sh
set -eu
cat <<'JSON'
{"bomFormat":"CycloneDX","specVersion":"1.5","serialNumber":"urn:uuid:00000000-0000-0000-0000-000000000008","metadata":{"component":{"type":"file","name":"fixture-input"}},"components":[{"type":"library","name":"fixture-scan-input"}]}
JSON
"#,
    );
}

fn write_executable(path: &Path, content: &str) {
    fs::write(path, content).unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

fn prepend_path(shims: &Path) -> std::ffi::OsString {
    let mut paths = vec![shims.to_owned()];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
    std::env::join_paths(paths).unwrap()
}

#[expect(
    clippy::disallowed_methods,
    reason = "local Git fixtures synchronously capture repository command output"
)]
fn git(directory: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(args)
        .output()
        .unwrap();
    assert_success(&output, &format!("git {}", args.join(" ")));
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn desktop_tasks(mise: &str) -> BTreeMap<String, toml::Value> {
    let document: toml::Value = toml::from_str(mise).unwrap();
    document["tasks"]
        .as_table()
        .unwrap()
        .iter()
        .filter(|(name, _)| *name == "desktop" || name.starts_with("desktop-"))
        .map(|(name, task)| (name.clone(), task.clone()))
        .collect()
}

fn assert_ordered(text: &str, needles: &[&str]) {
    let mut rest = text;
    for needle in needles {
        let found = rest.find(needle);
        assert!(
            found.is_some(),
            "missing or out-of-order release step: {needle}"
        );
        let found = found.unwrap_or_default();
        rest = &rest[found + needle.len()..];
    }
}

fn assert_success(output: &Output, label: &str) {
    assert!(
        output.status.success(),
        "{label} failed with {}.\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_failure_contains(output: &Output, expected: &str) {
    assert!(
        !output.status.success(),
        "unexpected success:\n{}",
        output_text(output)
    );
    assert!(
        output_text(output).contains(expected),
        "failure must identify {expected:?}:\n{}",
        output_text(output)
    );
}

fn output_text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[expect(
    clippy::panic,
    reason = "fixture only supports four explicit native runner triples"
)]
fn host_target() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        other => panic!("unsupported release-fixture host {other:?}"),
    }
}

#[expect(
    clippy::panic,
    reason = "unknown target in a successful package is a broken integration fixture"
)]
fn payload_for_target(target: &str) -> &'static str {
    match target {
        "aarch64-apple-darwin" => PAYLOADS[0],
        "x86_64-apple-darwin" => PAYLOADS[1],
        "aarch64-unknown-linux-gnu" => PAYLOADS[2],
        "x86_64-unknown-linux-gnu" => PAYLOADS[3],
        other => panic!("unknown release target {other}"),
    }
}

#[expect(
    clippy::panic,
    reason = "unexpected archive payload name is a test failure"
)]
fn target_for_payload(name: &str) -> &'static str {
    match name {
        "jackin-aarch64-apple-darwin.tar.gz" => "aarch64-apple-darwin",
        "jackin-x86_64-apple-darwin.tar.gz" => "x86_64-apple-darwin",
        "jackin-aarch64-unknown-linux-gnu.tar.gz"
        | "jackin-capsule-aarch64-unknown-linux-gnu.tar.gz" => "aarch64-unknown-linux-gnu",
        "jackin-x86_64-unknown-linux-gnu.tar.gz"
        | "jackin-capsule-x86_64-unknown-linux-gnu.tar.gz" => "x86_64-unknown-linux-gnu",
        other => panic!("unknown preview payload {other}"),
    }
}

fn assert_binary_target(bytes: &[u8], target: &str, archive: &str) {
    if target.ends_with("-apple-darwin") {
        assert!(
            matches!(
                bytes.get(..4),
                Some([0xcf, 0xfa, 0xed, 0xfe] | [0xfe, 0xed, 0xfa, 0xcf])
            ),
            "{archive} must contain a 64-bit Mach-O member for {target}"
        );
        assert!(bytes.len() >= 8, "{archive} Mach-O header must be complete");
        let cpu = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        let expected = if target.starts_with("aarch64-") {
            0x0100_000c
        } else {
            0x0100_0007
        };
        assert_eq!(cpu, expected, "{archive} Mach-O CPU type");
    } else {
        assert!(bytes.starts_with(b"\x7fELF"), "{archive} must be ELF");
        assert!(bytes.len() >= 20, "{archive} ELF header must be complete");
        assert_eq!(bytes[4], 2, "{archive} must be 64-bit ELF");
        assert_eq!(bytes[5], 1, "{archive} ELF byte order");
        let machine = u16::from_le_bytes([bytes[18], bytes[19]]);
        let expected = if target.starts_with("aarch64-") {
            183
        } else {
            62
        };
        assert_eq!(machine, expected, "{archive} ELF machine");
    }
}

fn archive_members(path: &Path) -> Vec<ArchiveMember> {
    let file = Cursor::new(fs::read(path).unwrap());
    let decoder = GzDecoder::new(file);
    let mut archive = Archive::new(decoder);
    archive
        .entries()
        .unwrap()
        .map(|entry| {
            let mut entry = entry.unwrap();
            let name = entry.path().unwrap().to_string_lossy().into_owned();
            let is_file = entry.header().entry_type().is_file();
            let mode = entry.header().mode().unwrap();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).unwrap();
            ArchiveMember {
                name,
                is_file,
                mode,
                bytes,
            }
        })
        .collect()
}

fn expected_package_names() -> BTreeSet<String> {
    let mut names = PAYLOADS
        .iter()
        .map(|name| (*name).to_owned())
        .collect::<BTreeSet<_>>();
    names.extend(expected_support_names());
    names.insert("release-manifest.json".to_owned());
    names.insert("identity.json".to_owned());
    names
}

fn expected_support_names() -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for payload in PAYLOADS {
        names.insert(format!("{payload}.sha256"));
        names.insert(format!("{payload}.bundle"));
        names.insert(format!("{payload}.sbom.json"));
    }
    names.insert("SHA256SUMS".to_owned());
    names.insert("capsule-manifest.json".to_owned());
    names.insert("capsule-manifest.json.bundle".to_owned());
    names
}

fn file_names(directory: &Path) -> BTreeSet<String> {
    fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect()
}

fn assert_asset_digests(
    package: &Path,
    rows: &Value,
    expected_names: &[&str],
) -> BTreeMap<String, String> {
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), expected_names.len());
    let mut actual = BTreeMap::new();
    for row in rows {
        let name = row["name"].as_str().unwrap().to_owned();
        let digest = row["sha256"].as_str().unwrap().to_owned();
        assert!(
            expected_names.contains(&name.as_str()),
            "unexpected asset {name}"
        );
        assert_eq!(digest.len(), 64, "SHA256 length for {name}");
        assert!(
            digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()),
            "SHA256 must be lowercase hexadecimal for {name}"
        );
        let bytes = fs::read(package.join(&name)).unwrap();
        assert_eq!(digest, sha256(&bytes), "manifest digest for {name}");
        assert!(
            actual.insert(name.clone(), digest).is_none(),
            "duplicate manifest asset {name}"
        );
    }
    assert_eq!(
        actual.keys().map(String::as_str).collect::<BTreeSet<_>>(),
        expected_names.iter().copied().collect(),
        "manifest must name the exact required asset set"
    );
    actual
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn executable_fixture(path: PathBuf, bytes: &[u8]) -> PathBuf {
    fs::write(&path, bytes).unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).unwrap();
    path
}

fn copy_files(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        assert!(entry.file_type().unwrap().is_file());
        fs::copy(entry.path(), destination.join(entry.file_name())).unwrap();
    }
}
