
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }
    let tag = data[0];
    let payload = data[1..].to_vec();
    let _ = jackin_protocol::attach::decode_client(tag, payload.clone());
    let _ = jackin_protocol::attach::decode_server(tag, payload);
});
