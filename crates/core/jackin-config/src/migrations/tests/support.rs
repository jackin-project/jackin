// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn nz(n: u32) -> NonZeroU32 {
    NonZeroU32::new(n).expect("non-zero literal in test")
}

pub(super) fn assert_registry_reaches(migrations: &[MigrationStep], current_raw: &str) {
    assert_registry_chain(migrations, current_raw);
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(super) fn alpha1_to_alpha2(doc: &mut DocumentMut) -> crate::ConfigResult<()> {
    doc["alpha1_to_alpha2"] = toml_edit::value(true);
    Ok(())
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "documented residual allow; prefer expect when site is lint-true"
)]
pub(super) fn alpha2_to_alpha3(doc: &mut DocumentMut) -> crate::ConfigResult<()> {
    doc["alpha2_to_alpha3"] = toml_edit::value(true);
    Ok(())
}
