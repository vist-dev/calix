use calix::{
    CalixError, Commit, Diff, GlobalOrder, MergeResult, Repository, SubmoduleKind,
};
use std::collections::HashMap;
use tempfile::tempdir;
use uuid::Uuid;

fn make_commit(
    submodule_id: &str,
    parent_id: Option<String>,
    diff: Diff,
) -> Commit {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    Commit {
        id: Uuid::new_v4().to_string(),
        parent_id,
        second_parent_id: None,
        message: "test commit".to_string(),
        submodule_id: submodule_id.to_string(),
        global_order: GlobalOrder {
            timestamp: now,
            sequence: 0,
        },
        diff,
        created_at: now,
    }
}

// ─── Phase 1-8 既存テスト ───

#[test]
fn test_init_and_register_submodule() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();

    let submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    assert_eq!(submodule.info.current_branch, "main");
    assert!(submodule.info.head_commit_id.is_none());
}

#[test]
fn test_commit_and_reconstruct() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();

    let mut submodule = repo
        .register_submodule(SubmoduleKind::Effect, "effects/blur".to_string())
        .unwrap();

    // 最初の状態
    let mut initial = HashMap::new();
    initial.insert("intensity".to_string(), "0.5".to_string());
    initial.insert("radius".to_string(), "10".to_string());

    let diff1 = Diff::from_states(&HashMap::new(), &initial);
    let commit1 = make_commit(&submodule.info.id, None, diff1);
    let commit1_id = commit1.id.clone();
    submodule.append_commit(commit1).unwrap();

    // 変更
    let mut updated = initial.clone();
    updated.insert("intensity".to_string(), "0.8".to_string());

    let diff2 = Diff::from_states(&initial, &updated);
    let commit2 = make_commit(&submodule.info.id, Some(commit1_id), diff2);
    let commit2_id = commit2.id.clone();
    submodule.append_commit(commit2).unwrap();

    // 状態の再構築
    let state = submodule.reconstruct_state(&commit2_id).unwrap();
    assert_eq!(state.get("intensity").unwrap(), "0.8");
    assert_eq!(state.get("radius").unwrap(), "10");
}

#[test]
fn test_double_init_fails() {
    let dir = tempdir().unwrap();
    Repository::init(dir.path()).unwrap();
    let result = Repository::init(dir.path());
    assert!(result.is_err());
}

#[test]
fn test_open_nonexistent_fails() {
    let dir = tempdir().unwrap();
    let result = Repository::open(dir.path());
    assert!(result.is_err());
}

// ─── Phase 9: ブランチ管理テスト ───

#[test]
fn test_branch_creation_persists() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    // 最初のコミットを追加
    let diff = Diff::from_states(&HashMap::new(), &{
        let mut m = HashMap::new();
        m.insert("key".to_string(), "value".to_string());
        m
    });
    let commit = make_commit(&submodule.info.id, None, diff);
    let commit_id = commit.id.clone();
    submodule.append_commit(commit).unwrap();

    // ブランチ作成
    submodule.create_branch("experiment", &commit_id).unwrap();

    // reload してもブランチ情報が保持されていること
    let submodule_reloaded = repo.load_submodule(&submodule.info.id).unwrap();
    let (branches, current) = submodule_reloaded.list_branches().unwrap();
    assert!(branches.contains(&"experiment".to_string()));
    assert!(branches.contains(&"main".to_string()));
    assert_eq!(current, "main");
}

#[test]
fn test_checkout_nonexistent_branch_fails() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    let result = submodule.checkout("nonexistent");
    assert!(result.is_err());
    match result.unwrap_err() {
        CalixError::BranchNotFound { name } => assert_eq!(name, "nonexistent"),
        e => panic!("Expected BranchNotFound, got {:?}", e),
    }
}

#[test]
fn test_delete_main_branch_fails() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    let result = submodule.delete_branch("main");
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), CalixError::CannotDeleteMainBranch));
}

#[test]
fn test_branch_switch_and_commit() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Effect, "effects/glow".to_string())
        .unwrap();

    // mainブランチに初期コミット
    let mut state1 = HashMap::new();
    state1.insert("brightness".to_string(), "1.0".to_string());
    let diff1 = Diff::from_states(&HashMap::new(), &state1);
    let commit1 = make_commit(&submodule.info.id, None, diff1);
    let commit1_id = commit1.id.clone();
    submodule.append_commit(commit1).unwrap();

    // experimentブランチを作成して切り替え
    submodule.create_branch("experiment", &commit1_id).unwrap();
    submodule.checkout("experiment").unwrap();
    assert_eq!(submodule.info.current_branch, "experiment");

    // experimentブランチにコミット
    let mut state2 = state1.clone();
    state2.insert("brightness".to_string(), "2.0".to_string());
    let diff2 = Diff::from_states(&state1, &state2);
    let commit2 = make_commit(&submodule.info.id, Some(commit1_id.clone()), diff2);
    let commit2_id = commit2.id.clone();
    submodule.append_commit(commit2).unwrap();

    // experimentブランチの状態を確認
    let state = submodule.reconstruct_state(&commit2_id).unwrap();
    assert_eq!(state.get("brightness").unwrap(), "2.0");

    // mainに戻ると、mainのHEADはcommit1のまま
    submodule.checkout("main").unwrap();
    assert_eq!(submodule.info.head_commit_id.as_deref(), Some(commit1_id.as_str()));
}

// ─── Phase 10: マージテスト ───

#[test]
fn test_auto_merge_no_conflict() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    // 共通の初期コミット
    let mut initial = HashMap::new();
    initial.insert("x".to_string(), "1".to_string());
    let diff0 = Diff::from_states(&HashMap::new(), &initial);
    let commit0 = make_commit(&submodule.info.id, None, diff0);
    let commit0_id = commit0.id.clone();
    submodule.append_commit(commit0).unwrap();

    // feature ブランチ作成
    submodule.create_branch("feature", &commit0_id).unwrap();

    // main にコミット（yを追加）
    let mut main_state = initial.clone();
    main_state.insert("y".to_string(), "2".to_string());
    let diff_main = Diff::from_states(&initial, &main_state);
    let commit_main = make_commit(&submodule.info.id, Some(commit0_id.clone()), diff_main);
    submodule.append_commit(commit_main).unwrap();

    // feature にコミット（zを追加）
    submodule.checkout("feature").unwrap();
    let mut feature_state = initial.clone();
    feature_state.insert("z".to_string(), "3".to_string());
    let diff_feature = Diff::from_states(&initial, &feature_state);
    let commit_feature = make_commit(&submodule.info.id, Some(commit0_id), diff_feature);
    submodule.append_commit(commit_feature).unwrap();

    // main に戻ってマージ
    submodule.checkout("main").unwrap();
    let result = submodule.merge("feature").unwrap();

    match result {
        MergeResult::Merged { ref commit } => {
            // マージコミットからの状態再構築
            let state = submodule.reconstruct_state(&commit.id).unwrap();
            assert_eq!(state.get("x").unwrap(), "1");
            assert_eq!(state.get("y").unwrap(), "2");
            assert_eq!(state.get("z").unwrap(), "3");
            // second_parent_id が設定されていること
            assert!(commit.second_parent_id.is_some());
        }
        MergeResult::FastForward { .. } => panic!("Expected Merged, got FastForward"),
    }
}

#[test]
fn test_merge_conflict_detection() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    // 共通の初期コミット
    let mut initial = HashMap::new();
    initial.insert("x".to_string(), "1".to_string());
    let diff0 = Diff::from_states(&HashMap::new(), &initial);
    let commit0 = make_commit(&submodule.info.id, None, diff0);
    let commit0_id = commit0.id.clone();
    submodule.append_commit(commit0).unwrap();

    // feature ブランチ作成
    submodule.create_branch("feature", &commit0_id).unwrap();

    // main で x を "2" に変更
    let mut main_state = initial.clone();
    main_state.insert("x".to_string(), "2".to_string());
    let diff_main = Diff::from_states(&initial, &main_state);
    let commit_main = make_commit(&submodule.info.id, Some(commit0_id.clone()), diff_main);
    submodule.append_commit(commit_main).unwrap();

    // feature で x を "3" に変更
    submodule.checkout("feature").unwrap();
    let mut feature_state = initial.clone();
    feature_state.insert("x".to_string(), "3".to_string());
    let diff_feature = Diff::from_states(&initial, &feature_state);
    let commit_feature = make_commit(&submodule.info.id, Some(commit0_id), diff_feature);
    submodule.append_commit(commit_feature).unwrap();

    // main に戻ってマージ → コンフリクト
    submodule.checkout("main").unwrap();
    let result = submodule.merge("feature");
    assert!(result.is_err());

    match result.unwrap_err() {
        CalixError::MergeConflict {
            submodule_id: _,
            conflicts,
        } => {
            assert_eq!(conflicts.len(), 1);
            assert_eq!(conflicts[0].key, "x");
            assert_eq!(conflicts[0].base_value.as_deref(), Some("1"));
            assert_eq!(conflicts[0].current_value.as_deref(), Some("2"));
            assert_eq!(conflicts[0].incoming_value.as_deref(), Some("3"));
        }
        e => panic!("Expected MergeConflict, got {:?}", e),
    }
}

#[test]
fn test_merge_conflict_resolution() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    // 共通の初期コミット
    let mut initial = HashMap::new();
    initial.insert("x".to_string(), "1".to_string());
    let diff0 = Diff::from_states(&HashMap::new(), &initial);
    let commit0 = make_commit(&submodule.info.id, None, diff0);
    let commit0_id = commit0.id.clone();
    submodule.append_commit(commit0).unwrap();

    // feature ブランチ
    submodule.create_branch("feature", &commit0_id).unwrap();

    // main で x = "2"
    let mut main_state = initial.clone();
    main_state.insert("x".to_string(), "2".to_string());
    let diff_main = Diff::from_states(&initial, &main_state);
    let commit_main = make_commit(&submodule.info.id, Some(commit0_id.clone()), diff_main);
    submodule.append_commit(commit_main).unwrap();

    // feature で x = "3"
    submodule.checkout("feature").unwrap();
    let mut feature_state = initial.clone();
    feature_state.insert("x".to_string(), "3".to_string());
    let diff_feature = Diff::from_states(&initial, &feature_state);
    let commit_feature = make_commit(&submodule.info.id, Some(commit0_id), diff_feature);
    submodule.append_commit(commit_feature).unwrap();

    // main に戻ってマージ → コンフリクト
    submodule.checkout("main").unwrap();
    let _err = submodule.merge("feature").unwrap_err();

    // 解決: x = "resolved"
    let mut resolved = HashMap::new();
    resolved.insert("x".to_string(), "resolved".to_string());
    let merge_commit = submodule.resolve_conflict(&resolved).unwrap();

    // マージコミットから状態を再構築
    let state = submodule.reconstruct_state(&merge_commit.id).unwrap();
    assert_eq!(state.get("x").unwrap(), "resolved");
    assert!(merge_commit.second_parent_id.is_some());
}

#[test]
fn test_fast_forward_merge() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    // 初期コミット
    let mut initial = HashMap::new();
    initial.insert("x".to_string(), "1".to_string());
    let diff0 = Diff::from_states(&HashMap::new(), &initial);
    let commit0 = make_commit(&submodule.info.id, None, diff0);
    let commit0_id = commit0.id.clone();
    submodule.append_commit(commit0).unwrap();

    // feature ブランチでコミット追加
    submodule.create_branch("feature", &commit0_id).unwrap();
    submodule.checkout("feature").unwrap();

    let mut feature_state = initial.clone();
    feature_state.insert("y".to_string(), "2".to_string());
    let diff_feature = Diff::from_states(&initial, &feature_state);
    let commit_feature = make_commit(&submodule.info.id, Some(commit0_id), diff_feature);
    let feature_commit_id = commit_feature.id.clone();
    submodule.append_commit(commit_feature).unwrap();

    // mainに戻ってFF merge
    submodule.checkout("main").unwrap();
    let result = submodule.merge("feature").unwrap();

    match result {
        MergeResult::FastForward { new_head_id } => {
            assert_eq!(new_head_id, feature_commit_id);
        }
        MergeResult::Merged { .. } => panic!("Expected FastForward, got Merged"),
    }
}

#[test]
fn test_merge_commit_state_reconstruction() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    // 初期: x=1
    let mut initial = HashMap::new();
    initial.insert("x".to_string(), "1".to_string());
    let diff0 = Diff::from_states(&HashMap::new(), &initial);
    let commit0 = make_commit(&submodule.info.id, None, diff0);
    let commit0_id = commit0.id.clone();
    submodule.append_commit(commit0).unwrap();

    submodule.create_branch("feature", &commit0_id).unwrap();

    // main: y=2
    let mut main_state = initial.clone();
    main_state.insert("y".to_string(), "2".to_string());
    let diff_main = Diff::from_states(&initial, &main_state);
    let commit_main = make_commit(&submodule.info.id, Some(commit0_id.clone()), diff_main);
    submodule.append_commit(commit_main).unwrap();

    // feature: z=3
    submodule.checkout("feature").unwrap();
    let mut feature_state = initial.clone();
    feature_state.insert("z".to_string(), "3".to_string());
    let diff_feature = Diff::from_states(&initial, &feature_state);
    let commit_feature = make_commit(&submodule.info.id, Some(commit0_id), diff_feature);
    submodule.append_commit(commit_feature).unwrap();

    // merge into main
    submodule.checkout("main").unwrap();
    let result = submodule.merge("feature").unwrap();

    if let MergeResult::Merged { commit } = result {
        let state = submodule.reconstruct_state(&commit.id).unwrap();
        assert_eq!(state.len(), 3);
        assert_eq!(state["x"], "1");
        assert_eq!(state["y"], "2");
        assert_eq!(state["z"], "3");
    } else {
        panic!("Expected Merged");
    }
}

// ─── Phase 11: リベーステスト ───

#[test]
fn test_rebase_no_conflict() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    // 初期: x=1
    let mut initial = HashMap::new();
    initial.insert("x".to_string(), "1".to_string());
    let diff0 = Diff::from_states(&HashMap::new(), &initial);
    let commit0 = make_commit(&submodule.info.id, None, diff0);
    let commit0_id = commit0.id.clone();
    submodule.append_commit(commit0).unwrap();

    // mainブランチ: y=2
    submodule.create_branch("feature", &commit0_id).unwrap();
    let mut main_state = initial.clone();
    main_state.insert("y".to_string(), "2".to_string());
    let diff_main = Diff::from_states(&initial, &main_state);
    let commit_main = make_commit(&submodule.info.id, Some(commit0_id.clone()), diff_main);
    submodule.append_commit(commit_main).unwrap();

    // featureブランチ: z=3
    submodule.checkout("feature").unwrap();
    let mut feature_state = initial.clone();
    feature_state.insert("z".to_string(), "3".to_string());
    let diff_feature = Diff::from_states(&initial, &feature_state);
    let commit_feature = make_commit(&submodule.info.id, Some(commit0_id), diff_feature);
    let original_feature_commit_id = commit_feature.id.clone();
    submodule.append_commit(commit_feature).unwrap();

    // feature を main にリベース
    submodule.rebase("main").unwrap();

    // リベース後: featureのHEADは新しいコミットID
    let new_head_id = submodule.info.head_commit_id.clone().unwrap();
    assert_ne!(new_head_id, original_feature_commit_id);

    // リベース後の状態再構築が正しいこと
    let state = submodule.reconstruct_state(&new_head_id).unwrap();
    assert_eq!(state["x"], "1");
    assert_eq!(state["y"], "2");
    assert_eq!(state["z"], "3");

    // リベース中ではないこと
    assert!(!submodule.is_rebasing().unwrap());
}

#[test]
fn test_rebase_commit_ids_change() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    // 初期
    let diff0 = Diff::from_states(&HashMap::new(), &{
        let mut m = HashMap::new();
        m.insert("x".to_string(), "1".to_string());
        m
    });
    let commit0 = make_commit(&submodule.info.id, None, diff0);
    let commit0_id = commit0.id.clone();
    submodule.append_commit(commit0).unwrap();

    submodule.create_branch("feature", &commit0_id).unwrap();

    // main にコミット
    let diff_main = Diff::from_states(
        &{
            let mut m = HashMap::new();
            m.insert("x".to_string(), "1".to_string());
            m
        },
        &{
            let mut m = HashMap::new();
            m.insert("x".to_string(), "1".to_string());
            m.insert("y".to_string(), "2".to_string());
            m
        },
    );
    let commit_main = make_commit(&submodule.info.id, Some(commit0_id.clone()), diff_main);
    submodule.append_commit(commit_main).unwrap();

    // feature にコミット
    submodule.checkout("feature").unwrap();
    let diff_feature = Diff::from_states(
        &{
            let mut m = HashMap::new();
            m.insert("x".to_string(), "1".to_string());
            m
        },
        &{
            let mut m = HashMap::new();
            m.insert("x".to_string(), "1".to_string());
            m.insert("z".to_string(), "3".to_string());
            m
        },
    );
    let commit_feature = make_commit(&submodule.info.id, Some(commit0_id), diff_feature);
    let old_id = commit_feature.id.clone();
    submodule.append_commit(commit_feature).unwrap();

    // rebase
    submodule.rebase("main").unwrap();
    let new_id = submodule.info.head_commit_id.clone().unwrap();

    // IDが変わっていること
    assert_ne!(old_id, new_id);
}

#[test]
fn test_rebase_conflict_and_continue() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    // 初期: x=1
    let mut initial = HashMap::new();
    initial.insert("x".to_string(), "1".to_string());
    let diff0 = Diff::from_states(&HashMap::new(), &initial);
    let commit0 = make_commit(&submodule.info.id, None, diff0);
    let commit0_id = commit0.id.clone();
    submodule.append_commit(commit0).unwrap();

    submodule.create_branch("feature", &commit0_id).unwrap();

    // main: x=2
    let mut main_state = initial.clone();
    main_state.insert("x".to_string(), "2".to_string());
    let diff_main = Diff::from_states(&initial, &main_state);
    let commit_main = make_commit(&submodule.info.id, Some(commit0_id.clone()), diff_main);
    submodule.append_commit(commit_main).unwrap();

    // feature: x=3
    submodule.checkout("feature").unwrap();
    let mut feature_state = initial.clone();
    feature_state.insert("x".to_string(), "3".to_string());
    let diff_feature = Diff::from_states(&initial, &feature_state);
    let commit_feature = make_commit(&submodule.info.id, Some(commit0_id), diff_feature);
    submodule.append_commit(commit_feature).unwrap();

    // rebase → コンフリクト
    let result = submodule.rebase("main");
    assert!(result.is_err());
    assert!(submodule.is_rebasing().unwrap());

    match result.unwrap_err() {
        CalixError::RebaseConflict {
            conflicts, ..
        } => {
            assert_eq!(conflicts.len(), 1);
            assert_eq!(conflicts[0].key, "x");
        }
        e => panic!("Expected RebaseConflict, got {:?}", e),
    }

    // 解決して続行
    let mut resolved = HashMap::new();
    resolved.insert("x".to_string(), "resolved".to_string());
    submodule.rebase_continue(&resolved).unwrap();

    // リベース完了
    assert!(!submodule.is_rebasing().unwrap());

    // 正しい最終状態
    let head_id = submodule.info.head_commit_id.clone().unwrap();
    let state = submodule.reconstruct_state(&head_id).unwrap();
    assert_eq!(state["x"], "resolved");
}

#[test]
fn test_rebase_abort() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    // 初期: x=1
    let mut initial = HashMap::new();
    initial.insert("x".to_string(), "1".to_string());
    let diff0 = Diff::from_states(&HashMap::new(), &initial);
    let commit0 = make_commit(&submodule.info.id, None, diff0);
    let commit0_id = commit0.id.clone();
    submodule.append_commit(commit0).unwrap();

    submodule.create_branch("feature", &commit0_id).unwrap();

    // main: x=2
    let mut main_state = initial.clone();
    main_state.insert("x".to_string(), "2".to_string());
    let diff_main = Diff::from_states(&initial, &main_state);
    let commit_main = make_commit(&submodule.info.id, Some(commit0_id.clone()), diff_main);
    submodule.append_commit(commit_main).unwrap();

    // feature: x=3
    submodule.checkout("feature").unwrap();
    let mut feature_state = initial.clone();
    feature_state.insert("x".to_string(), "3".to_string());
    let diff_feature = Diff::from_states(&initial, &feature_state);
    let commit_feature = make_commit(&submodule.info.id, Some(commit0_id), diff_feature);
    let original_head = commit_feature.id.clone();
    submodule.append_commit(commit_feature).unwrap();

    // rebase → コンフリクト
    let _err = submodule.rebase("main").unwrap_err();
    assert!(submodule.is_rebasing().unwrap());

    // abort
    submodule.rebase_abort().unwrap();

    // 元の状態に戻っていること
    assert!(!submodule.is_rebasing().unwrap());
    assert_eq!(submodule.info.head_commit_id.as_deref(), Some(original_head.as_str()));

    // 状態も元通り
    let state = submodule.reconstruct_state(&original_head).unwrap();
    assert_eq!(state["x"], "3");
}

#[test]
fn test_rebase_state_reconstruction() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    // 初期: a=1
    let mut initial = HashMap::new();
    initial.insert("a".to_string(), "1".to_string());
    let diff0 = Diff::from_states(&HashMap::new(), &initial);
    let commit0 = make_commit(&submodule.info.id, None, diff0);
    let commit0_id = commit0.id.clone();
    submodule.append_commit(commit0).unwrap();

    submodule.create_branch("feature", &commit0_id).unwrap();

    // main: b=2
    let mut main_s = initial.clone();
    main_s.insert("b".to_string(), "2".to_string());
    let diff_m = Diff::from_states(&initial, &main_s);
    let cm = make_commit(&submodule.info.id, Some(commit0_id.clone()), diff_m);
    submodule.append_commit(cm).unwrap();

    // feature: c=3, d=4 (2 commits)
    submodule.checkout("feature").unwrap();
    let mut fs1 = initial.clone();
    fs1.insert("c".to_string(), "3".to_string());
    let df1 = Diff::from_states(&initial, &fs1);
    let cf1 = make_commit(&submodule.info.id, Some(commit0_id), df1);
    let cf1_id = cf1.id.clone();
    submodule.append_commit(cf1).unwrap();

    let mut fs2 = fs1.clone();
    fs2.insert("d".to_string(), "4".to_string());
    let df2 = Diff::from_states(&fs1, &fs2);
    let cf2 = make_commit(&submodule.info.id, Some(cf1_id), df2);
    submodule.append_commit(cf2).unwrap();

    // rebase
    submodule.rebase("main").unwrap();

    let head_id = submodule.info.head_commit_id.clone().unwrap();
    let state = submodule.reconstruct_state(&head_id).unwrap();
    assert_eq!(state.len(), 4);
    assert_eq!(state["a"], "1");
    assert_eq!(state["b"], "2");
    assert_eq!(state["c"], "3");
    assert_eq!(state["d"], "4");
}

// ─── Phase 12: Global timeline テスト ───

#[test]
fn test_global_timeline_event_kinds() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();

    repo.record_global_event(
        "sub1".to_string(),
        "commit1".to_string(),
        calix::TimelineEventKind::Commit,
    )
    .unwrap();

    repo.record_merge_event(
        "sub1".to_string(),
        "merge1".to_string(),
        "parent2".to_string(),
    )
    .unwrap();

    let mut mapping = HashMap::new();
    mapping.insert("old1".to_string(), "new1".to_string());
    repo.record_rebase_event("sub1".to_string(), "head1".to_string(), mapping)
        .unwrap();

    assert_eq!(repo.state.timeline.len(), 3);
    assert_eq!(repo.state.global_sequence, 3);

    // Verify event kinds
    assert!(matches!(
        repo.state.timeline[0].event_kind,
        calix::TimelineEventKind::Commit
    ));
    assert!(matches!(
        repo.state.timeline[1].event_kind,
        calix::TimelineEventKind::Merge { .. }
    ));
    assert!(matches!(
        repo.state.timeline[2].event_kind,
        calix::TimelineEventKind::Rebase { .. }
    ));
}

// ─── Phase 13: 複雑なマージテスト ───

/// 複数キーが同時にコンフリクトするケース
#[test]
fn test_merge_multiple_key_conflicts() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    // 初期: x=1, y=10, z=100
    let mut initial = HashMap::new();
    initial.insert("x".to_string(), "1".to_string());
    initial.insert("y".to_string(), "10".to_string());
    initial.insert("z".to_string(), "100".to_string());
    let diff0 = Diff::from_states(&HashMap::new(), &initial);
    let commit0 = make_commit(&submodule.info.id, None, diff0);
    let commit0_id = commit0.id.clone();
    submodule.append_commit(commit0).unwrap();

    submodule.create_branch("feature", &commit0_id).unwrap();

    // main: x=2, y=20 (zはそのまま)
    let mut main_state = initial.clone();
    main_state.insert("x".to_string(), "2".to_string());
    main_state.insert("y".to_string(), "20".to_string());
    let diff_main = Diff::from_states(&initial, &main_state);
    let commit_main = make_commit(&submodule.info.id, Some(commit0_id.clone()), diff_main);
    submodule.append_commit(commit_main).unwrap();

    // feature: x=3, y=30, z=300
    submodule.checkout("feature").unwrap();
    let mut feature_state = initial.clone();
    feature_state.insert("x".to_string(), "3".to_string());
    feature_state.insert("y".to_string(), "30".to_string());
    feature_state.insert("z".to_string(), "300".to_string());
    let diff_feature = Diff::from_states(&initial, &feature_state);
    let commit_feature = make_commit(&submodule.info.id, Some(commit0_id), diff_feature);
    submodule.append_commit(commit_feature).unwrap();

    // main に戻ってマージ → x, y の2つがコンフリクト（zはfeatureだけ変更なのでOK）
    submodule.checkout("main").unwrap();
    let result = submodule.merge("feature");
    assert!(result.is_err());

    match result.unwrap_err() {
        CalixError::MergeConflict { conflicts, .. } => {
            assert_eq!(conflicts.len(), 2);
            let conflict_keys: Vec<&str> = conflicts.iter().map(|c| c.key.as_str()).collect();
            assert!(conflict_keys.contains(&"x"));
            assert!(conflict_keys.contains(&"y"));
        }
        e => panic!("Expected MergeConflict, got {:?}", e),
    }

    // コンフリクト解決: x=resolved_x, y=resolved_y, z=300 (自動マージされるはず)
    let mut resolved = HashMap::new();
    resolved.insert("x".to_string(), "resolved_x".to_string());
    resolved.insert("y".to_string(), "resolved_y".to_string());
    resolved.insert("z".to_string(), "300".to_string());
    let merge_commit = submodule.resolve_conflict(&resolved).unwrap();

    let state = submodule.reconstruct_state(&merge_commit.id).unwrap();
    assert_eq!(state["x"], "resolved_x");
    assert_eq!(state["y"], "resolved_y");
    assert_eq!(state["z"], "300");
}

/// feature1をマージした後にfeature2もマージする（連続マージ）
#[test]
fn test_sequential_merges_from_two_branches() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    // 初期: base=1
    let mut initial = HashMap::new();
    initial.insert("base".to_string(), "1".to_string());
    let diff0 = Diff::from_states(&HashMap::new(), &initial);
    let commit0 = make_commit(&submodule.info.id, None, diff0);
    let commit0_id = commit0.id.clone();
    submodule.append_commit(commit0).unwrap();

    // 2つのブランチを同時に作成
    submodule
        .create_branch("feature1", &commit0_id)
        .unwrap();
    submodule
        .create_branch("feature2", &commit0_id)
        .unwrap();

    // feature1: a=10
    submodule.checkout("feature1").unwrap();
    let mut f1_state = initial.clone();
    f1_state.insert("a".to_string(), "10".to_string());
    let diff_f1 = Diff::from_states(&initial, &f1_state);
    let commit_f1 = make_commit(&submodule.info.id, Some(commit0_id.clone()), diff_f1);
    submodule.append_commit(commit_f1).unwrap();

    // feature2: b=20
    submodule.checkout("feature2").unwrap();
    let mut f2_state = initial.clone();
    f2_state.insert("b".to_string(), "20".to_string());
    let diff_f2 = Diff::from_states(&initial, &f2_state);
    let commit_f2 = make_commit(&submodule.info.id, Some(commit0_id), diff_f2);
    submodule.append_commit(commit_f2).unwrap();

    // mainに戻ってfeature1をマージ
    submodule.checkout("main").unwrap();
    let result1 = submodule.merge("feature1").unwrap();
    let merge1_id = match &result1 {
        MergeResult::FastForward { new_head_id } => new_head_id.clone(),
        MergeResult::Merged { commit } => commit.id.clone(),
    };

    let state1 = submodule.reconstruct_state(&merge1_id).unwrap();
    assert_eq!(state1["base"], "1");
    assert_eq!(state1["a"], "10");

    // さらにfeature2をマージ
    let result2 = submodule.merge("feature2").unwrap();
    match result2 {
        MergeResult::Merged { ref commit } => {
            let state2 = submodule.reconstruct_state(&commit.id).unwrap();
            assert_eq!(state2.len(), 3);
            assert_eq!(state2["base"], "1");
            assert_eq!(state2["a"], "10");
            assert_eq!(state2["b"], "20");
        }
        MergeResult::FastForward { .. } => panic!("Expected Merged for second merge"),
    }
}

/// 削除を含むマージ: 一方が削除、もう一方がそのままのケース（自動解決できる）
#[test]
fn test_merge_with_deletion_auto_resolve() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    // 初期: x=1, y=2, z=3
    let mut initial = HashMap::new();
    initial.insert("x".to_string(), "1".to_string());
    initial.insert("y".to_string(), "2".to_string());
    initial.insert("z".to_string(), "3".to_string());
    let diff0 = Diff::from_states(&HashMap::new(), &initial);
    let commit0 = make_commit(&submodule.info.id, None, diff0);
    let commit0_id = commit0.id.clone();
    submodule.append_commit(commit0).unwrap();

    submodule.create_branch("feature", &commit0_id).unwrap();

    // main: yを削除、wを追加
    let mut main_state = initial.clone();
    main_state.remove("y");
    main_state.insert("w".to_string(), "4".to_string());
    let diff_main = Diff::from_states(&initial, &main_state);
    let commit_main = make_commit(&submodule.info.id, Some(commit0_id.clone()), diff_main);
    submodule.append_commit(commit_main).unwrap();

    // feature: zを変更（yはそのまま）
    submodule.checkout("feature").unwrap();
    let mut feature_state = initial.clone();
    feature_state.insert("z".to_string(), "33".to_string());
    let diff_feature = Diff::from_states(&initial, &feature_state);
    let commit_feature = make_commit(&submodule.info.id, Some(commit0_id), diff_feature);
    submodule.append_commit(commit_feature).unwrap();

    // main に戻ってマージ → 自動解決可能
    submodule.checkout("main").unwrap();
    let result = submodule.merge("feature").unwrap();

    match result {
        MergeResult::Merged { ref commit } => {
            let state = submodule.reconstruct_state(&commit.id).unwrap();
            // x=1 (変更なし), y は main で削除済み, z=33 (featureで変更), w=4 (mainで追加)
            assert_eq!(state["x"], "1");
            assert!(!state.contains_key("y"), "y should be deleted");
            assert_eq!(state["z"], "33");
            assert_eq!(state["w"], "4");
            assert_eq!(state.len(), 3);
        }
        MergeResult::FastForward { .. } => panic!("Expected Merged"),
    }
}

/// 削除 vs 変更のコンフリクト: 一方がキーを削除し、もう一方が同じキーを変更
#[test]
fn test_merge_delete_vs_modify_conflict() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    // 初期: x=1
    let mut initial = HashMap::new();
    initial.insert("x".to_string(), "1".to_string());
    let diff0 = Diff::from_states(&HashMap::new(), &initial);
    let commit0 = make_commit(&submodule.info.id, None, diff0);
    let commit0_id = commit0.id.clone();
    submodule.append_commit(commit0).unwrap();

    submodule.create_branch("feature", &commit0_id).unwrap();

    // main: xを削除
    let main_state: HashMap<String, String> = HashMap::new();
    let diff_main = Diff::from_states(&initial, &main_state);
    let commit_main = make_commit(&submodule.info.id, Some(commit0_id.clone()), diff_main);
    submodule.append_commit(commit_main).unwrap();

    // feature: xを変更
    submodule.checkout("feature").unwrap();
    let mut feature_state = initial.clone();
    feature_state.insert("x".to_string(), "999".to_string());
    let diff_feature = Diff::from_states(&initial, &feature_state);
    let commit_feature = make_commit(&submodule.info.id, Some(commit0_id), diff_feature);
    submodule.append_commit(commit_feature).unwrap();

    // main に戻ってマージ → コンフリクト
    submodule.checkout("main").unwrap();
    let result = submodule.merge("feature");
    assert!(result.is_err());

    match result.unwrap_err() {
        CalixError::MergeConflict { conflicts, .. } => {
            assert_eq!(conflicts.len(), 1);
            assert_eq!(conflicts[0].key, "x");
            assert_eq!(conflicts[0].base_value.as_deref(), Some("1"));
            assert_eq!(conflicts[0].current_value, None); // mainで削除
            assert_eq!(conflicts[0].incoming_value.as_deref(), Some("999")); // featureで変更
        }
        e => panic!("Expected MergeConflict, got {:?}", e),
    }
}

/// マージ後にさらにコミットを積む（マージコミットの上に新しい変更）
#[test]
fn test_commit_after_merge() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    // 初期: x=1
    let mut initial = HashMap::new();
    initial.insert("x".to_string(), "1".to_string());
    let diff0 = Diff::from_states(&HashMap::new(), &initial);
    let commit0 = make_commit(&submodule.info.id, None, diff0);
    let commit0_id = commit0.id.clone();
    submodule.append_commit(commit0).unwrap();

    submodule.create_branch("feature", &commit0_id).unwrap();

    // main: y=2
    let mut main_state = initial.clone();
    main_state.insert("y".to_string(), "2".to_string());
    let diff_main = Diff::from_states(&initial, &main_state);
    let commit_main = make_commit(&submodule.info.id, Some(commit0_id.clone()), diff_main);
    submodule.append_commit(commit_main).unwrap();

    // feature: z=3
    submodule.checkout("feature").unwrap();
    let mut feature_state = initial.clone();
    feature_state.insert("z".to_string(), "3".to_string());
    let diff_feature = Diff::from_states(&initial, &feature_state);
    let commit_feature = make_commit(&submodule.info.id, Some(commit0_id), diff_feature);
    submodule.append_commit(commit_feature).unwrap();

    // main に戻ってマージ
    submodule.checkout("main").unwrap();
    let result = submodule.merge("feature").unwrap();
    let merge_commit_id = match result {
        MergeResult::Merged { ref commit } => commit.id.clone(),
        _ => panic!("Expected Merged"),
    };

    // マージ後にさらにコミットを追加
    let post_merge_state = submodule.reconstruct_state(&merge_commit_id).unwrap();
    let mut new_state = post_merge_state.clone();
    new_state.insert("w".to_string(), "4".to_string());
    let diff_post = Diff::from_states(&post_merge_state, &new_state);
    let commit_post = make_commit(&submodule.info.id, Some(merge_commit_id), diff_post);
    let post_id = commit_post.id.clone();
    submodule.append_commit(commit_post).unwrap();

    // 最新状態の検証
    let final_state = submodule.reconstruct_state(&post_id).unwrap();
    assert_eq!(final_state.len(), 4);
    assert_eq!(final_state["x"], "1");
    assert_eq!(final_state["y"], "2");
    assert_eq!(final_state["z"], "3");
    assert_eq!(final_state["w"], "4");
}

// ─── Phase 14: 複雑なリベーステスト ───

/// リベースで複数コミットの途中でコンフリクトが発生し、解決後に残りが正常に適用されるケース
#[test]
fn test_rebase_multi_commit_conflict_in_middle() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    // 初期: x=1, a=0
    let mut initial = HashMap::new();
    initial.insert("x".to_string(), "1".to_string());
    initial.insert("a".to_string(), "0".to_string());
    let diff0 = Diff::from_states(&HashMap::new(), &initial);
    let commit0 = make_commit(&submodule.info.id, None, diff0);
    let commit0_id = commit0.id.clone();
    submodule.append_commit(commit0).unwrap();

    submodule.create_branch("feature", &commit0_id).unwrap();

    // main: x=2（コンフリクトの種）
    let mut main_state = initial.clone();
    main_state.insert("x".to_string(), "2".to_string());
    let diff_main = Diff::from_states(&initial, &main_state);
    let commit_main = make_commit(&submodule.info.id, Some(commit0_id.clone()), diff_main);
    submodule.append_commit(commit_main).unwrap();

    // feature: 3コミット
    // commit1: a=1 (コンフリクトなし)
    // commit2: x=99 (コンフリクト！)
    // commit3: b=new (コンフリクトなし)
    submodule.checkout("feature").unwrap();

    let mut fs1 = initial.clone();
    fs1.insert("a".to_string(), "1".to_string());
    let df1 = Diff::from_states(&initial, &fs1);
    let cf1 = make_commit(&submodule.info.id, Some(commit0_id), df1);
    let cf1_id = cf1.id.clone();
    submodule.append_commit(cf1).unwrap();

    let mut fs2 = fs1.clone();
    fs2.insert("x".to_string(), "99".to_string());
    let df2 = Diff::from_states(&fs1, &fs2);
    let cf2 = make_commit(&submodule.info.id, Some(cf1_id.clone()), df2);
    let cf2_id = cf2.id.clone();
    submodule.append_commit(cf2).unwrap();

    let mut fs3 = fs2.clone();
    fs3.insert("b".to_string(), "new".to_string());
    let df3 = Diff::from_states(&fs2, &fs3);
    let cf3 = make_commit(&submodule.info.id, Some(cf2_id.clone()), df3);
    submodule.append_commit(cf3).unwrap();

    // rebase → commit2でコンフリクト
    let result = submodule.rebase("main");
    assert!(result.is_err());
    assert!(submodule.is_rebasing().unwrap());

    match result.unwrap_err() {
        CalixError::RebaseConflict {
            commit_id,
            conflicts,
            ..
        } => {
            assert_eq!(commit_id, cf2_id);
            assert_eq!(conflicts.len(), 1);
            assert_eq!(conflicts[0].key, "x");
        }
        e => panic!("Expected RebaseConflict, got {:?}", e),
    }

    // 解決して続行（x=merged, a=1 は既に適用済み）
    let mut resolved = HashMap::new();
    resolved.insert("x".to_string(), "merged".to_string());
    resolved.insert("a".to_string(), "1".to_string());
    submodule.rebase_continue(&resolved).unwrap();

    // リベース完了
    assert!(!submodule.is_rebasing().unwrap());

    // 最終状態: x=merged, a=1, b=new
    let head_id = submodule.info.head_commit_id.clone().unwrap();
    let state = submodule.reconstruct_state(&head_id).unwrap();
    assert_eq!(state["x"], "merged");
    assert_eq!(state["a"], "1");
    assert_eq!(state["b"], "new");
}

/// リベース中に再度リベースを開始しようとすると失敗する
#[test]
fn test_rebase_in_progress_guard() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    // 初期: x=1
    let mut initial = HashMap::new();
    initial.insert("x".to_string(), "1".to_string());
    let diff0 = Diff::from_states(&HashMap::new(), &initial);
    let commit0 = make_commit(&submodule.info.id, None, diff0);
    let commit0_id = commit0.id.clone();
    submodule.append_commit(commit0).unwrap();

    submodule.create_branch("feature", &commit0_id).unwrap();

    // main: x=2
    let mut main_state = initial.clone();
    main_state.insert("x".to_string(), "2".to_string());
    let diff_main = Diff::from_states(&initial, &main_state);
    let commit_main = make_commit(&submodule.info.id, Some(commit0_id.clone()), diff_main);
    submodule.append_commit(commit_main).unwrap();

    // feature: x=3
    submodule.checkout("feature").unwrap();
    let mut feature_state = initial.clone();
    feature_state.insert("x".to_string(), "3".to_string());
    let diff_feature = Diff::from_states(&initial, &feature_state);
    let commit_feature = make_commit(&submodule.info.id, Some(commit0_id), diff_feature);
    submodule.append_commit(commit_feature).unwrap();

    // rebase → コンフリクト
    let _err = submodule.rebase("main").unwrap_err();
    assert!(submodule.is_rebasing().unwrap());

    // 再度リベースを開始しようとする → RebaseInProgress
    let result = submodule.rebase("main");
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), CalixError::RebaseInProgress));
}

/// リベース中でないのにcontinueしようとすると失敗する
#[test]
fn test_rebase_continue_without_rebase_fails() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    let resolved = HashMap::new();
    let result = submodule.rebase_continue(&resolved);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        CalixError::RebaseNotInProgress
    ));
}

/// リベース中でないのにabortしようとすると失敗する
#[test]
fn test_rebase_abort_without_rebase_fails() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    let result = submodule.rebase_abort();
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        CalixError::RebaseNotInProgress
    ));
}

/// リベース後にさらにコミットを追加し、再度mainからの変更をリベース
#[test]
fn test_rebase_then_commit_then_rebase_again() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    // 初期: x=1
    let mut initial = HashMap::new();
    initial.insert("x".to_string(), "1".to_string());
    let diff0 = Diff::from_states(&HashMap::new(), &initial);
    let commit0 = make_commit(&submodule.info.id, None, diff0);
    let commit0_id = commit0.id.clone();
    submodule.append_commit(commit0).unwrap();

    submodule.create_branch("feature", &commit0_id).unwrap();

    // main: y=2
    let mut main_state = initial.clone();
    main_state.insert("y".to_string(), "2".to_string());
    let diff_main = Diff::from_states(&initial, &main_state);
    let commit_main = make_commit(&submodule.info.id, Some(commit0_id.clone()), diff_main);
    let main_head1 = commit_main.id.clone();
    submodule.append_commit(commit_main).unwrap();

    // feature: z=3
    submodule.checkout("feature").unwrap();
    let mut feature_state = initial.clone();
    feature_state.insert("z".to_string(), "3".to_string());
    let diff_feature = Diff::from_states(&initial, &feature_state);
    let commit_feature = make_commit(&submodule.info.id, Some(commit0_id), diff_feature);
    submodule.append_commit(commit_feature).unwrap();

    // 1回目のリベース
    submodule.rebase("main").unwrap();
    let head_after_rebase1 = submodule.info.head_commit_id.clone().unwrap();
    let state1 = submodule.reconstruct_state(&head_after_rebase1).unwrap();
    assert_eq!(state1["x"], "1");
    assert_eq!(state1["y"], "2");
    assert_eq!(state1["z"], "3");

    // featureにさらにコミット追加: w=4
    let mut new_state = state1.clone();
    new_state.insert("w".to_string(), "4".to_string());
    let diff_new = Diff::from_states(&state1, &new_state);
    let commit_new = make_commit(
        &submodule.info.id,
        Some(head_after_rebase1),
        diff_new,
    );
    submodule.append_commit(commit_new).unwrap();

    // mainにもさらにコミット追加: v=5
    submodule.checkout("main").unwrap();
    let main_state_current = submodule.reconstruct_state(&main_head1).unwrap();
    let mut main_state2 = main_state_current.clone();
    main_state2.insert("v".to_string(), "5".to_string());
    let diff_main2 = Diff::from_states(&main_state_current, &main_state2);
    let commit_main2 = make_commit(&submodule.info.id, Some(main_head1), diff_main2);
    submodule.append_commit(commit_main2).unwrap();

    // 2回目のリベース
    submodule.checkout("feature").unwrap();
    submodule.rebase("main").unwrap();
    let final_head = submodule.info.head_commit_id.clone().unwrap();
    let final_state = submodule.reconstruct_state(&final_head).unwrap();
    assert_eq!(final_state["x"], "1");
    assert_eq!(final_state["y"], "2");
    assert_eq!(final_state["z"], "3");
    assert_eq!(final_state["w"], "4");
    assert_eq!(final_state["v"], "5");
    assert_eq!(final_state.len(), 5);
}

// ─── Phase 15: エッジケースとエラー処理テスト ───

/// 既に存在するブランチ名で作成を試みると失敗する
#[test]
fn test_create_duplicate_branch_fails() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    let mut initial = HashMap::new();
    initial.insert("x".to_string(), "1".to_string());
    let diff0 = Diff::from_states(&HashMap::new(), &initial);
    let commit0 = make_commit(&submodule.info.id, None, diff0);
    let commit0_id = commit0.id.clone();
    submodule.append_commit(commit0).unwrap();

    submodule.create_branch("feature", &commit0_id).unwrap();
    let result = submodule.create_branch("feature", &commit0_id);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        CalixError::BranchAlreadyExists { .. }
    ));
}

/// 現在チェックアウト中のブランチを削除しようとすると失敗する
#[test]
fn test_delete_current_branch_fails() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    let mut initial = HashMap::new();
    initial.insert("x".to_string(), "1".to_string());
    let diff0 = Diff::from_states(&HashMap::new(), &initial);
    let commit0 = make_commit(&submodule.info.id, None, diff0);
    let commit0_id = commit0.id.clone();
    submodule.append_commit(commit0).unwrap();

    submodule.create_branch("feature", &commit0_id).unwrap();
    submodule.checkout("feature").unwrap();

    let result = submodule.delete_branch("feature");
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        CalixError::CannotDeleteCurrentBranch { .. }
    ));
}

/// 存在しないブランチを削除しようとすると失敗する
#[test]
fn test_delete_nonexistent_branch_fails() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    let result = submodule.delete_branch("ghost");
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        CalixError::BranchNotFound { .. }
    ));
}

/// マージ中でないのにresolve_conflictを呼ぶと失敗する
#[test]
fn test_resolve_conflict_without_merge_fails() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    let resolved = HashMap::new();
    let result = submodule.resolve_conflict(&resolved);
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        CalixError::MergeNotInProgress
    ));
}

/// 長いコミットチェーンからの状態再構築が正しく動作する
#[test]
fn test_long_commit_chain_reconstruction() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Effect, "effects/long".to_string())
        .unwrap();

    let mut state = HashMap::new();
    let mut parent_id: Option<String> = None;

    // 20個のコミットを積む。各コミットでキーを1つ追加
    for i in 0..20 {
        let key = format!("key_{}", i);
        let value = format!("value_{}", i);
        let mut new_state = state.clone();
        new_state.insert(key, value);
        let diff = Diff::from_states(&state, &new_state);
        let commit = make_commit(&submodule.info.id, parent_id.clone(), diff);
        parent_id = Some(commit.id.clone());
        submodule.append_commit(commit).unwrap();
        state = new_state;
    }

    // 最終状態の再構築
    let head_id = parent_id.unwrap();
    let reconstructed = submodule.reconstruct_state(&head_id).unwrap();
    assert_eq!(reconstructed.len(), 20);
    for i in 0..20 {
        assert_eq!(
            reconstructed[&format!("key_{}", i)],
            format!("value_{}", i)
        );
    }
}

/// 途中のコミットで削除があっても状態再構築が正しい
#[test]
fn test_reconstruct_state_with_deletions_in_history() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Effect, "effects/delete".to_string())
        .unwrap();

    // commit1: a=1, b=2, c=3
    let mut s1 = HashMap::new();
    s1.insert("a".to_string(), "1".to_string());
    s1.insert("b".to_string(), "2".to_string());
    s1.insert("c".to_string(), "3".to_string());
    let d1 = Diff::from_states(&HashMap::new(), &s1);
    let c1 = make_commit(&submodule.info.id, None, d1);
    let c1_id = c1.id.clone();
    submodule.append_commit(c1).unwrap();

    // commit2: bを削除
    let mut s2 = s1.clone();
    s2.remove("b");
    let d2 = Diff::from_states(&s1, &s2);
    let c2 = make_commit(&submodule.info.id, Some(c1_id), d2);
    let c2_id = c2.id.clone();
    submodule.append_commit(c2).unwrap();

    // commit3: d=4を追加、aを変更
    let mut s3 = s2.clone();
    s3.insert("d".to_string(), "4".to_string());
    s3.insert("a".to_string(), "updated".to_string());
    let d3 = Diff::from_states(&s2, &s3);
    let c3 = make_commit(&submodule.info.id, Some(c2_id), d3);
    let c3_id = c3.id.clone();
    submodule.append_commit(c3).unwrap();

    // commit4: cを削除、dを変更
    let mut s4 = s3.clone();
    s4.remove("c");
    s4.insert("d".to_string(), "44".to_string());
    let d4 = Diff::from_states(&s3, &s4);
    let c4 = make_commit(&submodule.info.id, Some(c3_id), d4);
    let c4_id = c4.id.clone();
    submodule.append_commit(c4).unwrap();

    let state = submodule.reconstruct_state(&c4_id).unwrap();
    assert_eq!(state.len(), 2); // a, d のみ
    assert_eq!(state["a"], "updated");
    assert_eq!(state["d"], "44");
    assert!(!state.contains_key("b"));
    assert!(!state.contains_key("c"));
}

/// 空のDiffでもコミットが正しく扱える
#[test]
fn test_empty_diff_commit() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Effect, "effects/empty".to_string())
        .unwrap();

    let mut initial = HashMap::new();
    initial.insert("x".to_string(), "1".to_string());
    let d0 = Diff::from_states(&HashMap::new(), &initial);
    let c0 = make_commit(&submodule.info.id, None, d0);
    let c0_id = c0.id.clone();
    submodule.append_commit(c0).unwrap();

    // 同じ状態を「変更」 → 空のDiff
    let d1 = Diff::from_states(&initial, &initial);
    assert!(d1.is_empty());
    let c1 = make_commit(&submodule.info.id, Some(c0_id), d1);
    let c1_id = c1.id.clone();
    submodule.append_commit(c1).unwrap();

    // 空Diffの後でも状態は正しい
    let state = submodule.reconstruct_state(&c1_id).unwrap();
    assert_eq!(state.len(), 1);
    assert_eq!(state["x"], "1");
}

/// リポジトリの永続化: init → register → close → open → load_submodule
#[test]
fn test_repository_persistence_across_open() {
    let dir = tempdir().unwrap();

    // 作成して書き込み
    let submodule_id;
    {
        let mut repo = Repository::init(dir.path()).unwrap();
        let mut submodule = repo
            .register_submodule(SubmoduleKind::Track, "tracks/track_01".to_string())
            .unwrap();
        submodule_id = submodule.info.id.clone();

        let mut initial = HashMap::new();
        initial.insert("volume".to_string(), "0.8".to_string());
        let diff = Diff::from_states(&HashMap::new(), &initial);
        let commit = make_commit(&submodule.info.id, None, diff);
        submodule.append_commit(commit).unwrap();
    }

    // 再度オープンして検証
    let repo = Repository::open(dir.path()).unwrap();
    assert!(repo.state.submodule_index.contains_key(&submodule_id));

    let submodule = repo.load_submodule(&submodule_id).unwrap();
    assert!(submodule.info.head_commit_id.is_some());
    assert_eq!(submodule.info.current_branch, "main");
    assert_eq!(submodule.info.kind, SubmoduleKind::Track);

    let head_id = submodule.info.head_commit_id.clone().unwrap();
    let state = submodule.reconstruct_state(&head_id).unwrap();
    assert_eq!(state["volume"], "0.8");
}

// ─── Phase 16: マルチサブモジュールとグローバルタイムラインテスト ───

/// 複数サブモジュールでのインターリーブされたタイムラインイベント
#[test]
fn test_multi_submodule_interleaved_timeline() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();

    let mut sub_clip = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();
    let mut sub_effect = repo
        .register_submodule(SubmoduleKind::Effect, "effects/blur".to_string())
        .unwrap();
    let mut sub_transition = repo
        .register_submodule(SubmoduleKind::Transition, "transitions/fade".to_string())
        .unwrap();

    // 各サブモジュールにコミットしてタイムラインに記録
    // clip: コミット1
    let mut s1 = HashMap::new();
    s1.insert("duration".to_string(), "10".to_string());
    let d1 = Diff::from_states(&HashMap::new(), &s1);
    let c1 = make_commit(&sub_clip.info.id, None, d1);
    let c1_id = c1.id.clone();
    sub_clip.append_commit(c1).unwrap();
    repo.record_global_event(
        sub_clip.info.id.clone(),
        c1_id.clone(),
        calix::TimelineEventKind::Commit,
    )
    .unwrap();

    // effect: コミット1
    let mut s2 = HashMap::new();
    s2.insert("intensity".to_string(), "0.5".to_string());
    let d2 = Diff::from_states(&HashMap::new(), &s2);
    let c2 = make_commit(&sub_effect.info.id, None, d2);
    let c2_id = c2.id.clone();
    sub_effect.append_commit(c2).unwrap();
    repo.record_global_event(
        sub_effect.info.id.clone(),
        c2_id,
        calix::TimelineEventKind::Commit,
    )
    .unwrap();

    // transition: コミット1
    let mut s3 = HashMap::new();
    s3.insert("type".to_string(), "fade".to_string());
    let d3 = Diff::from_states(&HashMap::new(), &s3);
    let c3 = make_commit(&sub_transition.info.id, None, d3);
    let c3_id = c3.id.clone();
    sub_transition.append_commit(c3).unwrap();
    repo.record_global_event(
        sub_transition.info.id.clone(),
        c3_id,
        calix::TimelineEventKind::Commit,
    )
    .unwrap();

    // clip: コミット2
    let mut s4 = s1.clone();
    s4.insert("duration".to_string(), "20".to_string());
    let d4 = Diff::from_states(&s1, &s4);
    let c4 = make_commit(&sub_clip.info.id, Some(c1_id), d4);
    let c4_id = c4.id.clone();
    sub_clip.append_commit(c4).unwrap();
    repo.record_global_event(
        sub_clip.info.id.clone(),
        c4_id,
        calix::TimelineEventKind::Commit,
    )
    .unwrap();

    assert_eq!(repo.state.timeline.len(), 4);
    assert_eq!(repo.state.global_sequence, 4);

    // タイムラインのサブモジュールIDが正しい順序で記録されている
    assert_eq!(repo.state.timeline[0].submodule_id, sub_clip.info.id);
    assert_eq!(repo.state.timeline[1].submodule_id, sub_effect.info.id);
    assert_eq!(repo.state.timeline[2].submodule_id, sub_transition.info.id);
    assert_eq!(repo.state.timeline[3].submodule_id, sub_clip.info.id);

    // シーケンス番号が単調増加している
    for i in 1..repo.state.timeline.len() {
        assert!(repo.state.timeline[i].sequence > repo.state.timeline[i - 1].sequence);
    }
}

/// 依存関係の順序チェック: 依存関係なしではcheck_dependency_orderingが空を返す
#[test]
fn test_dependency_ordering_no_warnings_without_deps() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();

    let sub_clip = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();
    let sub_effect = repo
        .register_submodule(SubmoduleKind::Effect, "effects/blur".to_string())
        .unwrap();

    // effect のイベントを先に記録
    repo.record_global_event(
        sub_effect.info.id.clone(),
        "effect_commit1".to_string(),
        calix::TimelineEventKind::Commit,
    )
    .unwrap();

    // clip のイベントを後に記録
    repo.record_global_event(
        sub_clip.info.id.clone(),
        "clip_commit1".to_string(),
        calix::TimelineEventKind::Commit,
    )
    .unwrap();

    // 依存関係がセットされていないため、警告は出ない
    let warnings = repo
        .check_dependency_ordering(&sub_effect.info.id)
        .unwrap();
    assert!(warnings.is_empty());
}

/// 依存関係なしのサブモジュールでは警告が出ない
#[test]
fn test_dependency_ordering_no_deps_no_warnings() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();

    let sub = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    repo.record_global_event(
        sub.info.id.clone(),
        "commit1".to_string(),
        calix::TimelineEventKind::Commit,
    )
    .unwrap();

    let warnings = repo.check_dependency_ordering(&sub.info.id).unwrap();
    assert!(warnings.is_empty());
}

/// 複数サブモジュールの独立した操作（各サブモジュールが独自のブランチとコミット履歴を持つ）
#[test]
fn test_multi_submodule_independent_histories() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();

    let mut sub_clip = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();
    let mut sub_effect = repo
        .register_submodule(SubmoduleKind::Effect, "effects/blur".to_string())
        .unwrap();

    // clip: 初期コミット、ブランチ作成、各ブランチにコミット
    let mut clip_initial = HashMap::new();
    clip_initial.insert("start".to_string(), "0".to_string());
    let d_clip = Diff::from_states(&HashMap::new(), &clip_initial);
    let c_clip = make_commit(&sub_clip.info.id, None, d_clip);
    let c_clip_id = c_clip.id.clone();
    sub_clip.append_commit(c_clip).unwrap();
    sub_clip.create_branch("clip_feature", &c_clip_id).unwrap();

    // effect: 初期コミット、ブランチ作成
    let mut effect_initial = HashMap::new();
    effect_initial.insert("strength".to_string(), "low".to_string());
    let d_eff = Diff::from_states(&HashMap::new(), &effect_initial);
    let c_eff = make_commit(&sub_effect.info.id, None, d_eff);
    let c_eff_id = c_eff.id.clone();
    sub_effect.append_commit(c_eff).unwrap();
    sub_effect
        .create_branch("effect_feature", &c_eff_id)
        .unwrap();

    // clip のブランチ操作がeffectに影響しないこと
    sub_clip.checkout("clip_feature").unwrap();
    let mut clip_new = clip_initial.clone();
    clip_new.insert("end".to_string(), "100".to_string());
    let d_clip2 = Diff::from_states(&clip_initial, &clip_new);
    let c_clip2 = make_commit(&sub_clip.info.id, Some(c_clip_id), d_clip2);
    sub_clip.append_commit(c_clip2).unwrap();

    // effect は元の状態のまま
    assert_eq!(sub_effect.info.current_branch, "main");
    let eff_state = sub_effect.reconstruct_state(&c_eff_id).unwrap();
    assert_eq!(eff_state["strength"], "low");
    assert_eq!(eff_state.len(), 1);

    // clip は別のブランチで進んでいる
    assert_eq!(sub_clip.info.current_branch, "clip_feature");
    let clip_head = sub_clip.info.head_commit_id.clone().unwrap();
    let clip_state = sub_clip.reconstruct_state(&clip_head).unwrap();
    assert_eq!(clip_state["start"], "0");
    assert_eq!(clip_state["end"], "100");
}

// ─── Phase 17: 複雑なワークフローテスト ───

/// featureブランチからさらにfeatureブランチを分岐する（ネストブランチ）
#[test]
fn test_nested_feature_branches() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    // 初期: x=1
    let mut initial = HashMap::new();
    initial.insert("x".to_string(), "1".to_string());
    let diff0 = Diff::from_states(&HashMap::new(), &initial);
    let commit0 = make_commit(&submodule.info.id, None, diff0);
    let commit0_id = commit0.id.clone();
    submodule.append_commit(commit0).unwrap();

    // feature1 ブランチ作成
    submodule.create_branch("feature1", &commit0_id).unwrap();
    submodule.checkout("feature1").unwrap();

    // feature1 にコミット: y=2
    let mut f1_state = initial.clone();
    f1_state.insert("y".to_string(), "2".to_string());
    let diff_f1 = Diff::from_states(&initial, &f1_state);
    let commit_f1 = make_commit(&submodule.info.id, Some(commit0_id), diff_f1);
    let commit_f1_id = commit_f1.id.clone();
    submodule.append_commit(commit_f1).unwrap();

    // feature1 から feature2 を分岐
    submodule
        .create_branch("feature2", &commit_f1_id)
        .unwrap();
    submodule.checkout("feature2").unwrap();

    // feature2 にコミット: z=3
    let mut f2_state = f1_state.clone();
    f2_state.insert("z".to_string(), "3".to_string());
    let diff_f2 = Diff::from_states(&f1_state, &f2_state);
    let commit_f2 = make_commit(&submodule.info.id, Some(commit_f1_id.clone()), diff_f2);
    let commit_f2_id = commit_f2.id.clone();
    submodule.append_commit(commit_f2).unwrap();

    // feature2 の状態確認
    let state_f2 = submodule.reconstruct_state(&commit_f2_id).unwrap();
    assert_eq!(state_f2["x"], "1");
    assert_eq!(state_f2["y"], "2");
    assert_eq!(state_f2["z"], "3");

    // feature1 に戻って feature2 をマージ（fast-forward になるはず）
    submodule.checkout("feature1").unwrap();
    let merge_result = submodule.merge("feature2").unwrap();
    match merge_result {
        MergeResult::FastForward { new_head_id } => {
            assert_eq!(new_head_id, commit_f2_id);
        }
        _ => panic!("Expected FastForward"),
    }

    // main に戻って feature1 をマージ（fast-forward になるはず）
    submodule.checkout("main").unwrap();
    let merge_result2 = submodule.merge("feature1").unwrap();
    match merge_result2 {
        MergeResult::FastForward { new_head_id } => {
            let final_state = submodule.reconstruct_state(&new_head_id).unwrap();
            assert_eq!(final_state.len(), 3);
            assert_eq!(final_state["x"], "1");
            assert_eq!(final_state["y"], "2");
            assert_eq!(final_state["z"], "3");
        }
        _ => panic!("Expected FastForward"),
    }
}

/// リベースしてからマージ: featureをmainにリベース後、mainにマージ
#[test]
fn test_rebase_then_merge() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    // 初期: x=1
    let mut initial = HashMap::new();
    initial.insert("x".to_string(), "1".to_string());
    let diff0 = Diff::from_states(&HashMap::new(), &initial);
    let commit0 = make_commit(&submodule.info.id, None, diff0);
    let commit0_id = commit0.id.clone();
    submodule.append_commit(commit0).unwrap();

    submodule.create_branch("feature", &commit0_id).unwrap();

    // main: y=2
    let mut main_state = initial.clone();
    main_state.insert("y".to_string(), "2".to_string());
    let diff_main = Diff::from_states(&initial, &main_state);
    let commit_main = make_commit(&submodule.info.id, Some(commit0_id.clone()), diff_main);
    submodule.append_commit(commit_main).unwrap();

    // feature: z=3, w=4 (2コミット)
    submodule.checkout("feature").unwrap();
    let mut fs1 = initial.clone();
    fs1.insert("z".to_string(), "3".to_string());
    let df1 = Diff::from_states(&initial, &fs1);
    let cf1 = make_commit(&submodule.info.id, Some(commit0_id), df1);
    let cf1_id = cf1.id.clone();
    submodule.append_commit(cf1).unwrap();

    let mut fs2 = fs1.clone();
    fs2.insert("w".to_string(), "4".to_string());
    let df2 = Diff::from_states(&fs1, &fs2);
    let cf2 = make_commit(&submodule.info.id, Some(cf1_id), df2);
    submodule.append_commit(cf2).unwrap();

    // featureをmainにリベース
    submodule.rebase("main").unwrap();
    let rebased_head = submodule.info.head_commit_id.clone().unwrap();

    // リベース後の状態が正しいことを確認
    let rebased_state = submodule.reconstruct_state(&rebased_head).unwrap();
    assert_eq!(rebased_state["x"], "1");
    assert_eq!(rebased_state["y"], "2");
    assert_eq!(rebased_state["z"], "3");
    assert_eq!(rebased_state["w"], "4");

    // mainに戻ってマージ（リベース後はfast-forwardになるはず）
    submodule.checkout("main").unwrap();
    let merge_result = submodule.merge("feature").unwrap();
    match merge_result {
        MergeResult::FastForward { new_head_id } => {
            assert_eq!(new_head_id, rebased_head);
            let final_state = submodule.reconstruct_state(&new_head_id).unwrap();
            assert_eq!(final_state.len(), 4);
            assert_eq!(final_state["x"], "1");
            assert_eq!(final_state["y"], "2");
            assert_eq!(final_state["z"], "3");
            assert_eq!(final_state["w"], "4");
        }
        MergeResult::Merged { .. } => {
            panic!("Expected FastForward after rebase, got Merged")
        }
    }
}

/// 両方のブランチで同じ変更をした場合、コンフリクトにならない
#[test]
fn test_merge_both_sides_same_change() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    // 初期: x=1, y=2
    let mut initial = HashMap::new();
    initial.insert("x".to_string(), "1".to_string());
    initial.insert("y".to_string(), "2".to_string());
    let diff0 = Diff::from_states(&HashMap::new(), &initial);
    let commit0 = make_commit(&submodule.info.id, None, diff0);
    let commit0_id = commit0.id.clone();
    submodule.append_commit(commit0).unwrap();

    submodule.create_branch("feature", &commit0_id).unwrap();

    // main: x=10, y=20
    let mut main_state = initial.clone();
    main_state.insert("x".to_string(), "10".to_string());
    main_state.insert("y".to_string(), "20".to_string());
    let diff_main = Diff::from_states(&initial, &main_state);
    let commit_main = make_commit(&submodule.info.id, Some(commit0_id.clone()), diff_main);
    submodule.append_commit(commit_main).unwrap();

    // feature: x=10, y=20（同じ変更！）
    submodule.checkout("feature").unwrap();
    let mut feature_state = initial.clone();
    feature_state.insert("x".to_string(), "10".to_string());
    feature_state.insert("y".to_string(), "20".to_string());
    let diff_feature = Diff::from_states(&initial, &feature_state);
    let commit_feature = make_commit(&submodule.info.id, Some(commit0_id), diff_feature);
    submodule.append_commit(commit_feature).unwrap();

    // main に戻ってマージ → コンフリクトにならない
    submodule.checkout("main").unwrap();
    let result = submodule.merge("feature").unwrap();
    match result {
        MergeResult::Merged { ref commit } => {
            let state = submodule.reconstruct_state(&commit.id).unwrap();
            assert_eq!(state["x"], "10");
            assert_eq!(state["y"], "20");
        }
        MergeResult::FastForward { .. } => panic!("Expected Merged"),
    }
}

/// 多数のキーを持つ状態での複雑なマージ
#[test]
fn test_merge_many_keys_mixed_changes() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    // 初期: 10個のキー
    let mut initial = HashMap::new();
    for i in 0..10 {
        initial.insert(format!("key_{}", i), format!("v{}", i));
    }
    let diff0 = Diff::from_states(&HashMap::new(), &initial);
    let commit0 = make_commit(&submodule.info.id, None, diff0);
    let commit0_id = commit0.id.clone();
    submodule.append_commit(commit0).unwrap();

    submodule.create_branch("feature", &commit0_id).unwrap();

    // main: key_0 変更、key_1 削除、key_10 追加
    let mut main_state = initial.clone();
    main_state.insert("key_0".to_string(), "main_modified".to_string());
    main_state.remove("key_1");
    main_state.insert("key_10".to_string(), "new_main".to_string());
    let diff_main = Diff::from_states(&initial, &main_state);
    let commit_main = make_commit(&submodule.info.id, Some(commit0_id.clone()), diff_main);
    submodule.append_commit(commit_main).unwrap();

    // feature: key_2 変更、key_3 削除、key_11 追加
    submodule.checkout("feature").unwrap();
    let mut feature_state = initial.clone();
    feature_state.insert("key_2".to_string(), "feature_modified".to_string());
    feature_state.remove("key_3");
    feature_state.insert("key_11".to_string(), "new_feature".to_string());
    let diff_feature = Diff::from_states(&initial, &feature_state);
    let commit_feature = make_commit(&submodule.info.id, Some(commit0_id), diff_feature);
    submodule.append_commit(commit_feature).unwrap();

    // main に戻ってマージ → 全て異なるキーへの操作なので自動マージ可能
    submodule.checkout("main").unwrap();
    let result = submodule.merge("feature").unwrap();
    match result {
        MergeResult::Merged { ref commit } => {
            let state = submodule.reconstruct_state(&commit.id).unwrap();
            assert_eq!(state["key_0"], "main_modified");
            assert!(!state.contains_key("key_1")); // main で削除
            assert_eq!(state["key_2"], "feature_modified");
            assert!(!state.contains_key("key_3")); // feature で削除
            for i in 4..10 {
                assert_eq!(state[&format!("key_{}", i)], format!("v{}", i));
            }
            assert_eq!(state["key_10"], "new_main");
            assert_eq!(state["key_11"], "new_feature");
            // 10個 - 削除2個 + 追加2個 = 10個
            assert_eq!(state.len(), 10);
        }
        MergeResult::FastForward { .. } => panic!("Expected Merged"),
    }
}

/// ブランチを削除してから同名のブランチを再作成する
#[test]
fn test_delete_and_recreate_branch() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    let mut initial = HashMap::new();
    initial.insert("x".to_string(), "1".to_string());
    let d0 = Diff::from_states(&HashMap::new(), &initial);
    let c0 = make_commit(&submodule.info.id, None, d0);
    let c0_id = c0.id.clone();
    submodule.append_commit(c0).unwrap();

    // ブランチ作成
    submodule.create_branch("temp", &c0_id).unwrap();

    // temp にコミット
    submodule.checkout("temp").unwrap();
    let mut s1 = initial.clone();
    s1.insert("y".to_string(), "2".to_string());
    let d1 = Diff::from_states(&initial, &s1);
    let c1 = make_commit(&submodule.info.id, Some(c0_id.clone()), d1);
    submodule.append_commit(c1).unwrap();

    // main に戻って temp を削除
    submodule.checkout("main").unwrap();
    submodule.delete_branch("temp").unwrap();

    // ブランチ一覧から消えていること
    let (branches, _) = submodule.list_branches().unwrap();
    assert!(!branches.contains(&"temp".to_string()));

    // 同名ブランチを再作成
    submodule.create_branch("temp", &c0_id).unwrap();
    let (branches2, _) = submodule.list_branches().unwrap();
    assert!(branches2.contains(&"temp".to_string()));
}

/// タイムラインの永続化テスト: 記録してからリポジトリを再オープン
#[test]
fn test_timeline_persistence_across_open() {
    let dir = tempdir().unwrap();

    {
        let mut repo = Repository::init(dir.path()).unwrap();
        repo.record_global_event(
            "sub1".to_string(),
            "commit_a".to_string(),
            calix::TimelineEventKind::Commit,
        )
        .unwrap();
        repo.record_merge_event(
            "sub1".to_string(),
            "merge_b".to_string(),
            "parent2".to_string(),
        )
        .unwrap();
    }

    let repo = Repository::open(dir.path()).unwrap();
    assert_eq!(repo.state.timeline.len(), 2);
    assert_eq!(repo.state.global_sequence, 2);
    assert_eq!(repo.state.timeline[0].commit_id, "commit_a");
    assert_eq!(repo.state.timeline[1].commit_id, "merge_b");
    assert!(matches!(
        repo.state.timeline[0].event_kind,
        calix::TimelineEventKind::Commit
    ));
    assert!(matches!(
        repo.state.timeline[1].event_kind,
        calix::TimelineEventKind::Merge { .. }
    ));
}

/// 3ブランチからの連続マージ後に全体の状態が正しいこと
#[test]
fn test_triple_merge_complex_state() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    // 初期: base=0
    let mut initial = HashMap::new();
    initial.insert("base".to_string(), "0".to_string());
    let diff0 = Diff::from_states(&HashMap::new(), &initial);
    let commit0 = make_commit(&submodule.info.id, None, diff0);
    let commit0_id = commit0.id.clone();
    submodule.append_commit(commit0).unwrap();

    // 3つのブランチを作成
    submodule.create_branch("feat_a", &commit0_id).unwrap();
    submodule.create_branch("feat_b", &commit0_id).unwrap();
    submodule.create_branch("feat_c", &commit0_id).unwrap();

    // feat_a: a=1, a2=11 (2コミット)
    submodule.checkout("feat_a").unwrap();
    let mut sa1 = initial.clone();
    sa1.insert("a".to_string(), "1".to_string());
    let da1 = Diff::from_states(&initial, &sa1);
    let ca1 = make_commit(&submodule.info.id, Some(commit0_id.clone()), da1);
    let ca1_id = ca1.id.clone();
    submodule.append_commit(ca1).unwrap();

    let mut sa2 = sa1.clone();
    sa2.insert("a2".to_string(), "11".to_string());
    let da2 = Diff::from_states(&sa1, &sa2);
    let ca2 = make_commit(&submodule.info.id, Some(ca1_id), da2);
    submodule.append_commit(ca2).unwrap();

    // feat_b: b=2
    submodule.checkout("feat_b").unwrap();
    let mut sb1 = initial.clone();
    sb1.insert("b".to_string(), "2".to_string());
    let db1 = Diff::from_states(&initial, &sb1);
    let cb1 = make_commit(&submodule.info.id, Some(commit0_id.clone()), db1);
    submodule.append_commit(cb1).unwrap();

    // feat_c: c=3, c2=33
    submodule.checkout("feat_c").unwrap();
    let mut sc1 = initial.clone();
    sc1.insert("c".to_string(), "3".to_string());
    let dc1 = Diff::from_states(&initial, &sc1);
    let cc1 = make_commit(&submodule.info.id, Some(commit0_id.clone()), dc1);
    let cc1_id = cc1.id.clone();
    submodule.append_commit(cc1).unwrap();

    let mut sc2 = sc1.clone();
    sc2.insert("c2".to_string(), "33".to_string());
    let dc2 = Diff::from_states(&sc1, &sc2);
    let cc2 = make_commit(&submodule.info.id, Some(cc1_id), dc2);
    submodule.append_commit(cc2).unwrap();

    // main に戻って3つのブランチを順にマージ
    submodule.checkout("main").unwrap();

    // feat_a をマージ（fast-forward）
    let r1 = submodule.merge("feat_a").unwrap();
    assert!(matches!(r1, MergeResult::FastForward { .. }));

    // feat_b をマージ（3-wayマージ）
    let r2 = submodule.merge("feat_b").unwrap();
    assert!(matches!(r2, MergeResult::Merged { .. }));

    // feat_c をマージ（3-wayマージ）
    let r3 = submodule.merge("feat_c").unwrap();
    match r3 {
        MergeResult::Merged { ref commit } => {
            let state = submodule.reconstruct_state(&commit.id).unwrap();
            assert_eq!(state["base"], "0");
            assert_eq!(state["a"], "1");
            assert_eq!(state["a2"], "11");
            assert_eq!(state["b"], "2");
            assert_eq!(state["c"], "3");
            assert_eq!(state["c2"], "33");
            assert_eq!(state.len(), 6);
        }
        _ => panic!("Expected Merged for third merge"),
    }
}

/// リベースで3コミットの全てでコンフリクトが発生するケース
#[test]
fn test_rebase_all_commits_conflict() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    // 初期: x=0, y=0
    let mut initial = HashMap::new();
    initial.insert("x".to_string(), "0".to_string());
    initial.insert("y".to_string(), "0".to_string());
    let diff0 = Diff::from_states(&HashMap::new(), &initial);
    let commit0 = make_commit(&submodule.info.id, None, diff0);
    let commit0_id = commit0.id.clone();
    submodule.append_commit(commit0).unwrap();

    submodule.create_branch("feature", &commit0_id).unwrap();

    // main: x=main_x, y=main_y
    let mut main_state = initial.clone();
    main_state.insert("x".to_string(), "main_x".to_string());
    main_state.insert("y".to_string(), "main_y".to_string());
    let diff_main = Diff::from_states(&initial, &main_state);
    let commit_main = make_commit(&submodule.info.id, Some(commit0_id.clone()), diff_main);
    submodule.append_commit(commit_main).unwrap();

    // feature: 2コミットでx, yをそれぞれ変更
    submodule.checkout("feature").unwrap();

    // commit1: x=feat_x
    let mut fs1 = initial.clone();
    fs1.insert("x".to_string(), "feat_x".to_string());
    let df1 = Diff::from_states(&initial, &fs1);
    let cf1 = make_commit(&submodule.info.id, Some(commit0_id), df1);
    let cf1_id = cf1.id.clone();
    submodule.append_commit(cf1).unwrap();

    // commit2: y=feat_y
    let mut fs2 = fs1.clone();
    fs2.insert("y".to_string(), "feat_y".to_string());
    let df2 = Diff::from_states(&fs1, &fs2);
    let cf2 = make_commit(&submodule.info.id, Some(cf1_id), df2);
    submodule.append_commit(cf2).unwrap();

    // rebase → 1つ目のコミットでコンフリクト (x)
    let result = submodule.rebase("main");
    assert!(result.is_err());
    assert!(submodule.is_rebasing().unwrap());

    match result.unwrap_err() {
        CalixError::RebaseConflict { conflicts, .. } => {
            assert!(conflicts.iter().any(|c| c.key == "x"));
        }
        e => panic!("Expected RebaseConflict, got {:?}", e),
    }

    // 1つ目を解決: x=resolved_x
    let mut resolved1 = HashMap::new();
    resolved1.insert("x".to_string(), "resolved_x".to_string());
    resolved1.insert("y".to_string(), "main_y".to_string());

    // continue → 2つ目のコミットでも y がコンフリクト
    let result2 = submodule.rebase_continue(&resolved1);
    assert!(result2.is_err());
    assert!(submodule.is_rebasing().unwrap());

    match result2.unwrap_err() {
        CalixError::RebaseConflict { conflicts, .. } => {
            assert!(conflicts.iter().any(|c| c.key == "y"));
        }
        e => panic!("Expected RebaseConflict for second commit, got {:?}", e),
    }

    // 2つ目を解決: y=resolved_y
    let mut resolved2 = HashMap::new();
    resolved2.insert("x".to_string(), "resolved_x".to_string());
    resolved2.insert("y".to_string(), "resolved_y".to_string());
    submodule.rebase_continue(&resolved2).unwrap();

    assert!(!submodule.is_rebasing().unwrap());

    let head_id = submodule.info.head_commit_id.clone().unwrap();
    let state = submodule.reconstruct_state(&head_id).unwrap();
    assert_eq!(state["x"], "resolved_x");
    assert_eq!(state["y"], "resolved_y");
}

/// 全SubmoduleKind でサブモジュールが正しく作成できること
#[test]
fn test_all_submodule_kinds() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();

    let kinds = vec![
        (SubmoduleKind::Clip, "clips/c1"),
        (SubmoduleKind::Effect, "effects/e1"),
        (SubmoduleKind::Transition, "transitions/t1"),
        (SubmoduleKind::Subtitle, "subtitles/s1"),
        (SubmoduleKind::Track, "tracks/t1"),
        (SubmoduleKind::GlobalEffect, "global_effects/ge1"),
    ];

    for (kind, path) in kinds {
        let sub = repo
            .register_submodule(kind, path.to_string())
            .unwrap();
        assert_eq!(sub.info.current_branch, "main");
        assert!(sub.info.head_commit_id.is_none());
        assert!(sub.info.dependencies.is_empty());
    }

    assert_eq!(repo.state.submodule_index.len(), 6);
}

/// 存在しないコミットIDで状態再構築しようとすると失敗する
#[test]
fn test_reconstruct_nonexistent_commit_fails() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let submodule = repo
        .register_submodule(SubmoduleKind::Clip, "clips/clip_01".to_string())
        .unwrap();

    let result = submodule.reconstruct_state("nonexistent-commit-id");
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err(),
        CalixError::CommitNotFound { .. }
    ));
}

/// 途中のコミットの状態を正しく再構築できる（最新でなく中間コミット）
#[test]
fn test_reconstruct_intermediate_commit() {
    let dir = tempdir().unwrap();
    let mut repo = Repository::init(dir.path()).unwrap();
    let mut submodule = repo
        .register_submodule(SubmoduleKind::Effect, "effects/mid".to_string())
        .unwrap();

    // commit1: a=1
    let mut s1 = HashMap::new();
    s1.insert("a".to_string(), "1".to_string());
    let d1 = Diff::from_states(&HashMap::new(), &s1);
    let c1 = make_commit(&submodule.info.id, None, d1);
    let c1_id = c1.id.clone();
    submodule.append_commit(c1).unwrap();

    // commit2: a=1, b=2
    let mut s2 = s1.clone();
    s2.insert("b".to_string(), "2".to_string());
    let d2 = Diff::from_states(&s1, &s2);
    let c2 = make_commit(&submodule.info.id, Some(c1_id.clone()), d2);
    let c2_id = c2.id.clone();
    submodule.append_commit(c2).unwrap();

    // commit3: a=10, b=2, c=3
    let mut s3 = s2.clone();
    s3.insert("a".to_string(), "10".to_string());
    s3.insert("c".to_string(), "3".to_string());
    let d3 = Diff::from_states(&s2, &s3);
    let c3 = make_commit(&submodule.info.id, Some(c2_id.clone()), d3);
    submodule.append_commit(c3).unwrap();

    // commit1 の状態を再構築
    let state1 = submodule.reconstruct_state(&c1_id).unwrap();
    assert_eq!(state1.len(), 1);
    assert_eq!(state1["a"], "1");

    // commit2 の状態を再構築
    let state2 = submodule.reconstruct_state(&c2_id).unwrap();
    assert_eq!(state2.len(), 2);
    assert_eq!(state2["a"], "1");
    assert_eq!(state2["b"], "2");
}
