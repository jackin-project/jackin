// The launch-contract harness. Copy into Sources/<Feature>Proto/ and keep
// the contract intact: --tr-scenario, --tr-appearance, --tr-window,
// --tr-reduce, --tr-backdrop. Feature code supplies the scenarios and the
// root view; nothing here is feature-specific.

import AppKit
import SwiftUI

enum ProtoUsageWindowMetrics {
    static let defaultContentSize = CGSize(width: 1000, height: 680)
    static let minimumContentSize = CGSize(width: 800, height: 520)
}

struct ProtoConfig {
    var scenario = "default"
    var window: CGSize?
    var reduceTransparency = false
    var reduceMotion = false
    var increaseContrast = false
    var backdrop: NSColor?

    static func parse(_ args: [String]) -> ProtoConfig {
        do { return try validated(args) } catch { fatalError(String(describing: error)) }
    }

    static func validated(_ args: [String]) throws -> ProtoConfig {
        var config = ProtoConfig()
        func value(after flag: String) -> String? {
            guard let index = args.firstIndex(of: flag), args.indices.contains(index + 1)
            else { return nil }
            return args[index + 1]
        }
        if let name = value(after: "--tr-scenario") { config.scenario = name }
        if let appearance = value(after: "--tr-appearance"), appearance != "dark" {
            throw ProtoConfigError.unsupportedAppearance(appearance)
        }
        if let size = value(after: "--tr-window") {
            let parts = size.split(separator: "x").compactMap { Double($0) }
            guard parts.count == 2 else { throw ProtoConfigError.malformedWindow(size) }
            guard parts[0] >= ProtoUsageWindowMetrics.minimumContentSize.width,
                parts[1] >= ProtoUsageWindowMetrics.minimumContentSize.height
            else {
                throw ProtoConfigError.undersizedWindow(size)
            }
            config.window = CGSize(width: parts[0], height: parts[1])
        }
        if let list = value(after: "--tr-reduce") {
            for item in list.split(separator: ",") {
                switch item {
                case "transparency": config.reduceTransparency = true
                case "motion": config.reduceMotion = true
                default: throw ProtoConfigError.unknownReduction(String(item))
                }
            }
        }
        if args.contains("--tr-increase-contrast") { config.increaseContrast = true }
        switch value(after: "--tr-backdrop") {
        case "standard":
            config.backdrop = NSColor(srgbRed: 0.42, green: 0.45, blue: 0.50, alpha: 1)
        case .some(let hex):
            guard let color = NSColor(hex: hex) else { throw ProtoConfigError.badBackdrop(hex) }
            config.backdrop = color
        case nil: break
        }
        return config
    }
}

enum ProtoConfigError: Error, Equatable {
    case unsupportedAppearance(String)
    case malformedWindow(String)
    case undersizedWindow(String)
    case unknownReduction(String)
    case badBackdrop(String)
}

@MainActor
final class ProtoDelegate: NSObject, NSApplicationDelegate {
    let config: ProtoConfig
    /// Feature hook: installs scenario UI before the readiness tick loop.
    var onLaunch: ((NSApplication) -> Void)?
    private var backdrop: NSWindow?
    private var lastFrame: CGRect = .zero
    private var stableTicks = 0
    private var announced = false

    init(config: ProtoConfig) { self.config = config }

    func applicationDidFinishLaunching(_ notification: Notification) {
        onLaunch?(NSApp)
        // Defaults leak across runs (table column autosave, restoration);
        // a deterministic prototype starts clean every launch.
        if let bundleID = Bundle.main.bundleIdentifier {
            UserDefaults.standard.removePersistentDomain(forName: bundleID)
        }
        NSApp.appearance = NSAppearance(named: .darkAqua)
        Task { @MainActor in
            while !self.announced {
                try? await Task.sleep(for: .milliseconds(400))
                self.tick()
            }
        }
    }

    private func tick() {
        // Match only normal-level windows: an unfiltered lookup grabs the
        // backdrop, clamps it, and announces the wrong window number.
        guard
            let window = NSApp.windows.first(where: { $0.isVisible && $0.level == .normal })
        else { return }
        // The backdrop is created only after the main window exists — a
        // delegate-created window before SwiftUI scene setup suppresses
        // WindowGroup window creation entirely. Ordering holds: it exists
        // before TR-READY is announced.
        if backdrop == nil, let color = config.backdrop {
            let screen = NSScreen.main!.frame
            let back = NSWindow(
                contentRect: screen, styleMask: .borderless, backing: .buffered, defer: false)
            back.backgroundColor = color
            back.level = NSWindow.Level(rawValue: NSWindow.Level.normal.rawValue - 1)
            back.collectionBehavior = [.canJoinAllSpaces, .stationary]
            back.orderFrontRegardless()
            backdrop = back
        }
        if let size = config.window, window.contentLayoutRect.size != size {
            window.setContentSize(size)
            window.isRestorable = false
        }
        NSApp.activate(ignoringOtherApps: true)
        // Announce readiness only after geometry holds still twice — the
        // capture harness waits for this line.
        if window.frame == lastFrame {
            stableTicks += 1
            if stableTicks == 2 {
                print("TR-READY \(window.windowNumber)")
                fflush(stdout)
                announced = true
            }
        } else {
            stableTicks = 0
            lastFrame = window.frame
        }
    }
}

extension NSColor {
    convenience init?(hex: String) {
        var value: UInt64 = 0
        guard
            Scanner(string: hex.replacingOccurrences(of: "#", with: ""))
                .scanHexInt64(&value)
        else { return nil }
        self.init(
            srgbRed: CGFloat((value >> 16) & 0xFF) / 255,
            green: CGFloat((value >> 8) & 0xFF) / 255,
            blue: CGFloat(value & 0xFF) / 255,
            alpha: 1)
    }
}
