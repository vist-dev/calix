use calix::{Commit, Diff, GlobalOrder, Repository, SubmoduleKind};
use std::collections::HashMap;
use tempfile::tempdir;
use uuid::Uuid;

fn make_commit(submodule_id: &str, parent_id: Option<String>, diff: Diff) -> Commit {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    Commit {
        id: Uuid::new_v4().to_string(),
        parent_id,
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
