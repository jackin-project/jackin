/// Download, cache, and verify the `jackin-container` binary.
///
/// `jackin-container` runs as PID 1 inside every jackin-managed container.
/// It is published to GitHub Releases alongside the jackin CLI:
///   - Dev builds: `preview` rolling release tag
///   - Stable releases: `v<version>` tag
///
/// The binary is cached locally at:
///   `~/.jackin/cache/jackin-container/<version>/linux-<arch>/jackin-container`
///
/// Cache key is the full jackin version string (e.g. `0.6.0-dev+bf7df07`).
/// When jackin is upgraded, the version changes → cache miss → re-download.
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::paths::JackinPaths;

/// The version of jackin-container required by this build of jackin.
/// Must match `JACKIN_VERSION` exactly so that `jackin-container --version`
/// output and the cache key agree.
pub const REQUIRED_VERSION: &str = env!("JACKIN_VERSION");

/// Asset name prefix used in GitHub Releases.
const ASSET_PREFIX: &str = "jackin-container";

/// Ensure the `jackin-container` binary for the current version is available
/// in the local cache, downloading it if necessary.
///
/// Returns the path to the cached binary ready to copy into the derived image
/// build context.
pub async fn ensure_available(paths: &JackinPaths) -> Result<PathBuf> {
    let arch = container_arch();
    let cached = cached_binary_path(&paths.cache_dir, REQUIRED_VERSION, arch);

    if is_valid_cached_binary(&cached) {
        return Ok(cached);
    }

    download_and_cache(paths, REQUIRED_VERSION, arch, &cached).await?;
    Ok(cached)
}

/// Path in the local cache for a given version + arch.
pub fn cached_binary_path(cache_dir: &Path, version: &str, arch: &str) -> PathBuf {
    // Sanitize the version for use as a directory name: replace '+' with '_'
    // so shells and filesystems that dislike '+' don't choke.
    let safe_version = version.replace('+', "_");
    cache_dir
        .join("jackin-container")
        .join(safe_version)
        .join(format!("linux-{arch}"))
        .join("jackin-container")
}

/// Determine the Linux arch string for the container target.
/// Jackin always targets Linux containers regardless of the host OS.
/// The host arch determines whether to pull the amd64 or arm64 binary.
pub fn container_arch() -> &'static str {
    // `target_arch` is the host machine arch (where jackin CLI runs).
    // Containers built on Apple M-series or Linux arm64 use arm64 images;
    // x86_64 hosts use amd64.
    if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "amd64"
    }
}

/// Returns true if the cached binary exists and is executable.
fn is_valid_cached_binary(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    path.is_file()
        && path
            .metadata()
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

async fn download_and_cache(
    _paths: &JackinPaths,
    version: &str,
    arch: &str,
    dest: &Path,
) -> Result<()> {
    let url = download_url(version, arch);

    eprintln!(
        "[jackin] downloading jackin-container {version} for linux/{arch}..."
    );

    // Create cache directory.
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create cache dir {}", parent.display()))?;
    }

    // Download via curl (available on macOS and most Linux hosts).
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
        // Clean up partial download.
        let _ = std::fs::remove_file(&tmp);
        anyhow::bail!(
            "jackin-container binary for version {version} not found at the preview release.\n\
             This usually means:\n\
             • The CI build for this commit hasn't finished yet — wait a minute and retry.\n\
             • Your local jackin build is ahead of the latest published preview.\n\n\
             Check: https://github.com/jackin-project/jackin/releases/tag/preview\n\
             Or upgrade jackin to a version with a published preview build."
        );
    }

    // Make executable and move into place atomically.
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mut perms = std::fs::metadata(&tmp)
            .context("failed to read tmp binary metadata")?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&tmp, perms)
            .context("failed to mark jackin-container binary as executable")?;
    }
    std::fs::rename(&tmp, dest)
        .with_context(|| format!("failed to move jackin-container binary to {}", dest.display()))?;

    // Verify the downloaded binary reports the expected version.
    verify_version(dest, version)?;

    eprintln!("[jackin] jackin-container {version} cached at {}", dest.display());
    Ok(())
}

/// Build the GitHub Release download URL for the given version + arch.
fn download_url(version: &str, arch: &str) -> String {
    // linux arch → release asset suffix
    let target = match arch {
        "arm64" => "aarch64-unknown-linux-gnu",
        _ => "x86_64-unknown-linux-gnu",
    };
    let asset = format!("{ASSET_PREFIX}-{target}");

    if version.contains("-dev") || version.contains("-preview.") {
        // Dev and preview builds live under the rolling `preview` tag.
        format!(
            "https://github.com/jackin-project/jackin/releases/download/preview/{asset}"
        )
    } else {
        // Stable releases use a versioned tag.
        format!(
            "https://github.com/jackin-project/jackin/releases/download/v{version}/{asset}"
        )
    }
}

/// Run `jackin-container --version` and verify the output contains the expected version.
fn verify_version(binary: &Path, expected: &str) -> Result<()> {
    let output = std::process::Command::new(binary)
        .arg("--version")
        .output()
        .context("failed to run jackin-container --version for verification")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.contains(expected) {
        anyhow::bail!(
            "downloaded jackin-container reports version {:?} but expected version {expected}.\n\
             The preview release may not yet contain the build for this commit.\n\
             Delete the cached binary and retry after the CI build completes:\n  rm -f {}",
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
}
