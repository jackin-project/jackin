// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import JackinUsageBindings
import XCTest

@testable import JackinUsageBridge

/// Executes the pure `UsageWindowModel` with opaque sentinel strings so the
/// Usage window's order/selection/action behavior is proven without depending on
/// any Rust wording (Rust/FFI tests own the real Amp/Grok strings).
///
/// No formatting
/// logic is duplicated here.
final class UsageWindowModelTests: XCTestCase {
    private func glance(
        _ surfaceId: String, headline: String = "H"
    )
        -> PresentationStore
        .GlanceProviderRow
    {
        PresentationStore.GlanceProviderRow(
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

    private func line(leading: String? = nil, trailing: String? = nil) -> UsagePresentationLine {
        UsagePresentationLine(leading: leading, trailing: trailing)
    }

    private func detailRow(
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

    private func surface(
        _ id: String,
        enabled: Bool = true,
        detail: UsageDetailPresentation = .empty
    ) -> PresentationStore.SurfaceRow {
        PresentationStore.SurfaceRow(
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

    private func account(
        _ surfaceId: String, key: String, selected: Bool
    )
        -> PresentationStore
        .AccountRow
    {
        PresentationStore.AccountRow(
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
        // Rendered order is leading-then-trailing per line, preserving vector order.
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

    func testMultiAccountActionAndSelectedStyling() {
        let accounts = [
            account("codex", key: "a", selected: true),
            account("codex", key: "b", selected: false),
        ]
        let model = UsageWindowModel(
            glanceRows: [glance("codex")],
            surfaces: [surface("codex", detail: .empty)],
            accounts: accounts,
            selection: "codex"
        )
        XCTAssertEqual(model.content?.accounts.count, 2)
        XCTAssertEqual(model.content?.accounts.first?.selected, true)
        XCTAssertEqual(model.content?.headAccount?.accountKey, "a")

        let selected = UsageWindowModel(
            glanceRows: [glance("codex")],
            surfaces: [surface("codex", detail: .empty)],
            accounts: accounts,
            selection: "codex",
            accountSelection: "b"
        )
        XCTAssertEqual(selected.content?.headAccount?.accountKey, "b")
    }

    func testSentinelRowsTransmittedUnchanged() {
        // Opaque sentinels standing in for Amp/Grok Rust strings: the model must
        // pass them through byte-for-byte with no reformatting.
        let ampDaily = detailRow(
            "bucket:0",
            kind: .bucket,
            label: "Daily",
            lines: [line(leading: "61% left"), line(trailing: "Resets daily")]
        )
        let grokPlan = detailRow(
            "plan",
            kind: .metadata,
            label: "Plan",
            lines: [line(leading: "SuperGrok")]
        )
        let detail = UsageDetailPresentation(rows: [grokPlan, ampDaily])
        let model = UsageWindowModel(
            glanceRows: [glance("amp")],
            surfaces: [surface("amp", detail: detail)],
            accounts: [],
            selection: "amp"
        )
        guard let content = model.content else { return XCTFail("missing provider content") }
        let rows = content.detail.rows
        XCTAssertEqual(rows[0].displayLabel, "SuperGrok")
        XCTAssertEqual(rows[1].displayLabel, "61% left · Resets daily")
    }
}
