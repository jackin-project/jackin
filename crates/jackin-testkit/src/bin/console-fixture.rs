// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_testkit::{FixtureArgs, fixture_app};

fn main() -> std::io::Result<()> {
    let args: FixtureArgs =
        termrock::cli::parse(termrock::cli::motion_help(jackin_testkit::NO_MOTION_ENV));
    termrock::shell::run(&mut fixture_app(&args))
}
