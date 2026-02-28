use crate::branch::BranchStore;
use crate::commit::{Commit, Diff, GlobalOrder};
use crate::diff::three_way_merge;
use crate::error::{CalixError, CalixResult};
use crate::storage;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use uuid::Uuid;

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

/// マージの結果
#[derive(Debug)]
pub enum MergeResult {
    /// マージコミットが生成された
    Merged { commit: Commit },
    /// Fast-Forward: ブランチポインタのみ移動
    FastForward { new_head_id: String },
}

/// マージ中の状態（コンフリクト解決のために保存）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeState {
    /// マージ先ブランチ（現在のブランチ）
    pub source_branch: String,
    /// マージ元ブランチ
    pub target_branch: String,
    /// マージ先のHEADコミットID
    pub source_head_id: String,
    /// マージ元のHEADコミットID
    pub target_head_id: String,
    /// 共通祖先コミットID
    pub common_ancestor_id: String,
}

/// リベース中の状態
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebaseState {
    /// リベース元ブランチ名（現在のブランチ）
    pub source_branch: String,
    /// リベース先ブランチ名
    pub target_branch: String,
    /// リベース開始前のHEADコミットID
    pub original_head_id: String,
    /// リベース先のHEADコミットID
    pub target_head_id: String,
    /// 残りの適用待ちコミットIDリスト
    pub remaining_commit_ids: Vec<String>,
    /// 現在コンフリクト中のコミットID
    pub conflicting_commit_id: Option<String>,
    /// 最後に適用されたコミットの新ID（リベースチェーンの先端）
    pub last_applied_id: Option<String>,
}

/// サブモジュールの操作を担当する
pub struct Submodule {
    pub info: SubmoduleInfo,
    /// .calix/submodules/{id}/ 以下のディレクトリ
    pub storage_root: PathBuf,
}

impl Submodule {
    // ─── 基本操作 ───

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
            id: id.clone(),
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

        // BranchStoreも初期化する
        let branch_store = BranchStore::new(&id);
        branch_store.save(storage_root)?;

        Ok(submodule)
    }

    fn save_info(&self) -> CalixResult<()> {
        let info_path = self.storage_root.join("info.msgpack");
        storage::write_msgpack(&info_path, &self.info)
    }

    /// コミットファイルのみ書き込む（ブランチ更新なし）
    fn write_commit(&self, commit: &Commit) -> CalixResult<()> {
        let commit_path = self
            .storage_root
            .join("commits")
            .join(format!("{}.msgpack", commit.id));
        storage::write_msgpack(&commit_path, commit)
    }

    /// コミットを追加し、現在のブランチの先端を更新する
    pub fn append_commit(&mut self, commit: Commit) -> CalixResult<()> {
        self.write_commit(&commit)?;

        // BranchStoreの現在ブランチを更新
        let mut branch_store = self.load_branch_store()?;
        branch_store.advance_branch(&commit.id)?;
        branch_store.save(&self.storage_root)?;

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

    /// 指定コミットまでの祖先チェーンを古い順で返す（first-parentのみ辿る）
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

    fn load_branch_store(&self) -> CalixResult<BranchStore> {
        BranchStore::load(&self.storage_root)
    }

    fn now_timestamp() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    fn new_commit(
        submodule_id: &str,
        parent_id: Option<String>,
        second_parent_id: Option<String>,
        message: String,
        diff: Diff,
    ) -> Commit {
        let now = Self::now_timestamp();
        Commit {
            id: Uuid::new_v4().to_string(),
            parent_id,
            second_parent_id,
            message,
            submodule_id: submodule_id.to_string(),
            global_order: GlobalOrder {
                timestamp: now,
                sequence: 0,
            },
            diff,
            created_at: now,
        }
    }

    // ─── ブランチ操作 ───

    /// ブランチを作成する
    pub fn create_branch(&mut self, name: &str, fork_commit_id: &str) -> CalixResult<()> {
        // 分岐元コミットの存在確認
        self.load_commit(fork_commit_id)?;

        let mut branch_store = self.load_branch_store()?;
        branch_store.create_branch(name, &self.info.id, fork_commit_id)?;
        branch_store.save(&self.storage_root)?;
        Ok(())
    }

    /// ブランチを切り替える
    pub fn checkout(&mut self, branch_name: &str) -> CalixResult<()> {
        let mut branch_store = self.load_branch_store()?;
        branch_store.checkout(branch_name)?;
        branch_store.save(&self.storage_root)?;

        // SubmoduleInfoのcurrent_branchも更新
        self.info.current_branch = branch_name.to_string();
        // head_commit_idもブランチのHEADに揃える
        let head = branch_store.get_branch_head(branch_name)?;
        self.info.head_commit_id = head;
        self.save_info()?;
        Ok(())
    }

    /// ブランチを削除する
    pub fn delete_branch(&mut self, name: &str) -> CalixResult<()> {
        let mut branch_store = self.load_branch_store()?;
        branch_store.delete_branch(name)?;
        branch_store.save(&self.storage_root)?;
        Ok(())
    }

    /// ブランチ一覧を返す（ブランチ名, 現在のブランチかどうか）
    pub fn list_branches(&self) -> CalixResult<(Vec<String>, String)> {
        let branch_store = self.load_branch_store()?;
        let names: Vec<String> = branch_store
            .list_branches()
            .iter()
            .map(|b| b.name.clone())
            .collect();
        Ok((names, branch_store.current_branch.clone()))
    }

    // ─── マージ ───

    /// 2つのコミットの共通祖先を探索する（BFS）
    pub fn find_common_ancestor(
        &self,
        commit_id_a: &str,
        commit_id_b: &str,
    ) -> CalixResult<String> {
        // commit_aの全祖先を収集
        let ancestors_a = self.collect_all_ancestors(commit_id_a)?;

        // commit_bの祖先を辿り、ancestors_aとの最初の合流点を返す
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();
        queue.push_back(commit_id_b.to_string());

        while let Some(id) = queue.pop_front() {
            if ancestors_a.contains(&id) {
                return Ok(id);
            }
            if !visited.insert(id.clone()) {
                continue;
            }
            let commit = self.load_commit(&id)?;
            if let Some(parent) = &commit.parent_id {
                queue.push_back(parent.clone());
            }
            if let Some(second_parent) = &commit.second_parent_id {
                queue.push_back(second_parent.clone());
            }
        }

        Err(CalixError::NoCommonAncestor)
    }

    /// 指定コミットの全祖先IDを収集する（本人含む）
    fn collect_all_ancestors(&self, commit_id: &str) -> CalixResult<HashSet<String>> {
        let mut ancestors = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(commit_id.to_string());

        while let Some(id) = queue.pop_front() {
            if !ancestors.insert(id.clone()) {
                continue;
            }
            let commit = self.load_commit(&id)?;
            if let Some(parent) = &commit.parent_id {
                queue.push_back(parent.clone());
            }
            if let Some(second_parent) = &commit.second_parent_id {
                queue.push_back(second_parent.clone());
            }
        }

        Ok(ancestors)
    }

    /// マージを実行する
    pub fn merge(&mut self, target_branch_name: &str) -> CalixResult<MergeResult> {
        let branch_store = self.load_branch_store()?;

        let current_head_id = branch_store
            .get_branch_head(&branch_store.current_branch)?
            .ok_or_else(|| CalixError::CommitNotFound {
                id: "current branch has no commits".to_string(),
            })?;

        let target_head_id = branch_store
            .get_branch_head(target_branch_name)?
            .ok_or_else(|| CalixError::CommitNotFound {
                id: format!("branch {} has no commits", target_branch_name),
            })?;

        let ancestor_id = self.find_common_ancestor(&current_head_id, &target_head_id)?;

        // Fast-Forward判定: 共通祖先が現在のHEADと一致
        if ancestor_id == current_head_id {
            let mut branch_store = self.load_branch_store()?;
            branch_store.set_branch_head(
                &branch_store.current_branch.clone(),
                &target_head_id,
            )?;
            branch_store.save(&self.storage_root)?;

            self.info.head_commit_id = Some(target_head_id.clone());
            self.save_info()?;

            return Ok(MergeResult::FastForward {
                new_head_id: target_head_id,
            });
        }

        // 三方向マージ
        let ancestor_state = self.reconstruct_state(&ancestor_id)?;
        let current_state = self.reconstruct_state(&current_head_id)?;
        let target_state = self.reconstruct_state(&target_head_id)?;

        match three_way_merge(&ancestor_state, &current_state, &target_state) {
            Ok(merged_state) => {
                // 自動マージ成功 → マージコミット生成
                let merge_diff = Diff::from_states(&current_state, &merged_state);
                let commit = Self::new_commit(
                    &self.info.id,
                    Some(current_head_id),
                    Some(target_head_id),
                    format!("Merge branch '{}'", target_branch_name),
                    merge_diff,
                );
                self.append_commit(commit.clone())?;
                Ok(MergeResult::Merged { commit })
            }
            Err(conflicts) => {
                // コンフリクト → 状態を保存してエラーを返す
                let merge_state = MergeState {
                    source_branch: branch_store.current_branch.clone(),
                    target_branch: target_branch_name.to_string(),
                    source_head_id: current_head_id,
                    target_head_id,
                    common_ancestor_id: ancestor_id,
                };
                self.save_merge_state(&merge_state)?;

                Err(CalixError::MergeConflict {
                    submodule_id: self.info.id.clone(),
                    conflicts,
                })
            }
        }
    }

    /// コンフリクト解決後にマージコミットを生成する
    pub fn resolve_conflict(
        &mut self,
        resolved_state: &HashMap<String, String>,
    ) -> CalixResult<Commit> {
        let merge_state = self.load_merge_state()?;

        let current_state = self.reconstruct_state(&merge_state.source_head_id)?;
        let merge_diff = Diff::from_states(&current_state, resolved_state);

        let commit = Self::new_commit(
            &self.info.id,
            Some(merge_state.source_head_id),
            Some(merge_state.target_head_id),
            format!("Merge branch '{}' (resolved)", merge_state.target_branch),
            merge_diff,
        );

        self.append_commit(commit.clone())?;
        self.clear_merge_state()?;

        Ok(commit)
    }

    fn save_merge_state(&self, state: &MergeState) -> CalixResult<()> {
        let path = self.storage_root.join("merge_state.msgpack");
        storage::write_msgpack(&path, state)
    }

    fn load_merge_state(&self) -> CalixResult<MergeState> {
        let path = self.storage_root.join("merge_state.msgpack");
        if !path.exists() {
            return Err(CalixError::MergeNotInProgress);
        }
        storage::read_msgpack(&path)
    }

    fn clear_merge_state(&self) -> CalixResult<()> {
        let path = self.storage_root.join("merge_state.msgpack");
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    // ─── リベース ───

    /// リベースを開始する
    pub fn rebase(&mut self, target_branch_name: &str) -> CalixResult<()> {
        if self.is_rebasing()? {
            return Err(CalixError::RebaseInProgress);
        }

        let branch_store = self.load_branch_store()?;
        let source_branch = branch_store.current_branch.clone();

        let current_head_id = branch_store
            .get_branch_head(&source_branch)?
            .ok_or_else(|| CalixError::CommitNotFound {
                id: "current branch has no commits".to_string(),
            })?;

        let target_head_id = branch_store
            .get_branch_head(target_branch_name)?
            .ok_or_else(|| CalixError::CommitNotFound {
                id: format!("branch {} has no commits", target_branch_name),
            })?;

        let ancestor_id = self.find_common_ancestor(&current_head_id, &target_head_id)?;

        // リベース元ブランチから共通祖先以降のコミットを収集する（古い順）
        let commits_to_replay = self.collect_commits_since(&current_head_id, &ancestor_id)?;

        if commits_to_replay.is_empty() {
            // リベース不要（すでにターゲット以降にコミットがない）
            return Ok(());
        }

        let commit_ids: Vec<String> = commits_to_replay.iter().map(|c| c.id.clone()).collect();

        // リベース状態を保存
        let rebase_state = RebaseState {
            source_branch: source_branch.clone(),
            target_branch: target_branch_name.to_string(),
            original_head_id: current_head_id,
            target_head_id: target_head_id.clone(),
            remaining_commit_ids: commit_ids,
            conflicting_commit_id: None,
            last_applied_id: None,
        };
        self.save_rebase_state(&rebase_state)?;

        // コミットを順番に再適用する
        self.apply_rebase_commits(rebase_state)
    }

    /// 共通祖先以降のコミットを収集する（first-parentのみ、古い順）
    fn collect_commits_since(
        &self,
        head_id: &str,
        ancestor_id: &str,
    ) -> CalixResult<Vec<Commit>> {
        let mut commits = Vec::new();
        let mut current_id = Some(head_id.to_string());

        while let Some(id) = current_id {
            if id == ancestor_id {
                break;
            }
            let commit = self.load_commit(&id)?;
            current_id = commit.parent_id.clone();
            commits.push(commit);
        }

        commits.reverse();
        Ok(commits)
    }

    /// リベースコミットを順番に適用する
    fn apply_rebase_commits(&mut self, mut rebase_state: RebaseState) -> CalixResult<()> {
        let target_head_id = rebase_state.target_head_id.clone();
        let mut current_base_id = rebase_state
            .last_applied_id
            .clone()
            .unwrap_or(target_head_id);

        while !rebase_state.remaining_commit_ids.is_empty() {
            let commit_id = rebase_state.remaining_commit_ids[0].clone();
            let original_commit = self.load_commit(&commit_id)?;

            // 元コミットの親の状態, 元コミットの状態, 新しいベースの状態を取得
            let ancestor_state = if let Some(ref parent_id) = original_commit.parent_id {
                self.reconstruct_state(parent_id)?
            } else {
                HashMap::new()
            };
            let commit_state = self.reconstruct_state(&commit_id)?;
            let new_base_state = self.reconstruct_state(&current_base_id)?;

            match three_way_merge(&ancestor_state, &new_base_state, &commit_state) {
                Ok(merged_state) => {
                    // 適用成功 → 新しいコミットを作成
                    let new_diff = Diff::from_states(&new_base_state, &merged_state);
                    let new_commit = Self::new_commit(
                        &self.info.id,
                        Some(current_base_id.clone()),
                        None,
                        original_commit.message.clone(),
                        new_diff,
                    );
                    self.write_commit(&new_commit)?;
                    current_base_id = new_commit.id.clone();

                    rebase_state.remaining_commit_ids.remove(0);
                    rebase_state.last_applied_id = Some(new_commit.id.clone());
                    self.save_rebase_state(&rebase_state)?;
                }
                Err(conflicts) => {
                    // コンフリクト → 状態を保存して中断
                    rebase_state.conflicting_commit_id = Some(commit_id.clone());
                    self.save_rebase_state(&rebase_state)?;

                    return Err(CalixError::RebaseConflict {
                        submodule_id: self.info.id.clone(),
                        commit_id,
                        conflicts,
                    });
                }
            }
        }

        // 全コミット適用完了 → ブランチポインタを更新
        self.finalize_rebase(&rebase_state.source_branch, &current_base_id)?;
        self.clear_rebase_state()?;
        Ok(())
    }

    /// リベース継続（コンフリクト解決後）
    pub fn rebase_continue(
        &mut self,
        resolved_state: &HashMap<String, String>,
    ) -> CalixResult<()> {
        let mut rebase_state = self.load_rebase_state()?;

        let conflicting_id = rebase_state
            .conflicting_commit_id
            .take()
            .ok_or(CalixError::RebaseNotInProgress)?;

        let original_commit = self.load_commit(&conflicting_id)?;

        // 現在のベース状態を取得
        let base_id = rebase_state
            .last_applied_id
            .clone()
            .unwrap_or_else(|| rebase_state.target_head_id.clone());
        let base_state = self.reconstruct_state(&base_id)?;

        // 解決済み状態から新しいコミットを作成
        let new_diff = Diff::from_states(&base_state, resolved_state);
        let new_commit = Self::new_commit(
            &self.info.id,
            Some(base_id),
            None,
            original_commit.message.clone(),
            new_diff,
        );
        self.write_commit(&new_commit)?;

        // 適用済みとして更新
        rebase_state.remaining_commit_ids.remove(0);
        rebase_state.last_applied_id = Some(new_commit.id.clone());
        self.save_rebase_state(&rebase_state)?;

        // 残りのコミットを適用
        self.apply_rebase_commits(rebase_state)
    }

    /// リベースを中断し、元の状態に戻す
    pub fn rebase_abort(&mut self) -> CalixResult<()> {
        let rebase_state = self.load_rebase_state()?;

        // ブランチポインタをリベース開始前に戻す
        let mut branch_store = self.load_branch_store()?;
        branch_store.set_branch_head(
            &rebase_state.source_branch,
            &rebase_state.original_head_id,
        )?;
        branch_store.save(&self.storage_root)?;

        self.info.head_commit_id = Some(rebase_state.original_head_id.clone());
        self.save_info()?;

        self.clear_rebase_state()?;
        Ok(())
    }

    /// リベース中かどうかを確認する
    pub fn is_rebasing(&self) -> CalixResult<bool> {
        let path = self.storage_root.join("rebase_state.msgpack");
        Ok(path.exists())
    }

    /// リベース完了処理（ブランチ更新）
    fn finalize_rebase(&mut self, branch_name: &str, new_head_id: &str) -> CalixResult<()> {
        let mut branch_store = self.load_branch_store()?;
        branch_store.set_branch_head(branch_name, new_head_id)?;
        branch_store.save(&self.storage_root)?;

        self.info.head_commit_id = Some(new_head_id.to_string());
        self.save_info()?;
        Ok(())
    }

    fn save_rebase_state(&self, state: &RebaseState) -> CalixResult<()> {
        let path = self.storage_root.join("rebase_state.msgpack");
        storage::write_msgpack(&path, state)
    }

    fn load_rebase_state(&self) -> CalixResult<RebaseState> {
        let path = self.storage_root.join("rebase_state.msgpack");
        if !path.exists() {
            return Err(CalixError::RebaseNotInProgress);
        }
        storage::read_msgpack(&path)
    }

    fn clear_rebase_state(&self) -> CalixResult<()> {
        let path = self.storage_root.join("rebase_state.msgpack");
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    /// リベースのコミットID対応表を取得する（グローバルタイムライン記録用）
    pub fn get_rebase_commit_mapping(
        &self,
        original_head_id: &str,
        new_head_id: &str,
        ancestor_id: &str,
    ) -> CalixResult<HashMap<String, String>> {
        let old_commits = self.collect_commits_since(original_head_id, ancestor_id)?;
        let new_commits = self.collect_commits_since(new_head_id, ancestor_id)?;

        let mut mapping = HashMap::new();
        for (old, new) in old_commits.iter().zip(new_commits.iter()) {
            mapping.insert(old.id.clone(), new.id.clone());
        }
        Ok(mapping)
    }
}
