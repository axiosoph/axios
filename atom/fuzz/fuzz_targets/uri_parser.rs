#![no_main]

use libfuzzer_sys::fuzz_target;
use std::str::FromStr;
use atom_uri::RawAtomUri;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = RawAtomUri::from_str(s);
    }
});
