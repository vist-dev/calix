use crate::error::CalixResult;
use std::path::Path;

/// MessagePackでシリアライズしてアトミックにファイルへ書き込む
pub fn write_msgpack<T>(path: &Path, value: &T) -> CalixResult<()>
where
    T: serde::Serialize,
{
    let bytes = rmp_serde::to_vec(value)
        .map_err(|e| crate::error::CalixError::Serialize(e.to_string()))?;

    // tmp -> rename のアトミック書き込み
    let tmp_path = path.with_extension("tmp");

    if let Some(parent) = tmp_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(&tmp_path, &bytes)?;
    std::fs::rename(&tmp_path, path)?;

    Ok(())
}

/// ファイルからMessagePackをデシリアライズして読み込む
pub fn read_msgpack<T>(path: &Path) -> CalixResult<T>
where
    T: serde::de::DeserializeOwned,
{
    let bytes = std::fs::read(path)?;
    let value = rmp_serde::from_slice(&bytes)
        .map_err(|e| crate::error::CalixError::Deserialize(e.to_string()))?;
    Ok(value)
}

/// ファイルが存在する場合のみ読み込む
pub fn read_msgpack_optional<T>(path: &Path) -> CalixResult<Option<T>>
where
    T: serde::de::DeserializeOwned,
{
    if !path.exists() {
        return Ok(None);
    }
    read_msgpack(path).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use tempfile::tempdir;

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct TestData {
        value: String,
        number: u64,
    }

    #[test]
    fn test_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.msgpack");

        let original = TestData {
            value: "hello".to_string(),
            number: 42,
        };

        write_msgpack(&path, &original).unwrap();
        let loaded: TestData = read_msgpack(&path).unwrap();

        assert_eq!(original, loaded);
    }

    #[test]
    fn test_atomic_write_no_tmp_file_remains() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.msgpack");

        let data = TestData {
            value: "test".to_string(),
            number: 1,
        };

        write_msgpack(&path, &data).unwrap();

        let tmp = path.with_extension("tmp");
        assert!(!tmp.exists(), "tmpファイルが残っている");
        assert!(path.exists(), "本ファイルが存在しない");
    }
}
