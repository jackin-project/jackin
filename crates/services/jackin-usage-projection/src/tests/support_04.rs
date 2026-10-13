// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn parity_mega_screen(render_at: i64) -> UsageScreenState {
    // The fixture and renderer share one explicit epoch. This keeps relative
    // labels stable without mutating fixture timestamps to wall time.
    let providers = parity_mega_providers();
    let (projection, _) = parity_projection_at(
        render_at,
        &providers,
        parity_unresolved_entries(),
        vec![parity_projection_issue()],
    );
    UsageScreenState::from_projection(&projection)
}
