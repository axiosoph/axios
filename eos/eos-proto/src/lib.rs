//! Cap'n Proto schemas and generated code for the Eos protocol.

#![allow(clippy::all)]
#![allow(unused_qualifications)]

pub mod eos_capnp {
    include!(concat!(env!("OUT_DIR"), "/schema/eos_capnp.rs"));
}
