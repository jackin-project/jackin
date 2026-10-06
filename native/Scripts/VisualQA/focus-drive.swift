import AppKit
import ApplicationServices
import Foundation

guard AXIsProcessTrusted() else {
    fputs("Accessibility permission missing\n", stderr)
    exit(3)
}

let arguments = CommandLine.arguments
guard arguments.count == 3, let tabCount = Int(arguments[2]), tabCount >= 0 else {
    fputs("usage: focus-drive <pid|owner> <tab-count>\n", stderr)
    exit(2)
}

let pid: pid_t
if let rawPID = Int32(arguments[1]) {
    pid = rawPID
} else if let application = NSWorkspace.shared.runningApplications.first(where: {
    $0.localizedName == arguments[1]
}) {
    pid = application.processIdentifier
} else {
    fputs("application not found\n", stderr)
    exit(1)
}

let application = AXUIElementCreateApplication(pid)

func attribute(_ element: AXUIElement, _ name: CFString) -> CFTypeRef? {
    var value: CFTypeRef?
    return AXUIElementCopyAttributeValue(element, name, &value) == .success ? value : nil
}

func stringAttribute(_ element: AXUIElement, _ name: CFString) -> String {
    (attribute(element, name) as? String) ?? ""
}

func firstIdentifiedDescendant(_ root: AXUIElement) -> AXUIElement? {
    var queue = (attribute(root, kAXChildrenAttribute as CFString) as? [AXUIElement]) ?? []
    while !queue.isEmpty {
        let element = queue.removeFirst()
        if !stringAttribute(element, kAXIdentifierAttribute as CFString).isEmpty {
            return element
        }
        let children =
            (attribute(element, kAXChildrenAttribute as CFString) as? [AXUIElement]) ?? []
        queue.append(contentsOf: children)
    }
    return nil
}

func focusedDescription(step: Int) -> String {
    guard let focused = attribute(application, kAXFocusedUIElementAttribute as CFString) else {
        return "\(step)|none"
    }
    guard CFGetTypeID(focused) == AXUIElementGetTypeID() else {
        return "\(step)|unknown"
    }
    let element: AXUIElement = unsafeBitCast(focused, to: AXUIElement.self)
    var fields = [
        stringAttribute(element, kAXRoleAttribute as CFString),
        stringAttribute(element, kAXIdentifierAttribute as CFString),
        stringAttribute(element, kAXTitleAttribute as CFString),
        stringAttribute(element, kAXDescriptionAttribute as CFString),
        stringAttribute(element, kAXValueAttribute as CFString),
    ]
    if let descendant = firstIdentifiedDescendant(element) {
        fields.append(stringAttribute(descendant, kAXRoleAttribute as CFString))
        fields.append(stringAttribute(descendant, kAXIdentifierAttribute as CFString))
    }
    return ([String(step)] + fields).joined(separator: "|")
}

func pressTab() {
    let source = CGEventSource(stateID: .hidSystemState)
    let down = CGEvent(keyboardEventSource: source, virtualKey: 0x30, keyDown: true)
    let up = CGEvent(keyboardEventSource: source, virtualKey: 0x30, keyDown: false)
    down?.post(tap: .cghidEventTap)
    up?.post(tap: .cghidEventTap)
    Thread.sleep(forTimeInterval: 0.25)
}

NSRunningApplication(processIdentifier: pid)?.activate(options: [.activateAllWindows])
Thread.sleep(forTimeInterval: 0.5)
print(focusedDescription(step: 0))
if tabCount > 0 {
    for step in 1...tabCount {
        pressTab()
        print(focusedDescription(step: step))
    }
}
