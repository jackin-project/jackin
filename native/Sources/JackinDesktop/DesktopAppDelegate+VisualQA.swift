// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import JackinUsageBridge
import SwiftUI

@MainActor
extension DesktopAppDelegate {
    func installDesktopWindows() -> (UsageWindowController, StatusBarController) {
        let usageWindow = UsageWindowController(
            store: store,
            elevatesFixtureWindow: visualQALaunchOptions.elevatesFixtureWindow,
            onSplitControllerCreated: { [weak self] splitController in
                self?.mainMenu?.routeSidebar(to: splitController)
            }
        )
        self.usageWindow = usageWindow

        let menu = AppMainMenu(
            store: store,
            openUsage: { [weak usageWindow] in usageWindow?.show() }
        )
        menu.install()
        mainMenu = menu

        let router = StatusItemMenuRouter(
            openUsageWindow: { [weak usageWindow] surfaceId in
                usageWindow?.show(focusOn: surfaceId)
            },
            refresh: { [weak store] in store?.refreshAll() },
            quit: { NSApp.terminate(nil) }
        )
        let statusBar = StatusBarController(
            store: store,
            menuRouter: router,
            compactStatusItems: visualQALaunchOptions.usesFixture
        ) { [weak usageWindow] context in
            usageWindow?.show(context: context)
        }
        self.statusBar = statusBar
        return (usageWindow, statusBar)
    }

    func selectedFixtureUsage(_ fixture: VisualQAFixture?) -> String? {
        switch visualQALaunchOptions.selection {
        case .fixtureDefault:
            fixture?.usageSelection
        case .overview:
            nil
        case .provider(let surfaceId):
            surfaceId
        }
    }

    func showRequestedFixtureWindows(
        usageWindow: UsageWindowController,
        statusBar: StatusBarController,
        fixture: VisualQAFixture?,
        selection: String?
    ) {
        if visualQALaunchOptions.openUsage || visualQALaunchOptions.invalidFixtureID != nil {
            DispatchQueue.main.async { [weak usageWindow] in
                usageWindow?.show(focusOn: selection, size: self.visualQALaunchOptions.windowSize)
            }
        }
        guard visualQALaunchOptions.openPopover else { return }
        NSApp.setActivationPolicy(.regular)
        NSApp.activate()
        let popoverSelection = selection ?? fixture?.popoverSelection
        DispatchQueue.main.async { [weak statusBar] in
            statusBar?.showPopover(focusOn: popoverSelection)
        }
    }

    func registerVisualQANotifications() {
        let center = DistributedNotificationCenter.default()
        let notifications: [(Selector, Notification.Name)] = [
            (#selector(showUsageForVisualQA(_:)), Self.visualQAShowUsageNotification),
            (#selector(showPopoverForVisualQA(_:)), Self.visualQAShowPopoverNotification),
            (#selector(closePopoverForVisualQA(_:)), Self.visualQAClosePopoverNotification),
            (#selector(refreshForVisualQA(_:)), Self.visualQARefreshNotification),
            (
                #selector(selectPersonalAccountForVisualQA(_:)),
                Self.visualQASelectAccountNotification
            ),
            (#selector(toggleSidebarForVisualQA(_:)), Self.visualQAToggleSidebarNotification),
            (
                #selector(selectCodexProviderForVisualQA(_:)),
                Self.visualQASelectCodexProviderNotification
            ),
        ]
        for (selector, name) in notifications {
            center.addObserver(self, selector: selector, name: name, object: nil)
        }
    }

    public func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        false
    }

    public func applicationDidHide(_ notification: Notification) {
        restoreFixtureUsageVisibility()
    }

    func restoreFixtureUsageVisibility() {
        guard visualQALaunchOptions.usesFixture else { return }
        NSApp.unhideWithoutActivation()
        if visualQALaunchOptions.openUsage, usageWindow?.qiVisibilityDesired == true,
            let window = usageWindow?.qiWindow
        {
            window.orderFrontRegardless()
        }
        if visualQALaunchOptions.openPopover {
            statusBar?.showPopover(focusOn: store.popoverSelection)
        }
    }

    public func applicationShouldHandleReopen(
        _ sender: NSApplication, hasVisibleWindows: Bool
    ) -> Bool {
        if !hasVisibleWindows, !visualQALaunchOptions.openPopover {
            usageWindow?.show()
        }
        return true
    }

    public func applicationWillTerminate(_ notification: Notification) {
        DistributedNotificationCenter.default().removeObserver(self)
        statusBar?.invalidate()
        statusBar = nil
        usageWindow?.invalidate()
        usageWindow = nil
        mainMenu = nil
        store.shutdown()
    }

    @objc func showUsageForVisualQA(_: Notification) {
        usageWindow?.show(focusOn: visualQAUsageSelection)
    }

    @objc func showPopoverForVisualQA(_: Notification) {
        statusBar?.showPopover(focusOn: store.popoverSelection)
    }

    @objc func closePopoverForVisualQA(_: Notification) {
        statusBar?.closePopoverForVisualQA()
    }

    @objc func refreshForVisualQA(_: Notification) {
        store.refreshAll()
    }

    @objc func selectPersonalAccountForVisualQA(_: Notification) {
        store.setSelectedAccount(surfaceId: "codex", accountKey: "codex-personal")
    }

    @objc func toggleSidebarForVisualQA(_: Notification) {
        usageWindow?.qiToggleSidebar()
    }

    @objc func selectCodexProviderForVisualQA(_: Notification) {
        usageWindow?.show(focusOn: "codex")
    }

    func enforceDarkAppearance() {
        NSApp.appearance = NSAppearance(named: .darkAqua)
    }

    var initialActivationPolicy: NSApplication.ActivationPolicy {
        if visualQALaunchOptions.openUsage || visualQALaunchOptions.openPopover
            || visualQALaunchOptions.invalidFixtureID != nil
        {
            return .regular
        }
        return .accessory
    }

    func applyVisualQAFixtureIfRequested() -> VisualQAFixture? {
        if let id = visualQALaunchOptions.fixtureID {
            let fixture = VisualQAFixtures.fixture(id: id)
            store.applyQIFixture(fixture.input)
            return fixture
        }
        if let invalid = visualQALaunchOptions.invalidFixtureID {
            store.applyQIFixture(
                QIFixtureInput(
                    projection: QIFixtureProjection(
                        glanceRows: [],
                        statusBarGlanceRows: [],
                        surfaces: [],
                        accounts: [],
                        providerGroups: []
                    ),
                    selection: QIFixtureSelection(popover: nil, usage: nil),
                    status: QIFixtureStatus(
                        nextRefreshLabel: "",
                        lastError: "Unknown fixture ID: \(invalid)"
                    )
                )
            )
            return nil
        }
        store.openForLaunch(launchConfiguration)
        return nil
    }
}
