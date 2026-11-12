// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::fmt;
use std::io::Write;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use super::redact::Redactor;
use super::verdict::{LineId, Status, StepId, StepLine, Verdict, render};

#[derive(Clone)]
pub struct Sink {
    inner: Arc<Mutex<Box<dyn Write + Send>>>,
}

impl fmt::Debug for Sink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Sink")
    }
}

#[derive(Clone, Debug, Default)]
pub struct Capture {
    bytes: Arc<Mutex<Vec<u8>>>,
}

struct CaptureWriter(Arc<Mutex<Vec<u8>>>);

impl Write for CaptureWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Capture {
    #[must_use]
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes.lock().unwrap_or_else(PoisonError::into_inner))
            .into_owned()
    }
}

impl Sink {
    #[must_use]
    pub fn new(writer: Box<dyn Write + Send>) -> Sink {
        Sink {
            inner: Arc::new(Mutex::new(writer)),
        }
    }

    #[must_use]
    pub fn stdout() -> Sink {
        Sink::new(Box::new(std::io::stdout()))
    }

    #[must_use]
    pub fn stderr() -> Sink {
        Sink::new(Box::new(std::io::stderr()))
    }

    #[must_use]
    pub fn capture() -> (Sink, Capture) {
        let capture = Capture::default();
        (
            Sink::new(Box::new(CaptureWriter(Arc::clone(&capture.bytes)))),
            capture,
        )
    }

    fn write_line(&self, text: &str) {
        let mut guard: MutexGuard<'_, Box<dyn Write + Send>> =
            self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        drop(writeln!(guard, "{text}").and_then(|()| guard.flush()));
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Printer {
    stdout: Sink,
    stderr: Sink,
    redactor: Arc<Mutex<Redactor>>,
}

impl Printer {
    #[must_use]
    pub(crate) fn new(stdout: Sink, stderr: Sink) -> Printer {
        Printer {
            stdout,
            stderr,
            redactor: Arc::new(Mutex::new(Redactor::none())),
        }
    }

    pub(crate) fn set_redactor(&self, redactor: Redactor) {
        *self.redactor.lock().unwrap_or_else(PoisonError::into_inner) = redactor;
    }

    #[must_use]
    pub(crate) fn redactor(&self) -> Redactor {
        self.redactor
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub(crate) fn scrub(&self, text: &str) -> String {
        self.redactor().scrub(text)
    }

    pub(crate) fn out(&self, text: &str) {
        self.stdout.write_line(&self.scrub(text));
    }

    pub(crate) fn progress(&self, text: &str) {
        self.stderr.write_line(&self.scrub(text));
    }
}

#[derive(Debug)]
pub(crate) struct OrderedPrinter {
    printer: Printer,
    order: Vec<LineId>,
    waiting: BTreeMap<usize, StepLine>,
    printed: Vec<StepLine>,
    next: usize,
}

impl OrderedPrinter {
    #[must_use]
    pub(crate) fn new(printer: Printer) -> OrderedPrinter {
        let mut order: Vec<LineId> = StepId::ALL.iter().copied().map(LineId::Step).collect();
        order.push(LineId::Clean);
        OrderedPrinter {
            printer,
            order,
            waiting: BTreeMap::new(),
            printed: Vec::new(),
            next: 0,
        }
    }

    pub(crate) fn submit(&mut self, line: StepLine) {
        let Some(position) = self.order.iter().position(|id| *id == line.id) else {
            return;
        };
        if position < self.next || self.waiting.contains_key(&position) {
            return;
        }
        self.waiting.insert(position, line);
        while let Some(ready) = self.waiting.remove(&self.next) {
            self.printer.out(&render(&ready));
            self.printed.push(ready);
            self.next += 1;
        }
    }

    #[must_use]
    pub(crate) fn has(&self, id: LineId) -> bool {
        self.order
            .iter()
            .position(|known| *known == id)
            .is_some_and(|at| at < self.next || self.waiting.contains_key(&at))
    }

    pub(crate) fn skip_missing(&mut self, reason: &str) {
        let missing: Vec<LineId> = self
            .order
            .iter()
            .copied()
            .filter(|id| *id != LineId::Clean && !self.has(*id))
            .collect();
        for id in missing {
            self.submit(StepLine {
                id,
                verdict: Verdict::skip(reason),
                elapsed: Duration::ZERO,
            });
        }
    }

    #[must_use]
    pub(crate) fn lines(&self) -> Vec<StepLine> {
        let mut all = self.printed.clone();
        all.extend(self.waiting.values().cloned());
        all
    }

    pub(crate) fn result(&self, elapsed: Duration, host_login: &str) {
        let lines = self.lines();
        let count = |status| {
            lines
                .iter()
                .filter(|line| line.verdict.status() == status)
                .count()
        };
        self.printer.out(&format!(
            "RESULT PASS={} FAIL={} SKIP={} elapsed={:.1}s a1_host_login={host_login}",
            count(Status::Pass),
            count(Status::Fail),
            count(Status::Skip),
            elapsed.as_secs_f64()
        ));
    }
}
