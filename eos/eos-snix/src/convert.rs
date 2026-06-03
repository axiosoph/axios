//! Conversions between `eos-core` and Snix types.

use eos_core::digest::Blake3Digest;
use eos_core::store::StorePath;
use nix_compat::store_path::StorePath as NixStorePath;
use snix_castore::B3Digest;

use crate::error::SnixError;

/// Converts a `Blake3Digest` to a Snix `B3Digest`.
#[inline]
pub fn blake3_to_b3(digest: Blake3Digest) -> B3Digest {
    B3Digest::from(&digest.0)
}

/// Converts a Snix `B3Digest` to a `Blake3Digest`.
pub fn b3_to_blake3(digest: B3Digest) -> Result<Blake3Digest, SnixError> {
    let slice = &digest[..];
    if slice.len() != 32 {
        return Err(SnixError::ConversionError {
            from: "B3Digest",
            to: "Blake3Digest",
            detail: format!("digest length is {}, expected 32", slice.len()),
        });
    }
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(slice);
    Ok(Blake3Digest(bytes))
}

/// Converts a `StorePath` to a Snix `StorePath<String>`.
pub fn store_path_to_nix(path: StorePath) -> Result<NixStorePath<String>, SnixError> {
    NixStorePath::from_absolute_path(path.0.as_bytes()).map_err(|err| SnixError::ConversionError {
        from: "StorePath",
        to: "nix_compat::store_path::StorePath<String>",
        detail: err.to_string(),
    })
}

/// Converts a Snix `StorePath<String>` to a `StorePath`.
#[inline]
pub fn nix_to_store_path(path: NixStorePath<String>) -> StorePath {
    StorePath(path.to_absolute_path())
}

/// Converts a reference to a Snix `StorePath<String>` to a `StorePath`.
#[inline]
pub fn nix_ref_to_store_path(path: &NixStorePath<String>) -> StorePath {
    StorePath(path.to_absolute_path())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_digest_conversion_roundtrip() {
        bolero::check!().with_type::<[u8; 32]>().for_each(|&bytes| {
            let digest = Blake3Digest(bytes);
            let b3 = blake3_to_b3(digest);
            let back = b3_to_blake3(b3).unwrap();
            assert_eq!(back, digest);
        });
    }
}
