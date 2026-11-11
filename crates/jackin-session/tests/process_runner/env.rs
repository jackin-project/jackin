// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::error::Error;

use jackin_core::{EnvValue, ProcessRunner, ProcessSpec, ProcessStatus, Secret};
use jackin_session::TokioProcessRunner;
use tokio_util::sync::CancellationToken;

use crate::support::{Scratch, TestResult, guarded, text};

const REPORT: &str = "echo \"CARGO_PKG_NAME=${CARGO_PKG_NAME-<unset>}\"\necho \"CARGO_PKG_VERSION=${CARGO_PKG_VERSION-<unset>}\"\necho \"CARGO_MANIFEST_DIR=${CARGO_MANIFEST_DIR-<unset>}\"\necho \"JK_PLAIN=${JK_PLAIN-<unset>}\"\necho \"JK_SECRET=${JK_SECRET-<unset>}\"\npwd -P\n";
const PLAIN: &str = "a b=c $HOME 'q' \u{e9}";
const SECRET: &str = "S3cr3t-Child-52";

struct Report {
    vars: BTreeMap<String, String>,
    cwd: String,
}

async fn report(
    scratch: &Scratch,
    edit: impl FnOnce(&mut ProcessSpec),
) -> Result<Report, Box<dyn Error>> {
    let mut spec = scratch.spec(REPORT, &[])?;
    edit(&mut spec);
    let output = TokioProcessRunner
        .run(&spec, None, &CancellationToken::new())
        .await?;
    assert_eq!(output.status, ProcessStatus::Exited(0));
    let printed = String::from_utf8(output.stdout)?;
    let mut vars = BTreeMap::new();
    let mut cwd = String::new();
    for line in printed.lines() {
        match line.split_once('=') {
            Some((name, value)) if name.chars().all(|c| c.is_ascii_uppercase() || c == '_') => {
                vars.insert(name.to_owned(), value.to_owned());
            }
            _ => cwd = line.to_owned(),
        }
    }
    Ok(Report { vars, cwd })
}

fn var<'a>(report: &'a Report, name: &str) -> &'a str {
    report
        .vars
        .get(name)
        .map_or("<missing line>", String::as_str)
}

fn plain(name: &str, value: &str) -> (String, EnvValue) {
    (name.to_owned(), EnvValue::Plain(value.to_owned()))
}

#[tokio::test]
async fn the_child_gets_the_spec_environment_after_prefix_removal_and_the_spec_cwd() -> TestResult {
    guarded(async {
        let scratch = Scratch::new()?;
        let control = report(&scratch, |_| {}).await?;
        assert_eq!(
            var(&control, "CARGO_PKG_NAME"),
            env!("CARGO_PKG_NAME"),
            "positive control: run the test under cargo nextest"
        );
        assert_eq!(
            var(&control, "CARGO_PKG_VERSION"),
            env!("CARGO_PKG_VERSION")
        );
        assert_ne!(var(&control, "CARGO_MANIFEST_DIR"), "<unset>");
        assert_eq!(control.cwd, text(scratch.work())?, "spec cwd");

        let removed = report(&scratch, |spec| {
            spec.env_remove_prefixes.push("CARGO_PKG_".to_owned());
            spec.env.push(plain("JK_PLAIN", PLAIN));
            spec.env.push((
                "JK_SECRET".to_owned(),
                EnvValue::Secret(Secret::new(SECRET.to_owned())),
            ));
        })
        .await?;
        assert_eq!(var(&removed, "CARGO_PKG_NAME"), "<unset>");
        assert_eq!(var(&removed, "CARGO_PKG_VERSION"), "<unset>");
        assert_eq!(
            var(&removed, "CARGO_MANIFEST_DIR"),
            var(&control, "CARGO_MANIFEST_DIR"),
            "selective removal"
        );
        assert_eq!(var(&removed, "JK_PLAIN"), PLAIN);
        assert_eq!(var(&removed, "JK_SECRET"), SECRET);

        let overridden = report(&scratch, |spec| {
            spec.env_remove_prefixes.push("CARGO_PKG_".to_owned());
            spec.env.push(plain("CARGO_PKG_NAME", "override"));
        })
        .await?;
        assert_eq!(
            var(&overridden, "CARGO_PKG_NAME"),
            "override",
            "apply after removal"
        );
        assert_eq!(var(&overridden, "CARGO_PKG_VERSION"), "<unset>");

        let replaced = report(&scratch, |spec| {
            spec.env.push(plain("CARGO_MANIFEST_DIR", "override"));
        })
        .await?;
        assert_eq!(var(&replaced, "CARGO_MANIFEST_DIR"), "override");

        let empty_prefix = report(&scratch, |spec| {
            spec.env_remove_prefixes.push(String::new());
        })
        .await?;
        assert_eq!(
            var(&empty_prefix, "CARGO_PKG_NAME"),
            env!("CARGO_PKG_NAME"),
            "an empty prefix matches nothing"
        );
        Ok(())
    })
    .await
}
