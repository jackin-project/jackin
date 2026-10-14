// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0


import AppKit
import Foundation
import JackinDesktopUI
import JackinUsageBridge

@main
struct ProviderMarksHarness {
    static func main() {
        _ = NSApplication.shared
        var failed = 0
        let keys = ["codex", "claude", "amp", "grok", "zai", "kimi", "minimax"]
        fputs("ProviderMarksHarness: \(keys.count) Desktop keys\n", stderr)
        for key in keys {
            let has = ProviderMarks.hasMark(forIconKey: key)
            let mark = ProviderMarks.templateImage(forIconKey: key)
            let status = StatusItemRendering.icon(forIconKey: key)
            let ok = has && mark != nil && status.size.width > 0
            fputs(
                "\(ok ? "PASS" : "FAIL")  \(key) hasMark=\(has) markSize=\(mark?.size ?? .zero) statusTemplate=\(status.isTemplate)\n",
                stderr
            )
            if !ok { failed += 1 }
        }
        let unknown = ProviderMarks.hasMark(forIconKey: "not-a-provider")
        fputs("\(unknown ? "FAIL" : "PASS")  unknown key hasMark=false\n", stderr)
        if unknown { failed += 1 }

        for key in keys {
            guard let mark = ProviderMarks.templateImage(forIconKey: key),
                let maxA = maxAlpha(of: mark)
            else {
                fputs("FAIL  \(key) could not sample mark alpha\n", stderr)
                failed += 1
                continue
            }
            let ok = maxA >= 200
            fputs(
                "\(ok ? "PASS" : "FAIL")  \(key) mark maxA=\(maxA) (need ≥200)\n",
                stderr
            )
            if !ok { failed += 1 }
        }

        if failed > 0 {
            fputs("ProviderMarksHarness: \(failed) FAIL\n", stderr)
            exit(1)
        }
        fputs(
            "ProviderMarksHarness: ALL PASS (\(keys.count)/\(keys.count) official marks)\n", stderr)
    }

    private static func maxAlpha(of image: NSImage) -> Int? {
        guard let tiff = image.tiffRepresentation,
            let rep = NSBitmapImageRep(data: tiff)
        else { return nil }
        let w = rep.pixelsWide
        let h = rep.pixelsHigh
        var maxA = 0
        let step = max(1, min(w, h) / 64)
        for y in stride(from: 0, to: h, by: step) {
            for x in stride(from: 0, to: w, by: step) {
                guard let c = rep.colorAt(x: x, y: y) else { continue }
                maxA = max(maxA, Int((c.alphaComponent * 255.0).rounded()))
            }
        }
        if maxA < 200 {
            for y in 0..<h {
                for x in 0..<w {
                    guard let c = rep.colorAt(x: x, y: y) else { continue }
                    maxA = max(maxA, Int((c.alphaComponent * 255.0).rounded()))
                    if maxA >= 250 { return maxA }
                }
            }
        }
        return maxA
    }
}
