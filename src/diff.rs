use crate::error::ConflictEntry;
use std::collections::{HashMap, HashSet};

/// 三方向マージを実行する
///
/// ancestor, ours, theirs の3つの状態を比較し、
/// コンフリクトがなければマージ結果を返す。
/// コンフリクトがある場合はコンフリクト情報のリストを返す。
pub fn three_way_merge(
    ancestor: &HashMap<String, String>,
    ours: &HashMap<String, String>,
    theirs: &HashMap<String, String>,
) -> Result<HashMap<String, String>, Vec<ConflictEntry>> {
    let mut all_keys: HashSet<&String> = HashSet::new();
    all_keys.extend(ancestor.keys());
    all_keys.extend(ours.keys());
    all_keys.extend(theirs.keys());

    let mut merged = HashMap::new();
    let mut conflicts = Vec::new();

    for key in all_keys {
        let val_a = ancestor.get(key);
        let val_o = ours.get(key);
        let val_t = theirs.get(key);

        if val_o == val_t {
            // 両方同じ → そのまま採用
            if let Some(v) = val_o {
                merged.insert(key.clone(), v.clone());
            }
        } else if val_o == val_a {
            // oursは変更なし、theirsのみ変更 → theirsを採用
            if let Some(v) = val_t {
                merged.insert(key.clone(), v.clone());
            }
        } else if val_t == val_a {
            // theirsは変更なし、oursのみ変更 → oursを採用
            if let Some(v) = val_o {
                merged.insert(key.clone(), v.clone());
            }
        } else {
            // 両方が異なる変更を加えた → コンフリクト
            conflicts.push(ConflictEntry {
                key: key.clone(),
                base_value: val_a.cloned(),
                current_value: val_o.cloned(),
                incoming_value: val_t.cloned(),
            });
        }
    }

    if conflicts.is_empty() {
        Ok(merged)
    } else {
        Err(conflicts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_three_way_merge_no_conflict() {
        let mut ancestor = HashMap::new();
        ancestor.insert("x".to_string(), "1".to_string());

        let mut ours = ancestor.clone();
        ours.insert("y".to_string(), "2".to_string());

        let mut theirs = ancestor.clone();
        theirs.insert("z".to_string(), "3".to_string());

        let result = three_way_merge(&ancestor, &ours, &theirs).unwrap();
        assert_eq!(result.get("x").unwrap(), "1");
        assert_eq!(result.get("y").unwrap(), "2");
        assert_eq!(result.get("z").unwrap(), "3");
    }

    #[test]
    fn test_three_way_merge_conflict() {
        let mut ancestor = HashMap::new();
        ancestor.insert("x".to_string(), "1".to_string());

        let mut ours = ancestor.clone();
        ours.insert("x".to_string(), "2".to_string());

        let mut theirs = ancestor.clone();
        theirs.insert("x".to_string(), "3".to_string());

        let err = three_way_merge(&ancestor, &ours, &theirs).unwrap_err();
        assert_eq!(err.len(), 1);
        assert_eq!(err[0].key, "x");
        assert_eq!(err[0].base_value.as_deref(), Some("1"));
        assert_eq!(err[0].current_value.as_deref(), Some("2"));
        assert_eq!(err[0].incoming_value.as_deref(), Some("3"));
    }

    #[test]
    fn test_three_way_merge_same_change() {
        let mut ancestor = HashMap::new();
        ancestor.insert("x".to_string(), "1".to_string());

        let mut ours = ancestor.clone();
        ours.insert("x".to_string(), "2".to_string());

        let mut theirs = ancestor.clone();
        theirs.insert("x".to_string(), "2".to_string());

        let result = three_way_merge(&ancestor, &ours, &theirs).unwrap();
        assert_eq!(result.get("x").unwrap(), "2");
    }

    #[test]
    fn test_three_way_merge_delete_vs_modify() {
        let mut ancestor = HashMap::new();
        ancestor.insert("x".to_string(), "1".to_string());

        let ours = HashMap::new(); // deleted x

        let mut theirs = ancestor.clone();
        theirs.insert("x".to_string(), "2".to_string()); // modified x

        let err = three_way_merge(&ancestor, &ours, &theirs).unwrap_err();
        assert_eq!(err.len(), 1);
        assert_eq!(err[0].key, "x");
        assert_eq!(err[0].current_value, None); // deleted
        assert_eq!(err[0].incoming_value.as_deref(), Some("2")); // modified
    }
}
