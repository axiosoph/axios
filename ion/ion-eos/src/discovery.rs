//! Atom discovery client implementation.

use atom_id::AtomId;
use eos_core::index::{AtomMeta, AtomQuery};
use eos_proto::eos_capnp;

use crate::error::ClientError;

/// Client for querying atom metadata from the Eos daemon.
#[derive(Clone)]
pub struct DiscoveryClient {
    client: eos_capnp::atom_discovery::Client,
}

impl DiscoveryClient {
    /// Creates a new `DiscoveryClient`.
    #[must_use]
    pub fn new(client: eos_capnp::atom_discovery::Client) -> Self {
        Self { client }
    }

    /// Resolves an atom's metadata by its ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the RPC call fails or the response is invalid.
    pub async fn resolve(&self, atom_id: &AtomId) -> Result<Option<AtomMeta>, ClientError> {
        let mut req = self.client.resolve_request();
        {
            let mut id_builder = req.get().init_id();
            id_builder.set_digest(atom_id.to_string().as_bytes());
        }

        match req.send().promise.await {
            Ok(res) => {
                let res_reader = res.get().map_err(|e| ClientError::ProtocolError {
                    detail: e.to_string(),
                })?;
                if res_reader.has_meta() {
                    let meta_reader =
                        res_reader
                            .get_meta()
                            .map_err(|e| ClientError::ProtocolError {
                                detail: e.to_string(),
                            })?;
                    let meta = parse_atom_meta(meta_reader)?;
                    Ok(Some(meta))
                } else {
                    Ok(None)
                }
            },
            Err(e) => {
                if e.extra.contains("Atom not found") {
                    Ok(None)
                } else {
                    Err(ClientError::ProtocolError {
                        detail: e.to_string(),
                    })
                }
            },
        }
    }

    /// Checks if the daemon has knowledge of the given atom.
    ///
    /// # Errors
    ///
    /// Returns an error if the RPC call fails.
    pub async fn contains(&self, atom_id: &AtomId) -> Result<bool, ClientError> {
        let mut req = self.client.contains_request();
        {
            let mut id_builder = req.get().init_id();
            id_builder.set_digest(atom_id.to_string().as_bytes());
        }

        let res = req
            .send()
            .promise
            .await
            .map_err(|e| ClientError::ProtocolError {
                detail: e.to_string(),
            })?;

        let exists = res
            .get()
            .map_err(|e| ClientError::ProtocolError {
                detail: e.to_string(),
            })?
            .get_exists();
        Ok(exists)
    }

    /// Searches for atoms matching the given query.
    ///
    /// # Errors
    ///
    /// Returns an error if the RPC call fails.
    pub async fn search(&self, query: &AtomQuery) -> Result<Vec<AtomMeta>, ClientError> {
        let mut req = self.client.search_request();
        {
            let mut query_builder = req.get().init_query();
            query_builder.set_label_pattern(&query.label_pattern);
            if let Some(ref filter) = query.set_filter {
                query_builder.set_set_filter(filter);
            }
            query_builder.set_limit(query.limit);
        }

        let res = req
            .send()
            .promise
            .await
            .map_err(|e| ClientError::ProtocolError {
                detail: e.to_string(),
            })?;

        let results_list = res.get()?.get_results()?;
        let mut list = Vec::new();
        for i in 0..results_list.len() {
            let meta_reader = results_list.get(i);
            list.push(parse_atom_meta(meta_reader)?);
        }
        Ok(list)
    }
}

fn parse_atom_meta(reader: eos_capnp::atom_meta::Reader) -> Result<AtomMeta, ClientError> {
    let id_reader = reader.get_id().map_err(|e| ClientError::ProtocolError {
        detail: e.to_string(),
    })?;
    let digest_str =
        std::str::from_utf8(
            id_reader
                .get_digest()
                .map_err(|e| ClientError::ProtocolError {
                    detail: e.to_string(),
                })?,
        )
        .map_err(|e| ClientError::ProtocolError {
            detail: format!("Invalid UTF-8 in ID digest: {}", e),
        })?;
    let id = digest_str
        .parse::<AtomId>()
        .map_err(|e| ClientError::ProtocolError {
            detail: format!("Failed to parse AtomId: {}", e),
        })?;

    let label = reader
        .get_label()
        .map_err(|e| ClientError::ProtocolError {
            detail: e.to_string(),
        })?
        .to_str()
        .map_err(|e| ClientError::ProtocolError {
            detail: e.to_string(),
        })?
        .to_string();

    let versions_list = reader
        .get_versions()
        .map_err(|e| ClientError::ProtocolError {
            detail: e.to_string(),
        })?;
    let mut versions = Vec::new();
    for i in 0..versions_list.len() {
        let v_reader = versions_list.get(i);
        let version = v_reader
            .get_version()
            .map_err(|e| ClientError::ProtocolError {
                detail: e.to_string(),
            })?
            .to_str()
            .map_err(|e| ClientError::ProtocolError {
                detail: e.to_string(),
            })?
            .to_string();
        let rev = v_reader
            .get_rev()
            .map_err(|e| ClientError::ProtocolError {
                detail: e.to_string(),
            })?
            .to_str()
            .map_err(|e| ClientError::ProtocolError {
                detail: e.to_string(),
            })?
            .to_string();
        let set = v_reader
            .get_set()
            .map_err(|e| ClientError::ProtocolError {
                detail: e.to_string(),
            })?
            .to_str()
            .map_err(|e| ClientError::ProtocolError {
                detail: e.to_string(),
            })?
            .to_string();
        versions.push(eos_core::index::VersionInfo { version, rev, set });
    }

    let sets_list = reader.get_sets().map_err(|e| ClientError::ProtocolError {
        detail: e.to_string(),
    })?;
    let mut sets = Vec::new();
    for i in 0..sets_list.len() {
        let set = sets_list
            .get(i)
            .map_err(|e| ClientError::ProtocolError {
                detail: e.to_string(),
            })?
            .to_str()
            .map_err(|e| ClientError::ProtocolError {
                detail: e.to_string(),
            })?
            .to_string();
        sets.push(set);
    }

    Ok(AtomMeta {
        id,
        label,
        versions,
        sets,
    })
}
