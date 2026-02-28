use serde::{Deserialize, Serialize};
use thiserror::Error;

/// マージ・リベースのコンフリクト詳細
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictEntry {
    pub key: String,
    pub base_value: Option<String>,
    pub current_value: Option<String>,
    pub incoming_value: Option<String>,
}

/// サブモジュール間の依存関係に関する警告
#[derive(Debug, Clone)]
pub struct DependencyWarning {
    pub submodule_id: String,
    pub dependency_submodule_id: String,
    pub message: String,
}

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

    #[error("Branch already exists: {name}")]
    BranchAlreadyExists { name: String },

    #[error("Cannot delete main branch")]
    CannotDeleteMainBranch,

    #[error("Cannot delete current branch: {name}")]
    CannotDeleteCurrentBranch { name: String },

    #[error("Merge conflict in submodule {submodule_id}: {conflicts:?}")]
    MergeConflict {
        submodule_id: String,
        conflicts: Vec<ConflictEntry>,
    },

    #[error("Rebase conflict at commit {commit_id} in submodule {submodule_id}: {conflicts:?}")]
    RebaseConflict {
        submodule_id: String,
        commit_id: String,
        conflicts: Vec<ConflictEntry>,
    },

    #[error("Rebase already in progress")]
    RebaseInProgress,

    #[error("No rebase in progress")]
    RebaseNotInProgress,

    #[error("No common ancestor found")]
    NoCommonAncestor,

    #[error("No merge in progress")]
    MergeNotInProgress,

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("Repository already initialized at: {0}")]
    AlreadyInitialized(String),

    #[error("Repository not initialized")]
    NotInitialized,
}

pub type CalixResult<T> = Result<T, CalixError>;
