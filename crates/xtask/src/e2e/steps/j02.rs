// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::path::Path;

use crate::e2e::host::HostOutput;
use crate::e2e::run::{Fail, Res, Run, fail};
use crate::e2e::verdict::Verdict;

pub(crate) const MYSTERY: &str = "mystery_field";

pub(crate) fn copy_role(source: &Path, dest: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name();
        if name == ".git" || name == "target" {
            continue;
        }
        let kind = entry.file_type()?;
        let target = dest.join(&name);
        if kind.is_dir() {
            copy_role(&entry.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

pub(crate) fn judge_strict(control: &HostOutput, mutated: &HostOutput) -> Vec<String> {
    let mut problems = Vec::new();
    if !control.ok() {
        problems.push(format!(
            "the control role failed `role check`: {}",
            control.stderr_text().trim()
        ));
    }
    if mutated.ok() {
        problems.push("the role with an unknown field passed `role check`".to_owned());
    } else if !format!("{}{}", mutated.stdout_text(), mutated.stderr_text()).contains(MYSTERY) {
        problems.push(format!("`role check` did not name {MYSTERY}"));
    }
    problems
}

impl Run<'_> {
    pub(crate) async fn j02(&mut self) -> Verdict {
        self.j02_inner().await.unwrap_or_else(Fail::verdict)
    }

    pub(crate) fn mutated_role(&self, name: &str, edit: impl Fn(&Path) -> Res<()>) -> Res<String> {
        let dest = self.scratch.role_copies().join(name);
        copy_role(&self.role, &dest)
            .map_err(|error| fail(format!("cannot copy the role: {error}")))?;
        edit(&dest)?;
        Ok(dest.display().to_string())
    }

    async fn j02_inner(&mut self) -> Res<Verdict> {
        let manifest = self.mutated_role("strict", |dir| {
            let path = dir.join("jackin.role.toml");
            let text = fs::read_to_string(&path)?;
            let edited = text.replacen("schema = 1\n", &format!("schema = 1\n{MYSTERY} = 2\n"), 1);
            if edited == text {
                return Err(fail("the role manifest has no `schema = 1` line to extend"));
            }
            fs::write(&path, edited)?;
            Ok(())
        })?;
        let cwd = self.projects.main.clone();
        let role = self.role.display().to_string();
        let control = self
            .jackin("J02 control", &["role", "check", &role], &cwd)
            .await?;
        let mutated = self
            .jackin("J02 mutated", &["role", "check", &manifest], &cwd)
            .await?;
        let problems = judge_strict(&control, &mutated);
        if problems.is_empty() {
            Ok(Verdict::pass("mystery_field named, control exits 0"))
        } else {
            Ok(Verdict::fail(problems.join("; ")))
        }
    }
}
