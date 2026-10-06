// Drive a real NSStatusItem through the macOS Accessibility and event APIs.

import ApplicationServices
import Foundation

func attribute(_ name: String, from element: AXUIElement) -> CFTypeRef? {
    var value: CFTypeRef?
    guard AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success else {
        return nil
    }
    return value
}

func point(from value: CFTypeRef) -> CGPoint? {
    guard CFGetTypeID(value) == AXValueGetTypeID() else { return nil }
    var point = CGPoint.zero
    let axValue = unsafeBitCast(value, to: AXValue.self)
    guard AXValueGetValue(axValue, .cgPoint, &point) else { return nil }
    return point
}

func size(from value: CFTypeRef) -> CGSize? {
    guard CFGetTypeID(value) == AXValueGetTypeID() else { return nil }
    var size = CGSize.zero
    let axValue = unsafeBitCast(value, to: AXValue.self)
    guard AXValueGetValue(axValue, .cgSize, &size) else { return nil }
    return size
}

guard CommandLine.arguments.count == 4,
    let pid = pid_t(CommandLine.arguments[1]),
    let oneBasedIndex = Int(CommandLine.arguments[2]),
    oneBasedIndex > 0
else {
    FileHandle.standardError.write(Data("usage: status-item-drive PID INDEX left|right\n".utf8))
    exit(2)
}

let application = AXUIElementCreateApplication(pid)
guard
    let rawMenuBar = attribute(kAXExtrasMenuBarAttribute, from: application),
    CFGetTypeID(rawMenuBar) == AXUIElementGetTypeID()
else {
    FileHandle.standardError.write(Data("status menu bar not found\n".utf8))
    exit(1)
}
let menuBar = unsafeBitCast(rawMenuBar, to: AXUIElement.self)
guard
    let children = attribute(kAXChildrenAttribute, from: menuBar) as? [AXUIElement],
    children.indices.contains(oneBasedIndex - 1)
else {
    FileHandle.standardError.write(Data("status item not found\n".utf8))
    exit(1)
}

let item = children[oneBasedIndex - 1]
switch CommandLine.arguments[3] {
case "left":
    guard AXUIElementPerformAction(item, kAXPressAction as CFString) == .success else {
        FileHandle.standardError.write(Data("AXPress failed\n".utf8))
        exit(1)
    }
case "right":
    guard
        let rawPosition = attribute(kAXPositionAttribute, from: item),
        let rawSize = attribute(kAXSizeAttribute, from: item),
        let position = point(from: rawPosition),
        let itemSize = size(from: rawSize)
    else {
        FileHandle.standardError.write(Data("status item geometry unavailable\n".utf8))
        exit(1)
    }
    let location = CGPoint(
        x: position.x + itemSize.width / 2,
        y: position.y + itemSize.height / 2
    )
    guard
        let down = CGEvent(
            mouseEventSource: nil,
            mouseType: .rightMouseDown,
            mouseCursorPosition: location,
            mouseButton: .right
        ),
        let up = CGEvent(
            mouseEventSource: nil,
            mouseType: .rightMouseUp,
            mouseCursorPosition: location,
            mouseButton: .right
        )
    else {
        FileHandle.standardError.write(Data("right-click event creation failed\n".utf8))
        exit(1)
    }
    down.post(tap: .cghidEventTap)
    up.post(tap: .cghidEventTap)
default:
    FileHandle.standardError.write(Data("button must be left or right\n".utf8))
    exit(2)
}
