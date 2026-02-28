use crate::commit::Commit;
use crate::error::{CalixError, CalixResult};
use crate::storage;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// サブモジュールのメタ情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmoduleInfo {
    pub id: String,
    pub kind: SubmoduleKind,
    /// プロジェクトルートからの相対パス
    pub relative_path: String,
    /// 現在のHEADコミットID
    pub head_commit_id: Option<String>,
    /// 現在のブランチ名
    pub current_branch: String,
    /// 依存先サブモジュールのIDと優先順位
    pub dependencies: Vec<Dependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SubmoduleKind {
    Clip,
    Effect,
    Transition,
    Subtitle,
    Track,
    GlobalEffect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub submodule_id: String,
    pub priority: u32, // 1が最高優先
}

/// サブモジュールの操作を担当する
pub struct Submodule {
    pub info: SubmoduleInfo,
    /// .calix/submodules/{id}/ 以下のディレクトリ
    pub storage_root: PathBuf,
}

impl Submodule {
    /// ストレージからサブモジュールを読み込む
    pub fn load(storage_root: &Path) -> CalixResult<Self> {
        let info_path = storage_root.join("info.msgpack");
        let info: SubmoduleInfo = storage::read_msgpack(&info_path)?;
        Ok(Self {
            info,
            storage_root: storage_root.to_path_buf(),
        })
    }

    /// サブモジュールを新規作成してストレージに保存する
    pub fn create(
        storage_root: &Path,
        id: String,
        kind: SubmoduleKind,
        relative_path: String,
    ) -> CalixResult<Self> {
        let info = SubmoduleInfo {
            id,
            kind,
            relative_path,
            head_commit_id: None,
            current_branch: "main".to_string(),
            dependencies: Vec::new(),
        };

        let submodule = Self {
            info,
            storage_root: storage_root.to_path_buf(),
        };

        submodule.save_info()?;
        Ok(submodule)
    }

    fn save_info(&self) -> CalixResult<()> {
        let info_path = self.storage_root.join("info.msgpack");
        storage::write_msgpack(&info_path, &self.info)
    }

    /// コミットを追加する
    pub fn append_commit(&mut self, commit: Commit) -> CalixResult<()> {
        let commit_path = self
            .storage_root
            .join("commits")
            .join(format!("{}.msgpack", commit.id));

        storage::write_msgpack(&commit_path, &commit)?;

        self.info.head_commit_id = Some(commit.id.clone());
        self.save_info()?;

        Ok(())
    }

    /// コミットIDからコミットを読み込む
    pub fn load_commit(&self, commit_id: &str) -> CalixResult<Commit> {
        let commit_path = self
            .storage_root
            .join("commits")
            .join(format!("{}.msgpack", commit_id));

        storage::read_msgpack(&commit_path).map_err(|_| CalixError::CommitNotFound {
            id: commit_id.to_string(),
        })
    }

    /// HEADから指定コミットまで差分を積み上げて現在の状態を再構築する
    pub fn reconstruct_state(&self, commit_id: &str) -> CalixResult<HashMap<String, String>> {
        let chain = self.build_commit_chain(commit_id)?;
        let mut state: HashMap<String, String> = HashMap::new();

        for commit in chain {
            state = commit.diff.apply(&state);
        }

        Ok(state)
    }

    /// 指定コミットまでの祖先チェーンを古い順で返す
    fn build_commit_chain(&self, commit_id: &str) -> CalixResult<Vec<Commit>> {
        let mut chain = Vec::new();
        let mut current_id = Some(commit_id.to_string());

        while let Some(id) = current_id {
            let commit = self.load_commit(&id)?;
            current_id = commit.parent_id.clone();
            chain.push(commit);
        }

        chain.reverse();
        Ok(chain)
    }
}
