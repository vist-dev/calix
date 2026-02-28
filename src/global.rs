use crate::error::{CalixError, CalixResult, DependencyWarning};
use crate::storage;
use crate::submodule::{Submodule, SubmoduleKind};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// タイムラインイベントの種別
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TimelineEventKind {
    /// 通常のコミット
    Commit,
    /// マージコミット
    Merge { second_parent_id: String },
    /// リベース完了（旧ID→新IDの対応表）
    Rebase {
        commit_mapping: HashMap<String, String>,
    },
}

/// グローバルな操作の順序を管理するエントリ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalTimelineEntry {
    pub submodule_id: String,
    pub commit_id: String,
    pub timestamp: u64,
    pub sequence: u64,
    pub event_kind: TimelineEventKind,
}

/// グローバルリポジトリの状態
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalState {
    pub version: u32,
    /// submodule_id -> relative_path のインデックス（再構築可能なキャッシュ）
    pub submodule_index: HashMap<String, String>,
    /// グローバルタイムライン
    pub timeline: Vec<GlobalTimelineEntry>,
    pub global_sequence: u64,
}

/// リポジトリ全体を操作するエントリーポイント
pub struct Repository {
    /// プロジェクトルート（.calixの親）
    pub project_root: PathBuf,
    pub state: GlobalState,
}

impl Repository {
    /// リポジトリを新規初期化する
    pub fn init(project_root: &Path) -> CalixResult<Self> {
        let calix_dir = project_root.join(".calix");

        if calix_dir.exists() {
            return Err(CalixError::AlreadyInitialized(
                project_root.display().to_string(),
            ));
        }

        std::fs::create_dir_all(calix_dir.join("submodules"))?;

        let state = GlobalState {
            version: 1,
            submodule_index: HashMap::new(),
            timeline: Vec::new(),
            global_sequence: 0,
        };

        let repo = Self {
            project_root: project_root.to_path_buf(),
            state,
        };

        repo.save_state()?;
        Ok(repo)
    }

    /// 既存リポジトリを読み込む
    pub fn open(project_root: &Path) -> CalixResult<Self> {
        let state_path = project_root.join(".calix").join("state.msgpack");

        if !state_path.exists() {
            return Err(CalixError::NotInitialized);
        }

        let state: GlobalState = storage::read_msgpack(&state_path)?;
        Ok(Self {
            project_root: project_root.to_path_buf(),
            state,
        })
    }

    fn calix_dir(&self) -> PathBuf {
        self.project_root.join(".calix")
    }

    fn submodule_storage_root(&self, submodule_id: &str) -> PathBuf {
        self.calix_dir().join("submodules").join(submodule_id)
    }

    fn save_state(&self) -> CalixResult<()> {
        let state_path = self.calix_dir().join("state.msgpack");
        storage::write_msgpack(&state_path, &self.state)
    }

    /// 新しいサブモジュールを登録する
    pub fn register_submodule(
        &mut self,
        kind: SubmoduleKind,
        relative_path: String,
    ) -> CalixResult<Submodule> {
        let id = Uuid::new_v4().to_string();
        let storage_root = self.submodule_storage_root(&id);

        let submodule =
            Submodule::create(&storage_root, id.clone(), kind, relative_path.clone())?;

        self.state.submodule_index.insert(id, relative_path);
        self.save_state()?;

        Ok(submodule)
    }

    /// サブモジュールを読み込む
    pub fn load_submodule(&self, submodule_id: &str) -> CalixResult<Submodule> {
        let storage_root = self.submodule_storage_root(submodule_id);
        Submodule::load(&storage_root)
    }

    fn now_timestamp() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    /// グローバルタイムラインにコミットイベントを記録する
    pub fn record_global_event(
        &mut self,
        submodule_id: String,
        commit_id: String,
        event_kind: TimelineEventKind,
    ) -> CalixResult<()> {
        self.state.global_sequence += 1;

        let entry = GlobalTimelineEntry {
            submodule_id,
            commit_id,
            timestamp: Self::now_timestamp(),
            sequence: self.state.global_sequence,
            event_kind,
        };

        self.state.timeline.push(entry);
        self.save_state()
    }

    /// マージイベントを記録する
    pub fn record_merge_event(
        &mut self,
        submodule_id: String,
        merge_commit_id: String,
        second_parent_id: String,
    ) -> CalixResult<()> {
        self.record_global_event(
            submodule_id,
            merge_commit_id,
            TimelineEventKind::Merge { second_parent_id },
        )
    }

    /// リベースイベントを記録する
    pub fn record_rebase_event(
        &mut self,
        submodule_id: String,
        new_head_commit_id: String,
        commit_mapping: HashMap<String, String>,
    ) -> CalixResult<()> {
        self.record_global_event(
            submodule_id,
            new_head_commit_id,
            TimelineEventKind::Rebase { commit_mapping },
        )
    }

    /// サブモジュール間の依存関係の整合性チェック
    ///
    /// 依存先サブモジュールのコミットがグローバルタイムライン上で
    /// 自サブモジュールのコミットより後に来ている場合に警告を返す。
    pub fn check_dependency_ordering(
        &self,
        submodule_id: &str,
    ) -> CalixResult<Vec<DependencyWarning>> {
        let submodule = self.load_submodule(submodule_id)?;
        let mut warnings = Vec::new();

        if submodule.info.dependencies.is_empty() {
            return Ok(warnings);
        }

        // このサブモジュールのタイムライン上での最新シーケンス番号
        let self_max_seq = self
            .state
            .timeline
            .iter()
            .filter(|e| e.submodule_id == submodule_id)
            .map(|e| e.sequence)
            .max();

        let self_max_seq = match self_max_seq {
            Some(seq) => seq,
            None => return Ok(warnings), // タイムラインにエントリがなければ警告なし
        };

        for dep in &submodule.info.dependencies {
            // 依存先サブモジュールのタイムライン上での最新シーケンス番号
            let dep_max_seq = self
                .state
                .timeline
                .iter()
                .filter(|e| e.submodule_id == dep.submodule_id)
                .map(|e| e.sequence)
                .max();

            if let Some(dep_seq) = dep_max_seq {
                if dep_seq > self_max_seq {
                    warnings.push(DependencyWarning {
                        submodule_id: submodule_id.to_string(),
                        dependency_submodule_id: dep.submodule_id.clone(),
                        message: format!(
                            "Dependency submodule '{}' has commits (seq={}) after this submodule's latest commit (seq={})",
                            dep.submodule_id, dep_seq, self_max_seq
                        ),
                    });
                }
            }
        }

        Ok(warnings)
    }
}
