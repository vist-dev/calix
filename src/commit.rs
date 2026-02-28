use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 一つのコミットを表す構造体
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Commit {
    /// コミットの一意識別子（UUIDv4）
    pub id: String,

    /// 親コミットのID（最初のコミットはNone）
    pub parent_id: Option<String>,

    /// コミットメッセージ
    pub message: String,

    /// このコミットが属するサブモジュールID
    pub submodule_id: String,

    /// グローバルタイムライン上の順序（Unix timestamp + sequence）
    pub global_order: GlobalOrder,

    /// 前コミットからの変更差分
    pub diff: Diff,

    /// コミット作成日時（Unix timestamp）
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GlobalOrder {
    pub timestamp: u64,
    pub sequence: u64,
}

/// 差分の表現
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Diff {
    /// 追加・更新されたキーと値
    pub set: HashMap<String, String>,

    /// 削除されたキー
    pub remove: Vec<String>,
}

impl Diff {
    pub fn new() -> Self {
        Self {
            set: HashMap::new(),
            remove: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.set.is_empty() && self.remove.is_empty()
    }

    /// 現在の状態に差分を適用して新しい状態を返す
    pub fn apply(&self, current: &HashMap<String, String>) -> HashMap<String, String> {
        let mut next = current.clone();
        for key in &self.remove {
            next.remove(key);
        }
        for (key, value) in &self.set {
            next.insert(key.clone(), value.clone());
        }
        next
    }

    /// 2つの状態を比較して差分を生成する
    pub fn from_states(
        before: &HashMap<String, String>,
        after: &HashMap<String, String>,
    ) -> Self {
        let mut diff = Diff::new();

        // 追加・更新
        for (key, value) in after {
            match before.get(key) {
                Some(old) if old == value => {} // 変化なし
                _ => {
                    diff.set.insert(key.clone(), value.clone());
                }
            }
        }

        // 削除
        for key in before.keys() {
            if !after.contains_key(key) {
                diff.remove.push(key.clone());
            }
        }

        diff
    }
}

impl Default for Diff {
    fn default() -> Self {
        Self::new()
    }
}

impl Commit {
    /// デバッグ用のJSON出力
    pub fn to_debug_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "serialize error".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_apply() {
        let mut before = HashMap::new();
        before.insert("x".to_string(), "1".to_string());
        before.insert("y".to_string(), "2".to_string());

        let mut after = before.clone();
        after.insert("x".to_string(), "999".to_string());
        after.remove("y");
        after.insert("z".to_string(), "3".to_string());

        let diff = Diff::from_states(&before, &after);
        let result = diff.apply(&before);

        assert_eq!(result, after);
    }

    #[test]
    fn test_diff_empty() {
        let state: HashMap<String, String> = HashMap::new();
        let diff = Diff::from_states(&state, &state);
        assert!(diff.is_empty());
    }
}
