#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        if let Ok(lock) = eos::lock::LockFile::parse(s) {
            let _ = lock.validate();
        }
    }
});
