#[cfg(unix)]
use anyhow::{Context, Result};
#[cfg(unix)]
use jackin_core::JackinPaths;
#[cfg(unix)]
use jackin_runtime::host_daemon::{
    CoredumpPolicy, DaemonLayout, DaemonRequestKind, DaemonResponseKind, install_units, read_log,
    request, serve, uninstall_units,
};

#[cfg(unix)]
use crate::cli::DaemonCommand;

#[cfg(unix)]
pub(super) async fn handle(command: DaemonCommand, paths: &JackinPaths) -> Result<()> {
    let layout = DaemonLayout::new(paths);
    match command {
        DaemonCommand::Serve => {
            serve(&layout, env!("JACKIN_VERSION"))?;
            Ok(())
        }
        DaemonCommand::Install => install(paths),
        DaemonCommand::Uninstall => uninstall(paths),
        DaemonCommand::Start => start(&layout).await,
        DaemonCommand::Stop => stop(&layout),
        DaemonCommand::Restart => {
            drop(stop(&layout));
            start(&layout).await
        }
        DaemonCommand::Status => status(&layout),
        DaemonCommand::Logs => logs(&layout),
    }
}

#[cfg(unix)]
fn install(paths: &JackinPaths) -> Result<()> {
    let exe = std::env::current_exe().context("resolving current jackin executable")?;
    let units = install_units(paths, &exe)?;
    if cfg!(target_os = "macos") {
        println!("installed launchd unit {}", units.launchd_path.display());
        println!(
            "load it with: launchctl load {}",
            units.launchd_path.display()
        );
    } else {
        println!(
            "installed systemd user unit {}",
            units.systemd_path.display()
        );
        println!("enable it with: systemctl --user enable --now jackin-daemon.service");
    }
    Ok(())
}

#[cfg(unix)]
fn uninstall(paths: &JackinPaths) -> Result<()> {
    let exe = std::env::current_exe().context("resolving current jackin executable")?;
    let units = uninstall_units(paths, &exe)?;
    println!("removed {}", units.launchd_path.display());
    println!("removed {}", units.systemd_path.display());
    Ok(())
}

#[cfg(unix)]
async fn start(layout: &DaemonLayout) -> Result<()> {
    if request(
        &layout.socket_path,
        env!("JACKIN_VERSION"),
        DaemonRequestKind::Hello,
    )
    .is_ok()
    {
        println!("daemon already running at {}", layout.socket_path.display());
        return Ok(());
    }
    jackin_runtime::host_daemon::ensure_run_dir(layout)?;
    let log = std::fs::File::create(&layout.log_path)
        .with_context(|| format!("creating daemon log {}", layout.log_path.display()))?;
    let err = log
        .try_clone()
        .with_context(|| format!("cloning daemon log {}", layout.log_path.display()))?;
    let exe = std::env::current_exe().context("resolving current jackin executable")?;
    let child = tokio::process::Command::new(exe)
        .args(["daemon", "serve"])
        .stdin(std::process::Stdio::null())
        .stdout(log)
        .stderr(err)
        .spawn()
        .context("starting jackin daemon")?;
    println!(
        "started daemon pid {} at {}",
        child.id().unwrap_or_default(),
        layout.socket_path.display()
    );
    wait_until_ready(layout).await?;
    Ok(())
}

#[cfg(unix)]
async fn wait_until_ready(layout: &DaemonLayout) -> Result<()> {
    let mut last_error = None;
    for _ in 0..20 {
        match request(
            &layout.socket_path,
            env!("JACKIN_VERSION"),
            DaemonRequestKind::Hello,
        ) {
            Ok(_) => return Ok(()),
            Err(error) => last_error = Some(error),
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    Err(last_error
        .unwrap_or_else(|| anyhow::anyhow!("daemon did not become ready"))
        .context("waiting for daemon readiness"))
}

#[cfg(unix)]
fn stop(layout: &DaemonLayout) -> Result<()> {
    let response = request(
        &layout.socket_path,
        env!("JACKIN_VERSION"),
        DaemonRequestKind::Shutdown,
    )?;
    match response.kind {
        DaemonResponseKind::Shutdown { accepted: true } => {
            println!("daemon stopped");
            Ok(())
        }
        DaemonResponseKind::Error { message } => anyhow::bail!("{message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

#[cfg(unix)]
fn status(layout: &DaemonLayout) -> Result<()> {
    let response = request(
        &layout.socket_path,
        env!("JACKIN_VERSION"),
        DaemonRequestKind::Status,
    )?;
    match response.kind {
        DaemonResponseKind::Status(status) => {
            println!("status: running");
            println!("pid: {}", status.pid);
            println!("protocol: {}", status.protocol_version);
            println!("build: {}", status.build_id);
            println!("socket: {}", status.socket_path.display());
            println!("log: {}", status.log_path.display());
            if status.adapters_enabled.is_empty() {
                println!("adapters: none");
            } else {
                println!("adapters: {}", status.adapters_enabled.join(", "));
            }
            match status.coredump_policy {
                CoredumpPolicy::Disabled => println!("coredumps: disabled"),
                CoredumpPolicy::Unsupported { residual_risk } => {
                    println!("coredumps: unsupported ({residual_risk})");
                }
            }
            Ok(())
        }
        DaemonResponseKind::Error { message } => anyhow::bail!("{message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

#[cfg(unix)]
fn logs(layout: &DaemonLayout) -> Result<()> {
    let contents = read_log(layout)?;
    print!("{contents}");
    Ok(())
}
