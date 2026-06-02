#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
struct FuzzVerifyInput {
    is_claim: bool,
    alg: u8,
    payload: Vec<u8>,
    signature: Vec<u8>,
    public_key: Vec<u8>,
}

fuzz_target!(|input: FuzzVerifyInput| {
    let alg = match input.alg % 3 {
        0 => "Ed25519",
        1 => "ES256",
        _ => "UNSUPPORTED",
    };

    if input.is_claim {
        let _ = atom_id::verify_claim(&input.payload, &input.signature, alg, &input.public_key);
    } else {
        let _ = atom_id::verify_publish(&input.payload, &input.signature, alg, &input.public_key);
    }
});
