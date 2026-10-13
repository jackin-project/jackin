fn render() {
    // Closure body is a graph boundary — not walked as render path.
    let _f = || {
        let _ = std::fs::read("/tmp/x");
    };
}

fn main() {}
