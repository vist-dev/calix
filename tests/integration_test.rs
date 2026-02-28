use calix::{CalixError, Commit, Diff, GlobalOrder, MergeResult, Repository, SubmoduleKind};
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
