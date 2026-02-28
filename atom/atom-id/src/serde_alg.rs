/// Serde bridge for [`coz_rs::Alg`] via its string name.
///
/// `Alg` does not implement serde natively. This module serializes
/// as `Alg::name()` and deserializes via `Alg::from_str()`.
use coz_rs::Alg;
use serde::{self, Deserialize, Deserializer, Serializer};

pub(crate) fn serialize<S>(alg: &Alg, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(alg.name())
}

pub(crate) fn deserialize<'de, D>(deserializer: D) -> Result<Alg, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Alg::from_str(&s).ok_or_else(|| serde::de::Error::custom(format!("unknown algorithm: {s}")))
}
