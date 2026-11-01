// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use clap::error::ErrorKind;
use jackin_xtask::{CiArgs, CiProfile, CiStep};

#[derive(Parser)]
struct Harness {
    #[command(flatten)]
    ci: CiArgs,
}

fn profile_of(args: &[&str]) -> Result<CiProfile, clap::Error> {
    let argv = std::iter::once("xtask").chain(args.iter().copied());
    Harness::try_parse_from(argv).map(|harness| harness.ci.profile())
}

#[test]
fn ci_steps_are_listed_in_plan_order() {
    assert_eq!(
        CiStep::ALL,
        [
            CiStep::Fmt,
            CiStep::Clippy,
            CiStep::Nextest,
            CiStep::Deny,
            CiStep::Shear,
            CiStep::Comments,
            CiStep::Layers,
            CiStep::Containers,
            CiStep::Reuse,
            CiStep::Docs,
        ]
    );
}

#[test]
fn skip_docs_profile_runs_every_step_except_docs() {
    assert!(CiStep::ALL.iter().all(|step| step.runs_in(CiProfile::Full)));
    let skipped: Vec<CiStep> = CiStep::ALL
        .into_iter()
        .filter(|step| !step.runs_in(CiProfile::SkipDocs))
        .collect();
    assert_eq!(skipped, [CiStep::Docs]);
}

#[test]
fn fast_and_no_docs_flags_both_select_skip_docs_and_nothing_else_skips() {
    assert_eq!(profile_of(&[]).ok(), Some(CiProfile::Full));
    assert_eq!(profile_of(&["--fast"]).ok(), Some(CiProfile::SkipDocs));
    assert_eq!(profile_of(&["--no-docs"]).ok(), Some(CiProfile::SkipDocs));
    assert_eq!(
        profile_of(&["--fast", "--no-docs"]).ok(),
        Some(CiProfile::SkipDocs)
    );
    assert_eq!(
        profile_of(&["--skip-clippy"])
            .err()
            .map(|error| error.kind()),
        Some(ErrorKind::UnknownArgument)
    );
}
