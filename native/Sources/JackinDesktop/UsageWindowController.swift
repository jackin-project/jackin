// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import AppKit
import JackinUsageBridge
import SwiftUI

enum UsageWindowMetrics {
    static let defaultContentSize = NSSize(width: 1000, height: 680)
    static let minimumContentSize = NSSize(width: 800, height: 520)
}

/// Lazily creates and retains the AppKit Usage window and its native split controller.
///
/// SwiftUI owns pane content. AppKit owns split geometry, the full-height sidebar,
/// its standard toolbar toggle, and the detail top accessory.
///
/// Showing the window promotes the process to `.regular` so the **system menu
/// bar** ( + AppMainMenu) is available; closing the last titled window returns
/// to `.accessory` status-item mode.
///
@MainActor
public final class UsageWindowController: NSObject, NSWindowDelegate {
    private let store: PresentationStore
    private let elevatesFixtureWindow: Bool
    private let onSplitControllerCreated: (NSSplitViewController) -> Void
    private var window: NSWindow?
    private var splitController: UsageWindowSplitController?
    private var toolbarController: UsageWindowToolbar?
    private var centeredBrandContainer: NSView?
    private var sidebarKeyMonitor: Any?
    private var fixtureVisibilityTask: Task<Void, Never>?
    private var visibilityDesired = false
    private var retainedFrame: NSRect?

    public init(
        store: PresentationStore,
        elevatesFixtureWindow: Bool = false,
        onSplitControllerCreated: @escaping (NSSplitViewController) -> Void = { _ in }
    ) {
        self.store = store
        self.elevatesFixtureWindow = elevatesFixtureWindow
        self.onSplitControllerCreated = onSplitControllerCreated
        super.init()
    }

    /// Show the retained Usage window without changing its valid destination.
    public func show(size: CGSize? = nil) {
        present(size: size)
    }

    /// Show the Usage window at an explicit provider surface id (`nil` = Overview).
    public func show(focusOn surfaceId: String?, size: CGSize? = nil) {
        store.selectUsageSurface(surfaceId)
        present(size: size)
    }

    /// Show at an exact provider/account handoff captured by the popover.
    public func show(context: UsageNavigationContext?, size: CGSize? = nil) {
        store.selectUsageContext(
            surfaceId: context?.surfaceId,
            accountKey: context?.accountKey
        )
        present(size: size)
    }

    private func present(size: CGSize?) {
        let isNewWindow = window == nil
        let window = window ?? makeWindow()
        self.window = window
        visibilityDesired = true
        if let size {
            window.setContentSize(size)
        } else if let retainedFrame {
            window.setFrame(retainedFrame, display: false)
        }
        if isNewWindow, elevatesFixtureWindow {
            centerFixtureWindowOnPrimaryScreen(window)
        }
        if elevatesFixtureWindow {
            // UI-test fixtures are launched as a regular application, but the runner can
            // leave the newly-created process inactive before the first accessibility query.
            // Activate the owner once at presentation; the visibility lease below only keeps
            // the window ordered front and does not steal focus during gestures.
            NSApp.unhide(nil)
            NSApp.activate()
            NSApp.activate(ignoringOtherApps: true)
            window.makeKeyAndOrderFront(nil)
            window.orderFrontRegardless()
            renewFixtureVisibilityLease(for: window)
        } else {
            AppActivation.present(window)
        }
    }

    private func renewFixtureVisibilityLease(for window: NSWindow) {
        fixtureVisibilityTask?.cancel()
        fixtureVisibilityTask = Task { @MainActor [weak self, weak window] in
            while true {
                guard !Task.isCancelled, self?.visibilityDesired == true, let window else { return }
                NSApp.unhideWithoutActivation()
                window.orderFrontRegardless()
                try? await Task.sleep(for: .milliseconds(250))
            }
        }
    }

    private func centerFixtureWindowOnPrimaryScreen(_ window: NSWindow) {
        guard
            let screen = NSScreen.screens.first(where: {
                abs($0.frame.origin.x) < 0.5 && abs($0.frame.origin.y) < 0.5
            }) ?? NSScreen.main ?? NSScreen.screens.first
        else { return }
        let visible = screen.visibleFrame
        window.setFrameOrigin(
            NSPoint(
                x: visible.midX - window.frame.width / 2,
                y: visible.midY - window.frame.height / 2
            )
        )
    }

    private func makeWindow() -> NSWindow {
        let window = NSWindow(
            contentRect: NSRect(origin: .zero, size: UsageWindowMetrics.defaultContentSize),
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        window.title = "jackin❯ desktop"
        window.isReleasedWhenClosed = false
        window.hidesOnDeactivate = false
        window.delegate = self
        if store.usesFixture {
            // Deterministic UI/visual QA must stay observable when WindowServer assigns rapid
            // fixture launches and the test runner to different or full-screen Spaces.
            window.canHide = false
            window.collectionBehavior.formUnion([
                .canJoinAllSpaces,
                .canJoinAllApplications,
                .fullScreenAuxiliary,
            ])
            window.level = .floating
        } else {
            window.collectionBehavior.insert(.moveToActiveSpace)
        }
        window.contentMinSize = UsageWindowMetrics.minimumContentSize

        window.minSize =
            NSWindow.frameRect(
                forContentRect: NSRect(
                    origin: .zero, size: UsageWindowMetrics.minimumContentSize),
                styleMask: window.styleMask
            ).size
        window.identifier = NSUserInterfaceItemIdentifier("usage-window")
        window.setAccessibilityIdentifier("usage-window")
        if !store.usesFixture {
            window.setFrameAutosaveName("jackin.desktop.usage-window")
        }

        // Unified titlebar + standard AppKit split toolbar; no app-painted chrome.
        window.toolbarStyle = .unified
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.titlebarSeparatorStyle = .automatic

        let split = UsageWindowSplitController(store: store)
        splitController = split
        window.contentViewController = split
        onSplitControllerCreated(split)
        sidebarKeyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) {
            [weak window, weak split] event in
            guard window?.isKeyWindow == true, AppMainMenu.isSidebarKeyEquivalent(event) else {
                return event
            }
            split?.toggleSidebar(window)
            return nil
        }

        let toolbarController = UsageWindowToolbar(
            sidebarItem: split.splitViewItems[0],
            onSidebarStateChange: { [weak self, weak window] in
                guard let self, let window else { return }
                self.installCenteredBrand(in: window)
            })
        self.toolbarController = toolbarController
        let toolbar = toolbarController.makeToolbar()
        window.toolbar = toolbar
        toolbarController.installStandardItems(in: toolbar)
        installCenteredBrand(in: window)

        window.center()
        return window
    }

    private func installCenteredBrand(in window: NSWindow) {
        guard let titlebar = window.standardWindowButton(.closeButton)?.superview else { return }
        centeredBrandContainer?.removeFromSuperview()
        centeredBrandContainer = nil
        // AppKit may replace the titlebar container while the unified toolbar is
        // installed or the sidebar collapses. Remove the old host anywhere in
        // the frame hierarchy before adding the replacement, otherwise stale
        // hosts remain in the accessibility tree and duplicate the brand.
        if let frameView = window.contentView?.superview {
            removeCenteredBrandHosts(from: frameView)
        }
        let container = NSView()
        container.translatesAutoresizingMaskIntoConstraints = false
        container.setAccessibilityElement(true)
        container.setAccessibilityRole(.group)
        container.setAccessibilityLabel("jackin❯ desktop")
        container.setAccessibilityIdentifier("usage.brand-title")
        let host = NSHostingView(rootView: JackinBrandSignature(width: 68, height: 18))
        host.translatesAutoresizingMaskIntoConstraints = false
        host.setAccessibilityElement(false)
        container.addSubview(host)
        titlebar.addSubview(container)
        NSLayoutConstraint.activate([
            container.centerXAnchor.constraint(equalTo: titlebar.centerXAnchor),
            container.centerYAnchor.constraint(equalTo: titlebar.centerYAnchor),
            container.widthAnchor.constraint(equalToConstant: 68),
            container.heightAnchor.constraint(equalToConstant: 18),
            host.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            host.trailingAnchor.constraint(equalTo: container.trailingAnchor),
            host.topAnchor.constraint(equalTo: container.topAnchor),
            host.bottomAnchor.constraint(equalTo: container.bottomAnchor),
        ])
        centeredBrandContainer = container
    }

    private func removeCenteredBrandHosts(from view: NSView) {
        for subview in view.subviews {
            if subview.identifier?.rawValue == "usage.brand-title" {
                subview.removeFromSuperview()
            } else {
                removeCenteredBrandHosts(from: subview)
            }
        }
    }

    public func windowWillClose(_ notification: Notification) {
        visibilityDesired = false
        fixtureVisibilityTask?.cancel()
        if let window = notification.object as? NSWindow {
            retainedFrame = window.frame
        }
        // Window is still visible during willClose; resign on next run-loop turn.
        DispatchQueue.main.async {
            AppActivation.resignToAccessoryIfNeeded()
        }
    }

    public func invalidate() {
        visibilityDesired = false
        fixtureVisibilityTask?.cancel()
        fixtureVisibilityTask = nil
        if let sidebarKeyMonitor {
            NSEvent.removeMonitor(sidebarKeyMonitor)
            self.sidebarKeyMonitor = nil
        }
        window?.delegate = nil
        window?.orderOut(nil)
        window?.contentViewController = nil
        window?.toolbar = nil
        centeredBrandContainer = nil
        splitController = nil
        toolbarController = nil
        window = nil
    }

    /// Visual QA: the live `NSWindow` after `show` (nil if never shown).
    public var qiWindow: NSWindow? { window }
    public var qiVisibilityDesired: Bool { visibilityDesired }
    public func qiToggleSidebar() {
        splitController?.toggleSidebar(window)
    }
}
