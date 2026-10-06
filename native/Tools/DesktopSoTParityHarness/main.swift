// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

/// CLT-safe checks for the production navigation and grouped DTO adapters.

import Foundation
import JackinUsageBridge

@main
struct DesktopSoTParityHarness {
    static func main() {
        var failures = 0
        var checks = 0
        func check(_ name: String, _ ok: Bool) {
            checks += 1
            print("\(ok ? "PASS" : "FAIL")  \(name)")
            if !ok { failures += 1 }
        }

        check(
            "provider click preserves provider",
            StatusPopoverFocus.outcome(surfaceId: "claude", isFallbackItem: false)
                == .provider("claude")
        )
        check(
            "fallback opens Overview",
            StatusPopoverFocus.outcome(surfaceId: nil, isFallbackItem: true) == .overview
        )

        let codexA = account(surface: "codex", key: "a1", label: "a@example.test", pct: 57)
        let codexB = account(surface: "codex", key: "a2", label: "b@example.test", pct: 0)
        let claude = account(surface: "claude", key: "p1", label: "Personal", pct: 12)
        let groups = [
            group(surface: "claude", label: "Anthropic", accounts: [claude]),
            group(surface: "codex", label: "OpenAI", accounts: [codexA, codexB]),
        ]
        let tree = OverviewInventory.tree(groups: groups)
        check(
            "provider order remains Rust order",
            tree.map(\.providerLabel) == ["Anthropic", "OpenAI"])
        check(
            "OpenAI appears once as parent", tree.filter { $0.providerLabel == "OpenAI" }.count == 1
        )
        check("OpenAI has two account children", tree[1].children?.count == 2)
        check(
            "child provider cell is Rust placeholder", tree[1].children?.first?.providerLabel == "—"
        )
        check("child remaining is verbatim", tree[1].children?.last?.remainingLabel == "0%")
        check(
            "child accessibility keeps provider context",
            tree[1].children?.first?.accessibilityLabel
                == "OpenAI, a@example.test, Plan, 57%, Resets in 3d"
        )
        check(
            "usage URL rides grouped projection",
            groups[1].usageURL == "https://chatgpt.com/codex/settings/usage"
                && URL(string: groups[1].usageURL ?? "") != nil
        )
        check(
            "status menu remains native three-action model",
            StatusItemMenuModel.rows.map(\.action) == [.openUsageWindow, .refresh, .quit]
        )

        print("---")
        if failures == 0 {
            print("DesktopSoTParityHarness: ALL PASS (\(checks)/\(checks))")
            exit(0)
        }
        print("DesktopSoTParityHarness: \(failures) FAILURE(S)")
        exit(1)
    }

    private static func group(
        surface: String,
        label: String,
        accounts: [PresentationStore.AccountRow]
    ) -> PresentationStore.ProviderGroupRow {
        PresentationStore.ProviderGroupRow(
            surfaceId: surface,
            displayLabel: label,
            iconKey: surface,
            fallbackGlyph: surface == "codex" ? "Cx" : "Cl",
            usageURL: surface == "codex"
                ? "https://chatgpt.com/codex/settings/usage"
                : "https://claude.ai/settings/usage",
            accountColumnLabel: "—",
            planOrStatusLabel: "—",
            remainingLabel: "—",
            resetDisplayLabel: "—",
            accounts: accounts,
            accessibilityLabel: label,
            lastError: nil
        )
    }

    private static func account(
        surface: String,
        key: String,
        label: String,
        pct: UInt8
    ) -> PresentationStore.AccountRow {
        let provider = surface == "codex" ? "OpenAI" : "Anthropic"
        return PresentationStore.AccountRow(
            surfaceId: surface,
            providerColumnLabel: "—",
            accountKey: key,
            accountLabel: label,
            planLabel: "Plan",
            selected: key == "a1" || key == "p1",
            lifecycle: "current",
            lifecycleLabel: "Current account",
            provenanceLabel: "Live host",
            planOrStatusLabel: "Plan",
            remainingPercent: pct,
            remainingLabel: "\(pct)%",
            headline: "\(pct)% left",
            resetDisplayLabel: "Resets in 3d",
            statusWord: "fresh",
            statusLabel: "fresh",
            severity: "normal",
            updatedLabel: "Updated now",
            lastError: nil,
            dimmed: false,
            accessibilityLabel: "\(provider), \(label), Plan, \(pct)%, Resets in 3d"
        )
    }
}
