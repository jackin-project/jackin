// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "JackinDesktop",
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(
            name: "JackinDesktop",
            path: "Sources/JackinDesktop"
        )
    ]
)
