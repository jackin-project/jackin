// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import JackinUsageBridge
import SwiftUI

/// macOS 26 native split ownership: full-height sidebar plus detail-local top accessory.
@MainActor
final class UsageWindowSplitController: NSSplitViewController {
    private let sidebarHost: NSHostingController<UsageWindowSidebar>
    private let detailHost: NSHostingController<UsageWindowDetail>
    private let detailAccessoryHost: NSHostingController<UsageWindowDetailAccessory>

    init(store: PresentationStore) {
        sidebarHost = NSHostingController(rootView: UsageWindowSidebar(store: store))
        detailHost = NSHostingController(rootView: UsageWindowDetail(store: store))
        detailAccessoryHost = NSHostingController(
            rootView: UsageWindowDetailAccessory(store: store)
        )
        super.init(nibName: nil, bundle: nil)

        let sidebarItem = NSSplitViewItem(sidebarWithViewController: sidebarHost)
        sidebarItem.allowsFullHeightLayout = true
        sidebarItem.minimumThickness = 190
        sidebarItem.maximumThickness = 280
        sidebarItem.collapseBehavior = .preferResizingSiblingsWithFixedSplitView

        let detailItem = NSSplitViewItem(viewController: detailHost)
        detailItem.automaticallyAdjustsSafeAreaInsets = true
        detailItem.addTopAlignedAccessoryViewController(makeDetailAccessory())

        addSplitViewItem(sidebarItem)
        addSplitViewItem(detailItem)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) is unavailable")
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        sidebarHost.view.setAccessibilityIdentifier("usage.sidebar-pane")
        sidebarHost.view.setAccessibilityLabel("Usage sidebar pane")
    }

    private func makeDetailAccessory() -> NSSplitViewItemAccessoryViewController {
        let accessory = NSSplitViewItemAccessoryViewController()
        accessory.automaticallyAppliesContentInsets = true
        accessory.preferredContentSize = NSSize(width: 0, height: 40)

        accessory.addChild(detailAccessoryHost)
        accessory.view = NSView()
        accessory.view.setAccessibilityElement(true)
        accessory.view.setAccessibilityRole(.group)
        accessory.view.setAccessibilityIdentifier("usage.detail-pane")
        accessory.view.setAccessibilityLabel("Usage detail pane")
        accessory.view.addSubview(detailAccessoryHost.view)
        detailAccessoryHost.view.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            detailAccessoryHost.view.leadingAnchor.constraint(
                equalTo: accessory.view.leadingAnchor),
            detailAccessoryHost.view.trailingAnchor.constraint(
                equalTo: accessory.view.trailingAnchor),
            detailAccessoryHost.view.topAnchor.constraint(equalTo: accessory.view.topAnchor),
            detailAccessoryHost.view.bottomAnchor.constraint(equalTo: accessory.view.bottomAnchor),
            accessory.view.heightAnchor.constraint(equalToConstant: 40),
        ])
        return accessory
    }
}

/// Standard AppKit toolbar identifiers keep the system toggle in one stable leading slot.
@MainActor
final class UsageWindowToolbar: NSObject, NSToolbarDelegate {
    static let identifier = NSToolbar.Identifier("usage.window-toolbar")
    private weak var sidebarItem: NSSplitViewItem?
    private weak var toolbar: NSToolbar?
    private let onSidebarStateChange: () -> Void
    private var sidebarObservation: NSKeyValueObservation?
    private var sidebarToggleWidthConstraint: NSLayoutConstraint?

    init(sidebarItem: NSSplitViewItem, onSidebarStateChange: @escaping () -> Void) {
        self.sidebarItem = sidebarItem
        self.onSidebarStateChange = onSidebarStateChange
        super.init()
        sidebarObservation = sidebarItem.observe(\.isCollapsed, options: [.initial, .new]) {
            [weak self] _, _ in
            Task { @MainActor [weak self] in
                self?.updateSidebarItemLabel()
                self?.onSidebarStateChange()
            }
        }
    }

    func makeToolbar() -> NSToolbar {
        let toolbar = NSToolbar(identifier: Self.identifier)
        toolbar.delegate = self
        toolbar.displayMode = .iconOnly
        toolbar.allowsUserCustomization = false
        toolbar.autosavesConfiguration = false
        return toolbar
    }

    func installStandardItems(in toolbar: NSToolbar) {
        self.toolbar = toolbar
        toolbar.itemIdentifiers = [.toggleSidebar, .sidebarTrackingSeparator]
        updateSidebarItemLabel()
        lockSidebarToggleWidth()
        DispatchQueue.main.async { [weak self] in
            self?.updateSidebarItemLabel()
            self?.lockSidebarToggleWidth()
        }
    }

    func toolbarAllowedItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        [.toggleSidebar, .sidebarTrackingSeparator]
    }

    func toolbarDefaultItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        [.toggleSidebar, .sidebarTrackingSeparator]
    }

    private func updateSidebarItemLabel() {
        guard let toolbar, let sidebarItem else { return }
        let label = sidebarItem.isCollapsed ? "Show Sidebar" : "Hide Sidebar"
        guard let item = toolbar.items.first(where: { $0.itemIdentifier == .toggleSidebar }) else {
            return
        }
        item.label = label
        item.paletteLabel = label
        item.toolTip = label
        item.view?.setAccessibilityLabel(label)
        item.view?.setAccessibilityHelp(label)
        let button = item.view?.firstDescendant(of: NSButton.self)
        button?.setAccessibilityLabel(label)
        button?.setAccessibilityHelp(label)
    }

    private func lockSidebarToggleWidth() {
        guard sidebarToggleWidthConstraint == nil,
            let item = toolbar?.items.first(where: { $0.itemIdentifier == .toggleSidebar }),
            let view = item.view
        else { return }
        view.layoutSubtreeIfNeeded()
        let nativeWidth = view.frame.width > 0 ? view.frame.width : 44
        let constraint = view.widthAnchor.constraint(equalToConstant: nativeWidth)
        constraint.isActive = true
        sidebarToggleWidthConstraint = constraint
    }
}

extension NSView {
    fileprivate func firstDescendant<T: NSView>(of type: T.Type) -> T? {
        if let match = self as? T {
            return match
        }
        for subview in subviews {
            if let match = subview.firstDescendant(of: type) {
                return match
            }
        }
        return nil
    }
}
