use std::path::{Path, PathBuf};
use crate::artifact;
use crate::graph::Graph;
use crate::model::Flow;
use crate::state::{ProcessFile, Status};

fn instance_dir(root: &Path, workflow: &str, instance_id: &str) -> PathBuf {
    root.join(".workflows").join(workflow).join("instance").join(instance_id)
}

fn load_flow(root: &Path, workflow: &str) -> Result<Flow, String> {
    let path = root.join(".workflows").join(workflow).join("meta-data").join("flow.json");
    Flow::from_file(&path)
}

pub fn next(root: &Path, workflow: &str, instance_id: &str, json: bool) -> Result<String, String> {
    let inst_dir = instance_dir(root, workflow, instance_id);
    let mut pf = ProcessFile::read(&inst_dir.join("process.md"))?;

    if pf.state.status != Status::Idle {
        return Err("当前节点未完成，先 complete/choose".into());
    }

    let flow = load_flow(root, workflow)?;
    let node = flow.node(&pf.state.current).ok_or("当前节点不存在")?;

    if node.node_type == "end" {
        pf.state.status = Status::Completed;
        pf.append_trace("[completed] 工作流结束");
        pf.mermaid = render_mermaid(&flow, &pf.state);
        pf.write(&inst_dir.join("process.md"))?;
        return Ok("工作流已完成".into());
    }

    if let (Some(last_node), Some(last_invoke)) = (pf.state.last_node.as_deref(), pf.state.last_invoke.as_deref()) {
        if !artifact::has_detail(root, workflow, instance_id, last_node, last_invoke) {
            return Err(format!("节点 {last_node} ({last_invoke}) 未写入产物，请补写后再 next"));
        }
    }

    pf.state.step += 1;

    if let Err(msg) = crate::limits::check_step_limit(&pf.state) {
        pf.state.status = Status::Aborted;
        pf.mermaid = render_mermaid(&flow, &pf.state);
        pf.write(&inst_dir.join("process.md"))?;
        return Err(msg);
    }

    pf.state.status = if node.node_type == "decision" {
        Status::AwaitingChoice
    } else {
        Status::Executing
    };
    pf.append_trace(&format!("[active] {} ({})", node.data.label, pf.state.current_invoke));
    pf.mermaid = render_mermaid(&flow, &pf.state);
    pf.write(&inst_dir.join("process.md"))?;

    Ok(render_node(root, node, &pf.state.current_invoke, json))
}

pub fn complete(root: &Path, workflow: &str, instance_id: &str, output: &str) -> Result<String, String> {
    let inst_dir = instance_dir(root, workflow, instance_id);
    let mut pf = ProcessFile::read(&inst_dir.join("process.md"))?;

    if pf.state.status != Status::Executing {
        return Err("当前无执行中的业务节点".into());
    }
    if output.trim().is_empty() {
        return Err("请通过 --output / --output-file / stdin 提供产物".into());
    }

    let flow = load_flow(root, workflow)?;
    let graph = Graph::new(&flow);

    artifact::write_detail(root, workflow, instance_id, &pf.state.current_name, &pf.state.current_invoke, output)?;

    pf.append_trace(&format!("[completed] {} ({})", pf.state.current_name, pf.state.current_invoke));
    if !pf.state.completed.contains(&pf.state.current_name) {
        pf.state.completed.push(pf.state.current_name.clone());
    }

    let next_id = graph.next_node(&pf.state.current, None)?;
    let next_node = flow.node(&next_id).ok_or("下一个节点不存在")?;
    let next_invoke = crate::state::gen_invoke_id();

    pf.state.last_node = Some(pf.state.current_name.clone());
    pf.state.last_invoke = Some(pf.state.current_invoke.clone());
    pf.state.current = next_id;
    pf.state.current_name = next_node.data.label.clone();
    pf.state.current_invoke = next_invoke;
    pf.state.retry_count = 0;
    pf.state.status = Status::Idle;
    pf.mermaid = render_mermaid(&flow, &pf.state);
    pf.write(&inst_dir.join("process.md"))?;

    Ok(format!("已推进到 {}", next_node.data.label))
}

pub fn fail(root: &Path, workflow: &str, instance_id: &str, reason: &str) -> Result<String, String> {
    let inst_dir = instance_dir(root, workflow, instance_id);
    let mut pf = ProcessFile::read(&inst_dir.join("process.md"))?;

    if pf.state.status != Status::Executing {
        return Err("当前无执行中的业务节点".into());
    }

    let flow = load_flow(root, workflow)?;

    artifact::write_error(root, workflow, instance_id, &pf.state.current_name, &pf.state.current_invoke, reason)?;
    pf.append_trace(&format!("[failed] {} ({})", pf.state.current_name, pf.state.current_invoke));

    pf.state.retry_count += 1;
    if let Err(msg) = crate::limits::check_retry_limit(&pf.state) {
        pf.state.status = Status::Aborted;
        pf.mermaid = render_mermaid(&flow, &pf.state);
        pf.write(&inst_dir.join("process.md"))?;
        return Err(msg);
    }

    pf.state.last_node = None;
    pf.state.last_invoke = None;
    pf.state.current_invoke = crate::state::gen_invoke_id();
    pf.state.status = Status::Idle;
    pf.mermaid = render_mermaid(&flow, &pf.state);
    pf.write(&inst_dir.join("process.md"))?;

    Ok("已标记失败，可重新 next 重试".into())
}

pub fn choose(root: &Path, workflow: &str, instance_id: &str, branch: &str, reason: Option<&str>) -> Result<String, String> {
    let inst_dir = instance_dir(root, workflow, instance_id);
    let mut pf = ProcessFile::read(&inst_dir.join("process.md"))?;

    if pf.state.status != Status::AwaitingChoice {
        return Err("当前无待决策节点".into());
    }

    let flow = load_flow(root, workflow)?;
    let graph = Graph::new(&flow);
    let node = flow.node(&pf.state.current).ok_or("决策节点不存在")?;

    let names = graph.branch_names(&pf.state.current);
    if !names.iter().any(|n| n == branch) {
        return Err(format!("分支 {branch} 不存在，可选: {}", names.join("/")));
    }

    let detail = match reason {
        Some(r) => format!("选择分支: {branch}\n理由: {r}"),
        None => format!("选择分支: {branch}"),
    };
    artifact::write_detail(root, workflow, instance_id, &pf.state.current_name, &pf.state.current_invoke, &detail)?;
    pf.append_trace(&format!("[completed] {} (分支: {})", pf.state.current_name, branch));
    if !pf.state.completed.contains(&pf.state.current_name) {
        pf.state.completed.push(pf.state.current_name.clone());
    }

    if graph.is_catch_all_branch(&pf.state.current, branch) {
        pf.state.loop_count += 1;
    }

    let branch_id = node.data.branches.iter().find(|b| b.name == branch).map(|b| b.id.as_str());
    let next_id = graph.next_node(&pf.state.current, branch_id)?;
    let next_node = flow.node(&next_id).ok_or("下一个节点不存在")?;
    let next_invoke = crate::state::gen_invoke_id();

    pf.state.last_node = None;
    pf.state.last_invoke = None;
    pf.state.current = next_id;
    pf.state.current_name = next_node.data.label.clone();
    pf.state.current_invoke = next_invoke;
    pf.state.status = Status::Idle;

    if let Err(msg) = crate::limits::check_loop_limit(&pf.state) {
        pf.state.status = Status::Aborted;
        pf.mermaid = render_mermaid(&flow, &pf.state);
        pf.write(&inst_dir.join("process.md"))?;
        return Err(msg);
    }

    pf.mermaid = render_mermaid(&flow, &pf.state);
    pf.write(&inst_dir.join("process.md"))?;
    Ok(format!("已选择分支 {branch}，推进到 {}", next_node.data.label))
}

pub fn status(root: &Path, workflow: &str, instance_id: &str, json: bool) -> Result<String, String> {
    let inst_dir = instance_dir(root, workflow, instance_id);
    let pf = ProcessFile::read(&inst_dir.join("process.md"))?;
    let s = &pf.state;

    if json {
        return serde_json::to_string_pretty(s).map_err(|e| format!("序列化失败: {e}"));
    }

    Ok(format!(
        "| 字段 | 值 |\n|------|-----|\n| 实例 | {} |\n| 工作流 | {} |\n| 状态 | {} |\n| 当前节点 | {} ({}) |\n| 步数 | {} |\n| 环回次数 | {} |\n| 失败重试 | {} |\n| 限制 | max_steps={} / max_loop={} / max_retry={} |",
        s.instance_id, s.workflow, s.status.as_str(), s.current_name, s.current_invoke,
        s.step, s.loop_count, s.retry_count, s.limits.max_steps, s.limits.max_loop, s.limits.max_retry
    ))
}

pub fn list_workflows(root: &Path) -> Result<String, String> {
    let base = root.join(".workflows");
    let mut out = String::from("| 工作流 | 路径 |\n|--------|------|\n");
    if !base.exists() {
        return Ok(out);
    }
    let mut names: Vec<String> = std::fs::read_dir(&base)
        .map_err(|e| format!("读取 .workflows 失败: {e}"))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter(|e| e.path().join("meta-data/flow.json").exists())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    for n in names {
        out.push_str(&format!("| {n} | .workflows/{n} |\n"));
    }
    Ok(out)
}

pub fn list_instances(root: &Path, workflow_filter: Option<&str>) -> Result<String, String> {
    let base = root.join(".workflows");
    let mut out = String::from("| 实例 | 工作流 | 状态 |\n|------|--------|------|\n");
    if !base.exists() {
        return Ok(out);
    }
    let wf_dirs: Vec<PathBuf> = std::fs::read_dir(&base)
        .map_err(|e| format!("读取 .workflows 失败: {e}"))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .filter(|p| workflow_filter.map(|w| p.file_name().and_then(|n| n.to_str()) == Some(w)).unwrap_or(true))
        .collect();

    for wf_dir in wf_dirs {
        let wf_name = wf_dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
        let inst_dir = wf_dir.join("instance");
        if !inst_dir.exists() {
            continue;
        }
        let mut ids: Vec<String> = std::fs::read_dir(&inst_dir)
            .map_err(|e| format!("读取实例目录失败: {e}"))?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        ids.sort();
        for id in ids {
            let st = ProcessFile::read(&inst_dir.join(&id).join("process.md"))
                .map(|pf| pf.state.status.as_str().to_string())
                .unwrap_or_else(|_| "unknown".into());
            out.push_str(&format!("| {id} | {wf_name} | {st} |\n"));
        }
    }
    Ok(out)
}

fn render_node(root: &Path, node: &crate::model::Node, invoke: &str, json: bool) -> String {
    if json {
        return match node.node_type.as_str() {
            "decision" => serde_json::json!({
                "type": "decision",
                "node_id": node.id,
                "node_name": node.data.label,
                "invoke": invoke,
                "condition": node.data.condition,
                "branches": node.data.branches.iter().map(|b| {
                    serde_json::json!({"name": b.name, "description": b.description})
                }).collect::<Vec<_>>()
            }).to_string(),
            _ => serde_json::json!({
                "type": "business",
                "node_id": node.id,
                "node_name": node.data.label,
                "invoke": invoke,
                "content": read_node_md(root, node)
            }).to_string(),
        };
    }

    match node.node_type.as_str() {
        "decision" => {
            let mut s = String::from("# 判断条件\n\n");
            s.push_str(node.data.condition.as_deref().unwrap_or(""));
            s.push_str("\n\n# 可选分支\n\n");
            for b in &node.data.branches {
                match &b.description {
                    Some(d) => s.push_str(&format!("- {} ({})\n", b.name, d)),
                    None => s.push_str(&format!("- {}\n", b.name)),
                }
            }
            s
        }
        _ => read_node_md(root, node),
    }
}

fn read_node_md(root: &Path, node: &crate::model::Node) -> String {
    let ref_path = node.data.node_ref_path.as_deref().unwrap_or("");
    let full = root.join(ref_path);
    std::fs::read_to_string(&full).unwrap_or_else(|e| format!("节点文件缺失: {} ({e})", full.display()))
}

fn node_ref_name(flow: &Flow, id: &str) -> String {
    match flow.node(id) {
        Some(n) if n.node_type == "start" => "start_node".to_string(),
        Some(n) if n.node_type == "end" => "end_node".to_string(),
        Some(n) => n.data.label.clone(),
        None => id.to_string(),
    }
}

pub fn render_mermaid(flow: &Flow, state: &crate::state::ProcessState) -> String {
    let mut s = String::from("```mermaid\nflowchart TD\n\n");

    for node in &flow.nodes {
        let name = node_ref_name(flow, &node.id);
        match node.node_type.as_str() {
            "start" | "end" => s.push_str(&format!("    {name}([{}])\n", node.data.label)),
            "decision" => s.push_str(&format!("    {name}{{{}}}\n", node.data.label)),
            _ => s.push_str(&format!("    {name}[{}]\n", node.data.label)),
        }
    }
    s.push('\n');

    for edge in &flow.edges {
        let src = node_ref_name(flow, &edge.source);
        let dst = node_ref_name(flow, &edge.target);
        match &edge.branch_id {
            Some(bid) => {
                let branch_name = flow.node(&edge.source)
                    .and_then(|n| n.data.branches.iter().find(|b| &b.id == bid))
                    .map(|b| b.name.as_str())
                    .unwrap_or("");
                s.push_str(&format!("    {src} -->|{branch_name}| {dst}\n"));
            }
            None => {
                s.push_str(&format!("    {src} --> {dst}\n"));
            }
        }
    }
    s.push('\n');

    s.push_str("    classDef done fill:#4caf50,color:#fff;\n");
    s.push_str("    classDef current fill:#ff9800,color:#fff;\n");
    s.push_str("    classDef pending fill:#f5f5f5,color:#333;\n\n");

    let mut done_nodes = Vec::new();
    let mut current_nodes = Vec::new();
    let mut pending_nodes = Vec::new();

    for node in &flow.nodes {
        let name = node_ref_name(flow, &node.id);
        if node.node_type == "start" {
            done_nodes.push(name);
        } else if node.node_type == "end" {
            if state.status == crate::state::Status::Completed {
                done_nodes.push(name);
            } else {
                pending_nodes.push(name);
            }
        } else if state.current_name == node.data.label && state.status != crate::state::Status::Completed {
            current_nodes.push(name);
        } else if state.completed.contains(&node.data.label) {
            done_nodes.push(name);
        } else {
            pending_nodes.push(name);
        }
    }

    if !done_nodes.is_empty() {
        s.push_str(&format!("    class {} done;\n", done_nodes.join(",")));
    }
    if !current_nodes.is_empty() {
        s.push_str(&format!("    class {} current;\n", current_nodes.join(",")));
    }
    if !pending_nodes.is_empty() {
        s.push_str(&format!("    class {} pending;\n", pending_nodes.join(",")));
    }

    s.push_str("```");
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instance;
    use crate::state::{Limits, ProcessFile, Status};

    fn setup(root: &std::path::Path) {
        let wf = root.join(".workflows/wf");
        std::fs::create_dir_all(wf.join("meta-data")).unwrap();
        std::fs::write(wf.join("meta-data/flow.json"), r#"{
          "nodes": [
            {"id":"start","type":"start","data":{"label":"开始"}},
            {"id":"end","type":"end","data":{"label":"结束"}},
            {"id":"a","type":"business","data":{"label":"任务理解","nodeRefPath":".nodes/任务理解.md"}}
          ],
          "edges": [{"id":"e1","source":"start","target":"a","type":"default"},
                    {"id":"e2","source":"a","target":"end","type":"default"}]
        }"#).unwrap();
        std::fs::write(wf.join("WORKFLOW.md"), "# workflow\n").unwrap();
        std::fs::create_dir_all(root.join(".nodes")).unwrap();
        std::fs::write(root.join(".nodes/任务理解.md"), "# 任务\n- 理解任务\n").unwrap();
    }

    #[test]
    fn next_outputs_node_content() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        setup(root);
        instance::create(root, "wf", "i1", None, Limits::default()).unwrap();
        let out = next(root, "wf", "i1", false).unwrap();
        assert!(out.contains("- 理解任务"));
        let pf = ProcessFile::read(&root.join(".workflows/wf/instance/i1/process.md")).unwrap();
        assert_eq!(pf.state.status, Status::Executing);
    }

    #[test]
    fn next_requires_idle() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        setup(root);
        instance::create(root, "wf", "i1", None, Limits::default()).unwrap();
        next(root, "wf", "i1", false).unwrap();
        assert!(next(root, "wf", "i1", false).is_err());
    }

    #[test]
    fn complete_requires_output_and_advances() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        setup(root);
        instance::create(root, "wf", "i1", None, Limits::default()).unwrap();
        next(root, "wf", "i1", false).unwrap();
        assert!(complete(root, "wf", "i1", "").is_err());
        complete(root, "wf", "i1", "产物").unwrap();
        let pf = ProcessFile::read(&root.join(".workflows/wf/instance/i1/process.md")).unwrap();
        assert_eq!(pf.state.current, "end");
        assert_eq!(pf.state.status, Status::Idle);
        let out = next(root, "wf", "i1", false).unwrap();
        assert!(out.contains("已完成"));
    }
}
