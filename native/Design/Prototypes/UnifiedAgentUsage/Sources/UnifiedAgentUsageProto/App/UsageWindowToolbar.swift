import AppKit

/// Standard AppKit toolbar identifiers keep the system toggle in one stable
/// leading slot; the label follows collapse state while the width stays put.
@MainActor
final class UsageWindowToolbar: NSObject, NSToolbarDelegate {
    static let identifier = NSToolbar.Identifier("usage.window-toolbar")
    static let refreshIdentifier = NSToolbarItem.Identifier("usage.refresh")
    private weak var sidebarItem: NSSplitViewItem?
    private weak var toolbar: NSToolbar?
    private let store: ProtoStore
    private let refreshTitle: String
    private let onSidebarStateChange: () -> Void
    private var sidebarObservation: NSKeyValueObservation?
    private var sidebarToggleWidthConstraint: NSLayoutConstraint?

    init(
        sidebarItem: NSSplitViewItem, store: ProtoStore, refreshTitle: String,
        onSidebarStateChange: @escaping () -> Void
    ) {
        self.sidebarItem = sidebarItem
        self.store = store
        self.refreshTitle = refreshTitle
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
        toolbar.itemIdentifiers = [
            .toggleSidebar, .sidebarTrackingSeparator, .flexibleSpace, Self.refreshIdentifier,
        ]
        updateSidebarItemLabel()
        lockSidebarToggleWidth()
        DispatchQueue.main.async { [weak self] in
            self?.updateSidebarItemLabel()
            self?.lockSidebarToggleWidth()
        }
    }

    func toolbar(
        _ toolbar: NSToolbar, itemForItemIdentifier itemIdentifier: NSToolbarItem.Identifier,
        willBeInsertedIntoToolbar flag: Bool
    ) -> NSToolbarItem? {
        guard itemIdentifier == Self.refreshIdentifier else { return nil }
        let item = NSToolbarItem(itemIdentifier: itemIdentifier)
        item.image = NSImage(
            systemSymbolName: "arrow.clockwise",
            accessibilityDescription: refreshTitle)
        item.target = self
        item.action = #selector(refresh(_:))
        item.label = refreshTitle
        item.paletteLabel = refreshTitle
        item.toolTip = refreshTitle
        item.visibilityPriority = .high
        return item
    }

    @objc private func refresh(_ sender: Any?) {
        store.refresh()
    }

    func toolbarAllowedItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        [.toggleSidebar, .sidebarTrackingSeparator, Self.refreshIdentifier]
    }

    func toolbarDefaultItemIdentifiers(_ toolbar: NSToolbar) -> [NSToolbarItem.Identifier] {
        [
            .toggleSidebar, .sidebarTrackingSeparator, .flexibleSpace, Self.refreshIdentifier,
        ]
    }

    private func updateSidebarItemLabel() {
        guard let toolbar, let sidebarItem else { return }
        let label = sidebarItem.isCollapsed ? "Show Sidebar" : "Hide Sidebar"
        guard let item = toolbar.items.first(where: { $0.itemIdentifier == .toggleSidebar })
        else { return }
        item.label = label
        item.paletteLabel = label
        item.toolTip = label
        item.view?.setAccessibilityLabel(label)
        item.view?.setAccessibilityHelp(label)
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
