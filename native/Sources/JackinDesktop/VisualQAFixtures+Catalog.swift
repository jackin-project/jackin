// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import Foundation
import JackinUsageBindings
import JackinUsageBridge

extension VisualQAFixtures {
    static func catalogSnapshotFixture(
        id: VisualQAFixtureID,
        refreshingSurface: String? = nil,
        failingSurface: String? = nil,
        failureStatus: String = "fresh",
        failureError: String? = nil
    ) -> VisualQAFixture {
        let accounts = catalogAccounts().map { account in
            guard account.surfaceId == failingSurface else { return account }
            return copiedAccount(
                account,
                selected: account.selected,
                statusWord: failureStatus,
                lastError: failureError
            )
        }
        let glances = catalogGlances(
            accounts: accounts,
            refreshingSurface: refreshingSurface,
            failingSurface: failingSurface,
            failureStatus: failureStatus,
            failureError: failureError
        )
        let surfaces = catalogSurfaces(glances)
        return catalogFixture(id: id, glances: glances, surfaces: surfaces, accounts: accounts)
    }

    private static func catalogGlances(
        accounts: [AccountRow],
        refreshingSurface: String?,
        failingSurface: String?,
        failureStatus: String,
        failureError: String?
    ) -> [GlanceProviderRow] {
        Provider.allCases.map { provider in
            let selected = accounts.first { $0.surfaceId == provider.rawValue && $0.selected }
            let isFailure = provider.rawValue == failingSurface
            return glance(
                GlanceFixture(
                    surfaceId: provider.rawValue,
                    iconKey: provider.rawValue,
                    label: provider.label,
                    accountLabel: selected?.accountLabel ?? "",
                    planLabel: selected?.planLabel,
                    remaining: provider.remaining,
                    reset: provider.reset,
                    status: isFailure ? failureStatus : "fresh",
                    updated: "Updated now",
                    error: isFailure ? failureError : nil,
                    refreshing: provider.rawValue == refreshingSurface
                ))
        }
    }

    private static func catalogSurfaces(_ glances: [GlanceProviderRow]) -> [SurfaceRow] {
        glances.map { row in
            surface(
                glance: row,
                detail: detailPresentation(for: Provider(rawValue: row.surfaceId) ?? .amp),
                credentialOrigin: row.surfaceId == "codex" ? "OAuth · ~/.codex/auth.json" : nil
            )
        }
    }

    private static func catalogFixture(
        id: VisualQAFixtureID,
        glances: [GlanceProviderRow],
        surfaces: [SurfaceRow],
        accounts: [AccountRow]
    ) -> VisualQAFixture {
        let statusOrder = ["claude", "codex", "minimax"].compactMap { id in
            glances.first { $0.surfaceId == id }
        }
        return VisualQAFixture(
            id: id,
            glanceRows: glances,
            statusGlanceRows: statusOrder,
            surfaces: surfaces,
            accounts: accounts,
            providerGroups: providerGroups(glanceRows: glances, accounts: accounts),
            popoverSelection: "codex",
            usageSelection: nil,
            nextRefreshLabel: "next update 4m",
            isLoading: false,
            isRefreshing: false,
            globalError: nil
        )
    }

    static func layoutEnvelopeFixture() -> VisualQAFixture {
        let base = catalogSnapshotFixture(id: .layoutEnvelope)
        let accounts = layoutEnvelopeAccounts()
        let detail = layoutEnvelopeDetail()
        let surfaces = layoutEnvelopeSurfaces(base.surfaces, detail: detail)
        return VisualQAFixture(
            id: .layoutEnvelope,
            glanceRows: base.glanceRows,
            statusGlanceRows: base.statusGlanceRows,
            surfaces: surfaces,
            accounts: accounts,
            providerGroups: providerGroups(glanceRows: base.glanceRows, accounts: accounts),
            popoverSelection: "claude",
            usageSelection: "claude",
            nextRefreshLabel: base.nextRefreshLabel,
            isLoading: false,
            isRefreshing: false,
            globalError: nil
        )
    }

    private static func layoutEnvelopeAccounts() -> [AccountRow] {
        [
            codexPersonal(selected: false),
            codexPlus(selected: false),
            account(
                AccountFixture(

                    surfaceId: "codex",
                    key: "codex-organization",
                    label: "organization-production-sandbox@example.test",
                    plan: "Enterprise",
                    remaining: 88,
                    selected: true

                )),
            claudePersonal(selected: false),
            account(
                AccountFixture(

                    surfaceId: "claude",
                    key: "claude-work",
                    label: "Work",
                    plan: "Team",
                    remaining: nil,
                    selected: false

                )),
            account(
                AccountFixture(

                    surfaceId: "claude",
                    key: "claude-third",
                    label: "Research workspace",
                    plan: "Team",
                    remaining: 28,
                    selected: true

                )),
            defaultAccount(provider: .grok),
            defaultAccount(provider: .zai),
            defaultAccount(provider: .kimi),
            defaultAccount(provider: .minimax),
        ]
    }

    private static func layoutEnvelopeDetail() -> UsageDetailPresentation {
        let longRow = bucket(
            BucketFixture(
                id: "bucket:layout-long",
                label: "Organization-wide weekly accelerated-model allocation",
                remaining: "28% left",
                meter: 28,
                severity: "warn",
                supporting: ["Resets Tuesday, 18 August 2026 at 23:59 Indochina Time"]
            ))
        return UsageDetailPresentation(
            rows: [
                metadata(id: "plan", label: "Plan", value: "Team")
            ] + anthropicDetail().rows + [longRow]
        )
    }

    private static func layoutEnvelopeSurfaces(
        _ rows: [SurfaceRow],
        detail: UsageDetailPresentation
    ) -> [SurfaceRow] {
        rows.map { row in
            guard row.id == "claude" else { return row }
            return SurfaceRow(
                id: row.id,
                label: row.label,
                enabled: row.enabled,
                statusBarLabel: row.statusBarLabel,
                status: row.status,
                accountLabel: "Research workspace",
                username: row.username,
                planLabel: "Team",
                credentialOrigin: row.credentialOrigin,
                estimateCaption: row.estimateCaption,
                buckets: row.buckets,
                updatedLabel: row.updatedLabel,
                lastError: row.lastError,
                detailPresentation: detail,
                identity: IdentityRow(
                    dto: UsageIdentityPresentationDto(
                        providerTitle: row.label,
                        accountLabel: "Research workspace",
                        activityLabel: row.updatedLabel,
                        activityKind: "idle",
                        accessibilityLabel: "\(row.label), Research workspace, \(row.updatedLabel)"
                    )
                )
            )
        }
    }
}
