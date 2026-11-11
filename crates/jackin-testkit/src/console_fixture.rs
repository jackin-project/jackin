// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use jackin_console::app::App;
use jackin_console::fixtures::{Scenario, app_for};
use junie_tui::cli::CommonArgs;

pub const NO_MOTION_ENV: &str = "JACKIN_NO_MOTION";

#[derive(Clone, Debug, Parser)]
#[command(
    name = "console-fixture",
    about = "The jackin console over the baseline fixture worlds"
)]
pub struct FixtureArgs {
    #[command(flatten)]
    pub common: CommonArgs,
    #[arg(short = 's', long, value_parser = parse_scenario, default_value = "first-use")]
    pub scenario: Scenario,
}

fn parse_scenario(value: &str) -> Result<Scenario, String> {
    Scenario::from_name(value).ok_or_else(|| {
        let names: Vec<&str> = Scenario::ALL.iter().map(|s| s.name()).collect();
        format!(
            "unknown scenario {value:?}; use one of {}",
            names.join(", ")
        )
    })
}

pub fn fixture_app(args: &FixtureArgs) -> App {
    app_for(
        args.scenario,
        args.common.motion(NO_MOTION_ENV),
        args.common.frame,
        args.common.theme(),
    )
}
