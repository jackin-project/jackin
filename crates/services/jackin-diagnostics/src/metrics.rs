use jackin_telemetry::{Attr, Value, counter, histogram, metric};

pub fn record_frame(bytes: u64, cursor_moves: u64, painted_cells: u64) {
    let output = [Attr {
        key: jackin_telemetry::schema::attrs::STREAM_DIRECTION,
        value: Value::Str("output"),
    }];
    let _bytes_result = counter(&metric::TERMINAL_BYTES).add(bytes, &output);
    let _cursor_result = counter(&metric::TERMINAL_CURSOR_MOVES).add(cursor_moves, &[]);
    let _cells_result = counter(&metric::TERMINAL_RENDER_CELLS).add(painted_cells, &[]);
    let _frames_result = counter(&metric::TERMINAL_RENDER_FRAMES).add(1, &[]);
}

pub fn record_render(duration_us: u64, painted_cells: u64) {
    let _duration_result =
        histogram(&metric::TERMINAL_RENDER_DURATION).record(duration_us as f64 / 1_000_000.0, &[]);
    if painted_cells > 0 {
        let _cells_result = counter(&metric::TERMINAL_RENDER_CELLS).add(painted_cells, &[]);
    }
}

pub fn incr_terminal_bytes_received(bytes: u64) {
    let input = [Attr {
        key: jackin_telemetry::schema::attrs::STREAM_DIRECTION,
        value: Value::Str("input"),
    }];
    let _bytes_result = counter(&metric::TERMINAL_BYTES).add(bytes, &input);
}

pub fn incr_mouse_events() {
    let _mouse_result = counter(&metric::TERMINAL_INPUT_MOUSE).add(1, &[]);
}

pub fn incr_docker_inspect() {}

pub fn incr_db_statement(_statement_name: &'static str) {}

pub fn incr_accounts_refreshed(_count: u64) {}

pub fn incr_errors(_error_type: &str) {}
