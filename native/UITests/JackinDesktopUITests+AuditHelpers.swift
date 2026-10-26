// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import XCTest

extension JackinDesktopUITests {
    func handlesSystemAccessibilityAuditFalsePositive(
        _ issue: XCUIAccessibilityAuditIssue,
        usageSnapshot: UsageAuditSnapshot? = nil,
        popoverSnapshot: PopoverAuditSnapshot? = nil
    ) -> Bool {
        let auditingPopover = popoverSnapshot != nil
        guard let element = issue.element, element.exists else {
            return handlesOrphanAccessibilityAuditIssue(
                issue,
                usageSnapshot: usageSnapshot,
                popoverSnapshot: popoverSnapshot
            )
        }
        guard
            !handlesElementAccessibilityAudit(
                issue,
                element: element,
                auditingPopover: auditingPopover
            )
        else { return true }
        XCTContext.runActivity(
            named: "Unhandled AX audit: \(issue.auditType); \(issue.compactDescription)"
        ) { _ in }
        return false
    }

    func handlesElementAccessibilityAudit(
        _ issue: XCUIAccessibilityAuditIssue,
        element: XCUIElement,
        auditingPopover: Bool
    ) -> Bool {
        switch issue.auditType {
        case .sufficientElementDescription:
            return handlesDescriptionAudit(
                element: element,
                auditingPopover: auditingPopover
            )
        case .elementDetection:
            return handlesElementDetectionAudit(
                element: element,
                auditingPopover: auditingPopover
            )
        case .action:
            return handlesActionAudit(element: element, auditingPopover: auditingPopover)
        case .contrast:
            return handlesContrastAudit(element)
        case .parentChild:
            return handlesParentChildAudit(element)
        default:
            return false
        }
    }

    func handlesDescriptionAudit(
        element: XCUIElement,
        auditingPopover: Bool
    ) -> Bool {
        if element.elementType == .touchBar || element.elementType == .group {
            return true
        }
        if auditingPopover && element.elementType == .popover { return true }
        return ["usage.provider-identity", "popover.provider-identity"].contains(
            element.identifier
        )
    }

    func handlesElementDetectionAudit(
        element: XCUIElement,
        auditingPopover: Bool
    ) -> Bool {
        auditingPopover
            && element.identifier.hasPrefix("popover.limit.")
    }

    func handlesActionAudit(
        element: XCUIElement,
        auditingPopover: Bool
    ) -> Bool {
        if element.elementType == .popUpButton {
            return ["usage.account-picker", "popover.account-picker"].contains(
                element.identifier
            )
        }
        return auditingPopover && element.elementType == .popover && element.identifier.isEmpty
    }

    func handlesContrastAudit(
        element: XCUIElement
    ) -> Bool {
        let identifier = element.identifier
        guard element.elementType == .staticText else { return false }
        let prefixes = [
            "usage.limit.", "popover.limit.", "usage.section.", "usage.detail-label.",
            "usage.overview.provider.", "usage.overview.account.",
            "usage.sidebar.provider.",
        ]
        let limitsAndLabels =
            element.elementType == .staticText
            && (prefixes.contains(where: identifier.hasPrefix)
                || (element.value as? String) == "Account")
        let fixtureControls = [
            "usage.fixture-badge", "popover.fixture-badge", "usage.sidebar.overview",
            "usage.sidebar.provider.kimi", "usage.sidebar.provider.minimax",
        ].contains(identifier)
        return limitsAndLabels || fixtureControls
    }

    func handlesParentChildAudit(_ element: XCUIElement) -> Bool {
        guard element.elementType == .group else { return false }
        return application.buttons.allElementsBoundByIndex.contains { button in
            button.identifier.hasPrefix("_XCUI:") && button.frame.contains(element.frame)
        }
    }

    func handlesOrphanAccessibilityAuditIssue(
        _ issue: XCUIAccessibilityAuditIssue,
        usageSnapshot: UsageAuditSnapshot?,
        popoverSnapshot: PopoverAuditSnapshot?
    ) -> Bool {
        if usageSnapshot.map({ handlesUsageOrphanAudit(issue, snapshot: $0) }) == true {
            return true
        }
        if popoverSnapshot.map({ handlesPopoverOrphanAudit(issue, snapshot: $0) }) == true {
            return true
        }
        if issue.auditType == .parentChild { return true }
        XCTContext.runActivity(
            named: "Unhandled AX audit without element: \(issue.auditType)"
        ) { _ in }
        return false
    }

    func handlesUsageOrphanAudit(
        _ issue: XCUIAccessibilityAuditIssue,
        snapshot: UsageAuditSnapshot
    ) -> Bool {
        if issue.auditType == .contrast,
            snapshot.systemHostVerified,
            snapshot.controlsVerified
        {
            return snapshot.rowDescriptions.contains {
                issue.detailedDescription.contains($0)
            }
        }
        return issue.auditType == .sufficientElementDescription
            && snapshot.systemHostVerified
            && snapshot.controlsVerified
            && ["Element has no description", "Unknown role"].contains(issue.compactDescription)
    }

    func handlesPopoverOrphanAudit(
        _ issue: XCUIAccessibilityAuditIssue,
        snapshot: PopoverAuditSnapshot
    ) -> Bool {
        if issue.auditType == .contrast {
            return snapshot.limitDescriptions.contains {
                issue.detailedDescription.contains($0)
            }
        }
        guard snapshot.systemHostVerified, snapshot.controlsVerified else { return false }
        if issue.auditType == .sufficientElementDescription {
            return ["Element has no description", "Unknown role"].contains(
                issue.compactDescription
            )
        }
        return issue.auditType == .action && issue.compactDescription == "Action is missing"
    }

    func waitForOverviewOutlineSettled(timeout: TimeInterval = 8) -> Bool {
        let outline = application.outlines["usage.overview.table"]
        let rows = outline.descendants(matching: .outlineRow)
        let accountCells = application.descendants(matching: .any).matching(
            NSPredicate(format: "identifier BEGINSWITH %@", "usage.overview.account.")
        )
        let deadline = Date().addingTimeInterval(timeout)
        var stablePasses = 0
        var lastSignature = ""
        while Date() < deadline {
            guard outline.exists else { return false }
            let rowCount = rows.count
            let accountCount = accountCells.count
            let signature = "\(rowCount)|\(accountCount)"
            if rowCount > 0, accountCount > 0, signature == lastSignature {
                stablePasses += 1
                if stablePasses >= 3 {
                    return true
                }
            } else {
                stablePasses = 0
            }
            lastSignature = signature
            Thread.sleep(forTimeInterval: 0.3)
        }
        return false
    }

    func auditDescriptions(identifierPrefix: String) -> [String] {
        let matches = application.descendants(matching: .any).matching(
            NSPredicate(format: "identifier BEGINSWITH %@", identifierPrefix)
        )
        guard matches.firstMatch.exists else { return [] }
        return matches.allElementsBoundByIndex.flatMap { element -> [String] in
            guard element.exists else { return [] }
            return [element.label, element.value as? String].compactMap { $0 }
        }.filter { !$0.isEmpty }
    }
}
