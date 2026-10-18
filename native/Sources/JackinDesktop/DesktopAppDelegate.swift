// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import Combine
import JackinUsageBridge
import SwiftUI

@MainActor
public final class DesktopAppDelegate: NSObject, NSApplicationDelegate {
    static let visualQAShowUsageNotification = Notification.Name(
        "com.jackin-project.desktop.visual-qa.show-usage"
    )
    static let visualQAShowPopoverNotification = Notification.Name(
        "com.jackin-project.desktop.visual-qa.show-popover"
    )
    static let visualQAClosePopoverNotification = Notification.Name(
        "com.jackin-project.desktop.visual-qa.close-popover"
    )
    static let visualQARefreshNotification = Notification.Name(
        "com.jackin-project.desktop.visual-qa.refresh"
    )
    static let visualQASelectAccountNotification = Notification.Name(
        "com.jackin-project.desktop.visual-qa.select-personal-account"
    )
    static let visualQAToggleSidebarNotification = Notification.Name(
        "com.jackin-project.desktop.visual-qa.toggle-sidebar"
    )
    static let visualQASelectCodexProviderNotification = Notification.Name(
        "com.jackin-project.desktop.visual-qa.select-codex-provider"
    )
    let store: PresentationStore
    let launchConfiguration: PresentationStore.LaunchConfiguration
    let visualQALaunchOptions: VisualQALaunchOptions
    var visualQAUsageSelection: String?
    var statusBar: StatusBarController?
    var usageWindow: UsageWindowController?
    var mainMenu: AppMainMenu?

    override public init() {
        self.visualQALaunchOptions = VisualQALaunchOptions.resolve(
            arguments: ProcessInfo.processInfo.arguments,
            environment: ProcessInfo.processInfo.environment
        )
        self.launchConfiguration = PresentationStore.LaunchConfiguration.resolve(
            environment: ProcessInfo.processInfo.environment,
            homeDirectory: FileManager.default.homeDirectoryForCurrentUser.path
        )
        self.store = PresentationStore()
        super.init()
    }

    public func applicationWillFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(initialActivationPolicy)
    }

    public func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(initialActivationPolicy)
        enforceDarkAppearance()
        let fixture = applyVisualQAFixtureIfRequested()
        if visualQALaunchOptions.usesFixture {
            registerVisualQANotifications()
        }

        let (usageWindow, statusBar) = installDesktopWindows()
        let selection = selectedFixtureUsage(fixture)
        visualQAUsageSelection = selection
        showRequestedFixtureWindows(
            usageWindow: usageWindow,
            statusBar: statusBar,
            fixture: fixture,
            selection: selection
        )
    }
}
