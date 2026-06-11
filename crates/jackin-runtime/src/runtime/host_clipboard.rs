//! Host clipboard readers used by opt-in host attach.
//!
//! This is deliberately host-side. In-container `xclip` only talks to an X11
//! clipboard when DISPLAY/Xauthority/X11 sockets exist; it is not a macOS
//! clipboard bridge.

use std::path::Path;

#[cfg(any(target_os = "linux", test))]
use std::{
    ffi::OsStr,
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
};

#[cfg(all(target_os = "macos", not(test)))]
use std::process::Command;

use anyhow::{Context, Result};
use jackin_protocol::attach::{
    ClipboardImage, ClipboardImageFormat, MAX_CLIPBOARD_IMAGE_TRANSFER_BYTES,
};

const CTRL_V: u8 = 0x16;
const MAX_CLIPBOARD_TEXT_PATH_BYTES: usize = 8192;

#[must_use]
pub(super) fn is_image_paste_trigger(input: &[u8]) -> bool {
    input == [CTRL_V]
}

pub(super) async fn read_image_for_paste_trigger(input: &[u8]) -> Result<Option<ClipboardImage>> {
    if !is_image_paste_trigger(input) {
        return Ok(None);
    }
    read_image_from_clipboard().await
}

pub(super) async fn read_image_from_clipboard() -> Result<Option<ClipboardImage>> {
    read_host_clipboard_image().await
}

pub(super) async fn read_image_from_clipboard_text_path() -> Result<Option<ClipboardImage>> {
    read_host_clipboard_text_path_image().await
}

#[cfg(target_os = "macos")]
async fn read_host_clipboard_image() -> Result<Option<ClipboardImage>> {
    tokio::task::spawn_blocking(read_macos_clipboard_image)
        .await
        .map_err(|err| anyhow::anyhow!("joining macOS clipboard image reader: {err}"))?
}

#[cfg(target_os = "linux")]
async fn read_host_clipboard_image() -> Result<Option<ClipboardImage>> {
    tokio::task::spawn_blocking(read_linux_clipboard_image)
        .await
        .map_err(|err| anyhow::anyhow!("joining Linux clipboard image reader: {err}"))?
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
async fn read_host_clipboard_image() -> Result<Option<ClipboardImage>> {
    Ok(None)
}

#[cfg(target_os = "macos")]
async fn read_host_clipboard_text_path_image() -> Result<Option<ClipboardImage>> {
    tokio::task::spawn_blocking(read_macos_clipboard_text_path_image)
        .await
        .map_err(|err| anyhow::anyhow!("joining macOS clipboard text-path reader: {err}"))?
}

#[cfg(target_os = "linux")]
async fn read_host_clipboard_text_path_image() -> Result<Option<ClipboardImage>> {
    tokio::task::spawn_blocking(read_linux_clipboard_text_path_image)
        .await
        .map_err(|err| anyhow::anyhow!("joining Linux clipboard text-path reader: {err}"))?
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
async fn read_host_clipboard_text_path_image() -> Result<Option<ClipboardImage>> {
    Ok(None)
}

#[cfg(target_os = "macos")]
fn read_macos_clipboard_image() -> Result<Option<ClipboardImage>> {
    use std::fs;
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

    #[expect(
        clippy::disallowed_methods,
        reason = "host clipboard probes run in the foreground host attach client, not in Capsule render code"
    )]
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
    let furl_class = "\u{00ab}class furl\u{00bb}";
    let script = format!(
        r"try
  set fileRef to (the clipboard as {furl_class})
  return POSIX path of fileRef
on error errMsg number errNum
  error errMsg number errNum
end try"
    );
    #[expect(
        clippy::disallowed_methods,
        reason = "host clipboard probes run in the foreground host attach client, not in Capsule render code"
    )]
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

#[cfg(target_os = "macos")]
fn read_macos_clipboard_text_path_image() -> Result<Option<ClipboardImage>> {
    let script = r"try
  return the clipboard as text
on error errMsg number errNum
  error errMsg number errNum
end try";
    #[expect(
        clippy::disallowed_methods,
        reason = "host clipboard probes run in the foreground host attach client, not in Capsule render code"
    )]
    let output = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(script)
        .output()?;
    if !output.status.success() || output.stdout.len() > MAX_CLIPBOARD_TEXT_PATH_BYTES {
        return Ok(None);
    }
    let text = String::from_utf8_lossy(&output.stdout);
    image_from_path_text(&text)
}

#[cfg(target_os = "linux")]
fn read_linux_clipboard_image() -> Result<Option<ClipboardImage>> {
    validate_linux_clipboard_image_backend()?;

    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        if let Some(wl_paste) = find_program_in_path("wl-paste") {
            for (_format, mime) in image_mime_types() {
                if let Some(image) = read_image_command(&wl_paste, ["--type", mime])? {
                    return Ok(Some(image));
                }
            }
        }
    }

    if std::env::var_os("DISPLAY").is_some() {
        if let Some(xclip) = find_program_in_path("xclip") {
            for (_format, mime) in image_mime_types() {
                if let Some(image) =
                    read_image_command(&xclip, ["-selection", "clipboard", "-t", mime, "-o"])?
                {
                    return Ok(Some(image));
                }
            }
        }
    }

    Ok(None)
}

#[cfg(target_os = "linux")]
fn read_linux_clipboard_text_path_image() -> Result<Option<ClipboardImage>> {
    validate_linux_clipboard_text_backend()?;

    if std::env::var_os("WAYLAND_DISPLAY").is_some()
        && let Some(wl_paste) = find_program_in_path("wl-paste")
        && let Some(text) = read_text_command(&wl_paste, ["--no-newline"])?
        && let Some(image) = image_from_path_text(&text)?
    {
        return Ok(Some(image));
    }

    if std::env::var_os("DISPLAY").is_some()
        && let Some(xclip) = find_program_in_path("xclip")
        && let Some(text) = read_text_command(&xclip, ["-selection", "clipboard", "-o"])?
        && let Some(image) = image_from_path_text(&text)?
    {
        return Ok(Some(image));
    }

    Ok(None)
}

fn image_from_file(path: &Path) -> Result<Option<ClipboardImage>> {
    let metadata = std::fs::metadata(path)
        .with_context(|| format!("reading clipboard file metadata for {}", path.display()))?;
    if !metadata.is_file() || metadata.len() as usize > MAX_CLIPBOARD_IMAGE_TRANSFER_BYTES {
        return Ok(None);
    }
    let bytes = std::fs::read(path)
        .with_context(|| format!("reading clipboard file {}", path.display()))?;
    image_from_bytes(bytes)
}

fn image_from_path_text(text: &str) -> Result<Option<ClipboardImage>> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_CLIPBOARD_TEXT_PATH_BYTES {
        return Ok(None);
    }
    let unquoted = trimmed
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            trimmed
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(trimmed);
    let path = Path::new(unquoted);
    if !path.is_absolute() {
        return Ok(None);
    }
    image_from_file(path)
}

fn image_from_bytes(bytes: Vec<u8>) -> Result<Option<ClipboardImage>> {
    if bytes.len() > MAX_CLIPBOARD_IMAGE_TRANSFER_BYTES {
        return Ok(None);
    }
    let Some(format) = image_format_from_magic(&bytes) else {
        return Ok(None);
    };
    Ok(Some(ClipboardImage { format, bytes }))
}

#[cfg(any(target_os = "linux", test))]
fn read_text_command<I, S>(program: &Path, args: I) -> Result<Option<String>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("spawning clipboard text command {}", program.display()))?;
    let mut stdout = child
        .stdout
        .take()
        .context("clipboard text command did not expose stdout")?;
    let mut bytes = Vec::new();
    {
        let mut limited = stdout
            .by_ref()
            .take((MAX_CLIPBOARD_TEXT_PATH_BYTES + 1) as u64);
        limited
            .read_to_end(&mut bytes)
            .context("reading clipboard text command stdout")?;
    }
    drop(stdout);
    if bytes.len() > MAX_CLIPBOARD_TEXT_PATH_BYTES {
        drop(child.kill());
        drop(child.wait());
        return Ok(None);
    }

    let status = child
        .wait()
        .context("waiting for clipboard text command to exit")?;
    if !status.success() || bytes.is_empty() {
        return Ok(None);
    }
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

#[cfg(any(target_os = "linux", test))]
fn read_image_command<I, S>(program: &Path, args: I) -> Result<Option<ClipboardImage>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("spawning clipboard image command {}", program.display()))?;
    let mut stdout = child
        .stdout
        .take()
        .context("clipboard image command did not expose stdout")?;
    let mut bytes = Vec::new();
    {
        let mut limited = stdout
            .by_ref()
            .take((MAX_CLIPBOARD_IMAGE_TRANSFER_BYTES + 1) as u64);
        limited
            .read_to_end(&mut bytes)
            .context("reading clipboard image command stdout")?;
    }
    drop(stdout);
    if bytes.len() > MAX_CLIPBOARD_IMAGE_TRANSFER_BYTES {
        drop(child.kill());
        drop(child.wait());
        return Ok(None);
    }

    let status = child
        .wait()
        .context("waiting for clipboard image command to exit")?;
    if !status.success() || bytes.is_empty() {
        return Ok(None);
    }
    image_from_bytes(bytes)
}

#[cfg(any(target_os = "linux", test))]
fn image_mime_types() -> &'static [(ClipboardImageFormat, &'static str)] {
    &[
        (ClipboardImageFormat::Png, "image/png"),
        (ClipboardImageFormat::Jpeg, "image/jpeg"),
        (ClipboardImageFormat::Gif, "image/gif"),
        (ClipboardImageFormat::Webp, "image/webp"),
        (ClipboardImageFormat::Tiff, "image/tiff"),
    ]
}

#[cfg(target_os = "linux")]
fn find_program_in_path(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    find_program_in_path_value(program, &path)
}

#[cfg(target_os = "linux")]
fn validate_linux_clipboard_image_backend() -> Result<()> {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let display = std::env::var_os("DISPLAY").is_some();
    let wl_paste = find_program_in_path("wl-paste").is_some();
    let xclip = find_program_in_path("xclip").is_some();
    validate_linux_clipboard_backend(
        wayland,
        display,
        wl_paste,
        xclip,
        "Linux host clipboard image reader",
    )
}

#[cfg(target_os = "linux")]
fn validate_linux_clipboard_text_backend() -> Result<()> {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let display = std::env::var_os("DISPLAY").is_some();
    let wl_paste = find_program_in_path("wl-paste").is_some();
    let xclip = find_program_in_path("xclip").is_some();
    validate_linux_clipboard_backend(
        wayland,
        display,
        wl_paste,
        xclip,
        "Linux host clipboard text reader",
    )
}

#[cfg(any(target_os = "linux", test))]
fn validate_linux_clipboard_backend(
    wayland: bool,
    display: bool,
    wl_paste: bool,
    xclip: bool,
    label: &str,
) -> Result<()> {
    if !wayland && !display {
        anyhow::bail!("{label} needs WAYLAND_DISPLAY with wl-paste or DISPLAY with xclip");
    }
    if wayland && wl_paste {
        return Ok(());
    }
    if display && xclip {
        return Ok(());
    }
    match (wayland, display, wl_paste, xclip) {
        (true, false, false, _) => {
            anyhow::bail!("{label} needs wl-paste in host PATH because WAYLAND_DISPLAY is set")
        }
        (false, true, _, false) => {
            anyhow::bail!("{label} needs xclip in host PATH because DISPLAY is set")
        }
        (true, true, false, false) => {
            anyhow::bail!("{label} needs wl-paste or xclip in host PATH")
        }
        (true, true, false, true) | (false, true, _, true) => Ok(()),
        (true, true, true, false) | (true, false, true, _) => Ok(()),
        _ => anyhow::bail!("{label} has no usable Wayland or X11 clipboard backend"),
    }
}

#[cfg(any(target_os = "linux", test))]
fn find_program_in_path_value(program: &str, path: &OsStr) -> Option<PathBuf> {
    std::env::split_paths(path).find_map(|dir| {
        let candidate = dir.join(program);
        if candidate.is_file() {
            Some(candidate)
        } else {
            None
        }
    })
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

    #[test]
    fn image_from_path_text_requires_absolute_image_path() {
        assert!(image_from_path_text("relative.png").unwrap().is_none());

        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("copied.png");
        std::fs::write(&path, b"\x89PNG\r\n\x1a\npayload").unwrap();

        let image = image_from_path_text(&format!("  \"{}\"  ", path.display()))
            .unwrap()
            .expect("absolute image path should read");

        assert_eq!(image.format, ClipboardImageFormat::Png);
        assert_eq!(image.bytes, b"\x89PNG\r\n\x1a\npayload");
    }

    #[test]
    fn reads_supported_image_command_output() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("copied.png");
        std::fs::write(&path, b"\x89PNG\r\n\x1a\npayload").unwrap();

        let image = read_image_command(Path::new("/bin/cat"), [path.as_os_str()])
            .unwrap()
            .unwrap();
        assert_eq!(image.format, ClipboardImageFormat::Png);
        assert_eq!(image.bytes, b"\x89PNG\r\n\x1a\npayload");
    }

    #[test]
    fn reads_text_command_output() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("path.txt");
        std::fs::write(&path, b"/tmp/example.png").unwrap();

        let text = read_text_command(Path::new("/bin/cat"), [path.as_os_str()])
            .unwrap()
            .expect("text command should return output");

        assert_eq!(text, "/tmp/example.png");
    }

    #[test]
    fn ignores_non_image_command_output() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("copied.txt");
        std::fs::write(&path, b"not-image").unwrap();

        let image = read_image_command(Path::new("/bin/cat"), [path.as_os_str()]).unwrap();
        assert!(image.is_none());
    }

    #[test]
    fn finds_program_in_explicit_path_value() {
        let temp = tempfile::tempdir().unwrap();
        let tool = temp.path().join("wl-paste");
        std::fs::write(&tool, b"").unwrap();
        let path_value = std::env::join_paths([temp.path()]).unwrap();

        assert_eq!(
            find_program_in_path_value("wl-paste", &path_value),
            Some(tool)
        );
        assert_eq!(find_program_in_path_value("xclip", &path_value), None);
    }

    #[test]
    fn image_mime_order_prefers_png_then_common_raster_formats() {
        let mimes: Vec<_> = image_mime_types().iter().map(|(_, mime)| *mime).collect();
        assert_eq!(
            mimes,
            vec![
                "image/png",
                "image/jpeg",
                "image/gif",
                "image/webp",
                "image/tiff"
            ]
        );
    }

    #[test]
    fn linux_clipboard_backend_reports_missing_display_bridge() {
        let err = validate_linux_clipboard_backend(
            false,
            false,
            false,
            false,
            "Linux host clipboard image reader",
        )
        .expect_err("missing display bridge should explain setup");

        assert!(format!("{err:#}").contains("WAYLAND_DISPLAY with wl-paste or DISPLAY with xclip"));
    }

    #[test]
    fn linux_clipboard_backend_reports_missing_wayland_tool() {
        let err = validate_linux_clipboard_backend(
            true,
            false,
            false,
            false,
            "Linux host clipboard image reader",
        )
        .expect_err("missing wl-paste should explain setup");

        assert!(format!("{err:#}").contains("needs wl-paste in host PATH"));
    }

    #[test]
    fn linux_clipboard_backend_reports_missing_x11_tool() {
        let err = validate_linux_clipboard_backend(
            false,
            true,
            false,
            false,
            "Linux host clipboard image reader",
        )
        .expect_err("missing xclip should explain setup");

        assert!(format!("{err:#}").contains("needs xclip in host PATH"));
    }

    #[test]
    fn linux_clipboard_backend_accepts_any_available_display_tool_pair() {
        validate_linux_clipboard_backend(
            true,
            false,
            true,
            false,
            "Linux host clipboard image reader",
        )
        .expect("Wayland with wl-paste should work");
        validate_linux_clipboard_backend(
            false,
            true,
            false,
            true,
            "Linux host clipboard image reader",
        )
        .expect("X11 with xclip should work");
    }
}
