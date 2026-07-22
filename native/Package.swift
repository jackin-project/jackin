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
        .executable(name: "DesktopArchitectureLint", targets: ["DesktopArchitectureLint"]),
        .executable(name: "DesktopParityMatrixHarness", targets: ["DesktopParityMatrixHarness"]),
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
        // Mirrors ArchitectureTests usage-string token ban without XCTest (CLT-safe).
        .executableTarget(
            name: "DesktopArchitectureLint",
            dependencies: [],
            path: "Tools/DesktopArchitectureLint"
        ),
        // OpenUsage/CodexBar limits-only display matrix (full catalog, dual-bucket).
        .executableTarget(
            name: "DesktopParityMatrixHarness",
            dependencies: ["JackinUsageBridge"],
            path: "Tools/DesktopParityMatrixHarness"
        ),
        .testTarget(
            name: "JackinUsageBridgeTests",
            dependencies: ["JackinUsageBridge"],
            path: "Tests/JackinUsageBridgeTests"
        ),
    ]
)
