fn render() {
    match () {
        () if std::fs::read("/tmp/x").is_ok() => {},
        _ => {},
    }
}

fn main() {}
