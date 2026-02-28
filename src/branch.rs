use crate::error::{CalixError, CalixResult};
use crate::storage;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// ブランチの情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Branch {
    /// ブランチ名
    pub name: String,
    /// 所属するサブモジュールID
    pub submodule_id: String,
    /// ブランチの先端コミットID
    pub head_commit_id: Option<String>,
    /// 作成元コミットID（分岐点）
    pub fork_commit_id: Option<String>,
    /// 作成日時（Unix timestamp）
    pub created_at: u64,
}

/// 一つのサブモジュールが持つ全ブランチを管理する
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BranchStore {
    /// ブランチ名 -> Branch のマップ
    pub branches: HashMap<String, Branch>,
    /// 現在のブランチ名
    pub current_branch: String,
}

impl BranchStore {
    /// デフォルトのBranchStore（mainブランチのみ）を作成する
    pub fn new(submodule_id: &str) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let main_branch = Branch {
            name: "main".to_string(),
            submodule_id: submodule_id.to_string(),
            head_commit_id: None,
            fork_commit_id: None,
            created_at: now,
        };

        let mut branches = HashMap::new();
        branches.insert("main".to_string(), main_branch);

        Self {
            branches,
            current_branch: "main".to_string(),
        }
    }

    /// ストレージからBranchStoreを読み込む
    pub fn load(storage_root: &Path) -> CalixResult<Self> {
        let path = storage_root.join("branches.msgpack");
        storage::read_msgpack(&path)
    }

    /// ストレージにBranchStoreを保存する
    pub fn save(&self, storage_root: &Path) -> CalixResult<()> {
        let path = storage_root.join("branches.msgpack");
        storage::write_msgpack(&path, self)
    }

    /// ブランチを作成する
    pub fn create_branch(
        &mut self,
        name: &str,
        submodule_id: &str,
        fork_commit_id: &str,
    ) -> CalixResult<()> {
        if self.branches.contains_key(name) {
            return Err(CalixError::BranchAlreadyExists {
                name: name.to_string(),
            });
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let branch = Branch {
            name: name.to_string(),
            submodule_id: submodule_id.to_string(),
            head_commit_id: Some(fork_commit_id.to_string()),
            fork_commit_id: Some(fork_commit_id.to_string()),
            created_at: now,
        };

        self.branches.insert(name.to_string(), branch);
        Ok(())
    }

    /// 現在のブランチを切り替える
    pub fn checkout(&mut self, name: &str) -> CalixResult<()> {
        if !self.branches.contains_key(name) {
            return Err(CalixError::BranchNotFound {
                name: name.to_string(),
            });
        }
        self.current_branch = name.to_string();
        Ok(())
    }

    /// ブランチを削除する
    pub fn delete_branch(&mut self, name: &str) -> CalixResult<()> {
        if name == "main" {
            return Err(CalixError::CannotDeleteMainBranch);
        }
        if name == self.current_branch {
            return Err(CalixError::CannotDeleteCurrentBranch {
                name: name.to_string(),
            });
        }
        if self.branches.remove(name).is_none() {
            return Err(CalixError::BranchNotFound {
                name: name.to_string(),
            });
        }
        Ok(())
    }

    /// 全ブランチを返す
    pub fn list_branches(&self) -> Vec<&Branch> {
        self.branches.values().collect()
    }

    /// 現在のブランチの先端コミットIDを更新する
    pub fn advance_branch(&mut self, commit_id: &str) -> CalixResult<()> {
        let branch = self
            .branches
            .get_mut(&self.current_branch)
            .ok_or_else(|| CalixError::BranchNotFound {
                name: self.current_branch.clone(),
            })?;
        branch.head_commit_id = Some(commit_id.to_string());
        Ok(())
    }

    /// 指定ブランチのHEADコミットIDを取得する
    pub fn get_branch_head(&self, name: &str) -> CalixResult<Option<String>> {
        let branch = self
            .branches
            .get(name)
            .ok_or_else(|| CalixError::BranchNotFound {
                name: name.to_string(),
            })?;
        Ok(branch.head_commit_id.clone())
    }

    /// 指定ブランチのHEADコミットIDを直接設定する
    pub fn set_branch_head(&mut self, name: &str, commit_id: &str) -> CalixResult<()> {
        let branch = self
            .branches
            .get_mut(name)
            .ok_or_else(|| CalixError::BranchNotFound {
                name: name.to_string(),
            })?;
        branch.head_commit_id = Some(commit_id.to_string());
        Ok(())
    }
}
