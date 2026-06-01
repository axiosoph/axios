use thiserror::Error;

/// Custom error type representing all failures in the Git backend.
#[derive(Debug, Error)]
pub enum GitError {
    /// Failure during a Gitoxide reference transaction or edit.
    #[error("Git reference edit failed: {0}")]
    RefEdit(#[from] gix::reference::edit::Error),

    /// Failure to find a reference.
    #[error("Git reference find error: {0}")]
    RefFindError(#[from] gix::reference::find::Error),

    /// Failure to write an object in the Git repository.
    #[error("Git object write database error: {0}")]
    OdbWrite(#[from] gix::object::write::Error),

    /// Failure to find an existing reference in the repository.
    #[error("Git reference find error: {0}")]
    RefFind(#[from] gix::reference::find::existing::Error),

    /// Failure to find an existing object in the repository.
    #[error("Git object find error: {0}")]
    ObjectFind(#[from] gix::object::find::existing::Error),

    /// Failure during object conversion.
    #[error("Git object conversion error: {0}")]
    ObjectConversion(#[from] gix::object::try_into::Error),

    /// Failure during object decoding.
    #[error("Git object decoding error: {0}")]
    ObjectDecode(#[from] gix::objs::decode::Error),

    /// Error during Coz payload parsing or signature verification.
    #[error("Coz payload or verification error: {0}")]
    Coz(String),

    /// Verify error from atom-id.
    #[error("Signature verification error: {0}")]
    Verify(#[from] atom_id::VerifyError),

    /// Reference iterator initialization error.
    #[error("Reference iteration init error: {0}")]
    RefIterInit(#[from] gix::reference::iter::init::Error),

    /// Packed reference buffer error.
    #[error("Packed reference buffer error: {0}")]
    PackedBuffer(#[from] gix::reference::iter::Error),

    /// Invalid relative path.
    #[error("Relative path error: {0}")]
    RelativePath(#[from] gix::path::relative_path::Error),

    /// JSON serialization or deserialization failure.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// Git directory path resolution or initialization failure.
    #[error("Repository open/initialization error: {0}")]
    Init(String),

    /// The derived anchor did not match the expected anchor in the transaction.
    #[error("Anchor mismatch: derived {derived} but expected {expected}")]
    InvalidAnchor {
        /// The derived anchor base64ut value.
        derived: String,
        /// The expected anchor base64ut value.
        expected: String,
    },

    /// The publish source revision is not at or after the claim source revision.
    #[error(
        "Invalid temporal vector: publish src {publish_src} is not a descendant of claim src \
         {claim_src}"
    )]
    InvalidTemporalVector {
        /// Publish source revision commit hash.
        publish_src: String,
        /// Claim source revision commit hash.
        claim_src: String,
    },

    /// An active claim does not exist for the package being published.
    #[error("No active claim exists for label: {0}")]
    NoActiveClaim(String),

    /// An ingested publish tag references a claim czd that is missing from the store.
    #[error("Unclaimed publish: claim czd {0} is not present in the store")]
    UnclaimedPublish(String),

    /// A claim commit object has a non-empty tree.
    #[error("Invalid claim commit tree: claim commit MUST have the well-known empty tree")]
    NonEmptyClaimTree,

    /// General validation or specification violation error.
    #[error("Spec validation failure: {0}")]
    Validation(String),

    /// An I/O error occurred.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
