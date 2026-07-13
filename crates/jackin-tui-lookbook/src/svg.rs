// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! SVG generation from story renders: buffer-to-SVG conversion,
//! writing SVG files to disk, and checking whether existing files are current.

use std::{
    collections::BTreeSet,
    ffi::OsStr,
    fs, io,
    path::{Path, PathBuf},
};
use std::{fmt::Arguments, io::Write as _};

use jackin_tui::theme::PREVIEW_CARD;
use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Clear},
};

use crate::stories::{Story, stories};

/// Uniform charcoal padding ring around every exported story, in cells. Mirrors
/// the interactive preview's 1-cell `Margin` so a component floats inside the
/// `PREVIEW_CARD` surround instead of bleeding to the image edge.
const STORY_PAD: u16 = 1;

fn stdout_line(args: Arguments<'_>) {
    let mut stdout = io::stdout().lock();
    drop(writeln!(stdout, "{args}"));
}

fn stderr_line(args: Arguments<'_>) {
    let mut stderr = io::stderr().lock();
    drop(writeln!(stderr, "{args}"));
}

/// Render the story into a ratatui test buffer and return it.
pub(crate) fn render_story_to_buffer(story: Story) -> Buffer {
    let width = story.width.saturating_add(STORY_PAD * 2);
    let height = story.height.saturating_add(STORY_PAD * 2);
    let backend = TestBackend::new(width, height);
    let mut terminal = match Terminal::new(backend) {
        Ok(terminal) => terminal,
        Err(error) => match error {},
    };
    match terminal.draw(|frame| {
        let area = frame.area();
        // PREVIEW_CARD charcoal surround, matching the interactive preview so
        // the padding ring is visible against the black page background and
        // every component reads as a floating element.
        frame.render_widget(
            Block::default().style(Style::default().bg(PREVIEW_CARD)),
            area,
        );
        let inner = Rect {
            x: STORY_PAD,
            y: STORY_PAD,
            width: story.width,
            height: story.height,
        };
        // Clear the component area to the terminal default (black) so the story
        // renders on the same surface as the real app, with PREVIEW_CARD only
        // as the surround — identical to the interactive preview.
        frame.render_widget(Clear, inner);
        story.render(frame, inner);
    }) {
        Ok(_) => {}
        Err(error) => match error {},
    }
    terminal.backend().buffer().clone()
}

/// Render the story to an SVG string.
#[must_use]
pub(crate) fn render_story_to_svg(story: Story) -> String {
    let buffer = render_story_to_buffer(story);
    buffer_to_svg(&buffer, story.title)
}

/// Canonical filename for a story's SVG preview.
#[must_use]
pub(crate) fn story_svg_filename(story: Story) -> String {
    format!("{}.svg", story.id.replace('/', "-"))
}

/// Write all story SVGs to `out_dir`, creating it if needed.
pub(crate) fn write_story_svgs(out_dir: impl AsRef<Path>) -> io::Result<Vec<PathBuf>> {
    let out_dir = out_dir.as_ref();
    fs::create_dir_all(out_dir)?;
    let mut paths = Vec::new();
    for story in stories() {
        let path = out_dir.join(story_svg_filename(story));
        fs::write(&path, render_story_to_svg(story))?;
        paths.push(path);
    }
    Ok(paths)
}

/// Check that all SVGs in `dir` are current. Prints a success message and
/// returns `Ok(())` when they match; returns `Err` with failure details otherwise.
pub(crate) fn check_svgs(dir: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let expected = expected_svg_names();
    let actual = actual_svg_names(&dir)?;
    let mut failures = Vec::new();

    for missing in expected.difference(&actual) {
        failures.push(format!("missing generated preview: {missing}"));
    }
    for stale in actual.difference(&expected) {
        failures.push(format!("stale generated preview: {stale}"));
    }

    for story in stories() {
        let filename = story_svg_filename(story);
        let path = dir.join(&filename);
        if !path.exists() {
            continue;
        }
        let committed = fs::read_to_string(&path)?;
        let rendered = render_story_to_svg(story);
        if committed != rendered {
            failures.push(format!("generated preview is stale: {}", path.display()));
        }
    }

    if failures.is_empty() {
        stdout_line(format_args!("tui lookbook previews are current"));
        Ok(())
    } else {
        for failure in &failures {
            stderr_line(format_args!("{failure}"));
        }
        Err(concat!(
            "tui lookbook previews are out of date; regenerate with ",
            "`cargo run -p jackin-tui-lookbook -- docs/public/tui-lookbook`",
        )
        .into())
    }
}

pub(crate) fn expected_svg_names() -> BTreeSet<String> {
    stories().into_iter().map(story_svg_filename).collect()
}

pub(crate) fn actual_svg_names(dir: &Path) -> Result<BTreeSet<String>, Box<dyn std::error::Error>> {
    let mut names = BTreeSet::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension() != Some(OsStr::new("svg")) {
            continue;
        }
        let Some(name) = path.file_name().and_then(OsStr::to_str) else {
            return Err(format!("non-UTF-8 lookbook preview path: {}", path.display()).into());
        };
        names.insert(name.to_owned());
    }
    Ok(names)
}

fn buffer_to_svg(buffer: &Buffer, title: &str) -> String {
    const CELL_W: u16 = 9;
    const CELL_H: u16 = 18;
    const BASELINE: u16 = 14;

    let area = buffer.area;
    let width = area.width.saturating_mul(CELL_W);
    let height = area.height.saturating_mul(CELL_H);
    let mut out = String::new();
    out.push_str(&format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" role="img" aria-label="{}" style="background:#000000">"#,
        escape_xml(title)
    ));
    out.push_str(r##"<rect width="100%" height="100%" fill="#000000"/>"##);
    out.push_str(r#"<g font-family="ui-monospace, SFMono-Regular, Menlo, Consolas, monospace" font-size="14">"#);

    for y in 0..area.height {
        for x in 0..area.width {
            let cell = &buffer[(x, y)];
            let px = x.saturating_mul(CELL_W);
            let py = y.saturating_mul(CELL_H);
            let bg = color_to_css(cell.bg);
            if bg != "#000000" {
                out.push_str(&format!(
                    r#"<rect x="{px}" y="{py}" width="{CELL_W}" height="{CELL_H}" fill="{bg}"/>"#
                ));
            }
            let symbol = cell.symbol();
            if !symbol.trim().is_empty() {
                let fg = color_to_css(cell.fg);
                let text_y = py.saturating_add(BASELINE);
                out.push_str(&format!(
                    r#"<text x="{px}" y="{text_y}" fill="{fg}">{}</text>"#,
                    escape_xml(symbol)
                ));
            }
        }
    }
    out.push_str("</g></svg>\n");
    out
}

fn color_to_css(color: Color) -> &'static str {
    match color {
        Color::Black => "#000000",
        Color::Red => "#ff0000",
        Color::Green => "#00ff41",
        Color::Yellow => "#ffd85e",
        Color::Blue => "#0050b4",
        Color::Magenta => "#ff00ff",
        Color::Cyan => "#00ffff",
        Color::Gray | Color::DarkGray => "#808080",
        Color::LightRed => "#ff5e7a",
        Color::LightGreen => "#00ff41",
        Color::LightYellow => "#ffd85e",
        Color::LightBlue => "#7aa2ff",
        Color::LightMagenta => "#ff7aff",
        Color::LightCyan => "#7affff",
        Color::White => "#ffffff",
        Color::Rgb(0, 255, 65) => "#00ff41",
        Color::Rgb(0, 140, 30) => "#008c1e",
        Color::Rgb(0, 80, 18) => "#005012",
        Color::Rgb(255, 94, 122) => "#ff5e7a",
        Color::Rgb(255, 216, 94) => "#ffd85e",
        Color::Rgb(0, 80, 180) => "#0050b4",
        Color::Rgb(204, 92, 0) => "#cc5c00",
        Color::Rgb(80, 80, 80) => "#505050",
        Color::Rgb(28, 28, 28) => "#1c1c1c",
        Color::Rgb(180, 180, 180) => "#b4b4b4",
        Color::Reset => "#000000",
        Color::Rgb(_, _, _) | Color::Indexed(_) => "#ffffff",
    }
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Render a story's buffer to plain text (for debugging / snapshot tests).
#[must_use]
#[expect(
    dead_code,
    reason = "debug helper kept for snapshot triage outside normal lookbook flow"
)]
pub(crate) fn render_story_to_text(story: Story) -> String {
    let buffer = render_story_to_buffer(story);
    let mut out = String::new();
    for y in 0..story.height {
        for x in 0..story.width {
            out.push_str(buffer[(x, y)].symbol());
        }
        if y + 1 < story.height {
            out.push('\n');
        }
    }
    out
}
