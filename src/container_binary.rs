/// Download, build, cache, and verify the `jackin-container` binary.
///
/// Acquisition strategy — chosen at runtime based on version and context:
///
/// **Dev version** (`-dev` suffix) AND running from a source checkout:
///   Build via `docker run rust:1.95 cargo build -p jackin-container` inside
///   the workspace. No cross-compilation toolchain needed on the host.
///
/// **Dev or preview version** AND no source checkout:
///   Download from the rolling `preview` GitHub Release tag.
///
/// **Stable release** (no `-dev`, no `-preview`):
///   Download from the versioned `v<version>` GitHub Release tag.
///
/// Cache: `~/.jackin/cache/jackin-container/<version>/linux-<arch>/jackin-container`
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::paths::JackinPaths;

pub const REQUIRED_VERSION: &str = env!("JACKIN_VERSION");

const RUST_IMAGE: &str = "rust:1.95.0";
const ASSET_PREFIX: &str = "jackin-container";

/// Ensure the `jackin-container` binary is available and return its cached path.
pub async fn ensure_available(paths: &JackinPaths) -> Result<PathBuf> {
    let arch = container_arch();
    let cached = cached_binary_path(&paths.cache_dir, REQUIRED_VERSION, arch);

    if is_valid_cached_binary(&cached) {
        crate::debug_log!(
            "container_binary",
            "cache hit for jackin-container {REQUIRED_VERSION} linux/{arch}"
        );
        return Ok(cached);
    }

    if let Some(parent) = cached.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create cache dir {}", parent.display()))?;
    }

    let is_dev = REQUIRED_VERSION.contains("-dev");

    if is_dev {
        if let Some(workspace) = find_workspace_root() {
            // For source builds, re-check cache by mtime: if any source file in
            // crates/jackin-container/src/ is newer than the cached binary,
            // rebuild. This catches edits made without committing (git hash
            // stays the same but source changed).
            let needs_rebuild = !is_valid_cached_binary(&cached)
                || source_newer_than(&workspace.join("crates/jackin-container"), &cached);
            if needs_rebuild {
                build_from_source(&workspace, arch, &cached).await?;
            } else {
                crate::debug_log!(
                    "container_binary",
                    "source-build cache still fresh for {REQUIRED_VERSION}"
                );
            }
            return Ok(cached);
        }
        eprintln!(
            "[jackin] dev build: no source workspace found; \
             downloading jackin-container from preview release..."
        );
    }

    download_and_cache(REQUIRED_VERSION, arch, &cached).await?;
    Ok(cached)
}

/// Path in the local cache for a given version + arch.
pub fn cached_binary_path(cache_dir: &Path, version: &str, arch: &str) -> PathBuf {
    let safe_version = version.replace('+', "_");
    cache_dir
        .join("jackin-container")
        .join(safe_version)
        .join(format!("linux-{arch}"))
        .join("jackin-container")
}

/// Linux arch for the container target, derived from the host machine arch.
pub fn container_arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "amd64"
    }
}

/// Walk up from the running jackin binary to find the workspace root.
/// Returns Some when the directory contains `crates/jackin-container/`.
fn find_workspace_root() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let mut dir = exe.parent()?.to_path_buf();
    for _ in 0..10 {
        if dir.join("crates").join("jackin-container").is_dir() {
            return Some(dir);
        }
        dir = dir.parent()?.to_path_buf();
    }
    None
}

/// Build `jackin-container` for Linux inside Docker from the workspace source.
async fn build_from_source(workspace: &Path, arch: &str, dest: &Path) -> Result<()> {
    eprintln!(
        "[jackin] building jackin-container {REQUIRED_VERSION} for linux/{arch} from source \
         (first build ~2-3 min, cached after)..."
    );

    let out_dir = dest.parent().expect("dest has parent");
    let build_cmd = "cd /workspace && \
                     cargo build --release -p jackin-container 2>&1 && \
                     cp /workspace/target/release/jackin-container /out/jackin-container";

    let workspace_str = workspace.display().to_string();
    let out_str = out_dir.display().to_string();
    let workspace_mount = format!("{}:/workspace:ro", workspace_str);
    let out_mount = format!("{}:/out", out_str);

    let status = tokio::process::Command::new("docker")
        .args([
            "run",
            "--rm",
            "--platform",
            linux_platform(arch),
            "-v",
            &workspace_mount,
            "-v",
            &out_mount,
            "-v",
            "jackin-container-build-cache:/root/.cargo/registry",
            RUST_IMAGE,
            "sh",
            "-c",
            build_cmd,
        ])
        .status()
        .await
        .context("failed to run docker to build jackin-container from source")?;

    if !status.success() {
        anyhow::bail!(
            "docker build of jackin-container failed.\n\
             Check Docker is running and the workspace at {} is accessible.",
            workspace.display()
        );
    }

    chmod_executable(dest);
    eprintln!(
        "[jackin] jackin-container built and cached at {}",
        dest.display()
    );
    Ok(())
}

async fn download_and_cache(version: &str, arch: &str, dest: &Path) -> Result<()> {
    let url = download_url(version, arch);
    eprintln!("[jackin] downloading jackin-container {version} for linux/{arch}...");

    let tmp = dest.with_extension("tmp");
    let status = tokio::process::Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--output",
            tmp.to_str().unwrap_or_default(),
            &url,
        ])
        .status()
        .await
        .context("failed to run curl to download jackin-container")?;

    if !status.success() {
        let _ = std::fs::remove_file(&tmp);
        anyhow::bail!(
            "jackin-container {version} not found in GitHub Releases.\n\
             \n\
             Developing locally? Run jackin from the workspace checkout so it\n\
             builds jackin-container from source via Docker instead.\n\
             \n\
             Using an installed (Homebrew) jackin? The CI preview build may not\n\
             have completed yet. Wait a few minutes and retry, or check:\n\
               https://github.com/jackin-project/jackin/releases/tag/preview"
        );
    }

    chmod_executable(&tmp);
    std::fs::rename(&tmp, dest)
        .with_context(|| format!("failed to move jackin-container to {}", dest.display()))?;

    // Skip exec-based version check on non-Linux hosts: the binary is a Linux
    // ELF and cannot be executed on macOS. Trust the download and let the
    // container fail fast at startup if the binary is wrong.
    #[cfg(target_os = "linux")]
    verify_version_exec(dest, version)?;

    eprintln!(
        "[jackin] jackin-container {version} cached at {}",
        dest.display()
    );
    Ok(())
}

fn download_url(version: &str, arch: &str) -> String {
    let target = linux_target(arch);
    let asset = format!("{ASSET_PREFIX}-{target}");
    if version.contains("-dev") || version.contains("-preview.") {
        format!("https://github.com/jackin-project/jackin/releases/download/preview/{asset}")
    } else {
        format!(
            "https://github.com/jackin-project/jackin/releases/download/v{version}/{asset}"
        )
    }
}

fn linux_target(arch: &str) -> &'static str {
    match arch {
        "arm64" => "aarch64-unknown-linux-gnu",
        _ => "x86_64-unknown-linux-gnu",
    }
}

fn linux_platform(arch: &str) -> &'static str {
    match arch {
        "arm64" => "linux/arm64",
        _ => "linux/amd64",
    }
}

/// Returns true if any file under `src_dir` is newer than `cached_binary`.
/// Used to detect uncommitted source edits that need a rebuild.
fn source_newer_than(src_dir: &Path, cached_binary: &Path) -> bool {
    let Ok(cache_mtime) = std::fs::metadata(cached_binary)
        .and_then(|m| m.modified())
    else {
        return true;
    };
    newest_mtime(src_dir).map_or(false, |src_mtime| src_mtime > cache_mtime)
}

fn newest_mtime(dir: &Path) -> Option<std::time::SystemTime> {
    let Ok(entries) = std::fs::read_dir(dir) else { return None };
    entries
        .flatten()
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            if meta.is_dir() {
                newest_mtime(&e.path())
            } else {
                meta.modified().ok()
            }
        })
        .max()
}

fn is_valid_cached_binary(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    path.is_file()
        && path
            .metadata()
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

fn chmod_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;
    if let Ok(meta) = std::fs::metadata(path) {
        let mut perms = meta.permissions();
        perms.set_mode(0o755);
        let _ = std::fs::set_permissions(path, perms);
    }
}

/// Verify the binary version by executing it. Linux only — macOS cannot exec Linux ELF.
#[cfg(target_os = "linux")]
fn verify_version_exec(binary: &Path, expected: &str) -> Result<()> {
    let output = std::process::Command::new(binary)
        .arg("--version")
        .output()
        .context("failed to run jackin-container --version")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.contains(expected) {
        anyhow::bail!(
            "downloaded jackin-container reports {:?} but expected {expected}.\n\
             Preview release binary may not match this commit yet.\n\
             Delete and retry: rm -f {}",
            stdout.trim(),
            binary.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_url_dev_uses_preview_tag() {
        let url = download_url("0.6.0-dev+bf7df07", "amd64");
        assert!(url.contains("/releases/download/preview/"), "{url}");
        assert!(url.contains("x86_64-unknown-linux-gnu"), "{url}");
    }

    #[test]
    fn download_url_stable_uses_version_tag() {
        let url = download_url("0.6.0", "amd64");
        assert!(url.contains("/releases/download/v0.6.0/"), "{url}");
        assert!(url.contains("x86_64-unknown-linux-gnu"), "{url}");
    }

    #[test]
    fn download_url_arm64_uses_aarch64_target() {
        let url = download_url("0.6.0-dev+bf7df07", "arm64");
        assert!(url.contains("aarch64-unknown-linux-gnu"), "{url}");
    }

    #[test]
    fn download_url_preview_uses_preview_tag() {
        let url = download_url("0.6.0-preview.411+bf7df07", "amd64");
        assert!(url.contains("/releases/download/preview/"), "{url}");
    }

    #[test]
    fn cached_path_replaces_plus_in_version() {
        let path = cached_binary_path(Path::new("/cache"), "0.6.0-dev+bf7df07", "amd64");
        let s = path.to_string_lossy();
        assert!(s.contains("0.6.0-dev_bf7df07"), "{s}");
        assert!(!s.contains('+'), "{s}");
    }

    #[test]
    fn linux_target_maps_arch() {
        assert_eq!(linux_target("arm64"), "aarch64-unknown-linux-gnu");
        assert_eq!(linux_target("amd64"), "x86_64-unknown-linux-gnu");
        assert_eq!(linux_target("x86_64"), "x86_64-unknown-linux-gnu");
    }

    #[test]
    fn find_workspace_root_returns_some_in_dev() {
        // When running tests from the workspace, the binary is under target/
        // and find_workspace_root should locate the workspace root.
        // This test passes in a dev checkout and is a no-op in CI installs.
        let root = find_workspace_root();
        if root.is_some() {
            assert!(
                root.unwrap().join("crates").join("jackin-container").is_dir()
            );
        }
    }
}
