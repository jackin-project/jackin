# jackin' Desktop (macOS)

Native macOS app for jackin'. Built as a SwiftPM executable so it builds and runs from the terminal during early iteration; `.app` bundling and signing come later.

This is being rebuilt incrementally around the direction in the [Desktop Agent Hub roadmap](../../docs/content/docs/reference/roadmap/jackin-desktop-agent-hub.mdx): a window modeled on CMUX with a left sidebar grouping isolated sessions by workspace, and per-space tabs that embed [libghostty](https://github.com/ghostty-org/ghostty) terminal surfaces running `jackin load`.

## Status

**Milestone 1** — a SwiftUI window that launches. No sidebar, no terminal, no CLI integration yet.

## Requirements

- macOS 14 or newer.
- A Swift 6 toolchain. Any one of:
  - Xcode (App Store), or
  - Xcode Command Line Tools: `xcode-select --install`, or
  - [`swiftly`](https://www.swift.org/install/macos/) toolchain manager.

Verify: `swift --version` (expect Swift 6.x).

## Build and run

```sh
cd desktop/macos
swift run JackinDesktop
```

A window titled `jackin'` should appear and come to the foreground.

## Layout

```
desktop/macos/
  Package.swift                      SwiftPM manifest (executable, macOS 14+)
  Sources/JackinDesktop/
    JackinDesktopApp.swift           @main App + activation delegate
    ContentView.swift                milestone-1 window content
```
