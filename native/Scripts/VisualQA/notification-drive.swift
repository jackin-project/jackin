// Post a fixture-only distributed notification to the running native app.

import Foundation

guard CommandLine.arguments.count == 2 else {
    FileHandle.standardError.write(
        Data("usage: notification-drive <notification-name>\n".utf8)
    )
    exit(2)
}

DistributedNotificationCenter.default().postNotificationName(
    Notification.Name(CommandLine.arguments[1]),
    object: nil,
    userInfo: nil,
    deliverImmediately: true
)
