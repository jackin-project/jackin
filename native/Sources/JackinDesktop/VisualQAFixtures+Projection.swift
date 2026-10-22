// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import Foundation
import JackinUsageBridge

extension VisualQAFixtures {
    static func accountProjections(
        for fixture: VisualQAFixture
    ) -> [String: QIFixtureProjection] {
        var projections: [String: QIFixtureProjection] = [:]
        for selected in fixture.accounts {
            let accounts = fixture.accounts.map { row in
                copiedAccount(
                    row,
                    selected: row.surfaceId == selected.surfaceId
                        ? row.accountKey == selected.accountKey
                        : row.selected)
            }
            let glances = fixture.glanceRows.map { row in
                guard row.surfaceId == selected.surfaceId else { return row }
                return glance(
                    GlanceFixture(
                        surfaceId: row.surfaceId,
                        iconKey: row.iconKey,
                        label: row.displayLabel,
                        accountLabel: selected.accountLabel,
                        planLabel: selected.planLabel,
                        remaining: selected.remainingPercent,
                        reset: row.resetLabel,
                        status: selected.statusWord,
                        updated: selected.updatedLabel,
                        error: selected.lastError,
                        refreshing: false
                    ))
            }
            let bySurface = Dictionary(uniqueKeysWithValues: glances.map { ($0.surfaceId, $0) })
            let surfaces = fixture.surfaces.map { row in
                guard row.id == selected.surfaceId,
                    let glance = bySurface[row.id]
                else { return row }
                return surface(
                    glance: glance,
                    detail: selectedDetail(account: selected, fallback: row.detailPresentation),
                    credentialOrigin: row.credentialOrigin
                )
            }
            let groups = providerGroups(glanceRows: glances, accounts: accounts)
            projections["\(selected.surfaceId)#\(selected.accountKey)"] =
                QIFixtureProjection(
                    glanceRows: glances,
                    statusBarGlanceRows: fixture.statusGlanceRows.compactMap {
                        bySurface[$0.surfaceId]
                    },
                    surfaces: surfaces,
                    accounts: accounts,
                    providerGroups: groups
                )
        }
        return projections
    }

    static func refreshingProjection(
        for fixture: VisualQAFixture
    ) -> QIFixtureProjection {
        let glances = fixture.glanceRows.map { row in
            glance(
                GlanceFixture(
                    surfaceId: row.surfaceId,
                    iconKey: row.iconKey,
                    label: row.displayLabel,
                    accountLabel: row.accountLabel,
                    planLabel: row.planLabel,
                    remaining: row.glanceRemainingPercent,
                    reset: row.resetLabel,
                    status: row.statusWord,
                    updated: row.updatedLabel,
                    error: row.lastError,
                    refreshing: true
                ))
        }
        let bySurface = Dictionary(uniqueKeysWithValues: glances.map { ($0.surfaceId, $0) })
        let surfaces = fixture.surfaces.map { row in
            guard let glance = bySurface[row.id] else { return row }
            return surface(
                glance: glance,
                detail: row.detailPresentation,
                credentialOrigin: row.credentialOrigin
            )
        }
        return QIFixtureProjection(
            glanceRows: glances,
            statusBarGlanceRows: fixture.statusGlanceRows.compactMap {
                bySurface[$0.surfaceId]
            },
            surfaces: surfaces,
            accounts: fixture.accounts,
            providerGroups: fixture.providerGroups
        )
    }

    static func projection(
        for fixture: VisualQAFixture
    ) -> QIFixtureProjection {
        QIFixtureProjection(
            glanceRows: fixture.glanceRows,
            statusBarGlanceRows: fixture.statusGlanceRows,
            surfaces: fixture.surfaces,
            accounts: fixture.accounts,
            providerGroups: fixture.providerGroups
        )
    }

    static func copiedAccount(
        _ account: AccountRow,
        selected: Bool,
        statusWord: String? = nil,
        lastError: String? = nil
    ) -> AccountRow {
        let effectiveStatus = statusWord ?? account.statusWord
        let effectiveError = lastError ?? account.lastError
        return AccountRow(
            surfaceId: account.surfaceId,
            providerColumnLabel: account.providerColumnLabel,
            accountKey: account.accountKey,
            accountLabel: account.accountLabel,
            planLabel: account.planLabel,
            selected: selected,
            lifecycle: account.lifecycle,
            lifecycleLabel: account.lifecycleLabel,
            provenanceLabel: account.provenanceLabel,
            planOrStatusLabel: account.planLabel ?? statusWord ?? account.planOrStatusLabel,
            usedPercent: account.usedPercent,
            usedPercentOrigin: account.usedPercentOrigin,
            remainingPercent: account.remainingPercent,
            remainingPercentOrigin: account.remainingPercentOrigin,
            countQuota: account.countQuota,
            resetsAt: account.resetsAt,
            meterPercent: account.meterPercent,
            meterSide: account.meterSide,
            remainingLabel: account.remainingLabel,
            headline: account.headline,
            resetDisplayLabel: account.resetDisplayLabel,
            statusWord: effectiveStatus,
            statusLabel: statusWord ?? account.statusLabel,
            severity: account.severity,
            updatedLabel: account.updatedLabel,
            lastError: effectiveError,
            dimmed: account.dimmed || statusWord != nil,
            accessibilityLabel: [account.accessibilityLabel, effectiveError]
                .compactMap { $0 }
                .joined(separator: ", ")
        )
    }

    static func selectedDetail(
        account: AccountRow,
        fallback: UsageDetailPresentation
    ) -> UsageDetailPresentation {
        let metadataRows = fallback.rows.filter { $0.kind != .bucket }
        let reset = account.resetDisplayLabel == "—" ? nil : account.resetDisplayLabel
        let lines = [
            UsagePresentationLine(leading: account.headline, trailing: nil),
            reset.map { UsagePresentationLine(leading: nil, trailing: $0) },
        ].compactMap { $0 }
        let displayLabel = lines.flatMap { [$0.leading, $0.trailing].compactMap { $0 } }
            .joined(separator: " · ")
        return UsageDetailPresentation(
            rows: metadataRows + [
                UsageDetailRow(
                    rowId: "bucket:selected",
                    kind: .bucket,
                    label: "Account quota",
                    layoutLines: lines,
                    displayLabel: displayLabel,
                    meterPercent: account.meterPercent,
                    severity: account.severity,
                    meterSide: account.meterSide
                )
            ])
    }
}
