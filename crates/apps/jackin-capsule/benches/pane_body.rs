// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, layout::Rect, widgets::Widget};
use termpane::{Color as TermColor, DamageGrid};

const BENCH_COLS: u16 = 200;
const BENCH_ROWS: u16 = 50;

fn make_test_grid() -> DamageGrid {
    let mut grid = DamageGrid::new(BENCH_ROWS, BENCH_COLS, 1000);
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
        grid.process(line.as_bytes());
    }
    grid
}

struct CustomPaneBlit<'a> {
    grid: &'a DamageGrid,
}

impl Widget for CustomPaneBlit<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (screen_rows, screen_cols) = self.grid.size();
        for row in 0..area.height.min(screen_rows) {
            for col in 0..area.width.min(screen_cols) {
                let Some(cell) = self.grid.cell(row, col) else {
                    continue;
                };
                let buf_cell = &mut buf[(area.x + col, area.y + row)];
                let contents = cell.contents();
                if contents.is_empty() {
                    buf_cell.set_char(' ');
                } else {
                    buf_cell.set_symbol(contents);
                }
                buf_cell.set_fg(term_color_to_ratatui(cell.fgcolor()));
                buf_cell.set_bg(term_color_to_ratatui(cell.bgcolor()));
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

fn term_color_to_ratatui(color: TermColor) -> ratatui::style::Color {
    match color {
        TermColor::Default => ratatui::style::Color::Reset,
        TermColor::Idx(idx) => ratatui::style::Color::Indexed(idx),
        TermColor::Rgb(r, g, b) => ratatui::style::Color::Rgb(r, g, b),
    }
}

fn render_raw_ansi_baseline(grid: &DamageGrid, output: &mut Vec<u8>) {
    output.clear();
    let (rows, cols) = grid.size();
    for row in 0..rows {
        let r1 = row + 1;
        use std::io::Write as _;
        let _unused = write!(output, "\x1b[{r1};1H");
        let mut last_fg = TermColor::Default;
        let mut last_bg = TermColor::Default;
        for col in 0..cols {
            let Some(cell) = grid.cell(row, col) else {
                output.push(b' ');
                continue;
            };
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

fn bench_pane_body(c: &mut Criterion) {
    let grid = make_test_grid();
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
            format!("{BENCH_COLS}x{BENCH_ROWS}"),
        ),
        &grid,
        |b, grid| {
            b.iter(|| {
                terminal
                    .draw(|frame| {
                        frame.render_widget(CustomPaneBlit { grid }, area);
                    })
                    .unwrap();
            });
        },
    );

    group.bench_with_input(
        BenchmarkId::new("raw_ansi_baseline", format!("{BENCH_COLS}x{BENCH_ROWS}")),
        &grid,
        |b, grid| {
            b.iter(|| {
                render_raw_ansi_baseline(grid, &mut ansi_buf);
            });
        },
    );

    group.finish();
}

fn bench_socket_backend_output(c: &mut Criterion) {
    #[expect(
        clippy::unwrap_used,
        reason = "benchmark setup/render failures should abort the benchmark immediately"
    )]
    fn run_bench(c: &mut Criterion) {
        use jackin_capsule::tui::socket_backend::SocketBackend;
        use ratatui::Terminal;
        use ratatui::layout::Rect;

        let grid = make_test_grid();
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
                        frame.render_widget(CustomPaneBlit { grid: &grid }, area);
                    })
                    .unwrap();
                let output = terminal.backend_mut().take_output();
                black_box(output.len());
            });
        });

        group.finish();
    }

    run_bench(c);
}

fn bench_pty_byte_pump(c: &mut Criterion) {
    let payload = vec![b'x'; 4096];
    c.bench_function("pty_byte_pump_4k_with_telemetry", |b| {
        b.iter(|| {
            let mut grid = DamageGrid::new(50, 200, 1000);
            grid.process(black_box(&payload));
            jackin_diagnostics::metrics::incr_terminal_bytes_received(payload.len() as u64);
            black_box(grid.cursor_position());
        });
    });
}

criterion_group!(
    benches,
    bench_pane_body,
    bench_socket_backend_output,
    bench_pty_byte_pump
);
criterion_main!(benches);
