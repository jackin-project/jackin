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

/// Env opt-out for auto-staging bracketed-pasted host image paths.
const PASTE_IMAGE_PATHS_ENV: &str = "JACKIN_PASTE_IMAGE_PATHS";
const BRACKETED_PASTE_START: &[u8] = b"\x1b[200~";
const BRACKETED_PASTE_END: &[u8] = b"\x1b[201~";
/// Dotted so the suffix test needs no per-call allocation.
const IMAGE_PATH_EXTENSIONS: &[&str] = &[".png", ".jpg", ".jpeg", ".gif", ".webp", ".tiff", ".tif"];

/// Auto-staging of a `Cmd+V`-pasted host image path (the `CleanShot` flow: tools
/// that copy a *file path* to the clipboard, which the terminal pastes as
/// bracketed-paste text). On by default whenever host attach is active; set
/// `JACKIN_PASTE_IMAGE_PATHS` to `0`/`false`/`no`/`off` (or empty) to keep every
/// paste as ordinary terminal text. Resolved once — the value is fixed for the
/// process.
#[must_use]
pub(super) fn paste_image_paths_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| paste_image_paths_enabled_for(std::env::var_os(PASTE_IMAGE_PATHS_ENV)))
}

/// The opt-out decision, split out from the process-wide cache so it is unit
/// testable: unset → enabled; otherwise the shared truthy/falsy parse.
fn paste_image_paths_enabled_for(value: Option<std::ffi::OsString>) -> bool {
    value.is_none_or(|value| super::universe::env_flag_enabled(Some(value)))
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        // `windows(0)` panics, so the empty needle needs this guard. A needle
        // longer than the haystack needs none: `windows` yields nothing and
        // `position` returns `None`.
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// Strip one matched pair of surrounding single or double quotes. Terminals and
/// "copy as pathname" sources sometimes wrap a pasted path in quotes.
fn strip_surrounding_quotes(text: &str) -> &str {
    text.strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            text.strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(text)
}

/// Split one read carrying a bracketed paste (`ESC[200~ … ESC[201~`) into
/// `(prefix, body, suffix)` such that `prefix + START + body + END + suffix`
/// reconstructs `input`. Returns `None` when the read has no start marker, so
/// typed input is never treated as a paste. Leading/trailing bytes are tolerated;
/// a missing end marker (paste split across reads) puts the rest in `body` and
/// leaves an empty `suffix`. The `prefix`/`suffix` still have to reach the agent
/// so a coincident keystroke or mouse report sharing the read is not dropped when
/// the body is consumed as an image.
fn split_paste(input: &[u8]) -> Option<(&[u8], &[u8], &[u8])> {
    let start = find_subsequence(input, BRACKETED_PASTE_START)?;
    let prefix = &input[..start];
    let after = &input[start + BRACKETED_PASTE_START.len()..];
    let (body, suffix) = match find_subsequence(after, BRACKETED_PASTE_END) {
        Some(end) => (&after[..end], &after[end + BRACKETED_PASTE_END.len()..]),
        None => (after, &[][..]),
    };
    Some((prefix, body, suffix))
}

/// Cheap pre-check: does the pasted text look like a single absolute image-file
/// path or `file://` image URL? Gates the filesystem read so ordinary text
/// pastes never touch disk.
fn looks_like_image_path(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty()
        || trimmed.len() > MAX_CLIPBOARD_TEXT_PATH_BYTES
        || trimmed.contains(['\n', '\r'])
    {
        return false;
    }
    let unquoted = strip_surrounding_quotes(trimmed);
    let is_absolute = unquoted.starts_with("file://") || unquoted.starts_with('/');
    is_absolute
        && IMAGE_PATH_EXTENSIONS
            .iter()
            .any(|ext| has_extension(unquoted, ext))
}

/// Case-insensitive suffix test without allocating (no full-string lowercasing).
fn has_extension(path: &str, dotted_ext: &str) -> bool {
    path.len() >= dotted_ext.len()
        && path.as_bytes()[path.len() - dotted_ext.len()..]
            .eq_ignore_ascii_case(dotted_ext.as_bytes())
}

/// Drop one level of shell backslash-escaping. Terminals escape pasted file
/// paths (e.g. `\ ` for a space) before inserting them, so the literal bytes
/// would not resolve on disk without this.
fn unescape_shell_path(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Auto-stage path for `Cmd+V` parity: if `input` carries a bracketed-paste
/// start marker (leading/trailing bytes and a missing end marker are tolerated)
/// and the paste body is a single real, magic-validated host image file, return
/// that image plus the `prefix`/`suffix` bytes sharing the read around the paste,
/// so the caller stages the image, inserts the container path instead of the raw
/// host path, and still forwards the surrounding bytes. Anything else returns
/// `None` and is forwarded as ordinary text.
pub(super) async fn read_image_from_pasted_path(
    input: &[u8],
) -> Result<Option<(ClipboardImage, &[u8], &[u8])>> {
    if !paste_image_paths_enabled() {
        return Ok(None);
    }
    let Some((prefix, body, suffix)) = split_paste(input) else {
        return Ok(None);
    };
    let Ok(text) = std::str::from_utf8(body) else {
        return Ok(None);
    };
    if !looks_like_image_path(text) {
        return Ok(None);
    }
    let text = text.trim();
    // A candidate image-path paste was recognized; record it (path only, no
    // bytes) so a `--debug` run shows whether the host file resolved.
    jackin_diagnostics::debug_log!(
        "clipboard-image",
        "pasted-path candidate: {}",
        text.escape_default()
    );
    let owned = text.to_owned();
    let resolved = tokio::task::spawn_blocking(move || -> Result<Option<ClipboardImage>> {
        if let Some(image) = image_from_path_text(&owned)? {
            return Ok(Some(image));
        }
        // Terminals shell-escape pasted paths; retry once de-escaped when there
        // is anything to de-escape.
        if owned.contains('\\') {
            return image_from_path_text(&unescape_shell_path(&owned));
        }
        Ok(None)
    })
    .await
    .map_err(|err| anyhow::anyhow!("joining pasted-path image reader: {err}"))??;
    let Some(image) = resolved else {
        // A recognized candidate that did not resolve (missing file, unreadable,
        // not an image) is rare, so logging it is not firehose. It still forwards
        // as ordinary text rather than nagging — the implicit paste did not ask to
        // stage — but a `--debug` run can now see that a recognized candidate
        // failed (the path was logged above).
        jackin_diagnostics::debug_log!(
            "clipboard-image",
            "pasted-path candidate did not resolve to a readable image"
        );
        return Ok(None);
    };
    Ok(Some((image, prefix, suffix)))
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
    if let Some(image) = read_macos_clipboard_image_class("PNGf", "png")? {
        return Ok(Some(image));
    }
    if let Some(image) = read_macos_clipboard_image_class("TIFF", "tiff")? {
        return Ok(Some(image));
    }
    read_macos_clipboard_file_url()
}

#[cfg(target_os = "macos")]
fn read_macos_clipboard_image_class(
    class_code: &str,
    extension: &str,
) -> Result<Option<ClipboardImage>> {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "jackin-host-clipboard-{}-{nanos}.{extension}",
        std::process::id(),
    ));
    let clipboard_class = apple_event_class_literal(class_code);
    let script = format!(
        r#"set outputPath to system attribute "JACKIN_CLIPBOARD_IMAGE_OUT"
try
  set imageData to (the clipboard as {clipboard_class})
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
        return Ok(None);
    }

    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    drop(fs::remove_file(&path));
    image_from_bytes(bytes)
}

#[cfg(target_os = "macos")]
fn apple_event_class_literal(class_code: &str) -> String {
    format!("\u{00ab}class {class_code}\u{00bb}")
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

    if std::env::var_os("WAYLAND_DISPLAY").is_some()
        && let Some(wl_paste) = find_program_in_path("wl-paste")
    {
        for (_format, mime) in image_mime_types() {
            if let Some(image) = read_image_command(&wl_paste, ["--type", mime])? {
                return Ok(Some(image));
            }
        }
    }

    if std::env::var_os("DISPLAY").is_some()
        && let Some(xclip) = find_program_in_path("xclip")
    {
        for (_format, mime) in image_mime_types() {
            if let Some(image) =
                read_image_command(&xclip, ["-selection", "clipboard", "-t", mime, "-o"])?
            {
                return Ok(Some(image));
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
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(err)
            if matches!(
                err.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
            ) =>
        {
            return Ok(None);
        }
        Err(err) => return Err(err).context("reading clipboard file metadata"),
    };
    if !metadata.is_file() || metadata.len() as usize > MAX_CLIPBOARD_IMAGE_TRANSFER_BYTES {
        return Ok(None);
    }
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(err)
            if matches!(
                err.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
            ) =>
        {
            return Ok(None);
        }
        Err(err) => return Err(err).context("reading clipboard file"),
    };
    image_from_bytes(bytes)
}

fn image_from_path_text(text: &str) -> Result<Option<ClipboardImage>> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_CLIPBOARD_TEXT_PATH_BYTES {
        return Ok(None);
    }
    let unquoted = strip_surrounding_quotes(trimmed);
    let path_buf;
    let path = if unquoted.starts_with("file://") {
        let url = url::Url::parse(unquoted).context("parsing clipboard file URL")?;
        if url.scheme() != "file" {
            return Ok(None);
        }
        path_buf = match url.to_file_path() {
            Ok(path) => path,
            Err(()) => return Ok(None),
        };
        path_buf.as_path()
    } else {
        Path::new(unquoted)
    };
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

    fn bracketed(content: &str) -> Vec<u8> {
        let mut input = b"\x1b[200~".to_vec();
        input.extend_from_slice(content.as_bytes());
        input.extend_from_slice(b"\x1b[201~");
        input
    }

    #[test]
    fn split_paste_requires_a_start_marker_and_partitions_the_read() {
        let e = b"".as_slice();
        // Clean bracketed paste: empty prefix/suffix, body between markers.
        assert_eq!(
            split_paste(&bracketed("/tmp/x.png")),
            Some((e, b"/tmp/x.png".as_slice(), e))
        );
        // Trailing bytes after the end marker become the suffix.
        let mut trailing = bracketed("/tmp/x.png");
        trailing.extend_from_slice(b"\x1b[<0;1;1M");
        assert_eq!(
            split_paste(&trailing),
            Some((e, b"/tmp/x.png".as_slice(), b"\x1b[<0;1;1M".as_slice()))
        );
        // Leading typed bytes before the start marker become the prefix.
        let mut leading = b"ab".to_vec();
        leading.extend_from_slice(&bracketed("/tmp/x.png"));
        assert_eq!(
            split_paste(&leading),
            Some((b"ab".as_slice(), b"/tmp/x.png".as_slice(), e))
        );
        // Open paste (start marker, end not yet arrived): tail is the body.
        assert_eq!(
            split_paste(b"\x1b[200~/tmp/x.png"),
            Some((e, b"/tmp/x.png".as_slice(), e))
        );
        // No start marker — typed input is never treated as a paste.
        assert_eq!(split_paste(b"/tmp/x.png"), None);
    }

    #[test]
    fn looks_like_image_path_gates_to_absolute_image_files() {
        assert!(looks_like_image_path("/Users/me/shot.png"));
        assert!(looks_like_image_path("\"/Users/me/my shot.jpeg\""));
        assert!(looks_like_image_path("file:///Users/me/shot.gif"));
        // Not an image, not absolute, or multi-line — all forward as text.
        assert!(!looks_like_image_path("/Users/me/notes.txt"));
        assert!(!looks_like_image_path("relative/shot.png"));
        assert!(!looks_like_image_path("/Users/me/a.png\n/Users/me/b.png"));
        assert!(!looks_like_image_path("just some pasted prose"));
    }

    #[test]
    fn unescape_shell_path_drops_backslash_escapes() {
        assert_eq!(
            unescape_shell_path("/Users/me/Application\\ Support/x.png"),
            "/Users/me/Application Support/x.png"
        );
        assert_eq!(unescape_shell_path("/plain/path.png"), "/plain/path.png");
    }

    #[tokio::test]
    async fn pasted_path_stages_real_image_and_forwards_everything_else() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("shot.png");
        std::fs::write(&path, b"\x89PNG\r\n\x1a\npayload").unwrap();

        let input = bracketed(&path.display().to_string());
        let staged = read_image_from_pasted_path(&input)
            .await
            .unwrap()
            .expect("bracketed image path should stage");
        assert_eq!(staged.0.format, ClipboardImageFormat::Png);
        assert_eq!(staged.0.bytes, b"\x89PNG\r\n\x1a\npayload");

        // Without bracketed-paste markers the read is treated as typed input.
        assert!(
            read_image_from_pasted_path(path.display().to_string().as_bytes())
                .await
                .unwrap()
                .is_none()
        );

        // A non-image path, a missing file, and prose all forward as text.
        let notes = temp.path().join("notes.txt");
        std::fs::write(&notes, b"hello").unwrap();
        assert!(
            read_image_from_pasted_path(&bracketed(&notes.display().to_string()))
                .await
                .unwrap()
                .is_none()
        );
        let missing = temp.path().join("missing.png");
        assert!(
            read_image_from_pasted_path(&bracketed(&missing.display().to_string()))
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            read_image_from_pasted_path(b"just some pasted prose")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn pasted_path_resolves_shell_escaped_spaces() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("Application Support");
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("shot.png");
        std::fs::write(&path, b"\x89PNG\r\n\x1a\npayload").unwrap();

        let escaped = path.display().to_string().replace(' ', "\\ ");
        let input = bracketed(&escaped);
        let staged = read_image_from_pasted_path(&input)
            .await
            .unwrap()
            .expect("shell-escaped image path should resolve");
        assert_eq!(staged.0.format, ClipboardImageFormat::Png);
    }

    #[tokio::test]
    async fn pasted_path_rejects_image_extension_with_non_image_content() {
        // The extension gate is cheap; magic bytes are the real authority. A
        // `.png` whose bytes are not an image must not stage.
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("spoof.png");
        std::fs::write(&path, b"this is plain text, not an image").unwrap();

        assert!(
            read_image_from_pasted_path(&bracketed(&path.display().to_string()))
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn pasted_path_stages_file_url_image() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("copied image.png");
        std::fs::write(&path, b"\x89PNG\r\n\x1a\npayload").unwrap();
        let url = url::Url::from_file_path(&path).expect("temp path should map to file URL");

        let input = bracketed(url.as_str());
        let staged = read_image_from_pasted_path(&input)
            .await
            .unwrap()
            .expect("bracketed file:// image URL should stage");
        assert_eq!(staged.0.format, ClipboardImageFormat::Png);
    }

    #[tokio::test]
    async fn pasted_path_stages_open_paste_missing_end_marker() {
        // A paste whose end marker has not arrived on this read still stages from
        // the start-marker tail (the split-across-reads start case).
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("shot.png");
        std::fs::write(&path, b"\x89PNG\r\n\x1a\npayload").unwrap();

        let mut input = b"\x1b[200~".to_vec();
        input.extend_from_slice(path.display().to_string().as_bytes());
        let staged = read_image_from_pasted_path(&input)
            .await
            .unwrap()
            .expect("open bracketed paste should stage from the tail");
        assert_eq!(staged.0.format, ClipboardImageFormat::Png);
    }

    #[test]
    fn paste_image_paths_enabled_for_honors_opt_out() {
        use std::ffi::OsString;
        assert!(paste_image_paths_enabled_for(None));
        assert!(paste_image_paths_enabled_for(Some(OsString::from("1"))));
        assert!(paste_image_paths_enabled_for(Some(OsString::from("yes"))));
        for off in ["0", "false", "no", "off", ""] {
            assert!(
                !paste_image_paths_enabled_for(Some(OsString::from(off))),
                "{off:?} should disable"
            );
        }
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
    fn image_from_path_text_treats_missing_host_path_as_no_image_without_path_error() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("missing.png");

        let image = image_from_path_text(&missing.display().to_string()).unwrap();

        assert!(image.is_none());
    }

    #[test]
    fn image_from_path_text_accepts_file_url_image_path() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("copied image.png");
        std::fs::write(&path, b"\x89PNG\r\n\x1a\npayload").unwrap();
        let url = url::Url::from_file_path(&path).expect("temp file path should map to file URL");

        let image = image_from_path_text(url.as_str())
            .unwrap()
            .expect("file URL image path should read");

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
