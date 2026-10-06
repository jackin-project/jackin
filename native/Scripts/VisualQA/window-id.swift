// Resolve the CGWindowID of a running application's window.

import AppKit
import CoreGraphics
import Foundation

struct Window {
    let id: CGWindowID
    let pid: pid_t
    let owner: String
    let name: String
    let bounds: CGRect
    let onScreen: Bool
    let layer: Int
}

private let activeDisplayBounds: [CGRect] = {
    var count: UInt32 = 0
    guard CGGetActiveDisplayList(0, nil, &count) == .success, count > 0 else { return [] }
    var displays = [CGDirectDisplayID](repeating: 0, count: Int(count))
    guard CGGetActiveDisplayList(count, &displays, &count) == .success else { return [] }
    return displays.prefix(Int(count)).map(CGDisplayBounds)
}()

private func isFullyContainedOnScreen(_ bounds: CGRect) -> Bool {
    activeDisplayBounds.contains { $0.contains(bounds) }
}

func windows(ownedBy owner: String, pid: pid_t?) -> ([Window], Bool) {
    let options: CGWindowListOption = [.optionAll]
    guard let raw = CGWindowListCopyWindowInfo(options, kCGNullWindowID) as? [[String: Any]]
    else {
        return ([], false)
    }

    let titlesAvailable = raw.contains { entry in
        (entry[kCGWindowOwnerName as String] as? String) == owner
            && pid.map { entry[kCGWindowOwnerPID as String] as? Int == Int($0) } ?? true
            && entry[kCGWindowName as String] != nil
    }
    let result: [Window] = raw.compactMap { entry -> Window? in
        guard
            let id = entry[kCGWindowNumber as String] as? CGWindowID,
            let ownerPID = entry[kCGWindowOwnerPID as String] as? Int,
            let ownerName = entry[kCGWindowOwnerName as String] as? String,
            ownerName == owner,
            pid.map({ ownerPID == Int($0) }) ?? true
        else { return nil }

        let name = entry[kCGWindowName as String] as? String ?? ""
        let layer = entry[kCGWindowLayer as String] as? Int ?? 0
        let onScreen = (entry[kCGWindowIsOnscreen as String] as? Bool) ?? false
        var bounds = CGRect.zero
        if let dictionary = entry[kCGWindowBounds as String] as? NSDictionary {
            bounds = CGRect(dictionaryRepresentation: dictionary) ?? .zero
        }
        return Window(
            id: id,
            pid: pid_t(ownerPID),
            owner: ownerName,
            name: name,
            bounds: bounds,
            onScreen: onScreen,
            layer: layer
        )
    }
    return (result, titlesAvailable)
}

let arguments = CommandLine.arguments
guard arguments.count >= 2 else {
    FileHandle.standardError.write(
        Data("usage: window-id <ownerName> [windowName|--list] [--json] [--pid PID]\n".utf8)
    )
    exit(2)
}

let owner = arguments[1]
let jsonMode = arguments.contains("--json")
var requestedPID: pid_t?
var filter: String?
var index = 2
while index < arguments.count {
    switch arguments[index] {
    case "--json":
        break
    case "--pid":
        guard arguments.indices.contains(index + 1), let value = pid_t(arguments[index + 1]) else {
            FileHandle.standardError.write(Data("--pid requires a process ID\n".utf8))
            exit(2)
        }
        requestedPID = value
        index += 1
    default:
        guard filter == nil else {
            FileHandle.standardError.write(Data("unexpected argument: \(arguments[index])\n".utf8))
            exit(2)
        }
        filter = arguments[index]
    }
    index += 1
}
let layerMode = ProcessInfo.processInfo.environment["WINDOW_LAYER_MODE"]
let includesPanels = layerMode == "all" || layerMode == "transient"
let transientOnly = layerMode == "transient"
let (owned, titlesAvailable) = windows(ownedBy: owner, pid: requestedPID)
let candidates = owned.filter {
    (transientOnly ? $0.layer != 0 : (includesPanels || $0.layer == 0))
        && $0.bounds.width >= 64 && $0.bounds.height >= 64
}
.sorted {
    if $0.onScreen != $1.onScreen { return $0.onScreen && !$1.onScreen }
    let leftArea = $0.bounds.width * $0.bounds.height
    let rightArea = $1.bounds.width * $1.bounds.height
    if leftArea != rightArea { return leftArea > rightArea }
    return $0.id < $1.id
}

if filter == "--list" {
    for window in candidates {
        let flag = window.onScreen ? "onscreen" : "offscreen"
        let containment = isFullyContainedOnScreen(window.bounds) ? "contained" : "clipped"
        let origin = "@\(Int(window.bounds.minX)),\(Int(window.bounds.minY))"
        let size = "\(Int(window.bounds.width))x\(Int(window.bounds.height))"
        print(
            "id=\(window.id) name=\"\(window.name)\" layer=\(window.layer) "
                + "\(size)\(origin) \(flag) \(containment)"
        )
    }
    exit(candidates.isEmpty ? 1 : 0)
}

if !owned.isEmpty && !titlesAvailable {
    FileHandle.standardError.write(
        Data("window titles unavailable — Screen Recording permission likely missing\n".utf8)
    )
    exit(3)
}

let matches = filter.map { wanted in candidates.filter { $0.name == wanted } } ?? candidates
guard let window = matches.first else {
    FileHandle.standardError.write(Data("no window found for owner \(owner)\n".utf8))
    exit(1)
}

if matches.count > 1 {
    let alternatives = matches.map { "\($0.id):\($0.name)" }.joined(separator: ", ")
    FileHandle.standardError.write(
        Data("multiple windows matched [\(alternatives)]; pass a title to disambiguate\n".utf8)
    )
}

if jsonMode {
    let active = NSWorkspace.shared.frontmostApplication?.processIdentifier == window.pid
    let activationState = active ? "active" : "inactive"
    let keyState: String
    if filter != nil {
        keyState = active ? "key" : "non-key"
    } else {
        keyState = "not-applicable-transient"
    }
    let object: [String: Any] = [
        "windowID": Int(window.id),
        "ownerPID": Int(window.pid),
        "owner": window.owner,
        "windowTitle": window.name,
        "windowLayer": window.layer,
        "frameOrigin": ["x": window.bounds.minX, "y": window.bounds.minY],
        "frameSize": ["width": window.bounds.width, "height": window.bounds.height],
        "contentSize": NSNull(),
        "contentSizeNote": "CGWindow exposes frame bounds; content size requires app geometry",
        "onScreen": window.onScreen,
        "fullyContainedOnScreen": isFullyContainedOnScreen(window.bounds),
        "applicationActivationState": activationState,
        "keyStatus": keyState,
    ]
    let data = try JSONSerialization.data(
        withJSONObject: object,
        options: [.prettyPrinted, .sortedKeys]
    )
    FileHandle.standardOutput.write(data)
    FileHandle.standardOutput.write(Data("\n".utf8))
} else {
    print(window.id)
}
