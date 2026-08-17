use std::path::{Path, PathBuf};

pub fn artifact_dir(root: &Path, workflow: &str, instance_id: &str, node_name: &str, invoke: &str) -> PathBuf {
    root.join(".workflows")
        .join(workflow)
        .join("instance")
        .join(instance_id)
        .join("artifacts")
        .join(node_name)
        .join(invoke)
}

pub fn write_detail(root: &Path, workflow: &str, instance_id: &str, node_name: &str, invoke: &str, content: &str) -> Result<PathBuf, String> {
    let dir = artifact_dir(root, workflow, instance_id, node_name, invoke);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建产物目录失败: {e}"))?;
    let path = dir.join("detail.md");
    std::fs::write(&path, content).map_err(|e| format!("写入 detail.md 失败: {e}"))?;
    Ok(path)
}

pub fn write_error(root: &Path, workflow: &str, instance_id: &str, node_name: &str, invoke: &str, reason: &str) -> Result<PathBuf, String> {
    let dir = artifact_dir(root, workflow, instance_id, node_name, invoke);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建产物目录失败: {e}"))?;
    let path = dir.join("error.md");
    std::fs::write(&path, reason).map_err(|e| format!("写入 error.md 失败: {e}"))?;
    Ok(path)
}

pub fn has_detail(root: &Path, workflow: &str, instance_id: &str, node_name: &str, invoke: &str) -> bool {
    let path = artifact_dir(root, workflow, instance_id, node_name, invoke).join("detail.md");
    path.exists() && std::fs::metadata(&path).map(|m| m.len() > 0).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_and_check_detail() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write_detail(root, "wf", "inst", "任务调研", "invoke-2", "产物内容").unwrap();
        assert!(has_detail(root, "wf", "inst", "任务调研", "invoke-2"));
        assert!(!has_detail(root, "wf", "inst", "任务调研", "invoke-1"));
        let path = artifact_dir(root, "wf", "inst", "任务调研", "invoke-2").join("detail.md");
        assert_eq!(std::fs::read_to_string(path).unwrap(), "产物内容");
    }

    #[test]
    fn empty_detail_not_valid() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write_detail(root, "wf", "inst", "A", "invoke-1", "").unwrap();
        assert!(!has_detail(root, "wf", "inst", "A", "invoke-1"));
    }

    #[test]
    fn test_write_error() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write_error(root, "wf", "inst", "A", "invoke-1", "失败原因").unwrap();
        let path = artifact_dir(root, "wf", "inst", "A", "invoke-1").join("error.md");
        assert_eq!(std::fs::read_to_string(path).unwrap(), "失败原因");
    }
}
