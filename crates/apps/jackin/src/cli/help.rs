// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::io::Write;

use anyhow::Context as _;
use clap::CommandFactory;

use super::Cli;

pub fn exec(command: &[String]) -> anyhow::Result<()> {
    let root = Cli::command();

    let mut target: &clap::Command = &root;
    for part in command {
        target = target
            .find_subcommand(part.as_str())
            .ok_or_else(|| anyhow::anyhow!("no such subcommand: `{}`", command.join(" ")))?;
    }

    let mut roff: Vec<u8> = Vec::new();
    clap_mangen::Man::new(target.clone())
        .render(&mut roff)
        .context("failed to render man page")?;

    if try_man(&roff)? {
        return Ok(());
    }

    let text = target.clone().render_long_help().to_string();
    if try_pager(text.as_bytes())? {
        return Ok(());
    }

    println!("{text}");
    Ok(())
}

fn try_man(content: &[u8]) -> anyhow::Result<bool> {
    let mut tmp = tempfile::Builder::new()
        .suffix(".1")
        .tempfile()
        .context("failed to create man temp file")?;
    tmp.write_all(content)
        .context("failed to write man temp file")?;

    let mut request = inherited_request("man", &[]);
    request.args.push(tmp.path().into());
    Ok(crate::process_telemetry::exec_sync_optional(&request)?.is_some())
}

fn try_pager(content: &[u8]) -> anyhow::Result<bool> {
    let mut tmp = tempfile::Builder::new()
        .suffix(".txt")
        .tempfile()
        .context("failed to create pager temp file")?;
    tmp.write_all(content)
        .context("failed to write pager temp file")?;

    let pagers: &[(&str, &[&str])] = &[("less", &["-R"]), ("more", &[])];
    for (pager, args) in pagers {
        let mut request = inherited_request(pager, args);
        request.args.push(tmp.path().into());
        if crate::process_telemetry::exec_sync_optional(&request)?.is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn inherited_request(program: &str, args: &[&str]) -> jackin_process::ExecRequest {
    jackin_process::ExecRequest::new(program, args.iter().copied())
        .stdin_mode(jackin_process::StdioMode::Inherit)
        .stdout_mode(jackin_process::StdioMode::Inherit)
        .stderr_mode(jackin_process::StdioMode::Inherit)
}
