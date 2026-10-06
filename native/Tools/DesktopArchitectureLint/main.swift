// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

import Darwin
import Foundation

@main
struct DesktopArchitectureLint {
    static func main() throws {
        let manager = FileManager.default
        let current = URL(fileURLWithPath: manager.currentDirectoryPath)
        let root =
            manager.fileExists(atPath: current.appendingPathComponent("Sources").path)
            ? current
            : current.appendingPathComponent("native")
        let sources = root.appendingPathComponent("Sources")
        let desktop = sources.appendingPathComponent("JackinDesktop")
        var failures: [String] = []

        func read(_ relativePath: String) -> String {
            let url = desktop.appendingPathComponent(relativePath)
            guard let text = try? String(contentsOf: url, encoding: .utf8) else {
                failures.append("missing \(relativePath)")
                return ""
            }
            return text
        }

        func require(_ condition: Bool, _ message: String) {
            if condition {
                print("PASS  \(message)")
            } else {
                failures.append(message)
                print("FAIL  \(message)")
            }
        }

        let popover = read("PopoverRoot.swift")
        let delegate = read("DesktopAppDelegate.swift")
        let usage = read("UsageWindow/UsageWindowRoot.swift")
        let splitController = read("UsageWindow/UsageWindowSplitController.swift")
        let overview = read("UsageWindow/OverviewListView.swift")
        let provider = read("UsageWindow/ProviderDetailView.swift")
        let usageController = read("UsageWindowController.swift")
        let mainMenu = read("AppMainMenu.swift")

        require(delegate.contains("NSPopover()"), "real NSPopover host")
        require(
            delegate.contains("compactStatusItems ? .applicationDefined : .transient"),
            "production popover remains transient with fixture-only automation persistence"
        )
        require(
            delegate.contains("NSHostingController(rootView: root)"), "ordinary native popover host"
        )
        require(
            delegate.contains("NSApp.setActivationPolicy(initialActivationPolicy)")
                && delegate.contains("visualQALaunchOptions.openUsage")
                && delegate.contains("visualQALaunchOptions.openPopover")
                && !delegate.contains(
                    "if visualQALaunchOptions.elevatesFixtureWindow {\n            return .accessory"
                )
                && delegate.contains("return .accessory"),
            "fixture windows launch regular while production menu-bar mode stays accessory"
        )
        require(
            !delegate.contains("popover.appearance =")
                && !delegate.contains("popover.contentViewController?.view.wantsLayer"),
            "system popover appearance preserved"
        )
        require(
            !delegate.contains("popover.backgroundColor")
                && !delegate.contains("popover.hasShadow"),
            "system popover material and shadow preserved"
        )

        require(popover.contains("Form {"), "popover uses native Form")
        require(
            popover.contains("Picker(") && popover.contains("\"Account\","),
            "popover uses native account Picker"
        )
        require(popover.contains("ProgressView(value:"), "popover uses native limit progress")
        require(!popover.contains("PopoverTabGrid"), "popover has no provider tab strip")
        require(
            popover.contains("HStack(spacing: 4) {")
                && popover.contains(
                    "Label(\"Refresh\", systemImage: \"arrow.clockwise\")\n"
                        + "                        .labelStyle(.iconOnly)"
                )
                && popover.contains(
                    "Label(\"Open Usage\", systemImage: \"macwindow\")\n"
                        + "                        .labelStyle(.iconOnly)"
                )
                && !popover.contains("\n            .labelStyle(.iconOnly)")
                && popover.contains("Spacer(minLength: 12)")
                && popover.contains(".labelsHidden()")
                && popover.contains(".help(\"Refresh\")")
                && popover.contains(".help(\"Open Usage\")")
                && popover.contains(".help(\"Choose account\")"),
            "popover footer uses adjacent semantic icon actions and trailing account Picker"
        )

        require(
            splitController.contains("NSSplitViewController")
                && splitController.contains("NSSplitViewItem(sidebarWithViewController:"),
            "Usage uses native AppKit split ownership"
        )
        require(usage.contains(".listStyle(.sidebar)"), "Usage uses native sidebar List")
        require(
            usage.contains("List(selection: destination)")
                && usage.contains("private var destination: Binding")
                && !usage.contains("@State private var destination"),
            "Usage sidebar has one store-owned selection authority"
        )
        require(
            usage.contains("struct UsageWindowDetailAccessory: View")
                && splitController.contains("NSSplitViewItemAccessoryViewController")
                && splitController.contains("accessory.view.setAccessibilityIdentifier")
                && splitController.contains("sidebarItem.allowsFullHeightLayout = true")
                && splitController.contains("[.toggleSidebar, .sidebarTrackingSeparator]")
                && usageController.contains("usage.brand-title")
                && !usage.contains("Text(\"jackin❯ desktop\")")
                && !usage.contains(".toolbar(removing: .sidebarToggle)")
                && !usage.contains("usage.sidebar-toggle")
                && !usage.contains("UsageWindowNavigationState"),
            "detail accessory owns header and native split owns sidebar visibility"
        )
        require(
            usageController.contains(".moveToActiveSpace")
                && usageController.contains(".canJoinAllSpaces")
                && usageController.contains(".canJoinAllApplications")
                && usageController.contains(".fullScreenAuxiliary")
                && usageController.contains("window.orderFrontRegardless()")
                && usageController.contains("if store.usesFixture")
                && mainMenu.contains("NSApp.activate()")
                && mainMenu.contains("window.makeKeyAndOrderFront(nil)"),
            "retained Usage window follows explicit reopen to active Space"
        )
        require(
            mainMenu.contains("static let settingsKeyEquivalent = \",\"")
                && mainMenu.contains(
                    "static let settingsKeyModifiers: NSEvent.ModifierFlags = [.command]"
                )
                && mainMenu.contains("static let closeKeyEquivalent = \"w\"")
                && mainMenu.contains(
                    "static let closeKeyModifiers: NSEvent.ModifierFlags = [.command]"
                )
                && mainMenu.contains("static let sidebarKeyEquivalent = \"s\"")
                && mainMenu.contains(
                    "static let sidebarKeyModifiers: NSEvent.ModifierFlags = [.command, .control]"
                )
                && mainMenu.contains("static let refreshKeyEquivalent = \"r\"")
                && mainMenu.contains(
                    "static let refreshKeyModifiers: NSEvent.ModifierFlags = [.command]"
                )
                && mainMenu.contains("key: Self.settingsKeyEquivalent")
                && mainMenu.contains("modifiers: Self.settingsKeyModifiers")
                && mainMenu.contains("key: Self.closeKeyEquivalent")
                && mainMenu.contains("modifiers: Self.closeKeyModifiers")
                && mainMenu.contains("#selector(NSSplitViewController.toggleSidebar(_:))")
                && mainMenu.contains("item.target = nil")
                && mainMenu.contains("keyEquivalent: sidebarKeyEquivalent")
                && mainMenu.contains("item.keyEquivalentModifierMask = sidebarKeyModifiers")
                && mainMenu.contains("menu.delegate = self")
                && mainMenu.contains("item.target = nil")
                && usageController.contains("AppMainMenu.isSidebarKeyEquivalent(event)")
                && usageController.contains("NSEvent.addLocalMonitorForEvents")
                && usageController.contains("split?.toggleSidebar(window)")
                && mainMenu.contains("#selector(refreshAll(_:))")
                && mainMenu.contains("key: Self.refreshKeyEquivalent")
                && mainMenu.contains("modifiers: Self.refreshKeyModifiers"),
            "standard commands route to native owners"
        )
        require(overview.contains("Table("), "Overview uses native Table")
        require(provider.contains("List {"), "provider detail uses native List")
        require(
            provider.contains("Picker(\"Account\""), "provider detail uses native account Picker")
        require(
            provider.contains("ProgressView(value:"), "provider detail uses native limit progress")

        let retired = [
            "GlassFallbacks.swift",
            "GlassPopoverHostingController.swift",
            "Popover/PopoverTabGrid.swift",
            "Popover/PopoverFooter.swift",
            "UsageWindow/UsageAccountNestView.swift",
        ]
        for path in retired {
            require(
                !manager.fileExists(atPath: desktop.appendingPathComponent(path).path),
                "retired custom surface removed: \(path)"
            )
        }

        let forbiddenChrome = [
            ".background(.bar)",
            ".background(.material)",
            ".ultraThinMaterial",
            ".thinMaterial",
            ".regularMaterial",
            ".thickMaterial",
            ".ultraThickMaterial",
            "glassEffect",
            "GlassEffectContainer",
            "NSVisualEffectView",
        ]
        let enumerator = manager.enumerator(at: sources, includingPropertiesForKeys: nil)
        while let url = enumerator?.nextObject() as? URL {
            guard url.pathExtension == "swift" else { continue }
            guard url.lastPathComponent != "jackin_usage_ffi.swift" else { continue }
            let text = try String(contentsOf: url, encoding: .utf8)
            for token in forbiddenChrome {
                require(
                    !text.contains(token),
                    "no app-painted chrome token \(token) in \(url.lastPathComponent)"
                )
            }
        }

        if !failures.isEmpty {
            fputs("DesktopArchitectureLint: \(failures.count) failure(s)\n", stderr)
            Darwin.exit(1)
        }
        print("DesktopArchitectureLint: native production structure OK")
    }
}
