// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import Testing

@testable import JackinUsageBridge

@Suite("Generated project baseline")
struct ProjectBaselineTests {
    @Test("Provider status focus preserves provider identity")
    func providerStatusFocus() {
        let outcome = StatusPopoverFocus.outcome(surfaceId: "codex", isFallbackItem: false)

        #expect(outcome == .provider("codex"))
        #expect(StatusPopoverFocus.popoverSelection(for: outcome) == "codex")
    }

    @Test("Status-item context menu keeps native command order")
    func statusItemContextMenuOrder() {
        #expect(
            StatusItemMenuModel.rows.map(\.title) == [
                "Open Usage Window",
                "Refresh",
                "Quit jackin❯ desktop",
            ]
        )
        #expect(
            StatusItemMenuModel.rows.map(\.action) == [
                .openUsageWindow,
                .refresh,
                .quit,
            ]
        )
    }
}
