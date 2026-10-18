// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import XCTest

extension JackinDesktopUITests {
    func testProviderDetailPassesAccessibilityAudit() throws {
        defer { application.terminate() }
        guard launchUsage(fixture: "F03-multi-account", selection: "codex", size: "920x620")
        else { return }
        let ready = ensureUsageWindowVisible(contentIdentifier: "usage.account-picker")
        XCTAssertTrue(ready, application.debugDescription)
        guard ready else { return }
        let provider = element("usage.provider.codex")
        let identity = element("usage.provider-identity")
        let picker = element("usage.account-picker")
        let refresh = element("usage.refresh")
        XCTAssertTrue(provider.waitForExistence(timeout: 5))
        let snapshot = UsageAuditSnapshot(
            rowDescriptions:
                auditDescriptions(identifierPrefix: "usage.limit.")
                + auditDescriptions(identifierPrefix: "usage.detail-label.")
                + [identity.label, picker.label, picker.value as? String]
                .compactMap { $0 }
                .filter { !$0.isEmpty },
            systemHostVerified: application.windows["usage-window"].exists,
            controlsVerified:
                identity.exists && !identity.label.isEmpty
                && picker.exists && picker.elementType == .popUpButton
                && refresh.exists && refresh.elementType == .button
        )
        XCTAssertTrue(snapshot.systemHostVerified)
        XCTAssertTrue(snapshot.controlsVerified)
        XCTAssertFalse(snapshot.rowDescriptions.isEmpty)

        try application.performAccessibilityAudit { issue in
            self.handlesSystemAccessibilityAuditFalsePositive(
                issue,
                usageSnapshot: snapshot
            )
        }
    }

    func testOverviewPassesAccessibilityAudit() throws {
        defer { application.terminate() }
        guard launchUsage(fixture: "F02-catalog-normal", selection: "overview", size: "920x620")
        else { return }
        let overview = element("usage.overview.table")
        XCTAssertTrue(overview.waitForExistence(timeout: 5))
        XCTAssertEqual(overview.label, "Usage overview")
        XCTAssertEqual(element("usage.sidebar").label, "Usage providers sidebar")
        var snapshot: UsageAuditSnapshot?
        for _ in 0..<3 {
            guard ensureUsageWindowVisible(contentIdentifier: "usage.overview.table") else {
                continue
            }
            guard waitForOverviewOutlineSettled() else { continue }
            let candidate = overviewAuditSnapshot(overview: overview)
            guard candidate.systemHostVerified, candidate.controlsVerified,
                !candidate.rowDescriptions.isEmpty
            else { continue }
            snapshot = candidate
            break
        }
        guard let snapshot else {
            XCTFail("overview content did not stabilize before audit")
            return
        }

        try application.performAccessibilityAudit { issue in
            self.handlesSystemAccessibilityAuditFalsePositive(
                issue,
                usageSnapshot: snapshot
            )
        }
    }

    func overviewAuditSnapshot(overview: XCUIElement) -> UsageAuditSnapshot {
        let refresh = element("usage.refresh")
        let badge = element("usage.fixture-badge")
        let overviewLabel = overview.exists ? overview.label : nil
        return UsageAuditSnapshot(
            rowDescriptions:
                auditDescriptions(identifierPrefix: "usage.overview.provider.")
                + auditDescriptions(identifierPrefix: "usage.overview.account.")
                + auditDescriptions(identifierPrefix: "usage.sidebar.provider.")
                + [badge.exists ? badge.label : nil, overviewLabel]
                .compactMap { $0 }
                .filter { !$0.isEmpty },
            systemHostVerified: application.windows["usage-window"].exists,
            controlsVerified:
                overviewLabel == "Usage overview"
                && element("usage.sidebar").exists
                && refresh.exists && refresh.elementType == .button
        )
    }
}
