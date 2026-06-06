//! ANSI escape stripping for diagnostics and terminal text artifacts.

use anstyle_parse::{DefaultCharAccumulator, Parser, Perform};

#[must_use]
pub fn strip_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut parser = Parser::<DefaultCharAccumulator>::default();
    let mut performer = PlainPerformer { output: Vec::new() };
    for &byte in bytes {
        parser.advance(&mut performer, byte);
    }
    performer.output
}

struct PlainPerformer {
    output: Vec<u8>,
}

impl Perform for PlainPerformer {
    fn print(&mut self, c: char) {
        let mut buf = [0u8; 4];
        self.output
            .extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
    }

    fn execute(&mut self, byte: u8) {
        if matches!(byte, b'\n' | b'\r' | b'\t') {
            self.output.push(byte);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::strip_bytes;

    #[test]
    fn strip_removes_sgr_sequences() {
        assert_eq!(
            strip_bytes(b"\x1b[31merror\x1b[0m\n").as_slice(),
            b"error\n"
        );
    }
}
