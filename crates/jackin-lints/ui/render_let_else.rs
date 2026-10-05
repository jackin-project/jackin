fn render() {
    let Some(value) = Some(()) else {
        let _ = std::fs::read("/tmp/x");
        return;
    };
    let _ = value;
}

fn main() {}
