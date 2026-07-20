# jackin-usage-ffi

Synchronous UniFFI facade over `jackin-usage` host runtime for the native macOS
agent-usage menu bar. Mirrors TableRock’s `tablerock-ffi` split: Rust owns all
truth; Swift is display-only.

## Build

```sh
cargo build -p jackin-usage-ffi --release
cargo nextest run -p jackin-usage-ffi
cargo clippy -p jackin-usage-ffi --all-targets -- -D warnings
```

## Swift bindings

```sh
./scripts/generate-usage-swift-bindings.sh
```

## XCFramework

```sh
./scripts/build-usage-xcframework.sh
```
