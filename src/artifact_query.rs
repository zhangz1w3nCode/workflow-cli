use std::path::{Path, PathBuf};
use crate::artifact;
use crate::state::ProcessFile;

fn instance_dir(root: &Path, workflow: &str, instance_id: &str) -> PathBuf {
    root.join(".workflows").join(workflow).join("instance").join(instance_id)
}

#[derive(Clone)]
enum ArtifactType { Detail, Error, None }

impl ArtifactType {
    fn as_str(&self) -> &'static str {
        match self {
            ArtifactType::Detail => "detail",
            ArtifactType::Error => "error",
            ArtifactType::None => "none",
        }
    }
}

#[derive(Clone)]
struct ArtifactEntry {
    order: usize,
    node: String,
    invoke: String,
    artifact_type: ArtifactType,
    status: String,
    time: String,
    branch: Option<String>,
}

fn collect_entries(root: &Path, workflow: &str, instance_id: &str) -> Result<Vec<ArtifactEntry>, String> {
    let pf = ProcessFile::read(&instance_dir(root, workflow, instance_id).join("process.md"))?;
    let mut entries = Vec::new();
    for (i, event) in pf.trace.iter().enumerate() {
        let atype = if event.invoke == "-" {
            ArtifactType::None
        } else {
            let dir = artifact::artifact_dir(root, workflow, instance_id, &event.node, &event.invoke);
            let detail = dir.join("detail.md");
            let error = dir.join("error.md");
            if detail.exists() && std::fs::metadata(&detail).map(|m| m.len() > 0).unwrap_or(false) {
                ArtifactType::Detail
            } else if error.exists() && std::fs::metadata(&error).map(|m| m.len() > 0).unwrap_or(false) {
                ArtifactType::Error
            } else {
                ArtifactType::None
            }
        };
        entries.push(ArtifactEntry {
            order: i + 1,
            node: event.node.clone(),
            invoke: event.invoke.clone(),
            artifact_type: atype,
            status: event.status.clone(),
            time: event.time.clone(),
            branch: event.branch.clone(),
        });
    }
    Ok(entries)
}

fn read_content(root: &Path, workflow: &str, instance_id: &str, node: &str, invoke: &str) -> Result<Option<(ArtifactType, String)>, String> {
    let dir = artifact::artifact_dir(root, workflow, instance_id, node, invoke);
    let detail_path = dir.join("detail.md");
    let error_path = dir.join("error.md");
    if detail_path.exists() {
        let content = std::fs::read_to_string(&detail_path)
            .map_err(|e| format!("读取 detail.md 失败: {e}"))?;
        if !content.is_empty() {
            return Ok(Some((ArtifactType::Detail, content)));
        }
    }
    if error_path.exists() {
        let content = std::fs::read_to_string(&error_path)
            .map_err(|e| format!("读取 error.md 失败: {e}"))?;
        if !content.is_empty() {
            return Ok(Some((ArtifactType::Error, content)));
        }
    }
    Ok(None)
}

fn node_display(node: &str, branch: &Option<String>) -> String {
    match branch {
        Some(b) => format!("{}({})", node, b),
        None => node.to_string(),
    }
}

pub fn list(root: &Path, workflow: &str, instance_id: &str, json: bool) -> Result<String, String> {
    let entries = collect_entries(root, workflow, instance_id)?;
    let pf = ProcessFile::read(&instance_dir(root, workflow, instance_id).join("process.md"))?;

    if json {
        let artifacts: Vec<_> = entries.iter().map(|e| serde_json::json!({
            "order": e.order,
            "node": e.node,
            "invoke": e.invoke,
            "type": e.artifact_type.as_str(),
            "status": e.status,
            "time": e.time,
            "branch": e.branch,
        })).collect();
        let obj = serde_json::json!({
            "instance": instance_id,
            "workflow": workflow,
            "status": pf.state.status.as_str(),
            "artifacts": artifacts,
        });
        return serde_json::to_string_pretty(&obj).map_err(|e| format!("序列化失败: {e}"));
    }

    let mut s = String::from("| # | 节点 | 执行ID | 类型 | 状态 | 执行时间 |\n|---|------|--------|------|------|---------|\n");
    for e in &entries {
        s.push_str(&format!("| {} | {} | {} | {} | {} | {} |\n",
            e.order, node_display(&e.node, &e.branch), e.invoke,
            e.artifact_type.as_str(), e.status, e.time));
    }
    Ok(s)
}

pub fn view(_root: &Path, _workflow: &str, _instance_id: &str, _node: Option<&str>, _invoke: Option<&str>, _json: bool) -> Result<String, String> {
    Err("artifact view 尚未实现".into())
}

pub fn search(_root: &Path, _workflow: &str, _instance_id: &str, _keyword: &str, _json: bool) -> Result<String, String> {
    Err("artifact search 尚未实现".into())
}

pub fn timeline(_root: &Path, _workflow: &str, _instance_id: &str, _json: bool) -> Result<String, String> {
    Err("artifact timeline 尚未实现".into())
}

pub fn diff(_root: &Path, _workflow: &str, _instance_id: &str, _node: &str, _json: bool, _context: usize, _full: bool) -> Result<String, String> {
    Err("artifact diff 尚未实现".into())
}

pub fn context_set(_root: &Path, _workflow: &str, _instance_id: &str, _topic: &str, _content: &str) -> Result<String, String> {
    Err("context set 尚未实现".into())
}

pub fn context_get(_root: &Path, _workflow: &str, _instance_id: &str, _json: bool) -> Result<String, String> {
    Err("context get 尚未实现".into())
}
