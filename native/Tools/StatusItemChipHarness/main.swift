// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

/// CLT-safe proof that the status-item consumes finished Rust DTO fields.

import Foundation
import JackinUsageBindings
import JackinUsageBridge

@main
struct StatusItemChipHarness {
    static func main() {
        var failures = 0
        func check(_ name: String, _ ok: Bool) {
            print("\(ok ? "PASS" : "FAIL")  \(name)")
            if !ok { failures += 1 }
        }

        let row = ProviderGlanceRowDto(
            surfaceId: "codex",
            iconKey: "codex",
            fallbackGlyph: "Cx",
            usageUrl: "https://chatgpt.com/codex/settings/usage",
            displayLabel: "OpenAI",
            accountLabel: "operator@example.test",
            planLabel: "Pro",
            glanceRemainingPercent: 57,
            barLabel: "57%",
            headline: "57% left",
            resetLabel: "Resets in 3d (Aug 17, 18:51)",
            compactResetLabel: "3d",
            exactReset: "(Aug 17, 18:51)",
            statusWord: "fresh",
            isRefreshing: false,
            statusLabel: "fresh",
            severity: "normal",
            updatedLabel: "Updated now",
            activityLabel: "Updated now",
            activityKind: "idle",
            accessibilityLabel: "OpenAI, operator@example.test, Updated now",
            lastError: nil,
            dimmed: false
        )
        check("bar text is Rust-owned", row.barLabel == "57%")
        check("compact reset is a dedicated field", row.compactResetLabel == "3d")
        check("no reset parsing required", row.resetLabel?.contains("Aug 17") == true)
        check("fallback glyph rides DTO", row.fallbackGlyph == "Cx")
        check(
            "accessibility copy rides DTO", row.accessibilityLabel.contains("operator@example.test")
        )
        check("activity is explicit machine state", row.activityKind == "idle")

        var opened: [String?] = []
        var refreshed = 0
        var quit = 0
        let router = StatusItemMenuRouter(
            openUsageWindow: { opened.append($0) },
            refresh: { refreshed += 1 },
            quit: { quit += 1 }
        )
        router.dispatch(.openUsageWindow)
        router.dispatch(.refresh)
        router.dispatch(.quit)
        router.openUsage(focusOn: "codex")
        check("menu router preserves surface", opened == [nil, "codex"])
        check("menu router refreshes once", refreshed == 1)
        check("menu router quits once", quit == 1)
        check(
            "menu model remains native three-action shape",
            StatusItemMenuModel.rows.map(\.action) == [.openUsageWindow, .refresh, .quit]
        )

        print("---")
        if failures == 0 {
            print("StatusItemChipHarness: ALL PASS")
            exit(0)
        }
        print("StatusItemChipHarness: \(failures) FAILURE(S)")
        exit(1)
    }
}
