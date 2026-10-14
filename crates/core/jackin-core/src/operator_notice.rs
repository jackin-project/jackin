// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::{OnceLock, RwLock};

pub trait OperatorNoticeSink: Send + Sync + 'static {
    fn notice(&self, kind: &str, line: &str);
}

static GLOBAL_SINK: OnceLock<RwLock<Box<dyn OperatorNoticeSink>>> = OnceLock::new();

fn global() -> &'static RwLock<Box<dyn OperatorNoticeSink>> {
    GLOBAL_SINK.get_or_init(|| RwLock::new(Box::new(NoopSink)))
}

pub fn set_global_sink(sink: Box<dyn OperatorNoticeSink>) {
    let cell = global();
    if let Ok(mut guard) = cell.write() {
        *guard = sink;
    }
}

pub fn emit_compact_line(kind: &str, line: &str) {
    if let Ok(guard) = global().read() {
        (**guard).notice(kind, line);
    }
}

struct NoopSink;
impl OperatorNoticeSink for NoopSink {
    fn notice(&self, _kind: &str, _line: &str) {}
}
