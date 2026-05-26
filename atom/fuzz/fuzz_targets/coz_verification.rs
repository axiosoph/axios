#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < 5 {
        return;
    }
    
    let opt = data[0];
    let alg = match data[1] % 3 {
        0 => "Ed25519",
        1 => "ES256",
        _ => "UNSUPPORTED",
    };

    let len = data.len() - 2;
    let part_len = len / 3;
    let pay_json = &data[2..2+part_len];
    let sig = &data[2+part_len..2+2*part_len];
    let pub_key = &data[2+2*part_len..];

    if opt % 2 == 0 {
        let _ = atom_id::verify_claim(pay_json, sig, alg, pub_key);
    } else {
        let _ = atom_id::verify_publish(pay_json, sig, alg, pub_key);
    }
});
