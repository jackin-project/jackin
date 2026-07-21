// swift-tools-version: 6.0
import PackageDescription

// Static XCFramework produced by scripts/build-usage-xcframework.sh.
// Binary target name must match UniFFI's jackin_usage_ffiFFI module.
let package = Package(
    name: "JackinUsageMenuBar",
    platforms: [
        .macOS(.v14),
    ],
    products: [
        .library(name: "JackinUsageBridge", targets: ["JackinUsageBridge"]),
        .executable(name: "JackinUsageMenuBar", targets: ["JackinUsageMenuBar"]),
    ],
    targets: [
        .binaryTarget(
            name: "jackin_usage_ffiFFI",
            path: "../target/xcframework/JackinUsageFFI.xcframework"
        ),
        .target(
            name: "JackinUsageBridge",
            dependencies: ["jackin_usage_ffiFFI"],
            path: "Sources/JackinUsageBridge"
        ),
        .executableTarget(
            name: "JackinUsageMenuBar",
            dependencies: ["JackinUsageBridge"],
            path: "Sources/JackinUsageMenuBar"
        ),
        .testTarget(
            name: "JackinUsageBridgeTests",
            dependencies: ["JackinUsageBridge"],
            path: "Tests/JackinUsageBridgeTests"
        ),
    ]
)
