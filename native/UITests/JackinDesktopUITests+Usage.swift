// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import XCTest

extension JackinDesktopUITests {
    func testOverviewAndProviderNavigationAtMinimumSize() {
        defer { application.terminate() }
        guard launchUsage(fixture: "F02-catalog-normal", selection: "overview", size: "760x500")
        else { return }

        let usageWindow = application.windows["usage-window"]
        XCTAssertEqual(usageWindow.title, "jackin❯ desktop")
        let brandTitle = element("usage.brand-title")
        XCTAssertTrue(brandTitle.waitForExistence(timeout: 5))
        XCTAssertEqual(brandTitle.label, "jackin❯ desktop")
        let detailPane = element("usage.detail-pane")
        XCTAssertTrue(detailPane.waitForExistence(timeout: 5))
        XCTAssertEqual(brandTitle.frame.midX, usageWindow.frame.midX, accuracy: 2)
        let refresh = element("usage.refresh")
        XCTAssertTrue(refresh.waitForExistence(timeout: 5))
        XCTAssertGreaterThan(refresh.frame.midX, detailPane.frame.midX)
        XCTAssertLessThanOrEqual(refresh.frame.maxX, detailPane.frame.maxX + 1)
        XCTAssertTrue(element("usage.sidebar").waitForExistence(timeout: 5))
        XCTAssertFalse(application.staticTexts["Usage"].exists)
        let overview = element("usage.overview.table")
        XCTAssertTrue(overview.waitForExistence(timeout: 5))
        XCTAssertEqual(overview.label, "Usage overview")
        XCTAssertEqual(element("usage.sidebar").label, "Usage providers sidebar")

        let openAI = element("usage.sidebar.provider.codex")
        XCTAssertTrue(openAI.waitForExistence(timeout: 3))
        let openAIRow = application.outlineRows.containing(.any, identifier: openAI.identifier)
            .firstMatch
        XCTAssertTrue(openAIRow.waitForExistence(timeout: 3), application.debugDescription)
        XCTAssertTrue(application.windows["usage-window"].frame.contains(openAIRow.frame))
        DistributedNotificationCenter.default().postNotificationName(
            Notification.Name("com.jackin-project.desktop.visual-qa.select-codex-provider"),
            object: nil,
            userInfo: nil,
            deliverImmediately: true
        )

        XCTAssertTrue(element("usage.provider.codex").waitForExistence(timeout: 3))
        XCTAssertTrue(element("usage.limit.bucket:0").waitForExistence(timeout: 3))
        XCTAssertTrue(element("usage.refresh").isEnabled)
    }

    func testPartialFailureOverviewRemainsCoherentWhenRepresented() {
        defer { application.terminate() }
        guard launchUsage(fixture: "F08-partial-timeout", selection: "overview", size: "920x620")
        else { return }

        for _ in 0..<3 {
            DistributedNotificationCenter.default().postNotificationName(
                Notification.Name("com.jackin-project.desktop.visual-qa.show-usage"),
                object: nil,
                userInfo: nil,
                deliverImmediately: true
            )
        }

        XCTAssertTrue(element("usage.overview.table").waitForExistence(timeout: 5))
        XCTAssertFalse(element("usage.provider.codex").exists)
        XCTAssertTrue(element("usage.overview.error.kimi").waitForExistence(timeout: 3))
        XCTAssertTrue(element("usage.overview.retry.kimi").isEnabled)
        XCTAssertTrue(element("usage.sidebar.provider.codex").exists)
    }

    func testRefreshingUsageExposesNativeBusyState() {
        defer { application.terminate() }
        guard
            launchUsage(
                fixture: "F07-refreshing-last-good", selection: "overview", size: "920x620"
            )
        else { return }

        let refresh = element("usage.refresh")
        XCTAssertTrue(refresh.waitForExistence(timeout: 5))
        XCTAssertEqual(refresh.label, "Refresh")
        XCTAssertEqual(refresh.value as? String, "In progress")
        XCTAssertFalse(refresh.isEnabled)
        XCTAssertTrue(element("usage.overview.table").exists)
        XCTAssertTrue(ensureUsageWindowVisible())

        let usageWindow = application.windows["usage-window"]
        let sidebar = element("usage.sidebar")
        XCTAssertTrue(sidebar.exists)
        XCTAssertTrue(usageWindow.frame.contains(sidebar.frame))
        XCTAssertTrue(usageWindow.frame.contains(refresh.frame))
    }

    func testRefreshActivityTransitionReachesNativeChrome() {
        defer { application.terminate() }
        application.launchArguments = ["--fixture", "F02-catalog-normal", "--ui-test"]
        application.launch()
        let statusItem = application.statusItems.matching(
            NSPredicate(format: "label CONTAINS %@", "OpenAI")
        ).firstMatch
        XCTAssertTrue(statusItem.waitForLabelContaining("Updated now", timeout: 3))

        DistributedNotificationCenter.default().postNotificationName(
            Notification.Name("com.jackin-project.desktop.visual-qa.refresh"),
            object: nil,
            userInfo: nil,
            deliverImmediately: true
        )
        XCTAssertTrue(statusItem.waitForLabelContaining("Updating…", timeout: 3))
        XCTAssertTrue(statusItem.waitForLabelContaining("Updated now", timeout: 3))
    }

    func testNativeSidebarOwnsLeadingRegionAndToggleKeepsItsCoordinate() {
        defer { application.terminate() }
        guard launchUsage(fixture: "F03-multi-account", selection: "codex", size: "920x620")
        else { return }
        XCTAssertTrue(ensureUsageWindowVisible())

        let usageWindow = application.windows.firstMatch
        let sidebar = element("usage.sidebar-pane")
        let detail = element("usage.detail-pane")
        XCTAssertTrue(sidebar.waitForExistence(timeout: 5), application.debugDescription)
        XCTAssertTrue(detail.waitForExistence(timeout: 5), application.debugDescription)
        XCTAssertLessThanOrEqual(sidebar.frame.minX - usageWindow.frame.minX, 8)
        XCTAssertLessThanOrEqual(usageWindow.frame.height - sidebar.frame.height, 16)
        XCTAssertLessThan(sidebar.frame.minY, element("usage.brand-title").frame.minY)

        let hideSidebar = sidebarToggle(label: "Hide Sidebar", in: usageWindow)
        XCTAssertTrue(hideSidebar.waitForExistence(timeout: 5), application.debugDescription)
        XCTAssertTrue(ensureUsageWindowVisible())
        XCTAssertTrue(hideSidebar.waitForHittable(timeout: 5), application.debugDescription)
        XCTAssertEqual(hideSidebar.label, "Hide Sidebar")
        XCTAssertEqual(sidebarToggleCount(label: "Hide Sidebar", in: usageWindow), 1)
        XCTAssertLessThan(hideSidebar.frame.midX, detail.frame.minX)
        let expandedPosition = hideSidebar.position(relativeTo: usageWindow)
        let expandedDetailWidth = detail.frame.width
        DistributedNotificationCenter.default().postNotificationName(
            Notification.Name("com.jackin-project.desktop.visual-qa.toggle-sidebar"),
            object: nil,
            userInfo: nil,
            deliverImmediately: true
        )

        let showSidebar = sidebarToggle(label: "Show Sidebar", in: usageWindow)
        XCTAssertTrue(showSidebar.waitForExistence(timeout: 3), application.debugDescription)
        XCTAssertTrue(showSidebar.isHittable)
        XCTAssertEqual(sidebarToggleCount(label: "Show Sidebar", in: usageWindow), 1)
        XCTAssertTrue(
            showSidebar.waitForPosition(
                expandedPosition, relativeTo: usageWindow, accuracy: 1, timeout: 3),
            "expanded=\(expandedPosition), collapsed=\(showSidebar.position(relativeTo: usageWindow))"
        )
        XCTAssertGreaterThan(detail.frame.width, expandedDetailWidth)
    }

    func testMultiAccountProviderUsesNativePicker() {
        defer { application.terminate() }
        guard launchUsage(fixture: "F03-multi-account", selection: "codex", size: "920x620")
        else { return }

        let ready = ensureUsageWindowVisible(contentIdentifier: "usage.account-picker")
        XCTAssertTrue(ready, application.debugDescription)
        guard ready else { return }
        XCTAssertTrue(element("usage.provider.codex").waitForExistence(timeout: 5))
        let picker = element("usage.account-picker")
        XCTAssertTrue(picker.waitForExistence(timeout: 3), application.debugDescription)
        XCTAssertTrue(picker.waitForHittable(timeout: 5), application.debugDescription)
        XCTAssertEqual(picker.elementType, .popUpButton)
        DistributedNotificationCenter.default().postNotificationName(
            Notification.Name("com.jackin-project.desktop.visual-qa.select-personal-account"),
            object: nil,
            userInfo: nil,
            deliverImmediately: true
        )
        XCTAssertTrue(
            picker.waitForValue("personal@example.test", timeout: 5), application.debugDescription)
        XCTAssertTrue(
            element("usage.provider-identity").waitForLabelContaining(
                "personal@example.test", timeout: 5)
        )
        XCTAssertTrue(element("usage.limit.bucket:selected").waitForExistence(timeout: 5))
        XCTAssertFalse(application.staticTexts["Accounts"].exists)
    }

    func testSidebarShortcutPreservesDetailKeyboardFocus() {
        defer { application.terminate() }
        guard launchUsage(fixture: "F03-multi-account", selection: "codex", size: "920x620")
        else { return }

        let focusedDetail = application.outlines.matching(
            NSPredicate(
                format: "identifier == %@ AND hasKeyboardFocus == true", "usage.provider.codex")
        ).firstMatch
        XCTAssertTrue(focusedDetail.waitForExistence(timeout: 3), application.debugDescription)

        let usageWindow = application.windows["usage-window"]
        XCTAssertTrue(toggleSidebarWithShortcut(in: usageWindow, expecting: "Show Sidebar"))
        XCTAssertTrue(focusedDetail.exists, application.debugDescription)

        XCTAssertTrue(toggleSidebarWithShortcut(in: usageWindow, expecting: "Hide Sidebar"))
        XCTAssertTrue(focusedDetail.exists, application.debugDescription)
    }

    func testEmptyUsageStateIsDistinct() {
        defer { application.terminate() }
        guard launchUsage(fixture: "F00-no-providers", selection: "overview", size: "760x500")
        else { return }
        XCTAssertTrue(
            element("usage.overview.empty").waitForExistence(timeout: 3),
            application.debugDescription
        )
    }

    func testLoadingUsageStateIsDistinct() {
        defer { application.terminate() }
        guard launchUsage(fixture: "F13-initial-loading", selection: "overview", size: "760x500")
        else { return }
        XCTAssertTrue(ensureUsageWindowVisible(contentIdentifier: "usage.loading"))
    }

    func testGlobalErrorUsageStateIsDistinct() {
        defer { application.terminate() }
        guard
            launchUsage(
                fixture: "F14-global-bridge-error", selection: "overview", size: "760x500"
            )
        else { return }
        XCTAssertTrue(ensureUsageWindowVisible(contentIdentifier: "usage.global-error"))
        let retry = element("usage.retry")
        XCTAssertTrue(retry.waitForExistence(timeout: 3))
        XCTAssertEqual(retry.label, "Retry")
        XCTAssertTrue(retry.isEnabled)
        XCTAssertTrue(application.windows["usage-window"].frame.intersects(retry.frame))
    }
}
