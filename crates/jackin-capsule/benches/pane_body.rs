//! Pane-body rendering benchmark: custom cell widget vs raw-ANSI baseline.
//!
//! **Background for ADR-004:** The two candidate pane-body rendering approaches
//! for the capsule's Ratatui frame are:
//!
//! - `tui-term` (PseudoTerminal widget): renders vt100::Screen into a Ratatui
//!   Buffer via the Screen trait. **Incompatible with our vt100 fork** — tui-term
//!   implements `Screen` for the crates.io `vt100::Screen` type, but this
//!   codebase uses a git fork (`donbeave/vt100-rust`) that adds `set_window_title`,
//!   `copy_to_clipboard`, and `unhandled_csi`. Different crate identities means
//!   the impl does not apply. This would require switching to upstream vt100 or
//!   upstreaming our patches first.
//!
//! - **Custom cell widget**: a thin `Widget` that blits vt100 cells directly into
//!   the Ratatui `Buffer`. No per-cell allocation beyond what Ratatui's
//!   double-buffer diff already handles. Compatible with our fork today.
//!
//! This benchmark measures the custom cell widget approach. The raw-ANSI baseline
//! (current PaneBodyCache::render_full) is also benchmarked for comparison.
//! The ADR records this evidence and chooses the custom widget path.
//!
//! Run with:
//! ```sh
//! cargo bench -p jackin-capsule --bench pane_body
//! ```

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect, widgets::Widget};

const BENCH_COLS: u16 = 200;
const BENCH_ROWS: u16 = 50;

/// Build a vt100 parser + screen filled with representative content.
fn make_test_parser() -> vt100::Parser {
    let mut parser = vt100::Parser::new(BENCH_ROWS, BENCH_COLS, 1000);
    // Emit 50 lines of realistic shell output: line counter + wide text.
    for row in 0..BENCH_ROWS {
        let line = format!(
            "{:04}: {}\r\n",
            row,
            "The quick brown fox jumps over the lazy dog. "
                .repeat(4)
                .chars()
                .take((BENCH_COLS as usize).saturating_sub(7))
                .collect::<String>()
        );
        parser.process(line.as_bytes());
    }
    parser
}

// ── Custom cell widget ────────────────────────────────────────────────────────
// A minimal Widget that blits vt100 cells into the Ratatui Buffer.
// Compatible with our vt100 fork; no third-party dependency; avoids the
// tui-term Screen trait version mismatch.

struct CustomPaneBlit<'a> {
    screen: &'a vt100::Screen,
}

impl Widget for CustomPaneBlit<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (screen_rows, screen_cols) = self.screen.size();
        for row in 0..area.height.min(screen_rows) {
            for col in 0..area.width.min(screen_cols) {
                let Some(cell) = self.screen.cell(row, col) else {
                    continue;
                };
                let buf_cell = &mut buf[(area.x + col, area.y + row)];
                let contents = cell.contents();
                if !contents.is_empty() {
                    buf_cell.set_symbol(contents);
                } else {
                    buf_cell.set_char(' ');
                }
                buf_cell.set_fg(vt100_color_to_ratatui(cell.fgcolor()));
                buf_cell.set_bg(vt100_color_to_ratatui(cell.bgcolor()));
                if cell.bold() {
                    buf_cell.modifier |= ratatui::style::Modifier::BOLD;
                }
                if cell.italic() {
                    buf_cell.modifier |= ratatui::style::Modifier::ITALIC;
                }
                if cell.underline() {
                    buf_cell.modifier |= ratatui::style::Modifier::UNDERLINED;
                }
            }
        }
    }
}

fn vt100_color_to_ratatui(color: vt100::Color) -> ratatui::style::Color {
    match color {
        vt100::Color::Default => ratatui::style::Color::Reset,
        vt100::Color::Idx(idx) => ratatui::style::Color::Indexed(idx),
        vt100::Color::Rgb(r, g, b) => ratatui::style::Color::Rgb(r, g, b),
    }
}

// ── Raw-ANSI baseline ─────────────────────────────────────────────────────────
// The existing PaneBodyCache::render_full path as a reference point.
// Measures the cost of the hand-rolled ANSI diff approach that the Ratatui
// custom widget would replace.

fn render_raw_ansi_baseline(screen: &vt100::Screen, output: &mut Vec<u8>) {
    output.clear();
    let (rows, cols) = screen.size();
    for row in 0..rows {
        // Cursor positioning
        let r1 = row + 1;
        use std::io::Write as _;
        let _ = write!(output, "\x1b[{r1};1H");
        let mut last_fg = vt100::Color::Default;
        let mut last_bg = vt100::Color::Default;
        for col in 0..cols {
            let Some(cell) = screen.cell(row, col) else {
                output.push(b' ');
                continue;
            };
            // Color SGR (simplified)
            if cell.fgcolor() != last_fg || cell.bgcolor() != last_bg {
                output.extend_from_slice(b"\x1b[0m");
                last_fg = cell.fgcolor();
                last_bg = cell.bgcolor();
            }
            let contents = cell.contents();
            if contents.is_empty() {
                output.push(b' ');
            } else {
                output.extend_from_slice(contents.as_bytes());
            }
        }
    }
}

// ── Benchmarks ────────────────────────────────────────────────────────────────

fn bench_pane_body(c: &mut Criterion) {
    let parser = make_test_parser();
    let screen = parser.screen().clone();
    let backend = TestBackend::new(BENCH_COLS, BENCH_ROWS);
    let mut terminal = Terminal::new(backend).unwrap();
    let area = Rect::new(0, 0, BENCH_COLS, BENCH_ROWS);
    let mut ansi_buf = Vec::with_capacity(65536);

    let mut group = c.benchmark_group("pane_body");
    group.throughput(criterion::Throughput::Elements(
        u64::from(BENCH_COLS) * u64::from(BENCH_ROWS),
    ));

    group.bench_with_input(
        BenchmarkId::new(
            "custom_widget_ratatui",
            format!("{}x{}", BENCH_COLS, BENCH_ROWS),
        ),
        &screen,
        |b, screen| {
            b.iter(|| {
                terminal
                    .draw(|frame| {
                        frame.render_widget(CustomPaneBlit { screen }, area);
                    })
                    .unwrap();
            });
        },
    );

    group.bench_with_input(
        BenchmarkId::new(
            "raw_ansi_baseline",
            format!("{}x{}", BENCH_COLS, BENCH_ROWS),
        ),
        &screen,
        |b, screen| {
            b.iter(|| {
                render_raw_ansi_baseline(screen, &mut ansi_buf);
            });
        },
    );

    group.finish();
}

fn bench_socket_backend_output(c: &mut Criterion) {
    use jackin_capsule::socket_backend::SocketBackend;
    use ratatui::Terminal;
    use ratatui::layout::Rect;

    let parser = make_test_parser();
    let screen = parser.screen().clone();
    let area = Rect::new(0, 0, BENCH_COLS, BENCH_ROWS);

    let mut group = c.benchmark_group("socket_backend");
    group.throughput(criterion::Throughput::Elements(
        u64::from(BENCH_COLS) * u64::from(BENCH_ROWS),
    ));

    group.bench_function("custom_widget_full_diff", |b| {
        let backend = SocketBackend::new(BENCH_COLS, BENCH_ROWS);
        let mut terminal = Terminal::new(backend).unwrap();
        b.iter(|| {
            terminal
                .draw(|frame| {
                    frame.render_widget(CustomPaneBlit { screen: &screen }, area);
                })
                .unwrap();
            // Drain output (simulates sending to attach socket)
            let output = terminal.backend_mut().take_output();
            criterion::black_box(output.len());
        });
    });

    group.finish();
}

criterion_group!(benches, bench_pane_body, bench_socket_backend_output);
criterion_main!(benches);
