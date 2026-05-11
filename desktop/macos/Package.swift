// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "JackinDesktop",
    platforms: [
        .macOS(.v14),
    ],
    products: [
        .executable(name: "JackinDesktop", targets: ["JackinDesktop"]),
    ],
    targets: [
        .executableTarget(
            name: "JackinDesktop",
            path: "Sources/JackinDesktop"
        ),
    ]
)
