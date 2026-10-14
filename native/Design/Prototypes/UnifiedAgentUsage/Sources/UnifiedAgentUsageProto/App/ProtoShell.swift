import AppKit
import SwiftUI








@MainActor
final class ProtoShell: NSObject, NSMenuDelegate {
    let store: ProtoStore
    let config: ProtoConfig
    private var usageWindow: NSWindow?
    private var settingsWindow: NSWindow?
    private var popover: NSPopover?
    private var statusItems: [String: NSStatusItem] = [:]
    private var splitController: NSSplitViewController?
    private var toolbarController: UsageWindowToolbar?
    private var sidebarKeyMonitor: Any?

    init(store: ProtoStore, config: ProtoConfig) {
        self.store = store
        self.config = config
    }

    func install(into app: NSApplication) {
        app.mainMenu = buildMainMenu()
        installStatusItems()
        let window = makeUsageWindow()
        usageWindow = window
        window.center()
        window.makeKeyAndOrderFront(nil)
    }

    func wrap<V: View>(_ view: V) -> some View {










        view
            .tint(.jackinPhosphor)
            .accentColor(.jackinPhosphor)
            .environment(\.jackinIncreaseContrast, config.increaseContrast)
            .environment(\.jackinReduceTransparency, config.reduceTransparency)
            .environment(\.jackinReduceMotion, config.reduceMotion)
            .environment(\.locale, store.chrome.locale)
            .environment(\.layoutDirection, store.chrome.layoutDirection)
            .transaction { transaction in
                if self.config.reduceMotion { transaction.animation = nil }
            }
    }






    private func buildMainMenu() -> NSMenu {
        let main = NSMenu()
        main.addItem(wrap(appMenu(), title: "jackin❯ desktop"))
        main.addItem(wrap(fileMenu(), title: "File"))
        main.addItem(wrap(editMenu(), title: "Edit"))
        let view = viewMenu()
        main.addItem(wrap(view, title: "View"))
        main.addItem(wrap(scenarioMenu(), title: "Scenario"))
        let window = windowMenu()
        main.addItem(wrap(window, title: "Window"))
        main.addItem(wrap(helpMenu(), title: "Help"))
        NSApp.windowsMenu = window
        return main
    }



    private func scenarioMenu() -> NSMenu {
        let menu = NSMenu(title: "Scenario")
        for group in ProtoFixtures.scenarioMenu {
            for name in group.names {
                let description = ProtoFixtures.scenarioDescriptions[name] ?? name
                let item = NSMenuItem(
                    title: "\(name) — \(description)",
                    action: #selector(loadScenario(_:)), keyEquivalent: "")
                item.target = self
                item.representedObject = name
                item.toolTip = description
                if name == store.projection.scenario { item.state = .on }
                menu.addItem(item)
            }
            menu.addItem(.separator())
        }
        menu.delegate = self
        return menu
    }

    @objc private func loadScenario(_ sender: NSMenuItem) {
        guard let name = sender.representedObject as? String else { return }
        store.loadScenario(name)
    }

    private func appMenu() -> NSMenu {
        let menu = NSMenu(title: "jackin❯ desktop")
        menu.addItem(
            withTitle: "About jackin❯ desktop", action: #selector(orderFrontAbout(_:)),
            keyEquivalent: "")
        menu.addItem(.separator())
        menu.addItem(
            withTitle: "Settings…", action: #selector(showSettings),
            keyEquivalent: ",")
        menu.addItem(.separator())
        let services = NSMenu(title: "Services")
        let servicesItem = NSMenuItem(title: "Services", action: nil, keyEquivalent: "")
        servicesItem.submenu = services
        menu.addItem(servicesItem)
        NSApp.servicesMenu = services
        menu.addItem(.separator())
        menu.addItem(
            withTitle: "Hide jackin❯ desktop", action: #selector(NSApplication.hide(_:)),
            keyEquivalent: "h")
        let hideOthers = menu.addItem(
            withTitle: "Hide Others", action: #selector(NSApplication.hideOtherApplications(_:)),
            keyEquivalent: "h")
        hideOthers.keyEquivalentModifierMask = [.command, .option]
        menu.addItem(
            withTitle: "Show All", action: #selector(NSApplication.unhideAllApplications(_:)),
            keyEquivalent: "")
        menu.addItem(.separator())
        menu.addItem(
            withTitle: "Quit jackin❯ desktop",
            action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        return menu
    }

    private func fileMenu() -> NSMenu {
        let menu = NSMenu(title: "File")
        menu.addItem(
            withTitle: "Close Window", action: #selector(NSWindow.performClose(_:)),
            keyEquivalent: "w")
        return menu
    }

    private func editMenu() -> NSMenu {

        let menu = NSMenu(title: "Edit")
        menu.addItem(withTitle: "Undo", action: Selector(("undo:")), keyEquivalent: "z")
        let redo = menu.addItem(
            withTitle: "Redo", action: Selector(("redo:")), keyEquivalent: "z")
        redo.keyEquivalentModifierMask = [.command, .shift]
        menu.addItem(.separator())
        menu.addItem(withTitle: "Cut", action: #selector(NSText.cut(_:)), keyEquivalent: "x")
        menu.addItem(withTitle: "Copy", action: #selector(NSText.copy(_:)), keyEquivalent: "c")
        menu.addItem(withTitle: "Paste", action: #selector(NSText.paste(_:)), keyEquivalent: "v")
        menu.addItem(
            withTitle: "Select All", action: #selector(NSText.selectAll(_:)), keyEquivalent: "a")
        return menu
    }

    private func viewMenu() -> NSMenu {
        let menu = NSMenu(title: "View")
        menu.delegate = self
        let toggle = NSMenuItem(
            title: "Hide Sidebar",
            action: #selector(NSSplitViewController.toggleSidebar(_:)),
            keyEquivalent: "s")
        toggle.keyEquivalentModifierMask = [.command, .control]
        toggle.target = nil
        menu.addItem(toggle)
        menu.addItem(.separator())
        menu.addItem(
            withTitle: store.chrome.refreshTitle, action: #selector(refreshNow),
            keyEquivalent: "r")
        return menu
    }

    private func windowMenu() -> NSMenu {
        let menu = NSMenu(title: "Window")
        menu.addItem(
            withTitle: "Minimize", action: #selector(NSWindow.performMiniaturize(_:)),
            keyEquivalent: "m")
        menu.addItem(
            withTitle: "Zoom", action: #selector(NSWindow.performZoom(_:)), keyEquivalent: "")
        menu.addItem(.separator())
        menu.addItem(
            withTitle: "Usage", action: #selector(showUsage), keyEquivalent: "0")
        menu.addItem(.separator())
        menu.addItem(
            withTitle: "Bring All to Front",
            action: #selector(NSApplication.arrangeInFront(_:)), keyEquivalent: "")
        return menu
    }

    private func helpMenu() -> NSMenu {
        let menu = NSMenu(title: "Help")
        menu.addItem(
            withTitle: "jackin❯ desktop Help", action: #selector(NSApplication.showHelp(_:)),
            keyEquivalent: "?")
        return menu
    }

    private func wrap(_ menu: NSMenu, title: String) -> NSMenuItem {
        let item = NSMenuItem(title: title, action: nil, keyEquivalent: "")
        item.submenu = menu
        return item
    }

    @objc private func orderFrontAbout(_: Any?) {
        NSApp.orderFrontStandardAboutPanel(options: [
            .applicationName: "jackin❯ desktop",
            .credits: NSAttributedString(
                string: "Limits-only usage for agent credentials.\nDisplay shell over jackin-usage."
            ),
        ])
    }



    private func makeUsageWindow() -> NSWindow {
        let split = NSSplitViewController()
        split.splitView.isVertical = true

        let sidebarHost = NSHostingController(rootView: wrap(SidebarView(store: store)))



        sidebarHost.sizingOptions = []
        let sidebarItem = NSSplitViewItem(sidebarWithViewController: sidebarHost)
        sidebarItem.allowsFullHeightLayout = true
        sidebarItem.minimumThickness = 220
        sidebarItem.maximumThickness = 280
        sidebarItem.collapseBehavior = .preferResizingSiblingsWithFixedSplitView

        let detailHost = NSHostingController(
            rootView: wrap(
                DetailRootView(store: store) { [weak self] in
                    self?.showSettings()
                }))
        detailHost.sizingOptions = []
        let detailItem = NSSplitViewItem(viewController: detailHost)
        detailItem.automaticallyAdjustsSafeAreaInsets = true

        split.addSplitViewItem(sidebarItem)
        split.addSplitViewItem(detailItem)
        splitController = split

        let window = NSWindow(
            contentRect: NSRect(origin: .zero, size: ProtoUsageWindowMetrics.defaultContentSize),
            styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView],
            backing: .buffered,
            defer: false)
        window.title = "Usage"
        window.isReleasedWhenClosed = false
        window.setContentSize(config.window ?? ProtoUsageWindowMetrics.defaultContentSize)
        window.contentViewController = split




        window.toolbarStyle = .unified
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.titlebarSeparatorStyle = .automatic

        sidebarKeyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) {
            [weak window, weak split] event in
            guard window?.isKeyWindow == true,
                Self.isSidebarKeyEquivalent(event)
            else { return event }
            split?.toggleSidebar(window)
            return nil
        }

        let toolbarController = UsageWindowToolbar(
            sidebarItem: sidebarItem, store: store,
            refreshTitle: store.chrome.refreshTitle,
            onSidebarStateChange: { [weak self, weak window] in
                guard let self, let window else { return }
                self.installCenteredBrand(in: window)
            })
        self.toolbarController = toolbarController
        let toolbar = toolbarController.makeToolbar()
        window.toolbar = toolbar
        toolbarController.installStandardItems(in: toolbar)
        installCenteredBrand(in: window)







        window.contentMinSize = ProtoUsageWindowMetrics.minimumContentSize
        window.minSize =
            NSWindow.frameRect(
                forContentRect: NSRect(
                    origin: .zero, size: ProtoUsageWindowMetrics.minimumContentSize),
                styleMask: window.styleMask
            ).size


        window.center()
        observeWindowTitles(window)
        return window
    }

    private func installCenteredBrand(in window: NSWindow) {
        guard let titlebar = window.standardWindowButton(.closeButton)?.superview else { return }
        titlebar.subviews
            .filter { $0.identifier?.rawValue == "usage.brand-title" }
            .forEach { $0.removeFromSuperview() }
        let host = NSHostingView(rootView: JackinBrandSignature(width: 68, height: 18))
        host.translatesAutoresizingMaskIntoConstraints = false
        host.setAccessibilityLabel("jackin❯ desktop")
        host.identifier = NSUserInterfaceItemIdentifier("usage.brand-title")
        titlebar.addSubview(host)
        NSLayoutConstraint.activate([
            host.centerXAnchor.constraint(equalTo: titlebar.centerXAnchor),
            host.centerYAnchor.constraint(equalTo: titlebar.centerYAnchor),
            host.widthAnchor.constraint(equalToConstant: 68),
            host.heightAnchor.constraint(equalToConstant: 18),
        ])
    }



    private func observeWindowTitles(_ window: NSWindow) {
        withObservationTracking {
            applyWindowTitles(window)
        } onChange: { [weak self, weak window] in
            Task { @MainActor in
                guard let self, let window else { return }
                self.observeWindowTitles(window)
            }
        }
    }

    private func applyWindowTitles(_ window: NSWindow) {
        switch store.resolvedSidebar {
        case .overview:
            window.title = "Usage"
            window.subtitle = ""
        case .provider(let key):
            window.title = store.provider(key)?.name ?? "Usage"
            window.subtitle = ""
        case .account(let providerKey, _):
            let provider = store.provider(providerKey)
            window.title = provider?.name ?? "Usage"
            window.subtitle =
                provider.flatMap { store.account(for: $0) }?.label ?? ""
        }
    }

    static func isSidebarKeyEquivalent(_ event: NSEvent) -> Bool {
        let commandModifiers: NSEvent.ModifierFlags = [.command, .control, .option, .shift]
        return event.type == .keyDown
            && (event.keyCode == 1
                || event.charactersIgnoringModifiers?.lowercased() == "s")
            && event.modifierFlags.intersection(commandModifiers) == [.command, .control]
    }



    func menuWillOpen(_ menu: NSMenu) {
        if menu.title == "Scenario" {
            for item in menu.items {
                if let name = item.representedObject as? String {
                    item.state = name == store.projection.scenario ? .on : .off
                }
            }
            return
        }
        guard menu.title == "View",
            let toggle = menu.items.first(where: {
                $0.action == #selector(NSSplitViewController.toggleSidebar(_:))
            })
        else { return }
        let collapsed = splitController?.splitViewItems.first?.isCollapsed == true
        toggle.title = collapsed ? "Show Sidebar" : "Hide Sidebar"
    }


    private func installStatusItems() {
        reconcileStatusItems()
    }

    private func makeStatusItem(for key: String) {
        guard let provider = store.provider(key) else { return }
        let item = NSStatusBar.system.statusItem(
            withLength: NSStatusItem.variableLength)
        if let button = item.button {
            button.image =
                ProviderMarks.templateImage(forIconKey: provider.iconKey)
                ?? JackinBrandIdentity.templateMonogram()
            button.imagePosition = .imageLeading


            button.imageHugsTitle = true
            button.toolTip = provider.name
            button.setAccessibilityLabel(provider.name)
            button.target = self
            button.action = #selector(statusItemClicked(_:))
            button.sendAction(on: [.leftMouseUp, .rightMouseUp])
        }
        statusItems[key] = item
    }



    private func reconcileStatusItems() {
        withObservationTracking {
            let wanted = store.projection.statusRows
            for (key, item) in statusItems where !wanted.contains(key) {
                NSStatusBar.system.removeStatusItem(item)
                statusItems.removeValue(forKey: key)
            }
            for key in wanted where statusItems[key] == nil {
                makeStatusItem(for: key)
            }
            for (key, item) in statusItems {
                if let provider = store.provider(key) {
                    item.button?.attributedTitle = StatusItemRendering.title(
                        barLabel: store.statusPercent(provider) ?? "",
                        compactResetLabel: provider.compactResetLabel,
                        percentTint: store.statusTint(provider))
                }
            }
        } onChange: { [weak self] in
            Task { @MainActor in
                self?.reconcileStatusItems()
            }
        }
    }

    @objc private func statusItemClicked(_ sender: NSStatusBarButton) {
        guard
            let event = NSApp.currentEvent,
            let key = statusItems.first(where: { $0.value.button == sender })?.key,
            let provider = store.provider(key)
        else { return }
        if event.type == .rightMouseUp {
            showStatusMenu(for: sender)
        } else {
            togglePopover(for: provider, from: sender)
        }
    }



    private func showStatusMenu(for button: NSStatusBarButton) {
        let menu = NSMenu()
        menu.addItem(
            withTitle: "Open Usage Window", action: #selector(showUsage),
            keyEquivalent: "")
        menu.addItem(
            withTitle: store.chrome.refreshTitle, action: #selector(refreshNow),
            keyEquivalent: "")
        menu.addItem(.separator())
        menu.addItem(
            withTitle: "Quit jackin❯ desktop",
            action: #selector(NSApplication.terminate(_:)), keyEquivalent: "")
        for item in menu.items { item.target = self }
        menu.popUp(
            positioning: nil, at: NSPoint(x: 0, y: button.bounds.height + 4),
            in: button)
    }

    private func togglePopover(for provider: ProtoProvider, from button: NSView) {
        if let popover, popover.isShown {
            popover.performClose(nil)
            self.popover = nil
            return
        }




        NSApp.activate(ignoringOtherApps: true)
        let popover = NSPopover()
        popover.behavior = .transient
        popover.contentSize = PopoverView.contentSize
        popover.contentViewController = NSHostingController(
            rootView: wrap(
                PopoverView(
                    store: store, provider: provider,
                    onOpenUsage: { [weak self] in
                        guard let self else { return }

                        if let account = store.account(for: provider),
                            provider.accounts.count > 1
                        {
                            store.navigate(
                                to: .account(provider: provider.key, account: account.key))
                        } else {
                            store.navigate(to: .provider(provider.key))
                        }
                        popover.performClose(nil)
                        self.popover = nil
                        showUsage()
                    })))
        popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
        popover.contentViewController?.view.window?.makeKey()
        self.popover = popover
    }



    @objc func showUsage() {
        guard let window = usageWindow else { return }
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    @objc func refreshNow() {
        store.refresh()
    }

    @objc func showSettings() {
        if settingsWindow == nil {


            let window = NSWindow(
                contentRect: NSRect(x: 0, y: 0, width: 680, height: 520),
                styleMask: [.titled, .closable, .miniaturizable, .resizable],
                backing: .buffered,
                defer: false)
            window.title = "Settings"
            window.isReleasedWhenClosed = false
            window.toolbarStyle = .unified
            window.titlebarAppearsTransparent = false
            window.titleVisibility = .visible
            window.contentViewController = NSHostingController(
                rootView: wrap(SettingsView(store: store))
                    .frame(minWidth: 620, minHeight: 440))
            window.center()
            window.setFrameAutosaveName("jackin.desktop.settings-window")
            settingsWindow = window
        }
        settingsWindow?.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }
}
