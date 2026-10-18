// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import Combine
import JackinUsageBridge
import SwiftUI

@MainActor
public final class StatusBarController: NSObject {
    private let store: PresentationStore
    private let compactStatusItems: Bool
    private var providerItems: [String: NSStatusItem] = [:]
    private var fallbackItem: NSStatusItem?
    private var canonicalOrder: [String] = []
    private let popover = NSPopover()
    private let popoverPresentationState = PopoverPresentationState()
    private weak var anchoredButton: NSStatusBarButton?
    private var automationAnchorPanel: NSPanel?
    private var rightClickMonitors: [ObjectIdentifier: Any] = [:]
    private var cancellables: Set<AnyCancellable> = []
    private let onOpenUsage: (UsageNavigationContext?) -> Void
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

    private func apply(rows: [GlanceProviderRow]) {
        guard !rows.isEmpty else {
            removeAllProviderItems()
            ensureFallbackItem()
            return
        }
        removeFallbackItem()
        let newOrder = rows.map(\.surfaceId)
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

    private func configure(item: NSStatusItem, row: GlanceProviderRow) {
        guard let button = item.button else { return }
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
}

extension StatusBarController {
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
        if popover.isShown, anchoredButton === sender {
            popover.performClose(sender)
            anchoredButton = nil
            return
        }
        if popover.isShown {
            popover.performClose(sender)
        }
        let target = clickTarget(for: sender)
        let outcome = StatusPopoverFocus.outcome(
            surfaceId: target.surfaceId,
            isFallbackItem: target.isFallback
        )
        store.popoverSelection = StatusPopoverFocus.popoverSelection(for: outcome)

        NSApp.activate(ignoringOtherApps: true)
        anchoredButton = sender
        popover.show(relativeTo: sender.bounds, of: sender, preferredEdge: .minY)
        popover.contentViewController?.view.window?.makeKey()
        resetPopoverScrollAfterPresentation()
    }

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
