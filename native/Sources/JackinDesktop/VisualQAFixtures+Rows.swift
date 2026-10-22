// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import Foundation
import JackinUsageBindings
import JackinUsageBridge

struct AccountFixture {
    let surfaceId: String
    let key: String
    let label: String
    let plan: String?
    let remaining: Double?
    let selected: Bool
}

struct GlanceFixture {
    let surfaceId: String
    let iconKey: String
    let label: String
    let accountLabel: String
    let planLabel: String?
    let remaining: Double?
    let reset: String?
    let status: String
    let updated: String
    let error: String?
    let refreshing: Bool
}

extension VisualQAFixtures {
    enum Provider: String, CaseIterable {
        case codex
        case claude
        case amp
        case grok
        case zai
        case kimi
        case minimax

        var label: String {
            switch self {
            case .codex: "OpenAI"
            case .claude: "Anthropic"
            case .amp: "Amp"
            case .grok: "xAI"
            case .zai: "Z.AI"
            case .kimi: "Kimi"
            case .minimax: "MiniMax"
            }
        }

        var remaining: Double {
            switch self {
            case .codex: 57
            case .claude: 12
            case .amp: 100
            case .grok: 72
            case .zai: 81
            case .kimi: 45
            case .minimax: 33
            }
        }

        var reset: String? {
            switch self {
            case .codex: "Resets in 3d"
            case .claude: "Resets in 1h"
            case .amp: "Resets in 18h"
            default: nil
            }
        }

        var compactReset: String? {
            switch self {
            case .codex: "3d"
            case .claude: "1h"
            case .amp: "18h"
            default: nil
            }
        }

        var fallbackGlyph: String {
            switch self {
            case .codex: "Cx"
            case .claude: "Cl"
            case .amp: "Am"
            case .grok: "Gr"
            case .zai: "ZA"
            case .kimi: "Ki"
            case .minimax: "MM"
            }
        }
    }

    static func catalogAccounts() -> [AccountRow] {
        [
            codexPersonal(),
            codexPlus(selected: false),
            claudePersonal(),
            account(
                AccountFixture(

                    surfaceId: "claude",
                    key: "claude-work",
                    label: "Work",
                    plan: "Team",
                    remaining: nil,
                    selected: false

                )),
            defaultAccount(provider: .grok),
            defaultAccount(provider: .zai),
            defaultAccount(provider: .kimi),
            defaultAccount(provider: .minimax),
        ]
    }

    static func codexPersonal(
        selected: Bool = true,
        status: String = "fresh"
    ) -> AccountRow {
        let row = account(
            AccountFixture(

                surfaceId: "codex",
                key: "codex-personal",
                label: "personal@example.test",
                plan: "Pro 20×",
                remaining: 57,
                selected: selected

            ))
        return status == "fresh"
            ? row
            : copiedAccount(row, selected: selected, statusWord: status)
    }

    static func codexPlus(selected: Bool = true) -> AccountRow {
        account(
            AccountFixture(

                surfaceId: "codex",
                key: "codex-plus",
                label: "secondary@example.test",
                plan: "Plus",
                remaining: 0,
                selected: selected

            ))
    }

    static func claudePersonal(selected: Bool = true) -> AccountRow {
        account(
            AccountFixture(

                surfaceId: "claude",
                key: "claude-personal",
                label: "Personal",
                plan: "Max 20×",
                remaining: 12,
                selected: selected

            ))
    }

    static func defaultAccount(
        provider: Provider,
        status: String = "fresh"
    ) -> AccountRow {
        let label = provider == .amp ? "Free" : provider == .grok ? "Team" : "Default"
        let row = account(
            AccountFixture(

                surfaceId: provider.rawValue,
                key: "\(provider.rawValue)-default",
                label: label,
                plan: nil,
                remaining: provider.remaining,
                selected: true

            ))
        return status == "fresh"
            ? row
            : copiedAccount(row, selected: true, statusWord: status)
    }

    static func account(_ fixture: AccountFixture) -> AccountRow {
        let providerLabel = Provider(rawValue: fixture.surfaceId)?.label ?? fixture.surfaceId
        let status = "fresh"
        let planOrStatusLabel = fixture.plan ?? "—"
        let remainingLabel = fixture.remaining.map { "\(Self.percentLabel($0))%" } ?? "—"
        let resetDisplayLabel = "—"
        return AccountRow(
            surfaceId: fixture.surfaceId,
            providerColumnLabel: "—",
            accountKey: fixture.key,
            accountLabel: fixture.label,
            planLabel: fixture.plan,
            selected: fixture.selected,
            lifecycle: "current",
            lifecycleLabel: "Current account",
            provenanceLabel: "Fixture",
            planOrStatusLabel: planOrStatusLabel,
            remainingPercent: fixture.remaining,
            meterPercent: fixture.remaining,
            meterSide: fixture.remaining == nil ? nil : "remaining",
            remainingLabel: remainingLabel,
            headline: fixture.remaining.map { "\(Self.percentLabel($0))% left" } ?? "—",
            resetDisplayLabel: resetDisplayLabel,
            statusWord: status,
            statusLabel: status,
            severity: severity(for: fixture.remaining),
            updatedLabel: "Updated now",
            lastError: nil,
            dimmed: status != "fresh",
            accessibilityLabel:
                "\(providerLabel), \(fixture.label), \(planOrStatusLabel), \(remainingLabel), \(resetDisplayLabel)"
        )
    }

    static func glance(_ fixture: GlanceFixture) -> GlanceProviderRow {
        let provider = Provider(rawValue: fixture.surfaceId)
        let shownAccount =
            fixture.accountLabel.isEmpty ? "No authenticated account" : fixture.accountLabel
        let barLabel = fixture.remaining.map { "\(Self.percentLabel($0))%" } ?? "—"
        let headline = fixture.remaining.map { "\(Self.percentLabel($0))% left" } ?? "—"
        let activityLabel: String
        if fixture.refreshing {
            activityLabel = "Updating…"
        } else {
            switch fixture.status {
            case "fresh": activityLabel = fixture.updated
            case "stale": activityLabel = "Update delayed · \(fixture.updated)"
            case "unavailable": activityLabel = "Usage unavailable"
            default: activityLabel = "Update failed · \(fixture.updated)"
            }
        }
        return GlanceProviderRow(
            surfaceId: fixture.surfaceId,
            iconKey: fixture.iconKey,
            fallbackGlyph: provider?.fallbackGlyph ?? "?",
            usageURL: nil,
            displayLabel: fixture.label,
            accountLabel: shownAccount,
            planLabel: fixture.planLabel,
            glanceRemainingPercent: fixture.remaining,
            barLabel: barLabel,
            headline: headline,
            resetLabel: fixture.reset,
            compactResetLabel: provider?.compactReset,
            exactReset: fixture.surfaceId == "codex" ? "(15 Aug 2026, 17:02)" : nil,
            statusWord: fixture.status,
            isRefreshing: fixture.refreshing,
            statusLabel: fixture.refreshing ? "Refreshing \(fixture.label) usage" : fixture.status,
            severity: severity(for: fixture.remaining),
            updatedLabel: fixture.updated,
            activityLabel: activityLabel,
            activityKind: fixture.refreshing
                ? "updating" : (fixture.status == "fresh" ? "idle" : "exceptional"),
            accessibilityLabel: "\(fixture.label), \(shownAccount), \(activityLabel)",
            lastError: fixture.error,
            dimmed: fixture.status != "fresh",
            glanceMeterPercent: fixture.remaining,
            glanceMeterSide: fixture.remaining == nil ? nil : "remaining"
        )
    }

    static func surface(
        glance: GlanceProviderRow,
        detail: UsageDetailPresentation,
        credentialOrigin: String?
    ) -> SurfaceRow {
        SurfaceRow(
            id: glance.surfaceId,
            label: glance.displayLabel,
            enabled: true,
            statusBarLabel: glance.barLabel,
            status: glance.statusWord,
            accountLabel: glance.accountLabel,
            username: nil,
            planLabel: glance.planLabel,
            credentialOrigin: credentialOrigin,
            estimateCaption: nil,
            buckets: [],
            updatedLabel: glance.updatedLabel,
            lastError: glance.lastError,
            detailPresentation: detail,
            identity: IdentityRow(
                dto: UsageIdentityPresentationDto(
                    providerTitle: glance.displayLabel,
                    accountLabel: glance.accountLabel,
                    activityLabel: glance.activityLabel,
                    activityKind: glance.activityKind,
                    accessibilityLabel: glance.accessibilityLabel
                )
            )
        )
    }

    static func severity(for remaining: Double?) -> String {
        guard let remaining else { return "normal" }
        if remaining <= 15 { return "danger" }
        if remaining <= 60 { return "warn" }
        return "normal"
    }

    private static func percentLabel(_ value: Double) -> String {
        let text = String(value)
        guard text.hasSuffix(".0") else { return text }
        return String(text.dropLast(2))
    }
}
