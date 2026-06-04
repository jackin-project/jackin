//! Experiments 1-4 benchmarks: DamageGrid vs vt100 oracle.
//!
//! Run with: cargo bench -p jackin-term
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use jackin_term::{DamageGrid, DirtySpans, WireEmitter};

fn workload_tui_redraw(rows: u16, cols: u16) -> Vec<u8> {
    let mut out = Vec::new();
    for row in 0..rows {
        out.extend_from_slice(
            format!(
                "\x1b[{};1H\x1b[{}mRow {:04} content {:>width$}\x1b[0m",
                row + 1,
                31 + (row % 7),
                row,
                "end",
                width = (cols as usize).saturating_sub(20)
            )
            .as_bytes(),
        );
    }
    out
}

fn workload_scrollback_fill() -> Vec<u8> {
    let mut out = Vec::new();
    for i in 0..10_000usize {
        out.extend_from_slice(format!("Scrollback line {i}\r\n").as_bytes());
    }
    out
}

fn bench_present_frame_damagegrid(c: &mut Criterion) {
    let rows = 24u16;
    let cols = 80u16;
    let workload = workload_tui_redraw(rows, cols);
    let mut grid = DamageGrid::new(rows, cols, 10_000);
    grid.process(&workload);
    let mut emitter = WireEmitter::new();

    c.bench_function("present_frame/damagegrid/tui_redraw", |b| {
        b.iter(|| {
            grid.process(black_box(&workload));
            let snap = grid.dump();
            let spans = grid.dirty_spans();
            emitter.clear();
            emitter.emit_dirty(black_box(&snap), black_box(&spans));
            black_box(emitter.as_bytes());
        });
    });
}

fn bench_present_frame_vt100(c: &mut Criterion) {
    let rows = 24u16;
    let cols = 80u16;
    let workload = workload_tui_redraw(rows, cols);
    let mut parser = vt100::Parser::new(rows, cols, 10_000);
    parser.process(&workload);

    c.bench_function("present_frame/vt100/tui_redraw", |b| {
        b.iter(|| {
            parser.process(black_box(&workload));
            let screen = parser.screen_mut();
            let mut n = 0usize;
            for row in 0..rows {
                for col in 0..cols {
                    if let Some(cell) = screen.cell(row, col) {
                        n += cell.contents().len().max(1);
                    }
                }
            }
            black_box(n);
        });
    });
}

fn bench_wire_bytes(c: &mut Criterion) {
    let mut grid = DamageGrid::new(24, 80, 10_000);
    grid.process(workload_tui_redraw(24, 80).as_slice());
    let snap = grid.dump();
    let mut emitter = WireEmitter::new();

    c.bench_function("wire_bytes/damagegrid/full_frame", |b| {
        b.iter(|| {
            emitter.clear();
            emitter.emit_dirty(black_box(&snap), black_box(&DirtySpans::All));
            black_box(emitter.as_bytes().len())
        });
    });
}

fn bench_allocs_reuse(c: &mut Criterion) {
    let workload = workload_tui_redraw(24, 80);
    let mut grid = DamageGrid::new(24, 80, 10_000);
    grid.process(&workload);
    let mut emitter = WireEmitter::new();
    let snap = grid.dump();
    emitter.emit_full(&snap);

    c.bench_function("allocs/damagegrid/reuse_buffer", |b| {
        b.iter(|| {
            grid.process(black_box(&workload));
            let snap = grid.dump();
            let spans = grid.dirty_spans();
            emitter.clear();
            emitter.emit_dirty(black_box(&snap), black_box(&spans));
            black_box(emitter.as_bytes());
        });
    });
}

fn bench_scrollback_damagegrid(c: &mut Criterion) {
    let workload = workload_scrollback_fill();
    c.bench_function("scrollback/damagegrid/fill_10k", |b| {
        b.iter(|| {
            let mut grid = DamageGrid::new(24, 80, 10_000);
            grid.process(black_box(&workload));
            black_box(grid.scrollback_len())
        });
    });
}

fn bench_scrollback_vt100(c: &mut Criterion) {
    let workload = workload_scrollback_fill();
    c.bench_function("scrollback/vt100/fill_10k", |b| {
        b.iter(|| {
            let mut parser = vt100::Parser::new(24, 80, 10_000);
            parser.process(black_box(&workload));
            let screen = parser.screen_mut();
            let saved = screen.scrollback();
            screen.set_scrollback(usize::MAX);
            let filled = screen.scrollback();
            screen.set_scrollback(saved.min(filled));
            black_box(filled)
        });
    });
}

criterion_group!(
    benches,
    bench_present_frame_damagegrid,
    bench_present_frame_vt100,
    bench_wire_bytes,
    bench_allocs_reuse,
    bench_scrollback_damagegrid,
    bench_scrollback_vt100,
);
criterion_main!(benches);
