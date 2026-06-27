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

    // Bug 9: a `Cmd+V` whose read is a whole image path but NOT bracketed (the
    // terminal didn't wrap it — the symptom where the raw host path landed in the
    // prompt) still stages. Typing can't deliver a full path in one raw-mode read,
    // so a single read that is exactly one real image path is unambiguously a
    // paste/drop, not keystrokes.
    let raw_path = path.display().to_string();
    let unbracketed = read_image_from_pasted_path(raw_path.as_bytes())
        .await
        .unwrap()
        .expect("unbracketed whole image path should stage (Bug 9)");
    assert_eq!(unbracketed.0.format, ClipboardImageFormat::Png);
    assert_eq!(unbracketed.0.bytes, b"\x89PNG\r\n\x1a\npayload");

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
fn bounded_reader_kills_and_drops_output_over_the_cap() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("payload.bin");
    std::fs::write(&path, b"0123456789").unwrap();

    // Exactly at the cap is kept; one byte over is dropped (the child is killed
    // rather than draining an unbounded payload).
    assert_eq!(
        read_command_stdout_bounded(Path::new("/bin/cat"), [path.as_os_str()], 10, "text")
            .unwrap()
            .as_deref(),
        Some(b"0123456789".as_slice())
    );
    assert!(
        read_command_stdout_bounded(Path::new("/bin/cat"), [path.as_os_str()], 9, "text")
            .unwrap()
            .is_none()
    );
}

#[test]
fn bounded_reader_maps_empty_and_failed_commands_to_none() {
    let temp = tempfile::tempdir().unwrap();
    let empty = temp.path().join("empty.bin");
    std::fs::write(&empty, b"").unwrap();

    // Success with empty stdout → None (an empty clipboard is "nothing", not "").
    assert!(
        read_command_stdout_bounded(Path::new("/bin/cat"), [empty.as_os_str()], 10, "text")
            .unwrap()
            .is_none()
    );
    // Non-zero exit → None even with bytes on stdout (exercises the
    // `!status.success()` arm independently of the empty-output arm).
    assert!(
        read_command_stdout_bounded(
            Path::new("/bin/sh"),
            ["-c", "printf hi; exit 1"],
            10,
            "text"
        )
        .unwrap()
        .is_none()
    );
}

#[test]
fn bounded_reader_propagates_spawn_failure_as_error() {
    // A missing program is a real error, not an empty-clipboard `None`.
    assert!(
        read_command_stdout_bounded(
            Path::new("/nonexistent/jackin-no-such-binary"),
            Vec::<&OsStr>::new(),
            10,
            "text",
        )
        .is_err()
    );
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
fn linux_clipboard_backend_reports_both_tools_missing_when_both_servers_set() {
    let err = validate_linux_clipboard_backend(
        true,
        true,
        false,
        false,
        "Linux host clipboard image reader",
    )
    .expect_err("both servers set but neither tool present should explain setup");

    assert!(format!("{err:#}").contains("needs wl-paste or xclip in host PATH"));
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
    // Both servers advertised, only one tool present → still accepted.
    validate_linux_clipboard_backend(true, true, true, false, "Linux host clipboard image reader")
        .expect("both servers with only wl-paste should work");
}
