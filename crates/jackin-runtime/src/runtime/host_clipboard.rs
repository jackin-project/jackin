//! Host clipboard readers used by opt-in host attach.
//!
//! This is deliberately host-side. In-container `xclip` only talks to an X11
//! clipboard when DISPLAY/Xauthority/X11 sockets exist; it is not a macOS
//! clipboard bridge.

use std::path::Path;

use anyhow::{Context, Result};
use jackin_protocol::attach::{ClipboardImage, ClipboardImageFormat, MAX_CLIPBOARD_IMAGE_BYTES};

const CTRL_V: u8 = 0x16;

#[must_use]
pub(super) fn is_image_paste_trigger(input: &[u8]) -> bool {
    input == [CTRL_V]
}

pub(super) async fn read_image_for_paste_trigger(input: &[u8]) -> Result<Option<ClipboardImage>> {
    if !is_image_paste_trigger(input) {
        return Ok(None);
    }
    read_host_clipboard_image().await
}

#[cfg(target_os = "macos")]
async fn read_host_clipboard_image() -> Result<Option<ClipboardImage>> {
    tokio::task::spawn_blocking(read_macos_clipboard_image)
        .await
        .map_err(|err| anyhow::anyhow!("joining macOS clipboard image reader: {err}"))?
}

#[cfg(not(target_os = "macos"))]
async fn read_host_clipboard_image() -> Result<Option<ClipboardImage>> {
    Ok(None)
}

#[cfg(target_os = "macos")]
fn read_macos_clipboard_image() -> Result<Option<ClipboardImage>> {
    use std::fs;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "jackin-host-clipboard-{}-{nanos}.png",
        std::process::id()
    ));
    let png_class = "\u{00ab}class PNGf\u{00bb}";
    let script = format!(
        r#"set outputPath to system attribute "JACKIN_CLIPBOARD_IMAGE_OUT"
try
  set imageData to (the clipboard as {png_class})
on error errMsg number errNum
  error errMsg number errNum
end try
set outputFile to open for access POSIX file outputPath with write permission
try
  set eof outputFile to 0
  write imageData to outputFile
  close access outputFile
on error errMsg number errNum
  try
    close access outputFile
  end try
  error errMsg number errNum
end try"#
    );

    let output = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(script)
        .env("JACKIN_CLIPBOARD_IMAGE_OUT", &path)
        .output()?;
    if !output.status.success() {
        drop(fs::remove_file(&path));
        return read_macos_clipboard_file_url();
    }

    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return read_macos_clipboard_file_url();
        }
        Err(err) => return Err(err.into()),
    };
    drop(fs::remove_file(&path));
    match image_from_bytes(bytes)? {
        Some(image) => Ok(Some(image)),
        None => read_macos_clipboard_file_url(),
    }
}

#[cfg(target_os = "macos")]
fn read_macos_clipboard_file_url() -> Result<Option<ClipboardImage>> {
    use std::process::Command;

    let furl_class = "\u{00ab}class furl\u{00bb}";
    let script = format!(
        r#"try
  set fileRef to (the clipboard as {furl_class})
  return POSIX path of fileRef
on error errMsg number errNum
  error errMsg number errNum
end try"#
    );
    let output = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(script)
        .output()?;
    if !output.status.success() {
        return Ok(None);
    }
    let path = String::from_utf8_lossy(&output.stdout)
        .trim_end_matches(['\r', '\n'])
        .to_owned();
    if path.is_empty() {
        return Ok(None);
    }
    image_from_file(Path::new(&path))
}

fn image_from_file(path: &Path) -> Result<Option<ClipboardImage>> {
    let metadata = std::fs::metadata(path)
        .with_context(|| format!("reading clipboard file metadata for {}", path.display()))?;
    if !metadata.is_file() || metadata.len() as usize > MAX_CLIPBOARD_IMAGE_BYTES {
        return Ok(None);
    }
    let bytes = std::fs::read(path)
        .with_context(|| format!("reading clipboard file {}", path.display()))?;
    image_from_bytes(bytes)
}

fn image_from_bytes(bytes: Vec<u8>) -> Result<Option<ClipboardImage>> {
    if bytes.len() > MAX_CLIPBOARD_IMAGE_BYTES {
        return Ok(None);
    }
    let Some(format) = image_format_from_magic(&bytes) else {
        return Ok(None);
    };
    Ok(Some(ClipboardImage { format, bytes }))
}

fn image_format_from_magic(bytes: &[u8]) -> Option<ClipboardImageFormat> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some(ClipboardImageFormat::Png);
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some(ClipboardImageFormat::Jpeg);
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some(ClipboardImageFormat::Gif);
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some(ClipboardImageFormat::Webp);
    }
    if bytes.starts_with(b"MM\0*") || bytes.starts_with(b"II*\0") {
        return Some(ClipboardImageFormat::Tiff);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctrl_v_is_the_image_paste_trigger() {
        assert!(is_image_paste_trigger(&[0x16]));
        assert!(!is_image_paste_trigger(b"v"));
        assert!(!is_image_paste_trigger(&[0x16, b'x']));
        assert!(!is_image_paste_trigger(&[]));
    }

    #[tokio::test]
    async fn non_trigger_does_not_probe_clipboard() {
        let image = read_image_for_paste_trigger(b"abc").await.unwrap();
        assert!(image.is_none());
    }

    #[test]
    fn detects_supported_image_magic() {
        for (bytes, format) in [
            (
                b"\x89PNG\r\n\x1a\npayload".as_slice(),
                ClipboardImageFormat::Png,
            ),
            (&[0xff, 0xd8, 0xff, 0x00], ClipboardImageFormat::Jpeg),
            (b"GIF89apayload".as_slice(), ClipboardImageFormat::Gif),
            (
                b"RIFF\x00\x00\x00\x00WEBPpayload".as_slice(),
                ClipboardImageFormat::Webp,
            ),
            (b"MM\0*payload".as_slice(), ClipboardImageFormat::Tiff),
        ] {
            assert_eq!(image_format_from_magic(bytes), Some(format));
        }
        assert_eq!(image_format_from_magic(b"not-image"), None);
    }

    #[test]
    fn reads_supported_image_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("copied.png");
        std::fs::write(&path, b"\x89PNG\r\n\x1a\npayload").unwrap();

        let image = image_from_file(&path).unwrap().unwrap();
        assert_eq!(image.format, ClipboardImageFormat::Png);
        assert_eq!(image.bytes, b"\x89PNG\r\n\x1a\npayload");
    }
}
