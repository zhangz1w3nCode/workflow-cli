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

fn collect_entries(root: &Path, workflow: &str, instance_id: &str) -> Result<(Vec<ArtifactEntry>, ProcessFile), String> {
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
    Ok((entries, pf))
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
    let (entries, pf) = collect_entries(root, workflow, instance_id)?;

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

pub fn view(root: &Path, workflow: &str, instance_id: &str, node: Option<&str>, invoke: Option<&str>, json: bool) -> Result<String, String> {
    let (entries, _) = collect_entries(root, workflow, instance_id)?;

    let matched: Vec<&ArtifactEntry> = if let Some(inv) = invoke {
        entries.iter().filter(|e| e.invoke == inv).collect()
    } else if let Some(n) = node {
        entries.iter().filter(|e| e.node == n).collect()
    } else {
        return Err("请通过 --node 或 --invoke 指定要查看的产物".into());
    };

    if matched.is_empty() {
        return Err(if let Some(inv) = invoke {
            format!("未找到执行ID: {inv}")
        } else {
            format!("未找到节点: {}", node.unwrap_or("未知节点"))
        });
    }

    let mut results: Vec<(&ArtifactEntry, Option<(ArtifactType, String)>)> = Vec::new();
    for e in &matched {
        let content = read_content(root, workflow, instance_id, &e.node, &e.invoke)?;
        results.push((e, content));
    }

    if json {
        let artifacts: Vec<_> = results.iter().map(|(e, content)| {
            let (atype, content_val) = match content {
                Some((t, c)) => (t.as_str(), serde_json::Value::String(c.clone())),
                None => ("none", serde_json::Value::Null),
            };
            serde_json::json!({
                "order": e.order,
                "node": e.node,
                "invoke": e.invoke,
                "type": atype,
                "status": e.status,
                "time": e.time,
                "branch": e.branch,
                "content": content_val,
            })
        }).collect();
        let obj = serde_json::json!({
            "instance": instance_id,
            "artifacts": artifacts,
        });
        return serde_json::to_string_pretty(&obj).map_err(|e| format!("序列化失败: {e}"));
    }

    let total = results.len();
    let mut s = String::new();
    for (i, (e, content)) in results.iter().enumerate() {
        s.push_str(&format!("## [{}/{}] {} ({})\n", i + 1, total, node_display(&e.node, &e.branch), e.invoke));
        match content {
            Some((atype, text)) => {
                s.push_str(&format!("> 类型: {} | 状态: {} | 时间: {}\n\n", atype.as_str(), e.status, e.time));
                s.push_str(text);
                if !text.ends_with('\n') {
                    s.push('\n');
                }
            }
            None => {
                s.push_str(&format!("> 类型: none | 状态: {} | 时间: {}\n\n> 该执行无产物\n", e.status, e.time));
            }
        }
        if i + 1 < total {
            s.push_str("\n---\n\n");
        }
    }
    Ok(s)
}

pub fn search(root: &Path, workflow: &str, instance_id: &str, keyword: &str, json: bool) -> Result<String, String> {
    if keyword.is_empty() {
        return Err("搜索关键词不能为空".into());
    }
    let (entries, _) = collect_entries(root, workflow, instance_id)?;

    let mut results: Vec<&ArtifactEntry> = Vec::new();
    for e in &entries {
        if e.invoke == "-" {
            continue;
        }
        let content = read_content(root, workflow, instance_id, &e.node, &e.invoke)?;
        if content.is_some_and(|(_, text)| text.contains(keyword)) {
            results.push(e);
        }
    }

    if json {
        let artifacts: Vec<_> = results.iter().map(|e| serde_json::json!({
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
            "keyword": keyword,
            "results": artifacts,
        });
        return serde_json::to_string_pretty(&obj).map_err(|e| format!("序列化失败: {e}"));
    }

    let mut s = String::from("| # | 节点 | 执行ID | 类型 | 状态 | 执行时间 |\n|---|------|--------|------|------|---------|\n");
    for e in &results {
        s.push_str(&format!("| {} | {} | {} | {} | {} | {} |\n",
            e.order, node_display(&e.node, &e.branch), e.invoke,
            e.artifact_type.as_str(), e.status, e.time));
    }
    Ok(s)
}

pub fn timeline(root: &Path, workflow: &str, instance_id: &str, json: bool) -> Result<String, String> {
    let (entries, pf) = collect_entries(root, workflow, instance_id)?;

    let mut results: Vec<(&ArtifactEntry, Option<(ArtifactType, String)>)> = Vec::new();
    for e in &entries {
        let content = if e.invoke == "-" {
            None
        } else {
            read_content(root, workflow, instance_id, &e.node, &e.invoke)?
        };
        results.push((e, content));
    }

    let context_path = instance_dir(root, workflow, instance_id).join("context.md");
    let context = std::fs::read_to_string(&context_path).ok().filter(|c| !c.is_empty());

    if json {
        let timeline: Vec<_> = results.iter().map(|(e, content)| {
            let (atype, content_val) = match content {
                Some((t, c)) => (t.as_str(), serde_json::Value::String(c.clone())),
                None => (e.artifact_type.as_str(), serde_json::Value::Null),
            };
            serde_json::json!({
                "order": e.order,
                "node": e.node,
                "invoke": e.invoke,
                "type": atype,
                "status": e.status,
                "time": e.time,
                "branch": e.branch,
                "content": content_val,
            })
        }).collect();
        let obj = serde_json::json!({
            "instance": instance_id,
            "workflow": workflow,
            "status": pf.state.status.as_str(),
            "initial_input": pf.state.initial_input,
            "context": context,
            "timeline": timeline,
        });
        return serde_json::to_string_pretty(&obj).map_err(|e| format!("序列化失败: {e}"));
    }

    let mut s = String::new();
    s.push_str(&format!("# 实例: {}\n", instance_id));
    s.push_str(&format!("# 工作流: {}\n", workflow));
    s.push_str(&format!("# 状态: {}\n", pf.state.status.as_str()));
    if let Some(input) = &pf.state.initial_input {
        s.push_str(&format!("# 初始任务: {}\n", input));
    }
    if let Some(ctx) = &context {
        s.push_str("\n## 上下文\n\n");
        s.push_str(ctx);
        if !ctx.ends_with('\n') {
            s.push('\n');
        }
    }
    s.push_str("\n## 执行时间线\n\n");

    s.push_str("| # | 节点 | 执行ID | 类型 | 状态 | 执行时间 |\n|---|------|--------|------|------|---------|\n");
    for e in &entries {
        s.push_str(&format!("| {} | {} | {} | {} | {} | {} |\n",
            e.order, node_display(&e.node, &e.branch), e.invoke,
            e.artifact_type.as_str(), e.status, e.time));
    }

    s.push_str("\n## 产物详情\n\n");
    for (e, content) in &results {
        s.push_str(&format!("### [{}] {} ({})\n", e.order, node_display(&e.node, &e.branch), e.invoke));
        match content {
            Some((atype, text)) => {
                s.push_str(&format!("> 类型: {} | 状态: {} | 时间: {}\n\n", atype.as_str(), e.status, e.time));
                s.push_str(text);
                if !text.ends_with('\n') {
                    s.push('\n');
                }
            }
            None => {
                s.push_str(&format!("> 类型: none | 状态: {} | 时间: {}\n\n> 无产物\n", e.status, e.time));
            }
        }
        s.push('\n');
    }

    Ok(s)
}

#[derive(Clone)]
enum DiffLine {
    Context(String),
    Added(String),
    Removed(String),
    Separator,
}

impl DiffLine {
    fn tag(&self) -> &'static str {
        match self {
            DiffLine::Context(_) => "context",
            DiffLine::Added(_) => "added",
            DiffLine::Removed(_) => "removed",
            DiffLine::Separator => "separator",
        }
    }

    fn value(&self) -> &str {
        match self {
            DiffLine::Context(s) | DiffLine::Added(s) | DiffLine::Removed(s) => s,
            DiffLine::Separator => "...",
        }
    }
}

fn compute_diff(old: &str, new: &str) -> Vec<DiffLine> {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let m = old_lines.len();
    let n = new_lines.len();

    let mut dp = vec![vec![0u32; n + 1]; m + 1];
    for i in 1..=m {
        for j in 1..=n {
            if old_lines[i - 1] == new_lines[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
            }
        }
    }

    let mut result = Vec::new();
    let mut i = m;
    let mut j = n;
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && old_lines[i - 1] == new_lines[j - 1] {
            result.push(DiffLine::Context(old_lines[i - 1].to_string()));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || dp[i][j - 1] >= dp[i - 1][j]) {
            result.push(DiffLine::Added(new_lines[j - 1].to_string()));
            j -= 1;
        } else {
            result.push(DiffLine::Removed(old_lines[i - 1].to_string()));
            i -= 1;
        }
    }
    result.reverse();
    result
}

fn apply_context_limit(diff: &[DiffLine], context: usize) -> Vec<DiffLine> {
    let change_indices: Vec<usize> = diff.iter().enumerate()
        .filter(|(_, l)| !matches!(l, DiffLine::Context(_)))
        .map(|(i, _)| i)
        .collect();

    if change_indices.is_empty() {
        return Vec::new();
    }

    let mut include = vec![false; diff.len()];
    for &ci in &change_indices {
        let start = ci.saturating_sub(context);
        let end = (ci + context + 1).min(diff.len());
        include[start..end].fill(true);
    }

    let mut result = Vec::new();
    let mut prev_included = false;
    for (i, line) in diff.iter().enumerate() {
        if include[i] {
            result.push(line.clone());
            prev_included = true;
        } else if prev_included {
            result.push(DiffLine::Separator);
            prev_included = false;
        }
    }
    if let Some(DiffLine::Separator) = result.last() {
        result.pop();
    }
    result
}

pub fn diff(root: &Path, workflow: &str, instance_id: &str, node: &str, json: bool, context: usize, full: bool) -> Result<String, String> {
    let (entries, _) = collect_entries(root, workflow, instance_id)?;
    let matched: Vec<&ArtifactEntry> = entries.iter().filter(|e| e.node == node).collect();

    if matched.len() < 2 {
        return Err(format!("节点 {node} 仅执行 {} 次，需至少 2 次才能对比", matched.len()));
    }

    let mut contents: Vec<(&ArtifactEntry, Option<String>)> = Vec::new();
    for e in &matched {
        let content = read_content(root, workflow, instance_id, &e.node, &e.invoke)
            .map(|c| c.map(|(_, text)| text))?;
        contents.push((e, content));
    }

    let mut diffs = Vec::new();
    for i in 0..contents.len() - 1 {
        let (e1, c1) = &contents[i];
        let (e2, c2) = &contents[i + 1];
        let text1 = c1.as_deref().unwrap_or("");
        let text2 = c2.as_deref().unwrap_or("");
        const MAX_DIFF_LINES: usize = 5000;
        if text1.lines().count() > MAX_DIFF_LINES || text2.lines().count() > MAX_DIFF_LINES {
            return Err(format!("产物行数超过 {MAX_DIFF_LINES} 行上限，已跳过 diff 计算"));
        }
        let raw = compute_diff(text1, text2);
        let display = if full { raw } else { apply_context_limit(&raw, context) };
        diffs.push((e1, e2, display));
    }

    if json {
        let diff_arr: Vec<_> = diffs.iter().map(|(e1, e2, lines)| {
            let changes: Vec<_> = lines.iter().map(|l| serde_json::json!({
                "type": l.tag(),
                "value": l.value(),
            })).collect();
            serde_json::json!({
                "from_invoke": e1.invoke,
                "to_invoke": e2.invoke,
                "from_status": e1.status,
                "to_status": e2.status,
                "from_time": e1.time,
                "to_time": e2.time,
                "changes": changes,
            })
        }).collect();
        let obj = serde_json::json!({
            "instance": instance_id,
            "node": node,
            "diffs": diff_arr,
        });
        return serde_json::to_string_pretty(&obj).map_err(|e| format!("序列化失败: {e}"));
    }

    let mut s = String::new();
    for (i, (e1, e2, lines)) in diffs.iter().enumerate() {
        s.push_str(&format!("## [{}→{}] {} ({})\n", i + 1, i + 2, node_display(&e1.node, &e1.branch), e1.invoke));
        s.push_str(&format!("> 对比: {} ({}, {}) → {} ({}, {})\n\n",
            e1.invoke, e1.status, e1.time,
            e2.invoke, e2.status, e2.time));
        if lines.is_empty() {
            s.push_str("无变更\n");
        } else {
            s.push_str("```diff\n");
            for l in lines {
                match l {
                    DiffLine::Context(text) => s.push_str(&format!("  {text}\n")),
                    DiffLine::Added(text) => s.push_str(&format!("+ {text}\n")),
                    DiffLine::Removed(text) => s.push_str(&format!("- {text}\n")),
                    DiffLine::Separator => s.push_str("...\n"),
                }
            }
            s.push_str("```\n");
        }
        if i + 1 < diffs.len() {
            s.push('\n');
        }
    }
    Ok(s)
}

pub fn context_set(root: &Path, workflow: &str, instance_id: &str, topic: &str, content: &str) -> Result<String, String> {
    if topic.is_empty() {
        return Err("topic 不能为空".into());
    }
    if content.is_empty() {
        return Err("content 不能为空".into());
    }
    let topic = topic.replace(['\n', '\r'], " ");
    let path = instance_dir(root, workflow, instance_id).join("context.md");
    let time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let entry = format!("## [{}] {}\n\n{}\n", time, topic, content);

    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("打开 context.md 失败: {e}"))?;
    file.write_all(entry.as_bytes())
        .map_err(|e| format!("写入 context.md 失败: {e}"))?;

    Ok("上下文已暂存".into())
}

pub fn context_get(root: &Path, workflow: &str, instance_id: &str, json: bool) -> Result<String, String> {
    let path = instance_dir(root, workflow, instance_id).join("context.md");
    if !path.exists() {
        if json {
            return serde_json::to_string_pretty(&serde_json::json!({
                "instance": instance_id,
                "context": serde_json::Value::Null,
            })).map_err(|e| format!("序列化失败: {e}"));
        }
        return Ok("无暂存上下文".into());
    }
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("读取 context.md 失败: {e}"))?;

    if json {
        return serde_json::to_string_pretty(&serde_json::json!({
            "instance": instance_id,
            "context": content,
        })).map_err(|e| format!("序列化失败: {e}"));
    }
    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_diff_identical() {
        let result = compute_diff("aaa\nbbb", "aaa\nbbb");
        assert!(result.iter().all(|l| matches!(l, DiffLine::Context(_))));
    }

    #[test]
    fn compute_diff_empty() {
        let result = compute_diff("", "");
        assert!(result.is_empty());
    }

    #[test]
    fn compute_diff_add_only() {
        let result = compute_diff("", "aaa");
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0], DiffLine::Added(_)));
    }

    #[test]
    fn compute_diff_remove_only() {
        let result = compute_diff("aaa", "");
        assert_eq!(result.len(), 1);
        assert!(matches!(result[0], DiffLine::Removed(_)));
    }
}
