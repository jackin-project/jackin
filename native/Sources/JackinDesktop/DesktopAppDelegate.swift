// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import Combine
import JackinUsageBridge
import SwiftUI

/// Owns the per-provider `NSStatusItem`s, keyed by Rust `surfaceId`, and the one
/// shared transient popover.
///
/// Rust owns detection, ranking (SB-17), and every
/// string; this controller reconciles items against `store.statusBarGlanceRows`
/// and rebuilds when ranked order changes (SB-13).
@MainActor
public final class StatusBarController: NSObject {
    private let store: PresentationStore
    private let compactStatusItems: Bool
    private var providerItems: [String: NSStatusItem] = [:]
    private var fallbackItem: NSStatusItem?
    /// Last applied burn-first rank order (left → right = rank 1…n).
    private var canonicalOrder: [String] = []
    private let popover = NSPopover()
    private let popoverPresentationState = PopoverPresentationState()
    private weak var anchoredButton: NSStatusBarButton?
    private var automationAnchorPanel: NSPanel?
    private var rightClickMonitors: [ObjectIdentifier: Any] = [:]
    private var cancellables: Set<AnyCancellable> = []
    /// Opens the Usage window focused on a provider (`nil` = Overview).
    private let onOpenUsage: (UsageNavigationContext?) -> Void
    /// Owns the context menu and is the `NSMenuItem` target. Must stay retained
    ///
    /// for the bar lifetime (see `StatusItemMenu` docs — drop target ⇒ all rows disabled).
    private let statusItemMenu: StatusItemMenu

    init(
        store: PresentationStore,
        menuRouter: StatusItemMenuRouter,
        compactStatusItems: Bool = false,
        onOpenUsage: @escaping (UsageNavigationContext?) -> Void
    ) {
        self.store = store
        self.compactStatusItems = compactStatusItems
        self.onOpenUsage = onOpenUsage
        self.statusItemMenu = StatusItemMenu(router: menuRouter)
        super.init()
        // XCUITest steals application activation while synthesizing scroll gestures. Keep the
        // real NSPopover alive only in deterministic fixture runs; production remains transient.
        popover.behavior = compactStatusItems ? .applicationDefined : .transient
        popover.animates = true
        popover.contentSize = PopoverRoot.liveContentSize
        let root = PopoverRoot(
            store: store,
            presentationState: popoverPresentationState
        ) { [weak self] context in
            self?.popover.performClose(nil)
            self?.anchoredButton = nil
            self?.onOpenUsage(context)
        }
        popover.contentViewController = NSHostingController(rootView: root)

        // Burn-first chips only (SB-3/14/17/19) — not full providerGlanceRows inventory.
        store.$statusBarGlanceRows
            .receive(on: DispatchQueue.main)
            .sink { [weak self] rows in self?.apply(rows: rows) }
            .store(in: &cancellables)
        store.$statusBarShowsValues
            .receive(on: DispatchQueue.main)
            .sink { [weak self] _ in self?.refreshTitles() }
            .store(in: &cancellables)

        apply(rows: store.statusBarGlanceRows)
    }

    private func apply(rows: [PresentationStore.GlanceProviderRow]) {
        guard !rows.isEmpty else {
            removeAllProviderItems()
            ensureFallbackItem()
            return
        }
        removeFallbackItem()
        let newOrder = rows.map(\.surfaceId)
        // SB-13: NSStatusItem left→right order is creation order. When Rust
        // rank changes, remove and recreate so visual order tracks rank 1 first.
        if statusBarOrderRequiresRebuild(previous: canonicalOrder, next: newOrder) {
            removeAllProviderItems()
        } else {
            for id in Array(providerItems.keys) where !newOrder.contains(id) {
                removeProviderItem(id: id)
            }
        }
        canonicalOrder = newOrder
        for row in rows {
            let item = providerItems[row.surfaceId] ?? makeProviderItem(surfaceId: row.surfaceId)
            providerItems[row.surfaceId] = item
            configure(item: item, row: row)
        }
    }

    private func makeProviderItem(surfaceId: String) -> NSStatusItem {
        let item = NSStatusBar.system.statusItem(
            withLength: NSStatusItem.variableLength
        )
        if !compactStatusItems {
            item.autosaveName = "jackin.desktop.status.\(surfaceId)"
        }
        if let button = item.button {
            button.target = self
            button.action = #selector(handleClick(_:))
            button.sendAction(on: [.leftMouseUp])
            installRightClickMonitor(on: button)
        }
        return item
    }

    private func configure(item: NSStatusItem, row: PresentationStore.GlanceProviderRow) {
        guard let button = item.button else { return }
        // Template icon plus dual-stack values keeps status chrome system-owned.
        button.image = StatusItemRendering.icon(forIconKey: row.iconKey)
        button.imagePosition = .imageLeading
        button.attributedTitle =
            compactStatusItems
            ? NSAttributedString(string: " Fixture")
            : store.statusBarShowsValues
                ? StatusItemRendering.title(
                    barLabel: row.barLabel,
                    compactResetLabel: row.compactResetLabel
                )
                : NSAttributedString(string: "")
        button.appearsDisabled = row.dimmed
        button.toolTip = row.accessibilityLabel
        button.setAccessibilityLabel(row.accessibilityLabel)
    }

    private func ensureFallbackItem() {
        guard fallbackItem == nil else { return }
        let item = NSStatusBar.system.statusItem(
            withLength: NSStatusItem.variableLength
        )
        if !compactStatusItems {
            item.autosaveName = "jackin.desktop.status.fallback"
        }
        if let button = item.button {
            button.image = StatusItemRendering.fallbackIcon()
            button.target = self
            button.action = #selector(handleClick(_:))
            button.sendAction(on: [.leftMouseUp])
            installRightClickMonitor(on: button)
            button.setAccessibilityLabel("jackin❯ desktop usage")
            if compactStatusItems {
                button.title = " Fixture"
            }
        }
        fallbackItem = item
    }

    private func refreshTitles() {
        for row in store.statusBarGlanceRows {
            if let item = providerItems[row.surfaceId] {
                configure(item: item, row: row)
            }
        }
    }

    private func removeProviderItem(id: String) {
        guard let item = providerItems.removeValue(forKey: id) else { return }
        removeRightClickMonitor(from: item.button)
        if anchoredButton === item.button {
            popover.performClose(nil)
            anchoredButton = nil
        }
        NSStatusBar.system.removeStatusItem(item)
    }

    private func removeAllProviderItems() {
        for id in Array(providerItems.keys) {
            removeProviderItem(id: id)
        }
    }

    private func removeFallbackItem() {
        guard let item = fallbackItem else { return }
        removeRightClickMonitor(from: item.button)
        if anchoredButton === item.button {
            popover.performClose(nil)
            anchoredButton = nil
        }
        NSStatusBar.system.removeStatusItem(item)
        fallbackItem = nil
    }

    @objc private func handleClick(_ sender: NSStatusBarButton) {
        togglePopover(sender)
    }

    private func installRightClickMonitor(on button: NSStatusBarButton) {
        let identity = ObjectIdentifier(button)
        let monitor = NSEvent.addLocalMonitorForEvents(matching: .rightMouseUp) {
            [weak self, weak button] event in
            guard let self, let button,
                event.window === button.window,
                button.bounds.contains(button.convert(event.locationInWindow, from: nil))
            else { return event }
            DispatchQueue.main.async { [weak self, weak button] in
                guard let self, let button else { return }
                self.presentContextMenu(from: button)
            }
            return nil
        }
        rightClickMonitors[identity] = monitor
    }

    private func removeRightClickMonitor(from button: NSStatusBarButton?) {
        guard let button,
            let monitor = rightClickMonitors.removeValue(forKey: ObjectIdentifier(button))
        else { return }
        NSEvent.removeMonitor(monitor)
    }

    private func presentContextMenu(from sender: NSStatusBarButton) {
        statusItemMenu.popUp(
            positioning: nil,
            at: NSPoint(x: 0, y: sender.bounds.height + 4),
            in: sender
        )
    }

    /// Resolve which provider (or fallback) owns this status button.
    private func clickTarget(
        for button: NSStatusBarButton
    ) -> (surfaceId: String?, isFallback: Bool) {
        let identity = ObjectIdentifier(button)
        var map: [String: ObjectIdentifier] = [:]
        for (id, item) in providerItems {
            if let statusButton = item.button {
                map[id] = ObjectIdentifier(statusButton)
            }
        }
        if let sid = StatusPopoverFocus.surfaceId(
            matchingButtonIdentity: identity,
            providerButtonIdentities: map
        ) {
            return (sid, false)
        }
        if let fb = fallbackItem?.button, ObjectIdentifier(fb) == identity {
            return (nil, true)
        }
        return (nil, false)
    }

    private func togglePopover(_ sender: NSStatusBarButton) {
        // Anchored to the same button → toggle closed (keep last selection).
        if popover.isShown, anchoredButton === sender {
            popover.performClose(sender)
            anchoredButton = nil
            return
        }
        if popover.isShown {
            popover.performClose(sender)
        }
        // A primary status-item click focuses that provider; fallback has no provider focus.
        let target = clickTarget(for: sender)
        let outcome = StatusPopoverFocus.outcome(
            surfaceId: target.surfaceId,
            isFallbackItem: target.isFallback
        )
        store.popoverSelection = StatusPopoverFocus.popoverSelection(for: outcome)

        // A menu-bar click on a secondary display does not reliably activate
        // an accessory app. Presenting an inactive transient popover can then
        // fail silently even though the clicked status button is a valid
        // screen-local anchor.
        NSApp.activate(ignoringOtherApps: true)
        anchoredButton = sender
        popover.show(relativeTo: sender.bounds, of: sender, preferredEdge: .minY)
        popover.contentViewController?.view.window?.makeKey()
        resetPopoverScrollAfterPresentation()
    }

    /// Deterministic visual-QA launch seam.
    ///
    /// The real `NSPopover` remains anchored
    /// to a real status item; no detached preview host is introduced.
    func showPopover(focusOn surfaceId: String?, attemptsRemaining: Int = 200) {
        let button =
            surfaceId.flatMap { providerItems[$0]?.button }
            ?? providerItems[store.popoverSelection ?? ""]?.button
            ?? store.providerGlanceRows.first.flatMap { providerItems[$0.surfaceId]?.button }
            ?? fallbackItem?.button
        guard let button else { return }
        guard button.window != nil else {
            guard attemptsRemaining > 0 else { return }
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { [weak self] in
                self?.showPopover(
                    focusOn: surfaceId,
                    attemptsRemaining: attemptsRemaining - 1
                )
            }
            return
        }
        if compactStatusItems {
            showAutomationPopover(focusOn: surfaceId)
            return
        }
        togglePopover(button)
    }

    /// Close the real popover between deterministic visual-QA assertions.
    func closePopoverForVisualQA() {
        popover.performClose(nil)
        anchoredButton = nil
    }

    private func showAutomationPopover(focusOn surfaceId: String?) {
        if let surfaceId, providerItems[surfaceId] != nil {
            store.popoverSelection = surfaceId
        }
        let panel = automationAnchorPanel ?? makeAutomationAnchorPanel()
        panel.orderFrontRegardless()
        guard let anchor = panel.contentView else { return }
        anchor.setAccessibilityElement(false)
        automationAnchorPanel = panel
        popover.show(relativeTo: anchor.bounds, of: anchor, preferredEdge: .minY)
        resetPopoverScrollAfterPresentation()
    }

    private func resetPopoverScrollAfterPresentation() {
        DispatchQueue.main.async { [weak self] in
            self?.popoverPresentationState.beginPresentation()
        }
    }

    private func makeAutomationAnchorPanel() -> NSPanel {
        let frame = NSScreen.main?.frame ?? NSRect(x: 0, y: 0, width: 1_440, height: 900)
        let panel = NSPanel(
            contentRect: NSRect(x: frame.maxX - 4, y: frame.maxY - 4, width: 2, height: 2),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        panel.backgroundColor = .clear
        panel.isOpaque = false
        panel.hasShadow = false
        panel.ignoresMouseEvents = true
        panel.level = .statusBar
        panel.collectionBehavior = [.canJoinAllSpaces, .stationary]
        panel.setAccessibilityElement(false)
        return panel
    }

    /// Cancel subscriptions, close the popover, and remove every status item.
    ///
    /// Safe to call more than once.
    func invalidate() {
        cancellables.removeAll()
        if popover.isShown {
            popover.performClose(nil)
        }
        popover.contentViewController = nil
        anchoredButton = nil
        automationAnchorPanel?.orderOut(nil)
        automationAnchorPanel = nil
        removeAllProviderItems()
        removeFallbackItem()
    }
}

/// Application delegate for jackin❯ desktop (menu-bar agent).
///
/// Owns the store,
/// status-bar controller, main menu, and document windows (Usage / Settings).
@MainActor
public final class DesktopAppDelegate: NSObject, NSApplicationDelegate {
    private static let visualQAShowUsageNotification = Notification.Name(
        "com.jackin-project.desktop.visual-qa.show-usage"
    )
    private static let visualQAShowPopoverNotification = Notification.Name(
        "com.jackin-project.desktop.visual-qa.show-popover"
    )
    private static let visualQAClosePopoverNotification = Notification.Name(
        "com.jackin-project.desktop.visual-qa.close-popover"
    )
    private static let visualQARefreshNotification = Notification.Name(
        "com.jackin-project.desktop.visual-qa.refresh"
    )
    private static let visualQASelectAccountNotification = Notification.Name(
        "com.jackin-project.desktop.visual-qa.select-personal-account"
    )
    private static let visualQAToggleSidebarNotification = Notification.Name(
        "com.jackin-project.desktop.visual-qa.toggle-sidebar"
    )
    private static let visualQASelectCodexProviderNotification = Notification.Name(
        "com.jackin-project.desktop.visual-qa.select-codex-provider"
    )
    let store: PresentationStore
    private let launchConfiguration: PresentationStore.LaunchConfiguration
    private let visualQALaunchOptions: VisualQALaunchOptions
    private var visualQAUsageSelection: String?
    private var statusBar: StatusBarController?
    private var usageWindow: UsageWindowController?
    /// Retained: menu item targets point here / AppMainMenu for the process life.
    private var mainMenu: AppMainMenu?

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
        self.mainMenu = menu

        let router = StatusItemMenuRouter(
            openUsageWindow: { [weak usageWindow] surfaceId in usageWindow?.show(focusOn: surfaceId)
            },
            refresh: { [weak store] in store?.refreshAll() },
            quit: { NSApp.terminate(nil) }
        )
        let statusBar = StatusBarController(
            store: store,
            menuRouter: router,
            compactStatusItems: visualQALaunchOptions.usesFixture
        ) {
            [weak usageWindow] context in
            usageWindow?.show(context: context)
        }
        self.statusBar = statusBar

        if visualQALaunchOptions.usesFixture {
            DistributedNotificationCenter.default().addObserver(
                self,
                selector: #selector(showUsageForVisualQA(_:)),
                name: Self.visualQAShowUsageNotification,
                object: nil
            )
            DistributedNotificationCenter.default().addObserver(
                self,
                selector: #selector(showPopoverForVisualQA(_:)),
                name: Self.visualQAShowPopoverNotification,
                object: nil
            )
            DistributedNotificationCenter.default().addObserver(
                self,
                selector: #selector(closePopoverForVisualQA(_:)),
                name: Self.visualQAClosePopoverNotification,
                object: nil
            )
            DistributedNotificationCenter.default().addObserver(
                self,
                selector: #selector(refreshForVisualQA(_:)),
                name: Self.visualQARefreshNotification,
                object: nil
            )
            DistributedNotificationCenter.default().addObserver(
                self,
                selector: #selector(selectPersonalAccountForVisualQA(_:)),
                name: Self.visualQASelectAccountNotification,
                object: nil
            )
            DistributedNotificationCenter.default().addObserver(
                self,
                selector: #selector(toggleSidebarForVisualQA(_:)),
                name: Self.visualQAToggleSidebarNotification,
                object: nil
            )
            DistributedNotificationCenter.default().addObserver(
                self,
                selector: #selector(selectCodexProviderForVisualQA(_:)),
                name: Self.visualQASelectCodexProviderNotification,
                object: nil
            )
        }

        let selection: String?
        switch visualQALaunchOptions.selection {
        case .fixtureDefault:
            selection = fixture?.usageSelection
        case .overview:
            selection = nil
        case .provider(let surfaceId):
            selection = surfaceId
        }
        visualQAUsageSelection = selection
        if visualQALaunchOptions.openUsage || visualQALaunchOptions.invalidFixtureID != nil {
            let windowSize = visualQALaunchOptions.windowSize
            // AppKit can leave a synchronously constructed launch window absent from the
            // accessibility tree until the application-finished callback returns. Present on
            // the next main-loop turn, matching the real popover launch seam below.
            DispatchQueue.main.async { [weak usageWindow] in
                usageWindow?.show(focusOn: selection, size: windowSize)
            }
        }
        if visualQALaunchOptions.openPopover {
            // Automation launches without a status-item click. Front the fixture process so the
            // system menu bar attaches its real status-item window before NSPopover presentation.
            NSApp.setActivationPolicy(.regular)
            NSApp.activate()
            let popoverSelection = selection ?? fixture?.popoverSelection
            DispatchQueue.main.async { [weak statusBar] in
                statusBar?.showPopover(focusOn: popoverSelection)
            }
        }
    }

    public func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        // Menu-bar agent stays alive after Usage/Settings close.
        false
    }

    public func applicationDidHide(_ notification: Notification) {
        restoreFixtureUsageVisibility()
    }

    private func restoreFixtureUsageVisibility() {
        guard visualQALaunchOptions.usesFixture else { return }
        // The UI-test runner may own focus while scrolling or auditing. Reverse only an actual
        // application hide, without activating the fixture process or fighting the runner.
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
        // Dock click while regular (or after hide) → bring Usage forward.
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

    @objc private func showUsageForVisualQA(_: Notification) {
        usageWindow?.show(focusOn: visualQAUsageSelection)
    }

    @objc private func showPopoverForVisualQA(_: Notification) {
        statusBar?.showPopover(focusOn: store.popoverSelection)
    }

    @objc private func closePopoverForVisualQA(_: Notification) {
        statusBar?.closePopoverForVisualQA()
    }

    @objc private func refreshForVisualQA(_: Notification) {
        store.refreshAll()
    }

    @objc private func selectPersonalAccountForVisualQA(_: Notification) {
        store.setSelectedAccount(surfaceId: "codex", accountKey: "codex-personal")
    }

    @objc private func toggleSidebarForVisualQA(_: Notification) {
        usageWindow?.qiToggleSidebar()
    }

    @objc private func selectCodexProviderForVisualQA(_: Notification) {
        usageWindow?.show(focusOn: "codex")
    }

    private func enforceDarkAppearance() {
        NSApp.appearance = NSAppearance(named: .darkAqua)
    }

    private var initialActivationPolicy: NSApplication.ActivationPolicy {
        if visualQALaunchOptions.openUsage || visualQALaunchOptions.openPopover
            || visualQALaunchOptions.invalidFixtureID != nil
        {
            return .regular
        }
        return .accessory
    }

    private func applyVisualQAFixtureIfRequested() -> VisualQAFixture? {
        if let id = visualQALaunchOptions.fixtureID {
            let fixture = VisualQAFixtures.fixture(id: id)
            store.applyQIFixture(
                glanceRows: fixture.glanceRows,
                statusBarGlanceRows: fixture.statusGlanceRows,
                surfaces: fixture.surfaces,
                accounts: fixture.accounts,
                providerGroups: fixture.providerGroups,
                refreshingProjection: fixture.refreshingProjection,
                accountProjections: fixture.accountProjections,
                popoverSelection: fixture.popoverSelection,
                usageSelection: fixture.usageSelection,
                nextRefreshLabel: fixture.nextRefreshLabel,
                isLoading: fixture.isLoading,
                isRefreshing: fixture.isRefreshing,
                lastError: fixture.globalError
            )
            return fixture
        }
        if let invalid = visualQALaunchOptions.invalidFixtureID {
            store.applyQIFixture(
                glanceRows: [],
                surfaces: [],
                accounts: [],
                providerGroups: [],
                popoverSelection: nil,
                usageSelection: nil,
                nextRefreshLabel: "",
                lastError: "Unknown fixture ID: \(invalid)"
            )
            return nil
        }
        store.openForLaunch(launchConfiguration)
        return nil
    }
}
