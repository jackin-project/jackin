// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import Foundation
import JackinUsageBridge

private struct FixtureSelection {
    let popover: String?
    let usage: String?
}

private struct CustomProviderFixture {
    let id: VisualQAFixtureID
    let surfaceId: String
    let iconKey: String
    let displayLabel: String
    let accounts: [AccountRow]
    let remaining: Double?
    let status: String
    let updated: String
    let error: String?
    let detail: UsageDetailPresentation
}

extension VisualQAFixtures {
    static func makeFixture(id: VisualQAFixtureID) -> VisualQAFixture {
        switch id {
        case .noProviders, .initialLoading, .globalBridgeError:
            emptyFixture(id: id)
        case .singleNormal, .multiAccount, .nearlyExhausted, .exhausted, .staleLastGood,
            .permissionDenied, .offlineCached, .longLabels:
            singleProviderFixture(id: id)
        case .catalogNormal, .refreshingLastGood, .partialTimeout:
            catalogFixture(id: id)
        case .layoutEnvelope:
            layoutEnvelopeFixture()
        }
    }

    static func emptyFixture(id: VisualQAFixtureID) -> VisualQAFixture {
        switch id {
        case .noProviders:
            fixture(
                id: id,
                glanceRows: [],
                surfaces: [],
                accounts: [],
                selection: FixtureSelection(popover: nil, usage: nil)
            )
        case .initialLoading:
            emptyFixture(id: id, isLoading: true, error: nil)
        case .globalBridgeError:
            emptyFixture(id: id, isLoading: false, error: "Usage presentation is unavailable")
        default:
            preconditionFailure("not an empty fixture: \(id)")
        }
    }

    static func emptyFixture(
        id: VisualQAFixtureID,
        isLoading: Bool,
        error: String?
    ) -> VisualQAFixture {
        VisualQAFixture(
            id: id,
            glanceRows: [],
            statusGlanceRows: [],
            surfaces: [],
            accounts: [],
            providerGroups: [],
            popoverSelection: nil,
            usageSelection: nil,
            nextRefreshLabel: "",
            isLoading: isLoading,
            isRefreshing: false,
            globalError: error
        )
    }

    static func singleProviderFixture(id: VisualQAFixtureID) -> VisualQAFixture {
        switch id {
        case .singleNormal: normalProviderFixture(id: id)
        case .multiAccount: multiAccountFixture(id: id)
        case .nearlyExhausted: nearlyExhaustedFixture(id: id)
        case .exhausted: exhaustedFixture(id: id)
        default: exceptionalProviderFixture(id: id)
        }
    }

    private static func normalProviderFixture(id: VisualQAFixtureID) -> VisualQAFixture {
        singleProviderFixture(id: id, provider: .codex, accounts: [codexPersonal()])
    }

    private static func multiAccountFixture(id: VisualQAFixtureID) -> VisualQAFixture {
        let accounts = [
            codexPersonal(selected: false),
            codexPlus(selected: true),
            account(
                AccountFixture(
                    surfaceId: "codex",
                    key: "codex-organization",
                    label: "organization-production-sandbox@example.test",
                    plan: "Enterprise",
                    remaining: 88,
                    selected: false
                )),
        ]
        return singleProviderFixture(
            id: id,
            provider: .codex,
            accounts: accounts,
            glanceRemaining: 0,
            detail: exhaustedOpenAIDetail()
        )
    }

    private static func nearlyExhaustedFixture(id: VisualQAFixtureID) -> VisualQAFixture {
        singleProviderFixture(
            id: id,
            provider: .claude,
            accounts: [claudePersonal()],
            detail: anthropicDetail()
        )
    }

    private static func exhaustedFixture(id: VisualQAFixtureID) -> VisualQAFixture {
        singleProviderFixture(
            id: id,
            provider: .codex,
            accounts: [codexPlus()],
            glanceRemaining: 0,
            detail: exhaustedOpenAIDetail()
        )
    }

    private static func exceptionalProviderFixture(id: VisualQAFixtureID) -> VisualQAFixture {
        switch id {
        case .staleLastGood:
            singleProviderFixture(
                id: id,
                provider: .codex,
                accounts: [codexPersonal(status: "stale")],
                status: "stale",
                updated: "Updated 47m ago",
                error: "Codex provider usage unavailable; cached quota is stale"
            )
        case .permissionDenied: permissionDeniedFixture(id: id)
        case .offlineCached:
            singleProviderFixture(
                id: id,
                provider: .kimi,
                accounts: [defaultAccount(provider: .kimi, status: "stale")],
                status: "stale",
                updated: "Updated 1h ago",
                error: "Kimi billing endpoint unavailable; local presence only"
            )
        case .longLabels: longLabelsFixture(id: id)
        default: preconditionFailure("not an exceptional provider fixture: \(id)")
        }
    }

    static func permissionDeniedFixture(id: VisualQAFixtureID) -> VisualQAFixture {
        customSingleProviderFixture(
            CustomProviderFixture(
                id: id,
                surfaceId: "claude",
                iconKey: "claude",
                displayLabel: "Anthropic",
                accounts: [],
                remaining: nil,
                status: "unavailable",
                updated: "Updated now",
                error: "Claude Keychain access denied",
                detail: .empty
            ))
    }

    static func longLabelsFixture(id: VisualQAFixtureID) -> VisualQAFixture {
        let detail = UsageDetailPresentation(rows: [
            bucket(
                BucketFixture(
                    id: "bucket:long",
                    label: "Organization-wide weekly accelerated-model allocation",
                    remaining: "57% left",
                    meter: 57,
                    severity: "warn",
                    supporting: ["Resets Tuesday, 18 August 2026 at 23:59 Indochina Time"]
                ))
        ])
        let account = copiedAccount(
            account(
                AccountFixture(
                    surfaceId: "codex",
                    key: "codex-organization",
                    label: "organization-production-sandbox@example.test",
                    plan: "Enterprise workspace with centrally managed weekly limits",
                    remaining: 57,
                    selected: true
                )
            ),
            selected: true,
            statusWord: "stale"
        )
        return customSingleProviderFixture(
            CustomProviderFixture(
                id: id,
                surfaceId: "codex",
                iconKey: "codex",
                displayLabel: "OpenAI Organization Production Sandbox — Southeast Asia",
                accounts: [account],
                remaining: 57,
                status: "stale",
                updated: "Updated 47m ago",
                error:
                    "Provider response could not be refreshed; showing the last successful quota snapshot",
                detail: detail
            ))
    }

    static func catalogFixture(id: VisualQAFixtureID) -> VisualQAFixture {
        switch id {
        case .catalogNormal:
            catalogSnapshotFixture(id: id)
        case .refreshingLastGood:
            refreshingCatalogFixture(id: id)
        case .partialTimeout:
            catalogSnapshotFixture(
                id: id,
                failingSurface: "kimi",
                failureStatus: "unavailable",
                failureError: "usage provider probe timed out"
            )
        default:
            preconditionFailure("not a catalog fixture: \(id)")
        }
    }

    static func refreshingCatalogFixture(id: VisualQAFixtureID) -> VisualQAFixture {
        let fixture = catalogSnapshotFixture(id: id, refreshingSurface: "codex")
        return VisualQAFixture(
            id: fixture.id,
            glanceRows: fixture.glanceRows,
            statusGlanceRows: fixture.statusGlanceRows,
            surfaces: fixture.surfaces,
            accounts: fixture.accounts,
            providerGroups: fixture.providerGroups,
            popoverSelection: fixture.popoverSelection,
            usageSelection: fixture.usageSelection,
            nextRefreshLabel: fixture.nextRefreshLabel,
            isLoading: false,
            isRefreshing: true,
            globalError: nil
        )
    }

    private static func fixture(
        id: VisualQAFixtureID,
        glanceRows: [GlanceProviderRow],
        surfaces: [SurfaceRow],
        accounts: [AccountRow],
        selection: FixtureSelection
    ) -> VisualQAFixture {
        VisualQAFixture(
            id: id,
            glanceRows: glanceRows,
            statusGlanceRows: Array(glanceRows.prefix(3)),
            surfaces: surfaces,
            accounts: accounts,
            providerGroups: providerGroups(glanceRows: glanceRows, accounts: accounts),
            popoverSelection: selection.popover,
            usageSelection: selection.usage,
            nextRefreshLabel: "next update 4m",
            isLoading: false,
            isRefreshing: false,
            globalError: nil
        )
    }

    static func singleProviderFixture(
        id: VisualQAFixtureID,
        provider: Provider,
        accounts: [AccountRow],
        glanceRemaining: Double? = nil,
        status: String = "fresh",
        updated: String = "Updated now",
        error: String? = nil,
        detail: UsageDetailPresentation? = nil
    ) -> VisualQAFixture {
        customSingleProviderFixture(
            CustomProviderFixture(
                id: id,
                surfaceId: provider.rawValue,
                iconKey: provider.rawValue,
                displayLabel: provider.label,
                accounts: accounts,
                remaining: glanceRemaining ?? provider.remaining,
                status: status,
                updated: updated,
                error: error,
                detail: detail ?? detailPresentation(for: provider)
            ))
    }

    static func providerGroups(
        glanceRows: [GlanceProviderRow],
        accounts: [AccountRow]
    ) -> [ProviderGroupRow] {
        glanceRows.map { glance in
            let children = accounts.filter { $0.surfaceId == glance.surfaceId }
            return ProviderGroupRow(
                surfaceId: glance.surfaceId,
                displayLabel: glance.displayLabel,
                iconKey: glance.iconKey,
                fallbackGlyph: glance.fallbackGlyph,
                usageURL: glance.usageURL,
                accountColumnLabel: "—",
                planOrStatusLabel:
                    children.isEmpty && glance.statusWord != "fresh" ? glance.statusLabel : "—",
                remainingLabel: "—",
                resetDisplayLabel: "—",
                accounts: children,
                accessibilityLabel: glance.displayLabel,
                lastError: children.isEmpty ? glance.lastError : nil
            )
        }
    }

    private static func customSingleProviderFixture(
        _ fixture: CustomProviderFixture
    ) -> VisualQAFixture {
        let selected = fixture.accounts.first(where: \.selected) ?? fixture.accounts.first
        let provider = Provider(rawValue: fixture.surfaceId)
        let glance = glance(
            GlanceFixture(
                surfaceId: fixture.surfaceId,
                iconKey: fixture.iconKey,
                label: fixture.displayLabel,
                accountLabel: selected?.accountLabel ?? "",
                planLabel: selected?.planLabel,
                remaining: fixture.remaining,
                reset: provider?.reset,
                status: fixture.status,
                updated: fixture.updated,
                error: fixture.error,
                refreshing: false
            ))
        let surface = surface(
            glance: glance,
            detail: fixture.detail,
            credentialOrigin: fixture.surfaceId == "codex" ? "OAuth · ~/.codex/auth.json" : nil
        )
        return Self.fixture(
            id: fixture.id,
            glanceRows: [glance],
            surfaces: [surface],
            accounts: fixture.accounts,
            selection: FixtureSelection(
                popover: fixture.surfaceId,
                usage: fixture.surfaceId
            )
        )
    }
}
