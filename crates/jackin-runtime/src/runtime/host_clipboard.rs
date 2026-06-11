//! Host clipboard readers used by opt-in host attach.
//!
//! This is deliberately host-side. In-container `xclip` only talks to an X11
//! clipboard when DISPLAY/Xauthority/X11 sockets exist; it is not a macOS
//! clipboard bridge.

use anyhow::Result;
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
    tokio::task::spawn_blocking(read_macos_clipboard_png)
        .await
        .map_err(|err| anyhow::anyhow!("joining macOS clipboard image reader: {err}"))?
}

#[cfg(not(target_os = "macos"))]
async fn read_host_clipboard_image() -> Result<Option<ClipboardImage>> {
    Ok(None)
}

#[cfg(target_os = "macos")]
fn read_macos_clipboard_png() -> Result<Option<ClipboardImage>> {
    use std::fs;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";
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
        return Ok(None);
    }

    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    drop(fs::remove_file(&path));
    if bytes.len() > MAX_CLIPBOARD_IMAGE_BYTES || !bytes.starts_with(PNG_MAGIC) {
        return Ok(None);
    }
    Ok(Some(ClipboardImage {
        format: ClipboardImageFormat::Png,
        bytes,
    }))
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
}
