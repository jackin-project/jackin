// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn measured_bare(pairs: &[(&str, usize)]) -> Measured {
    let mut m = Measured::default();
    for &(name, n) in pairs {
        m.bare_by_crate.insert(name.to_owned(), n);
    }
    m
}

pub(super) fn measured_full(bare: &[(&str, usize)], expects: &[(&str, &str, usize)]) -> Measured {
    let mut m = measured_bare(bare);
    for &(lint, crate_name, n) in expects {
        m.expect_by_lint_crate
            .insert((lint.to_owned(), crate_name.to_owned()), n);
    }
    m
}

pub(super) fn budget_from(crates: &[(&str, usize)], expects: &[(&str, &str, usize)]) -> Budget {
    Budget {
        crates: crates
            .iter()
            .map(|&(name, bare_allow)| CrateBudget {
                name: name.to_owned(),
                bare_allow,
            })
            .collect(),
        expects: expects
            .iter()
            .map(|&(lint, crate_name, count)| ExpectBudget {
                lint: lint.to_owned(),
                crate_name: crate_name.to_owned(),
                count,
            })
            .collect(),
    }
}
