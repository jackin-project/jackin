fn helper_reads() {
    let _ = std::fs::read("/tmp/x");
}

fn render() {
    helper_reads();
}

fn main() {}
