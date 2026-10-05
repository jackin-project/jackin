struct View;

impl View {
    fn blocking_helper(&self) {
        let _ = std::fs::read("/tmp/x");
    }

    fn render(&self) {
        self.blocking_helper();
    }
}

fn main() {}
