// swift-tools-version: 6.2
import PackageDescription

// Static XCFramework produced by `cargo xtask desktop xcframework` (boltffi pack apple).
// Binary target name must match the boltffi FFI module JackinUsageFFI.
let package = Package(
    name: "JackinDesktop",
    platforms: [
        .macOS(.v26),
    ],
    products: [
        .library(name: "JackinUsageBridge", targets: ["JackinUsageBridge"]),
        .library(name: "JackinDesktopUI", targets: ["JackinDesktopUI"]),
        .executable(name: "StatusItemChipHarness", targets: ["StatusItemChipHarness"]),
        .executable(name: "DesktopArchitectureLint", targets: ["DesktopArchitectureLint"]),
        .executable(name: "DesktopParityMatrixHarness", targets: ["DesktopParityMatrixHarness"]),
        .executable(name: "DesktopSoTParityHarness", targets: ["DesktopSoTParityHarness"]),
        .executable(name: "ProviderMarksHarness", targets: ["ProviderMarksHarness"]),
    ],
    targets: [
        .binaryTarget(
            name: "JackinUsageFFI",
            path: "../target/xcframework/JackinUsage.xcframework"
        ),
        // Generated boltffi Swift only. Nothing handwritten lands here; only
        // JackinUsageBridge may depend on this target.
        .target(
            name: "JackinUsageBindings",
            dependencies: ["JackinUsageFFI"],
            path: "Sources/JackinUsageBindings"
        ),
        .target(
            name: "JackinUsageBridge",
            dependencies: ["JackinUsageBindings"],
            path: "Sources/JackinUsageBridge"
        ),
        // Hostable UI library (status/popover/Usage) for app + deterministic fixtures.
        .target(
            name: "JackinDesktopUI",
            dependencies: ["JackinUsageBindings", "JackinUsageBridge"],
            path: "Sources/JackinDesktop",
            resources: [
                .copy("Resources/Brand"),
                // Official provider logomarks (template PDF) — see ProviderMarks/PROVENANCE.md
                .copy("Resources/ProviderMarks"),
            ]
        ),
        .executableTarget(
            name: "StatusItemChipHarness",
            dependencies: ["JackinUsageBindings", "JackinUsageBridge"],
            path: "Tools/StatusItemChipHarness"
        ),
        .executableTarget(
            name: "DesktopArchitectureLint",
            dependencies: [],
            path: "Tools/DesktopArchitectureLint"
        ),
        .executableTarget(
            name: "DesktopParityMatrixHarness",
            dependencies: ["JackinUsageBindings", "JackinUsageBridge"],
            path: "Tools/DesktopParityMatrixHarness"
        ),
        .executableTarget(
            name: "DesktopSoTParityHarness",
            dependencies: ["JackinUsageBridge"],
            path: "Tools/DesktopSoTParityHarness"
        ),
        .executableTarget(
            name: "ProviderMarksHarness",
            dependencies: ["JackinDesktopUI", "JackinUsageBridge"],
            path: "Tools/ProviderMarksHarness"
        ),
        .testTarget(
            name: "JackinUsageBridgeTests",
            dependencies: ["JackinDesktopUI", "JackinUsageBindings", "JackinUsageBridge"],
            path: "Tests/JackinUsageBridgeTests"
        ),
    ]
)
