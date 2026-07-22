// swift-tools-version: 6.0
import PackageDescription

// Static XCFramework produced by `cargo xtask desktop xcframework`.
// Binary target name must match UniFFI's jackin_usage_ffiFFI module.
let package = Package(
    name: "JackinDesktop",
    platforms: [
        .macOS(.v14),
    ],
    products: [
        .library(name: "JackinUsageBridge", targets: ["JackinUsageBridge"]),
        .executable(name: "JackinDesktop", targets: ["JackinDesktop"]),
        .executable(name: "StatusItemChipHarness", targets: ["StatusItemChipHarness"]),
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
            name: "JackinDesktop",
            dependencies: ["JackinUsageBridge"],
            path: "Sources/JackinDesktop",
            resources: [.copy("Resources/JackinMark.pdf")]
        ),
        // Pure chip builder checks without XCTest (CodexBar/OpenUsage remaining% parity).
        .executableTarget(
            name: "StatusItemChipHarness",
            dependencies: ["JackinUsageBridge"],
            path: "Tools/StatusItemChipHarness"
        ),
        .testTarget(
            name: "JackinUsageBridgeTests",
            dependencies: ["JackinUsageBridge"],
            path: "Tests/JackinUsageBridgeTests"
        ),
    ]
)
