#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = ontaria::parse_document(data, "http://example.org/fuzz");
});
