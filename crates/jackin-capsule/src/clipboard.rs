//! In-container clipboard asset staging for host-affordance bridge frames.
//!
//! The host may claim an image format, but the Capsule validates magic bytes
//! before writing a container-readable path under jackin's runtime root.

use std::fs::{OpenOptions, create_dir_all, set_permissions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use jackin_protocol::attach::{ClipboardImage, ClipboardImageFormat};

pub(crate) const CLIPBOARD_RUN_DIR: &str = "/jackin/run/clipboard";

pub(crate) fn stage_clipboard_image(image: &ClipboardImage) -> Result<PathBuf> {
    stage_clipboard_image_at(Path::new(CLIPBOARD_RUN_DIR), image)
}

fn stage_clipboard_image_at(root: &Path, image: &ClipboardImage) -> Result<PathBuf> {
    validate_image_magic(image)?;
    create_dir_all(root).with_context(|| format!("creating {}", root.display()))?;
    set_permissions(root, std::fs::Permissions::from_mode(0o700))
        .with_context(|| format!("setting permissions on {}", root.display()))?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    for attempt in 0..16u8 {
        let path = root.join(format!(
            "clipboard-{now}-{}-{attempt}.{}",
            std::process::id(),
            image.format.extension()
        ));
        match OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(mut file) => {
                file.write_all(&image.bytes)
                    .with_context(|| format!("writing {}", path.display()))?;
                file.flush()
                    .with_context(|| format!("flushing {}", path.display()))?;
                return Ok(path);
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err).with_context(|| format!("opening {}", path.display())),
        }
    }
    bail!(
        "could not allocate unique clipboard image path under {}",
        root.display()
    );
}

fn validate_image_magic(image: &ClipboardImage) -> Result<()> {
    if image.bytes.is_empty() {
        bail!("clipboard image payload is empty");
    }
    let ok = match image.format {
        ClipboardImageFormat::Png => image.bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        ClipboardImageFormat::Jpeg => image.bytes.starts_with(&[0xff, 0xd8, 0xff]),
        ClipboardImageFormat::Gif => {
            image.bytes.starts_with(b"GIF87a") || image.bytes.starts_with(b"GIF89a")
        }
        ClipboardImageFormat::Webp => {
            image.bytes.len() >= 12
                && image.bytes.starts_with(b"RIFF")
                && &image.bytes[8..12] == b"WEBP"
        }
        ClipboardImageFormat::Tiff => {
            image.bytes.starts_with(b"MM\0*") || image.bytes.starts_with(b"II*\0")
        }
    };
    if !ok {
        bail!(
            "clipboard image magic bytes do not match {:?}",
            image.format
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_png_with_private_permissions() {
        let temp = tempfile::tempdir().unwrap();
        let image = ClipboardImage {
            format: ClipboardImageFormat::Png,
            bytes: b"\x89PNG\r\n\x1a\npayload".to_vec(),
        };

        let path = stage_clipboard_image_at(temp.path(), &image).unwrap();
        assert_eq!(path.extension().and_then(|ext| ext.to_str()), Some("png"));
        assert_eq!(std::fs::read(&path).unwrap(), image.bytes);
        assert_eq!(
            std::fs::metadata(temp.path()).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn rejects_mismatched_magic() {
        let temp = tempfile::tempdir().unwrap();
        let image = ClipboardImage {
            format: ClipboardImageFormat::Png,
            bytes: b"not a png".to_vec(),
        };

        let err = stage_clipboard_image_at(temp.path(), &image).unwrap_err();
        assert!(format!("{err:#}").contains("magic bytes"));
    }

    #[test]
    fn accepts_browser_tiff_magic() {
        let temp = tempfile::tempdir().unwrap();
        let image = ClipboardImage {
            format: ClipboardImageFormat::Tiff,
            bytes: b"MM\0*payload".to_vec(),
        };

        let path = stage_clipboard_image_at(temp.path(), &image).unwrap();
        assert_eq!(path.extension().and_then(|ext| ext.to_str()), Some("tiff"));
    }
}
