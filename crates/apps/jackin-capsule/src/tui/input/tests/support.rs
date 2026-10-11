// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
fn parse_all_default(input: &[u8]) -> Vec<InputEvent> {
    InputParser::default().parse(input)
}

fn parse_all_prefix_only(input: &[u8]) -> Vec<InputEvent> {
    InputParser::new(Some(0x02), None).parse(input)
}

