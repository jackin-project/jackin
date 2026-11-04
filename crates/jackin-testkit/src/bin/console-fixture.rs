// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_testkit::{FixtureArgs, fixture_app};

fn main() -> std::io::Result<()> {
    let args: FixtureArgs =
        junie_tui::cli::parse(junie_tui::cli::motion_help(jackin_testkit::NO_MOTION_ENV));
    junie_tui::shell::run(&mut fixture_app(&args))
}
