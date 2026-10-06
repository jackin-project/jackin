import AppKit
import ApplicationServices
import Foundation

guard AXIsProcessTrusted() else {
    fputs(
        "Accessibility permission missing; grant this terminal in System Settings > Privacy & Security > Accessibility\n",
        stderr
    )
    exit(3)
}
let args = CommandLine.arguments
guard args.count == 4, ["find", "press", "read"].contains(args[2]) else {
    fputs("usage: ax-drive <pid|owner> find|press|read <AXIdentifier>\n", stderr)
    exit(2)
}
let pid: pid_t
if let value = Int32(args[1]) {
    pid = value
} else if let app = NSWorkspace.shared.runningApplications.first(where: {
    $0.localizedName == args[1]
}) {
    pid = app.processIdentifier
} else {
    fputs("application not found\n", stderr)
    exit(1)
}

func attribute(_ element: AXUIElement, _ name: CFString) -> CFTypeRef? {
    var value: CFTypeRef?
    return AXUIElementCopyAttributeValue(element, name, &value) == .success ? value : nil
}

func find(_ root: AXUIElement, id: String) -> AXUIElement? {
    var queue = [root]
    while !queue.isEmpty {
        let item = queue.removeFirst()
        if (attribute(item, kAXIdentifierAttribute as CFString) as? String) == id {
            return item
        }
        if let children = attribute(item, kAXChildrenAttribute as CFString) as? [AXUIElement] {
            queue.append(contentsOf: children)
        }
    }
    return nil
}

guard let element = find(AXUIElementCreateApplication(pid), id: args[3]) else {
    fputs("identifier not found\n", stderr)
    exit(1)
}
switch args[2] {
case "find": print(args[3])
case "press":
    guard AXUIElementPerformAction(element, kAXPressAction as CFString) == .success else {
        exit(1)
    }
case "read":
    if let value = attribute(element, kAXValueAttribute as CFString) {
        print(value)
    } else {
        exit(1)
    }
default: exit(2)
}
