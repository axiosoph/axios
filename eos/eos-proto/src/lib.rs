//! Cap'n Proto schemas and generated code for the Eos protocol.

#![allow(clippy::all)]
#![allow(unused_qualifications, unused_parens, unused_imports, dead_code)]

pub mod eos_capnp {
    include!(concat!(env!("OUT_DIR"), "/schema/eos_capnp.rs"));
}

#[cfg(test)]
mod tests {
    #[test]
    fn assert_generated_types_importable() {
        let _option: Option<crate::eos_capnp::build_status::Reader<'static>> = None;
    }
}

