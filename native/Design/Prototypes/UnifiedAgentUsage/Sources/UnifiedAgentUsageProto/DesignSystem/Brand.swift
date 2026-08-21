import AppKit
import SwiftUI

private struct JackinIncreaseContrastKey: EnvironmentKey {
    static let defaultValue = false
}

private struct JackinReduceTransparencyKey: EnvironmentKey {
    static let defaultValue = false
}

private struct JackinReduceMotionKey: EnvironmentKey {
    static let defaultValue = false
}

extension EnvironmentValues {
    var jackinIncreaseContrast: Bool {
        get { self[JackinIncreaseContrastKey.self] }
        set { self[JackinIncreaseContrastKey.self] = newValue }
    }

    var jackinReduceTransparency: Bool {
        get { self[JackinReduceTransparencyKey.self] }
        set { self[JackinReduceTransparencyKey.self] = newValue }
    }

    var jackinReduceMotion: Bool {
        get { self[JackinReduceMotionKey.self] }
        set { self[JackinReduceMotionKey.self] = newValue }
    }
}

// Brand tokens, identity assets, and provider marks — lifted verbatim from
// the incumbent implementation (Sources/JackinDesktop,
// Sources/JackinUsageBridge/BrandColors.swift) so the prototype renders the
// same identity. Assets are bundled under Resources/.

/// jackin❯ design tokens — phosphor accent system.
///
/// Dark `#5CF07A` · light `#0B774E` (AA-friendly). Never system
/// `Color.accentColor` for healthy metrics, brand mark, or selection wells.
enum JackinBrand {
    static let phosphorDarkSRGB = (r: 0x5C / 255.0, g: 0xF0 / 255.0, b: 0x7A / 255.0)
    static let phosphorLightSRGB = (r: 0x0B / 255.0, g: 0x77 / 255.0, b: 0x4E / 255.0)

    static var phosphor: Color { Color(nsColor: phosphorNSColor) }
    static var phosphorWash: Color { Color(nsColor: phosphorWashNSColor) }
    static var selectionWell: Color { Color(nsColor: selectionWellNSColor) }
    static var selectionText: Color { Color(nsColor: selectionTextNSColor) }
    static var warning: Color { Color(nsColor: warningNSColor) }
    static var danger: Color { Color(nsColor: dangerNSColor) }
    static var stage: Color { Color(nsColor: stageNSColor) }
    static var card: Color { Color(nsColor: cardNSColor) }
    static var inset: Color { Color(nsColor: insetNSColor) }
    static var hover: Color { Color(nsColor: hoverNSColor) }
    static var separator: Color { Color(nsColor: separatorNSColor) }
    static var strongSeparator: Color { Color(nsColor: strongSeparatorNSColor) }
    static var meterTrack: Color { Color(nsColor: meterTrackNSColor) }
    static var muted: Color { Color(nsColor: mutedNSColor) }
    static var quiet: Color { Color(nsColor: quietNSColor) }
    static var focusRing: Color { phosphor }

    static let phosphorNSColor = NSColor(
        name: "jackinPhosphor",
        dynamicProvider: { appearance in
            let dark = appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            if dark {
                return NSColor(
                    srgbRed: phosphorDarkSRGB.r, green: phosphorDarkSRGB.g,
                    blue: phosphorDarkSRGB.b, alpha: 1)
            }
            return NSColor(
                srgbRed: phosphorLightSRGB.r, green: phosphorLightSRGB.g,
                blue: phosphorLightSRGB.b, alpha: 1)
        })

    /// Adaptive color table.
    ///
    /// Explicit semantic endpoints keep small status
    /// text above WCAG AA against the native content grounds in both appearances.
    /// Light grounds: stage #F3F4F1, card #FCFCFA. Dark grounds: stage
    /// #101618, card #162022. Semantic and supporting colors adapt here,
    /// never in a view.
    static let phosphorWashNSColor = dynamicColor(
        name: "jackinPhosphorWash",
        light: rgb(0xE3F3E7),
        dark: rgb(0x16372A))
    static let selectionWellNSColor = dynamicColor(
        name: "jackinSelectionWell",
        light: rgb(0x0B774E, alpha: 0.20),
        dark: rgb(0x5CF07A, alpha: 0.20))
    static let selectionTextNSColor = dynamicColor(
        name: "jackinSelectionText",
        light: rgb(0xFFFFFF),
        dark: rgb(0xE9F7ED))
    static let stageNSColor = dynamicColor(
        name: "jackinStage",
        light: rgb(0xEDF2EC),
        dark: rgb(0x101618))
    static let cardNSColor = dynamicColor(
        name: "jackinCard",
        light: rgb(0xFCFCFA),
        dark: rgb(0x162022))
    static let insetNSColor = dynamicColor(
        name: "jackinInset",
        light: rgb(0xE2E9E1),
        dark: rgb(0x1C2728))
    static let hoverNSColor = dynamicColor(
        name: "jackinHover",
        light: rgb(0x0B774E, alpha: 0.10),
        dark: rgb(0x5CF07A, alpha: 0.09))
    static let separatorNSColor = dynamicColor(
        name: "jackinSeparator",
        light: rgb(0xD4D7D2),
        dark: rgb(0x343D3F))
    static let strongSeparatorNSColor = dynamicColor(
        name: "jackinStrongSeparator",
        light: rgb(0xBEC3BC),
        dark: rgb(0x465254))
    static let meterTrackNSColor = dynamicColor(
        name: "jackinMeterTrack",
        light: rgb(0xE2E5E0),
        dark: rgb(0x293335))
    static let mutedNSColor = dynamicColor(
        name: "jackinMuted",
        light: rgb(0x59615D),
        dark: rgb(0xADB5B2))
    static let quietNSColor = dynamicColor(
        name: "jackinQuiet",
        light: rgb(0x58615C),
        dark: rgb(0xAAB2AF))
    static let warningNSColor = dynamicColor(
        name: "jackinWarning",
        light: rgb(0x7A4B00),
        dark: rgb(0xFFC15A))
    static let dangerNSColor = dynamicColor(
        name: "jackinDanger",
        light: rgb(0xB42318),
        dark: rgb(0xFF7B72))

    private static func rgb(
        _ hex: UInt32, alpha: CGFloat = 1
    ) -> (
        CGFloat, CGFloat, CGFloat, CGFloat
    ) {
        (
            CGFloat((hex >> 16) & 0xFF) / 255,
            CGFloat((hex >> 8) & 0xFF) / 255,
            CGFloat(hex & 0xFF) / 255,
            alpha
        )
    }

    private static func dynamicColor(
        name: String,
        light: (CGFloat, CGFloat, CGFloat, CGFloat),
        dark: (CGFloat, CGFloat, CGFloat, CGFloat)
    ) -> NSColor {
        NSColor(
            name: NSColor.Name(name),
            dynamicProvider: { appearance in
                let value =
                    appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
                    ? dark : light
                return NSColor(
                    srgbRed: value.0, green: value.1, blue: value.2, alpha: value.3)
            })
    }
}

/// Four-point spatial scale.
///
/// Native controls retain system-owned internal metrics.
enum JackinSpace {
    static let xxs: CGFloat = 4
    static let xs: CGFloat = 8
    static let sm: CGFloat = 12
    static let md: CGFloat = 16
    static let lg: CGFloat = 20
    static let xl: CGFloat = 24
}

/// Compact type ramp for authored content; system controls keep native fonts.
enum JackinType {
    static let heroMetric = Font.system(size: 28, weight: .semibold, design: .monospaced)
    static let detailMetric = Font.system(size: 20, weight: .semibold, design: .monospaced)
    static let technicalLabel = Font.system(size: 10, weight: .semibold, design: .monospaced)
    static let sectionTitle = Font.system(size: 11, weight: .semibold, design: .monospaced)
    static let metadata = Font.caption
    static let tertiary = Font.caption2
}

extension Color {
    /// Product phosphor accent — prefer over `Color.accentColor` for jackin chrome.
    static var jackinPhosphor: Color { JackinBrand.phosphor }
    static var jackinMuted: Color { JackinBrand.muted }
    static var jackinQuiet: Color { JackinBrand.quiet }
}

/// Meter tint from row state: danger red, warning orange, otherwise phosphor.
func meterTint(_ state: ProtoState) -> Color {
    switch state {
    case .danger, .depleted: JackinBrand.danger
    case .warning: JackinBrand.warning
    default: .jackinPhosphor
    }
}

/// Neutral instrument tile behind a monochrome provider mark. jackin❯ owns the
/// surrounding signal color; provider marks never sit in repeated green wells.
struct BrandMarkChip: View {
    let iconKey: String
    var fallbackGlyph: String = ""
    var markSize: CGFloat = 18
    var chipSize: CGFloat = 30

    var body: some View {
        Group {
            if let mark = ProviderMarks.swiftUIImage(forIconKey: iconKey) {
                mark
                    .resizable()
                    .scaledToFit()
                    .foregroundStyle(.primary)
            } else if !fallbackGlyph.isEmpty {
                Text(fallbackGlyph)
                    .font(.system(size: markSize * 0.6, weight: .semibold))
                    .foregroundStyle(.primary)
            }
        }
        .frame(width: markSize, height: markSize)
        .frame(width: chipSize, height: chipSize)
        .background(
            RoundedRectangle(cornerRadius: chipSize * 0.28, style: .continuous)
                .fill(JackinBrand.inset)
                .overlay(
                    RoundedRectangle(cornerRadius: chipSize * 0.28, style: .continuous)
                        .strokeBorder(JackinBrand.separator, lineWidth: 1)
                )
        )
        .accessibilityHidden(true)
    }
}

/// Canonical generated jackin❯ identity assets for native surfaces.
@MainActor
enum JackinBrandIdentity {
    static func wordmark(for colorScheme: ColorScheme) -> NSImage? {
        loadSVG(named: colorScheme == .dark ? "JackinWordmarkDark" : "JackinWordmarkLight")
    }

    static func templateMonogram() -> NSImage? {
        guard let image = loadSVG(named: "JackinMonogramDark") else { return nil }
        image.isTemplate = true
        return image
    }

    private static func loadSVG(named name: String) -> NSImage? {
        let candidates = [
            Bundle.module.url(forResource: name, withExtension: "svg", subdirectory: "Brand"),
            Bundle.module.url(forResource: name, withExtension: "svg"),
        ]
        for case let url? in candidates {
            if let image = NSImage(contentsOf: url) {
                image.isTemplate = false
                return image
            }
        }
        return nil
    }
}

/// Quiet product signature inside the sidebar's system-owned structural plane.
struct JackinBrandSignature: View {
    @Environment(\.colorScheme) private var colorScheme
    var width: CGFloat = 76
    var height: CGFloat = 20

    var body: some View {
        Group {
            if let wordmark = JackinBrandIdentity.wordmark(for: colorScheme) {
                Image(nsImage: wordmark)
                    .resizable()
                    .scaledToFit()
            }
        }
        .frame(width: width, height: height, alignment: .leading)
        .accessibilityHidden(true)
    }
}

/// The native counterpart of the shared CLI/web rain engine. Columns move at
/// independent rates and deposit mutating ASCII glyphs through the same
/// white-to-phosphor age ramp. Opaque cards preserve reading contrast.
struct JackinStageBackground: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.jackinReduceMotion) private var processReduceMotion
    @Environment(\.jackinReduceTransparency) private var processReduceTransparency

    private let glyphs = Array(
        "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz@#$%&*<>{}[]|/\\~")

    var body: some View {
        TimelineView(
            .animation(minimumInterval: 0.05, paused: reduceMotion || processReduceMotion)
        ) { context in
            let tick =
                reduceMotion || processReduceMotion
                ? 90 : Int(context.date.timeIntervalSinceReferenceDate / 0.05)
            Canvas { canvas, size in
                let columnSpacing: CGFloat = 16
                let rowSpacing: CGFloat = 18
                let columns = max(1, Int(size.width / columnSpacing))
                let rows = max(1, Int(size.height / rowSpacing))

                for column in 0..<columns {
                    let speed = 1 + Int(hash(column, 3) % 4)
                    let fade = 1 + Int(hash(column, 11) % 3)
                    let cycle = rows + 16 + Int(hash(column, 17) % 18)
                    let offset = Int(hash(column, 23) % UInt64(cycle))
                    let head = ((tick / speed) + offset) % cycle - 8

                    for distance in 0...24 {
                        let row = head - distance
                        guard row >= 0, row < rows else { continue }
                        let age = distance * fade
                        guard let tone = rainTone(age: age) else { continue }

                        let mutation = tick / max(1, speed * (age < 3 ? 2 : 7))
                        let glyphIndex = Int(hash(column, row, mutation) % UInt64(glyphs.count))
                        let text = Text(String(glyphs[glyphIndex]))
                            .font(
                                .system(
                                    size: 13, weight: age == 0 ? .semibold : .regular,
                                    design: .monospaced)
                            )
                            .foregroundStyle(tone.color)
                        var resolved = canvas.resolve(text)
                        resolved.shading = .color(tone.color)
                        canvas.opacity = tone.opacity
                        canvas.draw(
                            resolved,
                            at: CGPoint(
                                x: CGFloat(column) * columnSpacing + columnSpacing / 2,
                                y: CGFloat(row) * rowSpacing + rowSpacing / 2),
                            anchor: .center)
                    }
                }
            }
            .opacity(reduceTransparency || processReduceTransparency ? 0 : 1)
            .mask {
                LinearGradient(
                    colors: [.clear, .white.opacity(0.8), .white, .clear],
                    startPoint: .top, endPoint: .bottom)
            }
        }
        .background(JackinBrand.stage)
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }

    private func rainTone(age: Int) -> (color: Color, opacity: Double)? {
        switch age {
        case 0: (Color.white, 0.26)
        case 1...2: (Color(red: 180 / 255, green: 1, blue: 180 / 255), 0.22)
        case 3...5: (Color(red: 0, green: 1, blue: 65 / 255), 0.18)
        case 6...10: (Color(red: 0, green: 200 / 255, blue: 50 / 255), 0.14)
        case 11...16: (Color(red: 0, green: 140 / 255, blue: 30 / 255), 0.11)
        case 17...24: (Color(red: 0, green: 80 / 255, blue: 18 / 255), 0.09)
        default: nil
        }
    }

    private func hash(_ values: Int...) -> UInt64 {
        values.reduce(0xDEAD_BEEF_CAFE_1337) { seed, value in
            var result = seed ^ UInt64(bitPattern: Int64(value))
            result ^= result << 13
            result ^= result >> 7
            result ^= result << 17
            return result
        }
    }
}

/// Official provider logomarks (template) for status bar, popover, and Usage
/// chrome.
///
/// Status bar stays template monochrome. Marks are provenance-audited
/// against vendor brand assets — see Resources/ProviderMarks/PROVENANCE.md.
/// Load order: vector **PDF preferred** (rsvg-rendered from official SVGs with
/// transparent paper — resolution-independent), 512² PNG fallback. Diverges
/// from the incumbent's PNG-first order, whose PDFs carried opaque paper.
@MainActor
enum ProviderMarks {
    static func templateImage(forIconKey iconKey: String) -> NSImage? {
        for ext in ["pdf", "png"] {
            // SPM flattens `.process` resources; check subdirectory then root.
            let candidates = [
                Bundle.module.url(
                    forResource: iconKey, withExtension: ext,
                    subdirectory: "ProviderMarks"),
                Bundle.module.url(forResource: iconKey, withExtension: ext),
            ]
            for case let url? in candidates {
                guard let image = NSImage(contentsOf: url) else { continue }
                let copy = image.copy() as? NSImage ?? image
                copy.isTemplate = true
                copy.size = NSSize(width: 18, height: 18)
                return copy
            }
        }
        return nil
    }

    static func swiftUIImage(forIconKey iconKey: String) -> Image? {
        guard let ns = templateImage(forIconKey: iconKey) else { return nil }
        return Image(nsImage: ns)
    }
}
