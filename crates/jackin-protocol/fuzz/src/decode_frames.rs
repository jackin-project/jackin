//! Fuzz target: feed arbitrary bytes to the protocol wire decoders.
//! Goal: **zero panics**, ever, on any (tag, payload) split.
//!
//! Run locally (CI-suitable short budget):
//!   cargo fuzz run --fuzz-dir crates/jackin-protocol/fuzz --sanitizer none decode_frames -- -max_total_time=60
//! Run overnight:
//!   cargo fuzz run --fuzz-dir crates/jackin-protocol/fuzz --sanitizer none decode_frames -- -max_total_time=86400

#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    let tag = data[0];
    let payload = data[1..].to_vec();
    // A hostile/truncated frame must fail closed (Err), never panic.
    let _ = jackin_protocol::attach::decode_client(tag, payload.clone());
    let _ = jackin_protocol::attach::decode_server(tag, payload);
});
