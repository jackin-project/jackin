// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

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
pub fn is_image_paste_trigger(input: &[u8]) -> bool {
    input == [CTRL_V]
}

pub async fn read_image_for_paste_trigger(input: &[u8]) -> Result<Option<ClipboardImage>> {
    if !is_image_paste_trigger(input) {
        return Ok(None);
    }
    read_host_clipboard_image().await
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
pub fn paste_image_paths_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| paste_image_paths_enabled_for(std::env::var_os(PASTE_IMAGE_PATHS_ENV)))
}

/// The opt-out decision, split out from the process-wide cache so it is unit
/// testable: unset → enabled; otherwise the shared truthy/falsy parse.
fn paste_image_paths_enabled_for(value: Option<std::ffi::OsString>) -> bool {
    value.is_none_or(|value| crate::universe::env_flag_enabled(Some(value)))
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

/// Auto-stage path for `Cmd+V` parity: if `input`'s paste body is a single real,
/// magic-validated host image file, return that image plus the `prefix`/`suffix`
/// bytes sharing the read, so the caller stages the image, inserts the container
/// path instead of the raw host path, and still forwards the surrounding bytes.
/// Anything else returns `None` and is forwarded as ordinary text.
///
/// The body is the bracketed-paste content when the read carries the
/// `ESC[200~ … ESC[201~` markers; otherwise the whole read is the body. The
/// unbracketed fallback is what makes a `Cmd+V` work in terminals that do not
/// bracket the paste (the symptom: the raw host path lands in the prompt). It is
/// safe because raw-mode *typing* delivers one byte per read, so a whole absolute
/// image path can only arrive in a single read via a paste/drop — never
/// keystroke-by-keystroke — and `looks_like_image_path` still requires the entire
/// body to be one absolute image path with no other bytes.
pub async fn read_image_from_pasted_path(
    input: &[u8],
) -> Result<Option<(ClipboardImage, &[u8], &[u8])>> {
    if !paste_image_paths_enabled() {
        return Ok(None);
    }
    let (prefix, body, suffix) = split_paste(input).unwrap_or((&[], input, &[]));
    let Ok(text) = std::str::from_utf8(body) else {
        return Ok(None);
    };
    if !looks_like_image_path(text) {
        return Ok(None);
    }
    let text = text.trim();
    let owned = text.to_owned();
    let resolved =
        jackin_telemetry::spawn::joined_blocking(move || -> Result<Option<ClipboardImage>> {
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
        // An implicit path-like paste that does not resolve remains ordinary text;
        // it did not explicitly ask to stage an image or expose its host path.
        return Ok(None);
    };
    Ok(Some((image, prefix, suffix)))
}

/// Run a clipboard reader on a blocking pool, mapping a join failure to an
/// error tagged with the per-OS `label`.
#[cfg(any(target_os = "macos", target_os = "linux"))]
async fn spawn_clipboard_probe(
    label: &str,
    f: impl FnOnce() -> Result<Option<ClipboardImage>> + Send + 'static,
) -> Result<Option<ClipboardImage>> {
    jackin_telemetry::spawn::joined_blocking(f)
        .await
        .map_err(|err| anyhow::anyhow!("joining {label}: {err}"))?
}

#[cfg(target_os = "macos")]
pub async fn read_host_clipboard_image() -> Result<Option<ClipboardImage>> {
    spawn_clipboard_probe("macOS clipboard image reader", read_macos_clipboard_image).await
}

#[cfg(target_os = "linux")]
pub async fn read_host_clipboard_image() -> Result<Option<ClipboardImage>> {
    spawn_clipboard_probe("Linux clipboard image reader", read_linux_clipboard_image).await
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub async fn read_host_clipboard_image() -> Result<Option<ClipboardImage>> {
    Ok(None)
}

#[cfg(target_os = "macos")]
pub async fn read_host_clipboard_text_path_image() -> Result<Option<ClipboardImage>> {
    spawn_clipboard_probe(
        "macOS clipboard text-path reader",
        read_macos_clipboard_text_path_image,
    )
    .await
}

#[cfg(target_os = "linux")]
pub async fn read_host_clipboard_text_path_image() -> Result<Option<ClipboardImage>> {
    spawn_clipboard_probe(
        "Linux clipboard text-path reader",
        read_linux_clipboard_text_path_image,
    )
    .await
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub async fn read_host_clipboard_text_path_image() -> Result<Option<ClipboardImage>> {
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
    validate_linux_clipboard_backend_env("Linux host clipboard image reader")?;

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
    validate_linux_clipboard_backend_env("Linux host clipboard text reader")?;

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

/// Spawn `program` and read its stdout, capping the kept bytes at `max_bytes`
/// (one extra byte is read to detect overflow). Returns `Ok(None)` when the
/// command exits non-zero, produces no output, or exceeds `max_bytes` (the child
/// is killed in that case rather than draining an unbounded clipboard payload);
/// spawn and I/O failures propagate as `Err`. `what` ("text"/"image") names the
/// probe so a failure points at the right reader.
#[cfg(any(target_os = "linux", test))]
fn read_command_stdout_bounded<I, S>(
    program: &Path,
    args: I,
    max_bytes: usize,
    what: &str,
) -> Result<Option<Vec<u8>>>
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
        .with_context(|| format!("spawning clipboard {what} command {}", program.display()))?;
    let mut stdout = child
        .stdout
        .take()
        .with_context(|| format!("clipboard {what} command did not expose stdout"))?;
    let mut bytes = Vec::new();
    {
        let mut limited = stdout.by_ref().take((max_bytes + 1) as u64);
        limited
            .read_to_end(&mut bytes)
            .with_context(|| format!("reading clipboard {what} command stdout"))?;
    }
    drop(stdout);
    if bytes.len() > max_bytes {
        drop(child.kill());
        drop(child.wait());
        return Ok(None);
    }

    let status = child
        .wait()
        .with_context(|| format!("waiting for clipboard {what} command to exit"))?;
    if !status.success() || bytes.is_empty() {
        return Ok(None);
    }
    Ok(Some(bytes))
}

#[cfg(any(target_os = "linux", test))]
fn read_text_command<I, S>(program: &Path, args: I) -> Result<Option<String>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Ok(
        read_command_stdout_bounded(program, args, MAX_CLIPBOARD_TEXT_PATH_BYTES, "text")?
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned()),
    )
}

#[cfg(any(target_os = "linux", test))]
fn read_image_command<I, S>(program: &Path, args: I) -> Result<Option<ClipboardImage>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    match read_command_stdout_bounded(program, args, MAX_CLIPBOARD_IMAGE_TRANSFER_BYTES, "image")? {
        Some(bytes) => image_from_bytes(bytes),
        None => Ok(None),
    }
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

/// Probe the live host environment (display-server vars + tool availability) and
/// validate it. `label` names the caller so the failure message points at the
/// image or text reader.
#[cfg(target_os = "linux")]
fn validate_linux_clipboard_backend_env(label: &str) -> Result<()> {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let display = std::env::var_os("DISPLAY").is_some();
    let wl_paste = find_program_in_path("wl-paste").is_some();
    let xclip = find_program_in_path("xclip").is_some();
    validate_linux_clipboard_backend(wayland, display, wl_paste, xclip, label)
}

#[cfg(any(target_os = "linux", test))]
#[expect(
    clippy::fn_params_excessive_bools,
    reason = "Four orthogonal clipboard-backend availability booleans (wayland, \
              display, wl_paste, xclip) — each is an independent capability the \
              validator inspects to construct the missing-tool error message. \
              Named-arg reads match the capability-matrix idiom this validator \
              walks."
)]
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
    if (wayland && wl_paste) || (display && xclip) {
        return Ok(());
    }
    // Reached only with a display server set but its tool absent. The final bail
    // is exhaustive: `!wayland` here implies `display`, since no-display bailed above.
    if wayland && display {
        anyhow::bail!("{label} needs wl-paste or xclip in host PATH");
    }
    if wayland {
        anyhow::bail!("{label} needs wl-paste in host PATH because WAYLAND_DISPLAY is set");
    }
    anyhow::bail!("{label} needs xclip in host PATH because DISPLAY is set")
}

#[cfg(any(target_os = "linux", test))]
fn find_program_in_path_value(program: &str, path: &OsStr) -> Option<PathBuf> {
    std::env::split_paths(path)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
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
mod tests;
