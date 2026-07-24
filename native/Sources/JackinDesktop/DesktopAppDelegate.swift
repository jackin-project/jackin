// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import Combine
import JackinUsageBridge
import SwiftUI

/// Owns the per-provider `NSStatusItem`s, keyed by Rust `surfaceId`, and the one
/// shared transient popover. Rust owns detection, ordering, and every string;
/// this controller only reconciles items against `store.providerGlanceRows`.
@MainActor
final class StatusBarController: NSObject {
    private let store: PresentationStore
    private var providerItems: [String: NSStatusItem] = [:]
    private var fallbackItem: NSStatusItem?
    /// Rust canonical id order (never sorted in Swift), for reconciliation.
    private var canonicalOrder: [String] = []
    private let popover = NSPopover()
    private weak var anchoredButton: NSStatusBarButton?
    private var cancellables: Set<AnyCancellable> = []

    init(store: PresentationStore) {
        self.store = store
        super.init()
        popover.behavior = .transient
        popover.contentViewController = NSHostingController(rootView: PopoverRoot(store: store))

        store.$providerGlanceRows
            .receive(on: RunLoop.main)
            .sink { [weak self] rows in self?.apply(rows: rows) }
            .store(in: &cancellables)
        store.$statusBarShowsValues
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in self?.refreshTitles() }
            .store(in: &cancellables)

        apply(rows: store.providerGlanceRows)
    }

    private func apply(rows: [PresentationStore.GlanceProviderRow]) {
        guard !rows.isEmpty else {
            removeAllProviderItems()
            ensureFallbackItem()
            return
        }
        removeFallbackItem()
        canonicalOrder = rows.map(\.surfaceId)
        // Remove items whose id disappeared from the Rust list.
        for id in Array(providerItems.keys) where !canonicalOrder.contains(id) {
            removeProviderItem(id: id)
        }
        // Create only new ids while iterating the unchanged Rust order; update the rest in place.
        for row in rows {
            let item = providerItems[row.surfaceId] ?? makeProviderItem(surfaceId: row.surfaceId)
            providerItems[row.surfaceId] = item
            configure(item: item, row: row)
        }
    }

    private func makeProviderItem(surfaceId: String) -> NSStatusItem {
        let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        item.autosaveName = "jackin.desktop.status.\(surfaceId)"
        if let button = item.button {
            button.target = self
            button.action = #selector(togglePopover(_:))
            button.sendAction(on: [.leftMouseUp])
        }
        return item
    }

    private func configure(item: NSStatusItem, row: PresentationStore.GlanceProviderRow) {
        guard let button = item.button else { return }
        button.image = StatusItemRendering.icon(forIconKey: row.iconKey)
        button.attributedTitle =
            store.statusBarShowsValues ? StatusItemRendering.title(row.barLabel) : NSAttributedString(string: "")
        button.appearsDisabled = row.dimmed
        button.toolTip = row.headline
        button.setAccessibilityLabel("\(row.displayLabel) \(row.headline)")
    }

    private func ensureFallbackItem() {
        guard fallbackItem == nil else { return }
        let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        item.autosaveName = "jackin.desktop.status.fallback"
        if let button = item.button {
            button.image = StatusItemRendering.fallbackIcon()
            button.target = self
            button.action = #selector(togglePopover(_:))
            button.sendAction(on: [.leftMouseUp])
            button.setAccessibilityLabel("jackin❯ Desktop usage")
        }
        fallbackItem = item
    }

    private func refreshTitles() {
        for row in store.providerGlanceRows {
            if let item = providerItems[row.surfaceId] {
                configure(item: item, row: row)
            }
        }
    }

    private func removeProviderItem(id: String) {
        guard let item = providerItems.removeValue(forKey: id) else { return }
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
        if anchoredButton === item.button {
            popover.performClose(nil)
            anchoredButton = nil
        }
        NSStatusBar.system.removeStatusItem(item)
        fallbackItem = nil
    }

    @objc private func togglePopover(_ sender: NSStatusBarButton) {
        // Anchored to the same button → toggle closed.
        if popover.isShown, anchoredButton === sender {
            popover.performClose(sender)
            anchoredButton = nil
            return
        }
        if popover.isShown {
            popover.performClose(sender)
        }
        anchoredButton = sender
        popover.show(relativeTo: sender.bounds, of: sender, preferredEdge: .minY)
    }

    /// Cancel subscriptions, close the popover, and remove every status item.
    /// Safe to call more than once.
    func invalidate() {
        cancellables.removeAll()
        if popover.isShown {
            popover.performClose(nil)
        }
        popover.contentViewController = nil
        anchoredButton = nil
        removeAllProviderItems()
        removeFallbackItem()
    }
}

/// Application delegate for jackin❯ Desktop (menu-bar agent). Owns the store and
/// the status-bar controller; constructs no SwiftUI scene graph and no window.
@MainActor
final class DesktopAppDelegate: NSObject, NSApplicationDelegate {
    let store: PresentationStore
    private let launchConfiguration: PresentationStore.LaunchConfiguration
    private var statusBar: StatusBarController?

    override init() {
        self.launchConfiguration = PresentationStore.LaunchConfiguration.resolve(
            environment: ProcessInfo.processInfo.environment,
            homeDirectory: FileManager.default.homeDirectoryForCurrentUser.path
        )
        self.store = PresentationStore()
        super.init()
    }

    func applicationWillFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
        store.openForLaunch(launchConfiguration)
        statusBar = StatusBarController(store: store)
    }

    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        false
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows: Bool) -> Bool {
        false
    }

    func applicationWillTerminate(_ notification: Notification) {
        statusBar?.invalidate()
        statusBar = nil
        store.shutdown()
    }
}
