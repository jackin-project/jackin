// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::io::Write;
use std::path::{Path, PathBuf};

use jackin_xtask::{E2eArgs, E2eTarget};

const SYNTHETIC_SOURCE: &str = "env:SYNTHETIC_PLAN_SOURCE";

fn resolve_target(id: &str) -> Result<E2eTarget, Box<dyn Error>> {
    let (agent, provider) = match id {
        "T09" => ("omp", "zai"),
        "T14" => ("claude", "zai"),
        "T18" => ("codex", "zai"),
        other => return Err(format!("no selection for {other}").into()),
    };
    Ok(E2eTarget::resolve(
        &E2eArgs {
            agent: Some(agent.to_owned()),
            provider: Some(provider.to_owned()),
            credential: Some(SYNTHETIC_SOURCE.to_owned()),
            ..E2eArgs::default()
        },
        Some(Path::new("/home/test")),
    )?)
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os();
    let _program = arguments.next();
    let Some(directory) = arguments.next() else {
        return Err("usage: export_e2e_specs <output-directory>".into());
    };
    let directory = PathBuf::from(directory);
    std::fs::create_dir_all(&directory)?;
    for id in ["T09", "T14", "T18"] {
        let bytes = resolve_target(id)?.probe_spec_bytes()?;
        let path = directory.join(format!("{id}.json"));
        std::fs::write(&path, bytes)?;
        writeln!(std::io::stdout(), "{}", path.display())?;
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    run()
}
