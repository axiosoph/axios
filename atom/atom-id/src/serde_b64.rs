//! Serde bridge for `Vec<u8>` via base64url-unpadded encoding.
//!
//! Coz payloads encode binary fields as base64url strings.
//! This module provides the `#[serde(with = "serde_b64")]` attribute
//! for `Vec<u8>` fields.

use coz_rs::base64ct::{Base64UrlUnpadded, Encoding};
use serde::{self, Deserialize, Deserializer, Serializer};

pub(crate) fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&Base64UrlUnpadded::encode_string(bytes))
}

pub(crate) fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Base64UrlUnpadded::decode_vec(&s).map_err(serde::de::Error::custom)
}
