//! # Atom Core
//!
//! Protocol trait surface for the Atom ecosystem.
//!
//! This crate defines the behavioral contracts that all Atom backends must
//! implement. The traits are derived from the formal layer model's L1
//! coalgebras (see `models/publishing-stack-layers.md`):
//!
//! | Trait             | Model § | Role                              |
//! |:------------------|:--------|:----------------------------------|
//! | [`AtomSource`]    | §2.1    | Read-only observation             |
//! | [`AtomRegistry`]  | §2.2    | Claiming and publishing (source)  |
//! | [`AtomStore`]     | §2.3    | Local accumulation (consumer)     |
//! | [`Manifest`]      | §1      | Minimal package metadata          |
//!
//! Two implementations of the same trait are interchangeable if their
//! observations agree pointwise (bisimulation equivalence from the model).
//!
//! ## `AtomDigest`
//!
//! [`AtomDigest`] is a compact, self-describing multihash of an [`AtomId`],
//! used for store-level indexing and git ref paths. Multiple valid digests
//! exist per identity — one per algorithm.
//!
//! ## `content_hash`
//!
//! [`content_hash`] computes the BLAKE3 content-tree digest
//! (`docs/specs/atom-transactions.md` `[content-hash-algorithm]`) over a
//! backend-agnostic [`ContentEntry`] list — the free function a publisher
//! or consumer calls to produce or verify a `PublishPayload`'s optional
//! `content_hash` field. It lives here, not `atom-id`, because it operates
//! on [`ContentEntry`], a type `atom-id` has no access to.
//!
//! ## Design principles
//!
//! - **Backend-agnostic**: trait signatures contain no git types, no concrete version types, no
//!   serialization framework types. Backend specifics are expressed exclusively through associated
//!   types.
//! - **Identity/signature crypto-free**: all identity and signature-verification logic lives in
//!   `atom-id`. This crate consumes `atom-id`'s types and re-exported coz-rs primitives; its own
//!   `blake3` dependency computes [`content_hash`] only, never identity or signatures.
//! - **Minimal**: no gix, no semver, no tokio. Two dependencies: `atom-id`, `blake3`.

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
#![forbid(unsafe_code)]

pub use atom_id::{
    Alg, Anchor, AtomDigest, AtomId, Cad, Czd, HashAlg, Label, OwnerRef, RawVersion, Thumbprint,
    VersionScheme,
};

mod hash {
    //! BLAKE3 content-tree digest — `[content-hash-algorithm]`.
    //!
    //! Mirrors the recursive structure a backend's own canonical content-tree
    //! construction already builds (git: `[snapshot-deterministic]`'s tree),
    //! substituting BLAKE3 for the backend's object hash at every level, and
    //! deliberately omitting git's own object headers (`blob <len>\0`, `tree
    //! <len>\0`) — see `docs/specs/atom-transactions.md:674-745` for the full
    //! normative algorithm this module implements literally.

    use std::cmp::Ordering;
    use std::collections::HashMap;

    use crate::ContentEntry;

    /// A filename contained a forbidden NUL byte (`0x00`).
    ///
    /// `[content-hash-algorithm]` step 2 requires a producer to reject such
    /// content before hashing — the NUL byte is the serialization's own
    /// field delimiter, so an unrejected NUL in a filename would make the
    /// serialization ambiguous.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct NulInFilename(pub String);

    impl std::fmt::Display for NulInFilename {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "filename contains a forbidden NUL byte: {:?}", self.0)
        }
    }

    impl std::error::Error for NulInFilename {}

    /// One child triple — `(mode, filename, child_digest)` — collected
    /// under its parent directory before that parent's own digest is
    /// computed.
    struct Child {
        /// Git's own canonical ASCII tree-mode digits for this entry's kind.
        mode: &'static str,
        /// This entry's own basename (not the full path).
        filename: String,
        /// The leaf digest (regular/symlink) or per-directory digest
        /// (directory), 32 raw BLAKE3 bytes.
        digest: [u8; 32],
        /// Whether this child is itself a directory — needed by the git
        /// tie-break sort rule (a directory's implicit trailing `/`).
        is_dir: bool,
    }

    /// Split `path` into `(parent, basename)` on the last `/`, matching
    /// `GitStore::write_content_tree`'s own convention so both constructions
    /// agree on tree shape.
    fn split_path(path: &str) -> (&str, &str) {
        match path.rfind('/') {
            Some(idx) => (&path[..idx], &path[idx + 1..]),
            None => ("", path),
        }
    }

    /// Reject a filename containing a NUL byte.
    fn check_filename(filename: &str) -> Result<(), NulInFilename> {
        if filename.as_bytes().contains(&0) {
            Err(NulInFilename(filename.to_string()))
        } else {
            Ok(())
        }
    }

    /// Git's own canonical tree-entry tie-break comparison
    /// (`[content-hash-algorithm]` step 2), reused verbatim rather than
    /// reinvented: compare the first `n` bytes of both filenames, `n` being
    /// the shorter filename's length; if equal, compare each entry's
    /// tie-break byte (the `(n+1)`-th filename byte if longer than `n`,
    /// else `0x2F` for a directory, else no tie-break byte at all — a
    /// missing tie-break byte sorts first).
    fn tree_sort_cmp(a: &Child, b: &Child) -> Ordering {
        let (an, bn) = (a.filename.as_bytes(), b.filename.as_bytes());
        let n = an.len().min(bn.len());
        match an[..n].cmp(&bn[..n]) {
            Ordering::Equal => {},
            ord => return ord,
        }
        let tie_break = |name: &[u8], is_dir: bool| -> Option<u8> {
            if name.len() > n {
                Some(name[n])
            } else if is_dir {
                Some(b'/')
            } else {
                None
            }
        };
        match (tie_break(an, a.is_dir), tie_break(bn, b.is_dir)) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Less,
            (Some(_), None) => Ordering::Greater,
            (Some(x), Some(y)) => x.cmp(&y),
        }
    }

    /// Serialize a directory's sorted children and return its BLAKE3 digest
    /// (`[content-hash-algorithm]` step 2): for each child, the mode digits,
    /// one ASCII space, the filename bytes, one NUL byte, and the child
    /// digest's raw 32 bytes, concatenated in tie-break sort order. A
    /// directory with zero children serializes to the empty byte string.
    fn directory_digest(mut children: Vec<Child>) -> [u8; 32] {
        children.sort_by(tree_sort_cmp);
        let mut buf = Vec::new();
        for child in &children {
            buf.extend_from_slice(child.mode.as_bytes());
            buf.push(b' ');
            buf.extend_from_slice(child.filename.as_bytes());
            buf.push(0);
            buf.extend_from_slice(&child.digest);
        }
        *blake3::hash(&buf).as_bytes()
    }

    /// Compute `content_hash` (`[content-hash-is-tree-digest]`) over a
    /// backend-agnostic content-entry list.
    ///
    /// `entries` MUST be ordered children-before-parents (the same contract
    /// [`crate::AtomContent::content`] already guarantees) — every directory,
    /// including intermediate ones, MUST have its own explicit
    /// [`ContentEntry::Directory`] marker so this function can find its
    /// children before it is asked to digest them, mirroring
    /// `GitStore::write_content_tree`'s own bottom-up construction.
    ///
    /// Returns [`NulInFilename`] if any entry's basename contains a NUL byte.
    pub fn content_hash(entries: &[ContentEntry]) -> Result<[u8; 32], NulInFilename> {
        let mut children_by_parent: HashMap<&str, Vec<Child>> = HashMap::new();

        for entry in entries {
            let (path, mode, is_dir, digest): (&str, &'static str, bool, [u8; 32]) = match entry {
                ContentEntry::Regular {
                    path,
                    data,
                    executable,
                } => {
                    let mode = if *executable { "100755" } else { "100644" };
                    (path.as_str(), mode, false, *blake3::hash(data).as_bytes())
                },
                ContentEntry::Symlink { path, target } => (
                    path.as_str(),
                    "120000",
                    false,
                    *blake3::hash(target).as_bytes(),
                ),
                ContentEntry::Directory { path } => {
                    let children = children_by_parent.remove(path.as_str()).unwrap_or_default();
                    (path.as_str(), "40000", true, directory_digest(children))
                },
            };

            let (parent, filename) = split_path(path);
            check_filename(filename)?;
            children_by_parent.entry(parent).or_default().push(Child {
                mode,
                filename: filename.to_string(),
                digest,
                is_dir,
            });
        }

        let root_children = children_by_parent.remove("").unwrap_or_default();
        Ok(directory_digest(root_children))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn regular(path: &str, data: &[u8]) -> ContentEntry {
            ContentEntry::Regular {
                path: path.to_string(),
                data: data.to_vec(),
                executable: false,
            }
        }

        fn executable(path: &str, data: &[u8]) -> ContentEntry {
            ContentEntry::Regular {
                path: path.to_string(),
                data: data.to_vec(),
                executable: true,
            }
        }

        fn symlink(path: &str, target: &[u8]) -> ContentEntry {
            ContentEntry::Symlink {
                path: path.to_string(),
                target: target.to_vec(),
            }
        }

        fn dir(path: &str) -> ContentEntry {
            ContentEntry::Directory {
                path: path.to_string(),
            }
        }

        /// Root of zero entries — `[content-hash-algorithm]` step 2's
        /// "zero children serializes to the empty byte string" case applies
        /// to the content root exactly as to any other directory.
        #[test]
        fn empty_root_hashes_to_blake3_of_empty_string() {
            let digest = content_hash(&[]).expect("no filenames to reject");
            assert_eq!(digest, *blake3::hash(b"").as_bytes());
        }

        /// An explicit empty subdirectory also serializes to `BLAKE3("")` as
        /// its own per-directory digest, then is folded into the root exactly
        /// like any other child.
        #[test]
        fn explicit_empty_directory_hashes_to_blake3_of_empty_string() {
            let entries = [dir("empty")];
            let digest = content_hash(&entries).unwrap();

            let mut expected_buf = Vec::new();
            expected_buf.extend_from_slice(b"40000 empty\0");
            expected_buf.extend_from_slice(blake3::hash(b"").as_bytes());
            let expected = *blake3::hash(&expected_buf).as_bytes();

            assert_eq!(digest, expected);
        }

        /// Single regular file at the root: leaf digest is BLAKE3(data) with
        /// no length/type prefix (step 1), folded into the root's own
        /// per-directory serialization (step 2).
        #[test]
        fn single_regular_file_at_root() {
            let entries = [regular("lib.rs", b"fn test() {}")];
            let digest = content_hash(&entries).unwrap();

            let leaf = *blake3::hash(b"fn test() {}").as_bytes();
            let mut buf = Vec::new();
            buf.extend_from_slice(b"100644 lib.rs\0");
            buf.extend_from_slice(&leaf);
            let expected = *blake3::hash(&buf).as_bytes();

            assert_eq!(digest, expected);
        }

        /// An executable regular file uses mode `100755`, not `100644` — a
        /// flipped executable bit must change the digest.
        #[test]
        fn executable_bit_changes_digest_and_uses_100755() {
            let regular_entries = [regular("run.sh", b"#!/bin/sh\n")];
            let exec_entries = [executable("run.sh", b"#!/bin/sh\n")];

            let regular_digest = content_hash(&regular_entries).unwrap();
            let exec_digest = content_hash(&exec_entries).unwrap();
            assert_ne!(
                regular_digest, exec_digest,
                "flipping the executable bit must change the digest"
            );

            let leaf = *blake3::hash(b"#!/bin/sh\n").as_bytes();
            let mut buf = Vec::new();
            buf.extend_from_slice(b"100755 run.sh\0");
            buf.extend_from_slice(&leaf);
            let expected = *blake3::hash(&buf).as_bytes();
            assert_eq!(exec_digest, expected);
        }

        /// Single symlink: leaf digest is BLAKE3(target), mode `120000`.
        #[test]
        fn single_symlink_at_root() {
            let entries = [symlink("link", b"lib.rs")];
            let digest = content_hash(&entries).unwrap();

            let leaf = *blake3::hash(b"lib.rs").as_bytes();
            let mut buf = Vec::new();
            buf.extend_from_slice(b"120000 link\0");
            buf.extend_from_slice(&leaf);
            let expected = *blake3::hash(&buf).as_bytes();

            assert_eq!(digest, expected);
        }

        /// Nested directories: children-before-parents, mode `40000` (five
        /// digits) for a directory, recursion bottom-up.
        #[test]
        fn nested_directories_recurse_bottom_up() {
            let entries = [
                regular("src/lib.rs", b"fn test() {}"),
                dir("src"),
                regular("Cargo.toml", b"[package]"),
            ];
            let digest = content_hash(&entries).unwrap();

            let file_leaf = *blake3::hash(b"fn test() {}").as_bytes();
            let mut src_buf = Vec::new();
            src_buf.extend_from_slice(b"100644 lib.rs\0");
            src_buf.extend_from_slice(&file_leaf);
            let src_digest = *blake3::hash(&src_buf).as_bytes();

            let cargo_leaf = *blake3::hash(b"[package]").as_bytes();

            // Root sort order: "Cargo.toml" vs "src" — plain byte comparison
            // decides it (no shared prefix), 'C' (0x43) < 's' (0x73).
            let mut root_buf = Vec::new();
            root_buf.extend_from_slice(b"100644 Cargo.toml\0");
            root_buf.extend_from_slice(&cargo_leaf);
            root_buf.extend_from_slice(b"40000 src\0");
            root_buf.extend_from_slice(&src_digest);
            let expected = *blake3::hash(&root_buf).as_bytes();

            assert_eq!(digest, expected);
        }

        /// The concrete git tie-break disagreement case this rule exists for:
        /// directory `foo` vs. file `foo.txt`. Plain filename comparison
        /// would order them by ordinary string sort; git's own tie-break rule
        /// (a directory's implicit trailing `/`, 0x2F) must instead sort
        /// `foo.txt` BEFORE the `foo` directory, since `.` (0x2E) < `/` (0x2F).
        #[test]
        fn git_tie_break_orders_foo_txt_before_foo_directory() {
            let entries = [
                regular("foo.txt", b"a file named foo.txt"),
                regular("foo/bar", b"a file inside directory foo"),
                dir("foo"),
            ];
            let digest = content_hash(&entries).unwrap();

            let bar_leaf = *blake3::hash(b"a file inside directory foo").as_bytes();
            let mut foo_dir_buf = Vec::new();
            foo_dir_buf.extend_from_slice(b"100644 bar\0");
            foo_dir_buf.extend_from_slice(&bar_leaf);
            let foo_dir_digest = *blake3::hash(&foo_dir_buf).as_bytes();

            let txt_leaf = *blake3::hash(b"a file named foo.txt").as_bytes();

            // foo.txt (tie-break byte '.' = 0x2E) sorts before foo/ (tie-break
            // byte '/' = 0x2F) — the opposite of plain "foo" < "foo.txt"
            // string order.
            let mut root_buf = Vec::new();
            root_buf.extend_from_slice(b"100644 foo.txt\0");
            root_buf.extend_from_slice(&txt_leaf);
            root_buf.extend_from_slice(b"40000 foo\0");
            root_buf.extend_from_slice(&foo_dir_digest);
            let expected = *blake3::hash(&root_buf).as_bytes();

            assert_eq!(digest, expected);
        }

        /// `[content-hash-algorithm]`'s own acceptance-criteria row: two
        /// independently-constructed but content-equal entry sets — here,
        /// deliberately built in a different insertion order — MUST produce
        /// byte-identical digests (c2-determinism). Both orderings still
        /// respect the required children-before-parents contract (`dir("src")`
        /// always follows `"src/lib.rs"`) — only sibling order varies.
        #[test]
        fn two_independent_constructions_of_same_content_are_byte_identical() {
            let a = [
                regular("src/lib.rs", b"fn test() {}"),
                dir("src"),
                regular("Cargo.toml", b"[package]"),
            ];
            let b = [
                regular("Cargo.toml", b"[package]"),
                regular("src/lib.rs", b"fn test() {}"),
                dir("src"),
            ];

            let digest_a = content_hash(&a).unwrap();
            let digest_b = content_hash(&b).unwrap();
            assert_eq!(
                digest_a, digest_b,
                "content-equal entry sets in different construction order must agree"
            );
        }

        /// Renaming a file must change the digest — content identity is not
        /// just raw bytes, per `[content-hash-is-tree-digest]`.
        #[test]
        fn renaming_a_file_changes_the_digest() {
            let original = [regular("a.txt", b"same bytes")];
            let renamed = [regular("b.txt", b"same bytes")];
            assert_ne!(
                content_hash(&original).unwrap(),
                content_hash(&renamed).unwrap()
            );
        }

        /// A filename containing a NUL byte is a rejected input, not a
        /// silently-mishandled one (`[content-hash-algorithm]` step 2).
        #[test]
        fn nul_in_filename_is_rejected() {
            let entries = [regular("a\0b", b"data")];
            let err = content_hash(&entries).expect_err("NUL filename must be rejected");
            assert_eq!(err.0, "a\0b");
        }
    }
}

pub use hash::{NulInFilename, content_hash};

// ============================================================================
// Traits
// ============================================================================

/// Minimal package metadata.
///
/// Every package format defines its own manifest (e.g., `Cargo.toml`,
/// `package.json`, `atom.toml`). The atom protocol requires exactly
/// two properties — everything else is ecosystem-specific.
///
/// Spec constraint: `[manifest-minimal]`.
pub trait Manifest {
    /// The human-readable package name.
    fn label(&self) -> &Label;

    /// The unparsed version string.
    ///
    /// Implementors resolve this via [`VersionScheme`] at consumption time.
    fn version(&self) -> &RawVersion;
}

/// Read-only observation of an atom store or source.
///
/// The common interface shared by sources and stores (model §2.1).
/// Two implementations are interchangeable if `resolve` and `discover`
/// agree pointwise (bisimulation equivalence).
/// Trait representing an observed entry in an atom source.
pub trait AtomEntry {
    /// Concrete version observation type.
    type Version: AtomVersion;

    /// Iterator over the versions of this entry.
    type VersionIter<'a>: Iterator<Item = &'a Self::Version> + 'a
    where
        Self: 'a;

    /// The unique identity of the atom.
    fn id(&self) -> &AtomId;

    /// Iterate over all resolved versions of the atom.
    fn versions(&self) -> Self::VersionIter<'_>;
}

/// Trait representing an observed version of an atom.
pub trait AtomVersion {
    /// The unparsed version string.
    fn version(&self) -> &RawVersion;

    /// Content snapshot digest.
    fn dig(&self) -> &[u8];

    /// Opaque Coz digest of the authorizing claim.
    fn czd(&self) -> Option<&Czd>;

    /// Raw claim Coz message envelope JSON string, if signed.
    fn claim_msg(&self) -> Option<&str>;

    /// Raw publish Coz message envelope JSON string, if signed.
    fn publish_msg(&self) -> Option<&str>;
}

/// Read-only observation of an atom store or source.
///
/// The common interface shared by sources and stores (model §2.1).
/// Two implementations are interchangeable if `resolve` and `discover`
/// agree pointwise (bisimulation equivalence).
///
/// Observations are wrapped in `Result` to distinguish "not found"
/// (`Ok(None)`) from backend failure (`Err`).
pub trait AtomSource: Send + Sync + 'static {
    /// Backend-defined observation type returned by [`resolve`](Self::resolve).
    type Entry: AtomEntry;

    /// Backend-specific error type.
    type Error: std::error::Error + Send + Sync + 'static;

    /// Look up an atom by its identity.
    ///
    /// Returns `Ok(None)` if the atom is not present in this source.
    /// Returns `Err` on backend failure (network, disk, permission, etc.).
    fn resolve(
        &self,
        id: &AtomId,
    ) -> impl std::future::Future<Output = Result<Option<Self::Entry>, Self::Error>> + Send;

    /// Search for atoms matching a query string.
    ///
    /// Returns atom identities, not full entries — use
    /// [`resolve`](Self::resolve) for observation data.
    fn discover(
        &self,
        query: &str,
    ) -> impl std::future::Future<Output = Result<Vec<AtomId>, Self::Error>> + Send;
}

/// A single entry in an atom's content tree.
///
/// Represents one node in the abstract tree yielded by
/// [`AtomContent::content`]. Entries are ordered
/// children-before-parents (leaves-to-root) to satisfy
/// castore ingestion ordering requirements.
#[derive(Clone, Debug)]
pub enum ContentEntry {
    /// A regular file with content bytes.
    Regular {
        /// Relative path within the atom tree (e.g., "src/lib.rs").
        path: String,
        /// Raw file content.
        data: Vec<u8>,
        /// Whether the file is executable.
        executable: bool,
    },
    /// A symbolic link.
    Symlink {
        /// Relative path of the symlink.
        path: String,
        /// Target of the symlink.
        target: Vec<u8>,
    },
    /// A directory marker.
    Directory {
        /// Relative path of the directory.
        path: String,
    },
}

/// Content observation interface (model §2.1a).
///
/// Extends [`AtomSource`] with the ability to yield the content
/// tree for a specific atom version. This is the _content recovery_
/// functor — it recovers the tree data that `AtomSource` (the
/// forgetful functor) deliberately omits.
///
/// Implementations provide backend-specific tree extraction:
/// - Git backend: walks `gix` tree objects
/// - Future backends: extract from their native representation
///
/// Consumers (e.g., [`AtomStore::ingest`], eos bridge) use this
/// trait to transfer content across backend boundaries without
/// runtime downcasting.
pub trait AtomContent: AtomSource {
    /// Yield the content tree for a specific atom version.
    ///
    /// Returns the full tree as a `Vec<ContentEntry>` ordered
    /// children-before-parents (leaves-to-root).
    ///
    /// # Arguments
    ///
    /// * `id` — the atom's identity
    /// * `dig` — backend-specific content snapshot digest (e.g., 20-byte git tree OID)
    ///
    /// Returns `None` if the content is not found.
    fn content(
        &self,
        id: &AtomId,
        dig: &[u8],
    ) -> impl std::future::Future<Output = Result<Option<Vec<ContentEntry>>, Self::Error>> + Send;
}

/// Claiming and publishing interface (source-side).
///
/// Extends [`AtomSource`] with write operations. Lives at the canonical
/// source (model §2.2, spec §Source/Store).
///
/// Session ordering is enforced by data flow: [`claim`](Self::claim)
/// returns a [`Czd`] that [`publish`](Self::publish) requires as input.
pub trait AtomRegistry: AtomSource {
    /// Establish ownership of an atom identity.
    ///
    /// Returns the claim's [`Czd`] (coz digest), which must be passed
    /// to [`publish`](Self::publish) to authorize version publication.
    ///
    /// `owner` is a single owner-reference (`[claim-owner-single]`) — the
    /// one identity accountable for this label.
    fn claim(&self, id: &AtomId, owner: &OwnerRef) -> Result<Czd, Self::Error>;

    /// Publish a version against an existing claim.
    ///
    /// # Arguments
    ///
    /// * `id` — the atom being published
    /// * `claim` — czd of the authorizing claim (from [`claim`](Self::claim))
    /// * `version` — unparsed version string
    /// * `dig` — content snapshot digest
    /// * `src` — source revision identifier
    /// * `path` — subtree path within the source tree
    #[allow(clippy::too_many_arguments)]
    fn publish(
        &self,
        id: &AtomId,
        claim: &Czd,
        version: &RawVersion,
        dig: &[u8],
        src: &[u8],
        path: &str,
    ) -> Result<(), Self::Error>;

    /// Charter (found or succeed) an atom-set.
    ///
    /// `prior: None` founds a new atom-set: the returned [`Czd`] becomes
    /// the atom-set's [`Anchor`]. `prior: Some(czd)` signs a successor to
    /// the charter named by `czd`, transferring ownership without
    /// changing the anchor.
    ///
    /// # Arguments
    ///
    /// * `owner` — non-empty set of owner-references recognized under this anchor
    ///   (`[charter-owner-set]`)
    /// * `src` — source revision demarking the chartering point
    /// * `prior` — czd of the charter this one succeeds, or `None` to found a new atom-set
    fn charter(
        &self,
        owner: &[OwnerRef],
        src: &[u8],
        prior: Option<&Czd>,
    ) -> Result<Czd, Self::Error>;
}

/// Local accumulation interface (consumer-side).
///
/// Extends [`AtomSource`] with ingestion from remote sources (model §2.3,
/// spec §Source/Store).
///
/// **Accumulation guarantee** (spec `[ingest-preserves-identity]`):
/// after [`ingest`](Self::ingest), for every atom in the source,
/// [`resolve`](AtomSource::resolve) on this store MUST return at least
/// what the source's `resolve` returns. The store accumulates — it never
/// loses atoms through ingestion.
pub trait AtomStore: AtomContent {
    /// Import atoms from a source into this store.
    ///
    /// After completion, this store contains at least every atom
    /// that was in `source` (⊇ condition).
    fn ingest<S: AtomContent>(
        &self,
        source: &S,
    ) -> impl std::future::Future<Output = Result<(), Self::Error>> + Send;

    /// Check whether an atom is present in this store.
    fn contains(
        &self,
        id: &AtomId,
    ) -> impl std::future::Future<Output = Result<bool, Self::Error>> + Send;
}
