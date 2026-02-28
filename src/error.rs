use thiserror::Error;

#[derive(Debug, Error)]
pub enum CalixError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialize(String),

    #[error("Deserialization error: {0}")]
    Deserialize(String),

    #[error("Submodule not found: {0}")]
    SubmoduleNotFound(String),

    #[error("Commit not found: {id}")]
    CommitNotFound { id: String },

    #[error("Branch not found: {name}")]
    BranchNotFound { name: String },

    #[error("Merge conflict in submodule: {submodule_id}")]
    MergeConflict { submodule_id: String },

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("Repository already initialized at: {0}")]
    AlreadyInitialized(String),

    #[error("Repository not initialized")]
    NotInitialized,
}

pub type CalixResult<T> = Result<T, CalixError>;
