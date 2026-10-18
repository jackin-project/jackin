// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import JackinUsageBindings
import XCTest

@testable import JackinUsageBridge

final class UsageWindowModelTests: XCTestCase {
    func glance(
        _ surfaceId: String, headline: String = "H"
    ) -> GlanceProviderRow {
        GlanceProviderRow(
            surfaceId: surfaceId,
            iconKey: surfaceId,
            fallbackGlyph: "?",
            usageURL: "https://example.test/usage",
            displayLabel: "label:\(surfaceId)",
            accountLabel: "acct:\(surfaceId)",
            planLabel: nil,
            glanceRemainingPercent: nil,
            barLabel: "bar:\(surfaceId)",
            headline: headline,
            resetLabel: nil,
            compactResetLabel: nil,
            exactReset: nil,
            statusWord: "fresh",
            isRefreshing: false,
            statusLabel: "status:\(surfaceId)",
            severity: "normal",
            updatedLabel: "u",
            activityLabel: "activity:\(surfaceId)",
            activityKind: "idle",
            accessibilityLabel: "accessibility:\(surfaceId)",
            lastError: nil,
            dimmed: false
        )
    }

    func line(leading: String? = nil, trailing: String? = nil) -> UsagePresentationLine {
        UsagePresentationLine(leading: leading, trailing: trailing)
    }

    func detailRow(
        _ rowId: String,
        kind: UsageDetailRowKind,
        label: String,
        lines: [UsagePresentationLine]
    ) -> UsageDetailRow {
        let display = lines.flatMap { [$0.leading, $0.trailing].compactMap { $0 } }
            .joined(separator: " · ")
        return UsageDetailRow(
            rowId: rowId,
            kind: kind,
            label: label,
            layoutLines: lines,
            displayLabel: display,
            meterPercent: kind == .bucket ? 40 : nil,
            severity: "normal"
        )
    }

    func surface(
        _ id: String,
        enabled: Bool = true,
        detail: UsageDetailPresentation = .empty
    ) -> SurfaceRow {
        SurfaceRow(
            id: id,
            label: "label:\(id)",
            enabled: enabled,
            statusBarLabel: "",
            status: "fresh",
            accountLabel: "",
            username: nil,
            planLabel: nil,
            credentialOrigin: nil,
            estimateCaption: nil,
            buckets: [],
            updatedLabel: "",
            lastError: nil,
            detailPresentation: detail,
            identity: .init(
                dto: UsageIdentityPresentationDto(
                    providerTitle: "label:\(id)",
                    accountLabel: "acct:\(id)",
                    activityLabel: "activity:\(id)",
                    activityKind: "idle",
                    accessibilityLabel: "accessibility:\(id)"
                )
            )
        )
    }

    func account(
        _ surfaceId: String, key: String, selected: Bool
    ) -> AccountRow {
        AccountRow(
            surfaceId: surfaceId,
            providerColumnLabel: "",
            accountKey: key,
            accountLabel: "acct:\(key)",
            planLabel: nil,
            selected: selected,
            lifecycle: "authenticated",
            lifecycleLabel: "Authenticated",
            provenanceLabel: "fixture",
            planOrStatusLabel: "Ready",
            remainingPercent: nil,
            remainingLabel: "–",
            headline: "–",
            resetDisplayLabel: "–",
            statusWord: "fresh",
            statusLabel: "Ready",
            severity: "normal",
            updatedLabel: "Updated now",
            lastError: nil,
            dimmed: false,
            accessibilityLabel: "acct:\(key)"
        )
    }

    func providerGroup(
        _ surfaceId: String,
        accounts: [AccountRow],
        route: SelectedAccountRoute?
    ) -> ProviderGroupRow {
        ProviderGroupRow(
            surfaceId: surfaceId,
            displayLabel: "label:\(surfaceId)",
            iconKey: surfaceId,
            fallbackGlyph: "?",
            usageURL: nil,
            accountColumnLabel: "Accounts",
            planOrStatusLabel: "Ready",
            remainingLabel: "—",
            resetDisplayLabel: "—",
            accounts: accounts,
            accessibilityLabel: "provider:\(surfaceId)",
            lastError: nil,
            selectedAccountRoute: route
        )
    }

    func testSidebarOrderAndOverviewSelection() {
        let rows = ["codex", "claude", "amp", "grok", "zai", "kimi", "minimax"].map { glance($0) }
        let model = UsageWindowModel(
            glanceRows: rows,
            surfaces: [],
            accounts: [],
            selection: nil
        )
        XCTAssertEqual(model.sidebar.map(\.surfaceId), rows.map(\.surfaceId))
        XCTAssertEqual(model.selection, .overview)
        XCTAssertNil(model.content)
        XCTAssertFalse(model.isEmpty)
    }

    func testIncomingProviderSelectionResolvesToDetail() {
        let detail = UsageDetailPresentation(rows: [
            detailRow("focused", kind: .metadata, label: "Focused", lines: [line(leading: "F")]),
            detailRow("bucket:0", kind: .bucket, label: "Weekly", lines: [line(leading: "40")]),
        ])
        let model = UsageWindowModel(
            glanceRows: [glance("codex")],
            surfaces: [surface("codex", detail: detail)],
            accounts: [],
            selection: "codex"
        )
        XCTAssertEqual(model.selection, .provider("codex"))
        XCTAssertEqual(model.content?.surfaceId, "codex")
        XCTAssertEqual(model.content?.detail.rows.map(\.rowId), ["focused", "bucket:0"])
    }

    func testDisabledIncomingSelectionFallsBackToOverview() {
        let model = UsageWindowModel(
            glanceRows: [glance("codex")],
            surfaces: [surface("codex", enabled: false)],
            accounts: [],
            selection: "codex"
        )
        XCTAssertEqual(model.selection, .overview)
        XCTAssertNil(model.content)
    }

    func testDetailRowAndLineOrderFlattenedExactlyOnce() {
        let bucket = detailRow(
            "bucket:0",
            kind: .bucket,
            label: "Weekly",
            lines: [
                line(leading: "40% left"),
                line(leading: "5% in deficit"),
                line(leading: "Runs out in 3d"),
                line(trailing: "Resets in 6d"),
            ]
        )
        let model = UsageWindowModel(
            glanceRows: [glance("codex")],
            surfaces: [surface("codex", detail: UsageDetailPresentation(rows: [bucket]))],
            accounts: [],
            selection: "codex"
        )
        guard let content = model.content else { return XCTFail("missing provider content") }
        let row = content.detail.rows[0]
        let flattened = row.layoutLines.flatMap { [$0.leading, $0.trailing].compactMap { $0 } }
        XCTAssertEqual(
            flattened,
            ["40% left", "5% in deficit", "Runs out in 3d", "Resets in 6d"]
        )
        XCTAssertEqual(row.displayLabel, flattened.joined(separator: " · "))
        XCTAssertNil(row.layoutLines.last?.leading)
        XCTAssertEqual(row.layoutLines.last?.trailing, "Resets in 6d")
    }

    func testDuplicateBucketLabelsKeepDistinctIds() {
        let detail = UsageDetailPresentation(rows: [
            detailRow("bucket:0", kind: .bucket, label: "Weekly", lines: [line(leading: "80")]),
            detailRow("bucket:1", kind: .bucket, label: "Weekly", lines: [line(leading: "20")]),
        ])
        let model = UsageWindowModel(
            glanceRows: [glance("codex")],
            surfaces: [surface("codex", detail: detail)],
            accounts: [],
            selection: "codex"
        )
        guard let content = model.content else { return XCTFail("missing provider content") }
        let rows = content.detail.rows
        XCTAssertEqual(rows.map(\.id), ["bucket:0", "bucket:1"])
        XCTAssertEqual(rows.map(\.label), ["Weekly", "Weekly"])
        XCTAssertEqual(Set(rows.map(\.id)).count, 2)
    }

    func testStaleDetailAndLastGoodBucketCoexist() {
        let detail = UsageDetailPresentation(rows: [
            detailRow("bucket:0", kind: .bucket, label: "Weekly", lines: [line(leading: "57")]),
            detailRow(
                "detail", kind: .detail, label: "Detail", lines: [line(leading: "upstream 503")]),
        ])
        let model = UsageWindowModel(
            glanceRows: [glance("codex")],
            surfaces: [surface("codex", detail: detail)],
            accounts: [],
            selection: "codex"
        )
        guard let content = model.content else { return XCTFail("missing provider content") }
        let rows = content.detail.rows
        XCTAssertTrue(rows.contains { $0.rowId == "bucket:0" })
        XCTAssertEqual(rows.filter { $0.kind == .detail }.count, 1)
        XCTAssertEqual(rows.last?.rowId, "detail")
    }

    func testEmptyEnabledSet() {
        let model = UsageWindowModel(glanceRows: [], surfaces: [], accounts: [], selection: nil)
        XCTAssertTrue(model.isEmpty)
        XCTAssertEqual(model.selection, .overview)
        XCTAssertEqual(UsageWindowModel.emptyHint, "no agent credentials found")
    }
}
