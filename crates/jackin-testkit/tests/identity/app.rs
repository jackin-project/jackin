// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::time::{Duration, Instant};

use tempfile::TempDir;
use tui_test::{
    CheckedMouseAction, LaunchEnvMode, LaunchOptions, MouseOptions, OpenOptions, Operation,
    OperationResult, RunOptions, Session, SnapshotCell, SnapshotScope, TerminalSnapshot,
    WaitQuietOptions,
};
use tuiscotti::check::{CheckOutcome, FrozenStore};
use tuiscotti::sample::{Observation, Sample};

use crate::baseline;

const TIMEOUT: Duration = Duration::from_secs(20);
const POLL: Duration = Duration::from_millis(25);
const QUIET: Duration = Duration::from_millis(150);
const STABLE: Duration = Duration::from_millis(700);
const STABLE_POLL: Duration = Duration::from_millis(100);
const STABLE_LIMIT: Duration = Duration::from_millis(5000);

pub(crate) const FIXTURE: &str = env!("CARGO_BIN_EXE_console-fixture");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Point {
    pub(crate) x: u16,
    pub(crate) y: u16,
}

pub(crate) struct App {
    session: Session,
    _home: TempDir,
}

#[expect(
    clippy::disallowed_methods,
    reason = "the driver polls a blocking terminal session on its own test thread"
)]
fn pause(duration: Duration) {
    std::thread::sleep(duration);
}

fn artifacts() -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("identity")
}

impl App {
    pub(crate) fn new(name: &str, args: &[&str], cols: u16, rows: u16, color: &str) -> Self {
        let home = tempfile::Builder::new()
            .prefix(name)
            .tempdir()
            .expect("create the isolated application home");
        let path = home.path();
        let mut launch = LaunchOptions::new(FIXTURE)
            .with_args(args.iter().copied())
            .with_cwd(path)
            .with_env_mode(LaunchEnvMode::Cleared)
            .with_env("HOME", path)
            .with_env("XDG_CONFIG_HOME", path.join("config"))
            .with_env("XDG_CACHE_HOME", path.join("cache"))
            .with_env("XDG_DATA_HOME", path.join("data"))
            .with_env("SHELL", "/bin/sh")
            .with_env("PATH", "/usr/bin:/bin")
            .with_env("TERM", "xterm-256color")
            .with_env("COLORTERM", "truecolor")
            .with_env("LANG", "C.UTF-8")
            .with_env("LC_ALL", "C.UTF-8")
            .with_env("TZ", "UTC")
            .with_env("JUNIE_TUI_CLIENT_PLATFORM", "macos")
            .with_env("JUNIE_TUI_TEST_CLOCK", "1")
            .with_env("HOLLA_NO_HISTORY", "1");
        if color == "nocolor" {
            launch = launch.with_env("NO_COLOR", "1");
        } else {
            launch = launch.with_args(["--color", color]);
        }
        let session = Session::new(name);
        let defaults = OpenOptions::default();
        session
            .spawn_with(
                launch,
                RunOptions {
                    backend: defaults.backend,
                    program: String::new(),
                    args: Vec::new(),
                    profile: defaults.profile,
                    cols,
                    rows,
                    cwd: None,
                    env: Vec::new(),
                    wait_ready: Some(false),
                    restart: false,
                    timeouts: defaults.timeouts,
                    recording: defaults.recording,
                    style: defaults.style,
                },
            )
            .expect("spawn console-fixture on a PTY");
        Self {
            session,
            _home: home,
        }
    }

    pub(crate) fn screen(&self) -> TerminalSnapshot {
        self.session
            .capture_snapshot(SnapshotScope::ActiveScreen)
            .expect("capture the live terminal")
    }

    pub(crate) fn wait_screen(
        &self,
        reason: &str,
        predicate: impl Fn(&TerminalSnapshot) -> bool,
    ) -> TerminalSnapshot {
        self.wait_screen_within(reason, TIMEOUT, predicate)
    }

    fn wait_screen_within(
        &self,
        reason: &str,
        budget: Duration,
        predicate: impl Fn(&TerminalSnapshot) -> bool,
    ) -> TerminalSnapshot {
        let deadline = Instant::now() + budget;
        loop {
            let screen = self.screen();
            self.assert_running(reason);
            if predicate(&screen) {
                return screen;
            }
            assert!(
                Instant::now() < deadline,
                "timed out: {reason}\n{}",
                text(&screen)
            );
            pause(POLL);
        }
    }

    fn stable_screen(&self) -> TerminalSnapshot {
        let fingerprint = |screen: &TerminalSnapshot| {
            let mut bytes = serde_json::to_vec(screen.cells()).expect("serialize the cells");
            let cursor = screen.cursor();
            bytes.extend_from_slice(&cursor.col().to_le_bytes());
            bytes.extend_from_slice(&cursor.row().to_le_bytes());
            bytes.push(u8::from(cursor.visible()));
            bytes
        };
        let deadline = Instant::now() + STABLE_LIMIT;
        let mut screen = self.screen();
        let mut previous = fingerprint(&screen);
        let mut stable_since = Instant::now();
        loop {
            pause(STABLE_POLL);
            let candidate = self.screen();
            let current = fingerprint(&candidate);
            if current != previous {
                stable_since = Instant::now();
            }
            screen = candidate;
            previous = current;
            if stable_since.elapsed() >= STABLE || Instant::now() >= deadline {
                break;
            }
        }
        screen
    }

    pub(crate) fn visual(&self, test_id: &str) {
        drop(self.settle());
        let screen = self.stable_screen();
        let sample = Sample::state(test_id, screen, Observation::empty())
            .expect("build the Tuiscotti state sample");
        let store = FrozenStore::new(baseline::root(), &artifacts())
            .expect("open the Tuiscotti frozen store");
        let outcome = store
            .check(&sample.to_bytes().expect("serialize the Tuiscotti sample"))
            .expect("run the Tuiscotti frozen check");
        if let CheckOutcome::Failure(failure) = outcome {
            panic!(
                "{test_id} differs from the approved jackin-preview baseline: {} changed cells {:?}, fields {:?}, report {}",
                failure.changed_cell_total(),
                failure.changed_cells(),
                failure.changed_fields(),
                failure.report_path().display()
            );
        }
    }

    pub(crate) fn wait_text(&self, needle: &str) -> TerminalSnapshot {
        self.wait_screen(&format!("visible {needle}"), |screen| {
            text(screen).contains(needle)
        })
    }

    pub(crate) fn wait_text_after_animation(
        &self,
        needle: &str,
        animation: Duration,
    ) -> TerminalSnapshot {
        self.wait_screen_within(
            &format!("visible {needle}"),
            TIMEOUT + animation,
            |screen| text(screen).contains(needle),
        )
    }

    pub(crate) fn wait_absent(&self, needle: &str) -> TerminalSnapshot {
        self.wait_screen(&format!("absent {needle}"), |screen| {
            !text(screen).contains(needle)
        })
    }

    pub(crate) fn settle(&self) -> TerminalSnapshot {
        self.session
            .wait_quiet(&WaitQuietOptions::new(TIMEOUT, QUIET).expect("a valid quiet window"))
            .expect("the terminal settles after the semantic wait");
        self.screen()
    }

    pub(crate) fn point(&self, needle: &str) -> Point {
        find(&self.wait_text(needle), needle)
    }

    pub(crate) fn key(&self, key: &str) {
        self.session
            .execute(Operation::Key {
                keys: vec![key.to_owned()],
                action: tui_test::KeyAction::Press,
            })
            .unwrap_or_else(|error| panic!("key {key}: {error}"));
    }

    pub(crate) fn type_text(&self, value: &str) {
        self.session
            .write_bytes(value.as_bytes())
            .expect("type through the terminal input");
    }

    pub(crate) fn paste(&self, value: &str) {
        self.session
            .paste(value)
            .expect("paste in the negotiated terminal mode");
    }

    pub(crate) fn advance_clock(&self, milliseconds: u64) {
        self.paste(&format!("junie-tui:advance-clock:{milliseconds}"));
    }

    pub(crate) fn move_to(&self, point: Point) {
        self.mouse(CheckedMouseAction::Motion {
            x: point.x,
            y: point.y,
        });
    }

    pub(crate) fn down(&self, point: Point) {
        self.mouse(CheckedMouseAction::Press {
            x: point.x,
            y: point.y,
            options: MouseOptions::default(),
        });
    }

    pub(crate) fn up(&self, point: Point) {
        self.mouse(CheckedMouseAction::Release {
            x: point.x,
            y: point.y,
            options: MouseOptions::default(),
        });
    }

    fn mouse(&self, action: CheckedMouseAction) {
        self.session
            .mouse_checked(action)
            .expect("the application negotiates SGR mouse input");
    }

    pub(crate) fn click(&self, point: Point) {
        self.down(point);
        self.up(point);
    }

    pub(crate) fn click_text(&self, needle: &str) {
        self.click(self.point(needle));
    }

    pub(crate) fn wheel(&self, point: Point, down: bool, amount: usize) {
        self.move_to(point);
        let bytes = tui_test::input::mouse::scroll(point.x, point.y, !down);
        for _ in 0..amount {
            self.session
                .write_bytes(bytes.as_bytes())
                .expect("wheel through the application PTY");
        }
    }

    fn assert_running(&self, reason: &str) {
        let OperationResult::State(state) = self
            .session
            .execute(Operation::State)
            .expect("process state")
        else {
            panic!("missing process state while {reason}");
        };
        assert!(
            state.exited.is_none(),
            "console-fixture exited while {reason}: {:?}\n{}",
            state.exited,
            text(&self.screen())
        );
    }
}

impl Drop for App {
    fn drop(&mut self) {
        drop(self.session.close());
    }
}

pub(crate) fn text(screen: &TerminalSnapshot) -> String {
    (0..screen.rows())
        .map(|row| row_text(screen, row))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn row_text(screen: &TerminalSnapshot, row: u32) -> String {
    screen
        .row(row)
        .expect("a row in the terminal")
        .iter()
        .map(SnapshotCell::text)
        .collect()
}

pub(crate) fn find_all(screen: &TerminalSnapshot, needle: &str) -> Vec<Point> {
    assert!(!needle.is_empty(), "a locator must not be empty");
    let mut matches = Vec::new();
    for row in 0..screen.rows() {
        let line = row_text(screen, row);
        for (byte, _) in line.match_indices(needle) {
            let mut offset = 0;
            for (column, cell) in screen.row(row).expect("a row").iter().enumerate() {
                let next = offset + cell.text().len();
                if byte >= offset && byte < next {
                    matches.push(Point {
                        x: u16::try_from(column).expect("a column"),
                        y: u16::try_from(row).expect("a row index"),
                    });
                    break;
                }
                offset = next;
            }
        }
    }
    matches
}

pub(crate) fn find(screen: &TerminalSnapshot, needle: &str) -> Point {
    let matches = find_all(screen, needle);
    assert_eq!(
        matches.len(),
        1,
        "expected one physical-row match for {needle:?}, got {matches:?}\n{}",
        text(screen)
    );
    matches[0]
}

pub(crate) fn crop(
    screen: &TerminalSnapshot,
    point: Point,
    width: u16,
    height: u16,
) -> Vec<SnapshotCell> {
    (point.y..point.y + height)
        .flat_map(|y| {
            (point.x..point.x + width).map(move |x| {
                screen
                    .cell_at(u32::from(x), u32::from(y))
                    .expect("a crop inside the terminal")
                    .clone()
            })
        })
        .collect()
}
