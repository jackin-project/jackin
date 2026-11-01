// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! P0.6 pane prototype: two panes against one sandbox. Left is the main process over the
//! `openshell-main` SSH subsystem, right is a shell over `ExecSandboxInteractive`.

mod app;
mod keys;
mod term;
mod transport;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Result, bail};
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

use app::{App, AppEvent, Config, PaneId};
use transport::Remote;

struct Args {
    gateway: String,
    workspace: String,
    sandbox: String,
    dump: Option<PathBuf>,
    log: Option<PathBuf>,
    nudge: bool,
    resync: bool,
}

fn parse_args() -> Result<Args> {
    let mut positional = Vec::new();
    let (mut dump, mut log, mut nudge, mut resync) = (None, None, false, false);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--dump" => dump = args.next().map(PathBuf::from),
            "--log" => log = args.next().map(PathBuf::from),
            "--nudge" => nudge = true,
            "--resync" => resync = true,
            _ => positional.push(arg),
        }
    }
    let [gateway, workspace, sandbox] = &positional[..] else {
        bail!(
            "usage: p06-panes <gateway> <workspace> <sandbox> [--dump DIR] [--log FILE] [--nudge] [--resync]"
        );
    };
    Ok(Args {
        gateway: gateway.clone(),
        workspace: workspace.clone(),
        sandbox: sandbox.clone(),
        dump,
        log,
        nudge,
        resync,
    })
}

fn buffer_text(buffer: &Buffer) -> String {
    let area = buffer.area;
    let mut out = String::new();
    for y in area.top()..area.bottom() {
        let row: String = (area.left()..area.right())
            .map(|x| buffer[(x, y)].symbol())
            .collect();
        out.push_str(row.trim_end());
        out.push('\n');
    }
    out
}

fn sgr(color: Color, foreground: bool) -> String {
    let base = if foreground { 38 } else { 48 };
    match color {
        Color::Indexed(index) => format!("{base};5;{index}"),
        Color::Rgb(r, g, b) => format!("{base};2;{r};{g};{b}"),
        _ => (base + 1).to_string(),
    }
}

/// The buffer with its styles as ANSI SGR, for style assertions.
fn buffer_ansi(buffer: &Buffer) -> String {
    let area = buffer.area;
    let mut out = String::new();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let cell = &buffer[(x, y)];
            let bold = cell.modifier.contains(Modifier::BOLD);
            out.push_str(&format!(
                "\x1b[0;{};{}{}m{}",
                sgr(cell.fg, true),
                sgr(cell.bg, false),
                if bold { ";1" } else { "" },
                cell.symbol()
            ));
        }
        out.push_str("\x1b[0m\n");
    }
    out
}

fn dump(dir: &Path, buffer: &Buffer) -> std::io::Result<()> {
    for (name, text) in [
        ("screen.txt", buffer_text(buffer)),
        ("screen.ansi", buffer_ansi(buffer)),
    ] {
        let tmp = dir.join(format!("{name}.tmp"));
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, dir.join(name))?;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = parse_args()?;
    let client = transport::connect(&args.gateway).await?;
    let remote = Remote {
        client,
        workspace: args.workspace,
        sandbox: args.sandbox,
    };
    let mut log = args.log.as_ref().map(std::fs::File::create).transpose()?;
    let started = Instant::now();

    let (tx, mut rx) = mpsc::unbounded_channel::<AppEvent>();
    let mut terminal = ratatui::init();
    let size = terminal.size()?;
    let config = Config {
        color: std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty()),
        nudge: args.nudge,
        resync: args.resync,
    };
    let mut app = App::new((size.width, size.height), remote, tx.clone(), config);
    app.attach(PaneId::Left);
    app.attach(PaneId::Right);

    let keys = tx.clone();
    tokio::spawn(async move {
        let mut stream = crossterm::event::EventStream::new();
        while let Some(Ok(event)) = stream.next().await {
            if keys.send(AppEvent::Term(event)).is_err() {
                break;
            }
        }
    });

    let result = run(
        &mut terminal,
        &mut app,
        &mut rx,
        args.dump.as_deref(),
        &mut log,
        started,
    )
    .await;
    ratatui::restore();
    result
}

async fn run(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    rx: &mut mpsc::UnboundedReceiver<AppEvent>,
    dump_dir: Option<&Path>,
    log: &mut Option<std::fs::File>,
    started: Instant,
) -> Result<()> {
    loop {
        let completed = terminal.draw(|frame| app.render(frame))?;
        if let Some(dir) = dump_dir {
            dump(dir, completed.buffer)?;
        }
        if let Some(file) = log.as_mut() {
            for line in app.log.drain(..) {
                writeln!(file, "[+{:7.2}s] {line}", started.elapsed().as_secs_f32())?;
            }
        } else {
            app.log.clear();
        }
        if app.quit {
            app.detach(PaneId::Left);
            return Ok(());
        }
        let Some(event) = rx.recv().await else {
            return Ok(());
        };
        app.update(event);
        while let Ok(event) = rx.try_recv() {
            app.update(event);
        }
        app.sync_sizes();
    }
}
