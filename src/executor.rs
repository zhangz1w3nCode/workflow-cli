use crate::artifact;
use crate::graph::Graph;
use crate::model::Flow;
use crate::state::{ProcessFile, Status};
use std::path::{Path, PathBuf};

fn instance_dir(root: &Path, workflow: &str, instance_id: &str) -> PathBuf {
    root.join(".workflows")
        .join(workflow)
        .join("instance")
        .join(instance_id)
}

fn load_flow(root: &Path, workflow: &str) -> Result<Flow, String> {
    let path = root
        .join(".workflows")
        .join(workflow)
        .join("meta-data")
        .join("flow.json");
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
        pf.append_trace("completed", &node.data.label, "-", None);
        crate::state::log_trace(
            &inst_dir,
            crate::state::TraceLogEntry {
                ts: chrono::Local::now().format("%Y-%m-%d-%H-%M-%S").to_string(),
                command: "next".into(),
                node: Some(node.data.label.clone()),
                invoke: Some("-".into()),
                status: Some("completed".into()),
                branch: None,
            },
        )?;
        pf.mermaid = render_mermaid(&flow, &pf.state);
        pf.write(&inst_dir.join("process.md"))?;
        return Ok("工作流已完成".into());
    }

    if let (Some(last_node), Some(last_invoke)) = (
        pf.state.last_node.as_deref(),
        pf.state.last_invoke.as_deref(),
    ) {
        if !artifact::has_detail(root, workflow, instance_id, last_node, last_invoke) {
            return Err(format!(
                "节点 {last_node} ({last_invoke}) 未写入产物，请补写后再 next"
            ));
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
    let invoke = pf.state.current_invoke.clone();
    pf.append_trace("active", &node.data.label, &invoke, None);
    crate::state::log_trace(
        &inst_dir,
        crate::state::TraceLogEntry {
            ts: chrono::Local::now().format("%Y-%m-%d-%H-%M-%S").to_string(),
            command: "next".into(),
            node: Some(node.data.label.clone()),
            invoke: Some(invoke.clone()),
            status: Some("active".into()),
            branch: None,
        },
    )?;
    pf.mermaid = render_mermaid(&flow, &pf.state);
    pf.write(&inst_dir.join("process.md"))?;

    Ok(render_node(root, node, &pf.state.current_invoke, json))
}

pub fn complete(
    root: &Path,
    workflow: &str,
    instance_id: &str,
    output: &str,
) -> Result<String, String> {
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

    artifact::write_detail(
        root,
        workflow,
        instance_id,
        &pf.state.current_name,
        &pf.state.current_invoke,
        output,
    )?;

    let name = pf.state.current_name.clone();
    let invoke = pf.state.current_invoke.clone();
    pf.append_trace("completed", &name, &invoke, None);
    crate::state::log_trace(
        &inst_dir,
        crate::state::TraceLogEntry {
            ts: chrono::Local::now().format("%Y-%m-%d-%H-%M-%S").to_string(),
            command: "complete".into(),
            node: Some(name.clone()),
            invoke: Some(invoke.clone()),
            status: Some("completed".into()),
            branch: None,
        },
    )?;
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

pub fn fail(
    root: &Path,
    workflow: &str,
    instance_id: &str,
    reason: &str,
) -> Result<String, String> {
    let inst_dir = instance_dir(root, workflow, instance_id);
    let mut pf = ProcessFile::read(&inst_dir.join("process.md"))?;

    if pf.state.status != Status::Executing {
        return Err("当前无执行中的业务节点".into());
    }

    let flow = load_flow(root, workflow)?;

    artifact::write_error(
        root,
        workflow,
        instance_id,
        &pf.state.current_name,
        &pf.state.current_invoke,
        reason,
    )?;
    let name = pf.state.current_name.clone();
    let invoke = pf.state.current_invoke.clone();
    pf.append_trace("failed", &name, &invoke, None);
    crate::state::log_trace(
        &inst_dir,
        crate::state::TraceLogEntry {
            ts: chrono::Local::now().format("%Y-%m-%d-%H-%M-%S").to_string(),
            command: "fail".into(),
            node: Some(name.clone()),
            invoke: Some(invoke.clone()),
            status: Some("failed".into()),
            branch: None,
        },
    )?;

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

pub fn choose(
    root: &Path,
    workflow: &str,
    instance_id: &str,
    branch: &str,
    reason: Option<&str>,
) -> Result<String, String> {
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
        Some(r) => format!("## 选择分支\n- {branch}\n\n## 理由\n- {r}"),
        None => format!("## 选择分支\n- {branch}"),
    };
    artifact::write_detail(
        root,
        workflow,
        instance_id,
        &pf.state.current_name,
        &pf.state.current_invoke,
        &detail,
    )?;
    let name = pf.state.current_name.clone();
    let invoke = pf.state.current_invoke.clone();
    pf.append_trace("completed", &name, &invoke, Some(branch));
    crate::state::log_trace(
        &inst_dir,
        crate::state::TraceLogEntry {
            ts: chrono::Local::now().format("%Y-%m-%d-%H-%M-%S").to_string(),
            command: "choose".into(),
            node: Some(name.clone()),
            invoke: Some(invoke.clone()),
            status: Some("completed".into()),
            branch: Some(branch.to_string()),
        },
    )?;
    if !pf.state.completed.contains(&pf.state.current_name) {
        pf.state.completed.push(pf.state.current_name.clone());
    }

    let branch_id = node
        .data
        .branches
        .iter()
        .find(|b| b.name == branch)
        .map(|b| b.id.as_str());
    if let Some(bid) = branch_id {
        if graph.is_loop_back(&pf.state.current, bid, &pf.state.completed) {
            pf.state.loop_count += 1;
        }
    }

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
    Ok(format!(
        "已选择分支 {branch}，推进到 {}",
        next_node.data.label
    ))
}

pub fn status(
    root: &Path,
    workflow: &str,
    instance_id: &str,
    json: bool,
) -> Result<String, String> {
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
    if !base.exists() {
        return Ok(String::new());
    }
    let mut names: Vec<String> = std::fs::read_dir(&base)
        .map_err(|e| format!("读取 .workflows 失败: {e}"))?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter(|e| e.path().join("meta-data/flow.json").exists())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    Ok(names.join("\n"))
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
        .filter(|p| {
            workflow_filter
                .map(|w| p.file_name().and_then(|n| n.to_str()) == Some(w))
                .unwrap_or(true)
        })
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
            })
            .to_string(),
            _ => serde_json::json!({
                "type": node.node_type,
                "node_id": node.id,
                "node_name": node.data.label,
                "invoke": invoke,
                "content": read_node_md(root, node)
            })
            .to_string(),
        };
    }

    match node.node_type.as_str() {
        "decision" => {
            let mut s = String::from("# 判断内容\n\n");
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
    if node.node_type == "process" {
        return node
            .data
            .content
            .clone()
            .unwrap_or_else(|| format!("process 节点 {} 缺少 content", node.data.label));
    }
    let ref_path = node.data.node_ref_path.as_deref().unwrap_or("");
    if ref_path.is_empty() {
        return format!("节点 {} 缺少 nodeRefPath", node.data.label);
    }
    let full = root.join(ref_path);
    std::fs::read_to_string(&full)
        .unwrap_or_else(|e| format!("节点文件缺失: {} ({e})", full.display()))
}

/// Mermaid 节点 ID 不允许包含空格及多数特殊字符；把非 [字母/数字/_] 的字符统一替换为下划线，
/// 避免带空格/标点的节点 label 被直接当作 ID 导致 mermaid 解析失败。
fn mermaid_id(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn node_ref_name(flow: &Flow, id: &str) -> String {
    match flow.node(id) {
        Some(n) if n.node_type == "start" => "start_node".to_string(),
        Some(n) if n.node_type == "end" => "end_node".to_string(),
        Some(n) => mermaid_id(&n.id),
        None => mermaid_id(id),
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
                let branch_name = flow
                    .node(&edge.source)
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
        } else if state.current_name == node.data.label
            && state.status != crate::state::Status::Completed
        {
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

    #[test]
    fn process_node_outputs_inline_content() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let wf = root.join(".workflows/wf");
        std::fs::create_dir_all(wf.join("meta-data")).unwrap();
        std::fs::write(
            wf.join("meta-data/flow.json"),
            r#"{
          "nodes": [
            {"id":"start","type":"start","data":{"label":"开始"}},
            {"id":"end","type":"end","data":{"label":"结束"}},
            {"id":"p","type":"process","data":{"label":"代码审核","content":"详细审核代码看看"}}
          ],
          "edges": [{"id":"e1","source":"start","target":"p","type":"default"},
                    {"id":"e2","source":"p","target":"end","type":"default"}]
        }"#,
        )
        .unwrap();
        instance::create(root, "wf", "i1", None, Limits::default()).unwrap();
        let out = next(root, "wf", "i1", false).unwrap();
        assert!(out.contains("详细审核代码看看"));
    }

    #[test]
    fn business_node_missing_ref_reports_clearly() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let wf = root.join(".workflows/wf");
        std::fs::create_dir_all(wf.join("meta-data")).unwrap();
        std::fs::write(
            wf.join("meta-data/flow.json"),
            r#"{
          "nodes": [
            {"id":"start","type":"start","data":{"label":"开始"}},
            {"id":"end","type":"end","data":{"label":"结束"}},
            {"id":"a","type":"business","data":{"label":"任务理解"}}
          ],
          "edges": [{"id":"e1","source":"start","target":"a","type":"default"},
                    {"id":"e2","source":"a","target":"end","type":"default"}]
        }"#,
        )
        .unwrap();
        instance::create(root, "wf", "i1", None, Limits::default()).unwrap();
        let out = next(root, "wf", "i1", false).unwrap();
        assert!(out.contains("缺少 nodeRefPath"));
    }

    #[test]
    fn mermaid_render_sanitizes_node_ids_with_spaces() {
        let flow: crate::model::Flow = serde_json::from_str(r#"{
          "nodes": [
            {"id":"start","type":"start","data":{"label":"开始"}},
            {"id":"end","type":"end","data":{"label":"结束"}},
            {"id":"d","type":"decision","data":{"label":"检测 审核","branches":[{"id":"b1","name":"没问题"},{"id":"b2","name":"其他"}]}}
          ],
          "edges": [
            {"id":"e1","source":"start","target":"d","type":"default"},
            {"id":"e2","source":"d","target":"end","branchId":"b1","type":"default"},
            {"id":"e3","source":"d","target":"end","branchId":"b2","type":"default"}
          ]
        }"#).unwrap();
        let state = crate::state::ProcessState {
            workflow: "wf".into(),
            instance_id: "id".into(),
            initial_input: None,
            status: Status::Idle,
            current: "d".into(),
            current_name: "检测 审核".into(),
            current_invoke: "invoke-1".into(),
            step: 0,
            loop_count: 0,
            retry_count: 0,
            last_node: None,
            last_invoke: None,
            completed: vec![],
            limits: Limits::default(),
        };
        let mermaid = render_mermaid(&flow, &state);
        assert!(
            mermaid.contains("d{检测 审核}"),
            "节点 ID 应为安全标识而非带空格的 label:\n{mermaid}"
        );
        assert!(
            !mermaid.contains("检测 审核{检测 审核}"),
            "节点 ID 不应包含空格:\n{mermaid}"
        );
    }

    #[test]
    fn choose_forward_does_not_increment_loop_count() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let wf = root.join(".workflows/wf");
        std::fs::create_dir_all(wf.join("meta-data")).unwrap();
        std::fs::write(wf.join("meta-data/flow.json"), r#"{
          "nodes": [
            {"id":"start","type":"start","data":{"label":"开始"}},
            {"id":"end","type":"end","data":{"label":"结束"}},
            {"id":"a","type":"business","data":{"label":"任务理解","nodeRefPath":".nodes/任务理解.md"}},
            {"id":"d","type":"decision","data":{"label":"查类型","branches":[
              {"id":"code","name":"写代码"},
              {"id":"other","name":"其他","description":"均不符合上述分类的进入本分支"}
            ]}},
            {"id":"p","type":"process","data":{"label":"搜索代码库","content":"直接搜索"}}
          ],
          "edges": [
            {"id":"e1","source":"start","target":"a","type":"default"},
            {"id":"e2","source":"a","target":"d","type":"default"},
            {"id":"e3","source":"d","target":"p","branchId":"other","type":"default"},
            {"id":"e4","source":"p","target":"end","type":"default"},
            {"id":"e5","source":"d","target":"end","branchId":"code","type":"default"}
          ]
        }"#).unwrap();
        std::fs::write(wf.join("WORKFLOW.md"), "# workflow\n").unwrap();
        std::fs::create_dir_all(root.join(".nodes")).unwrap();
        std::fs::write(root.join(".nodes/任务理解.md"), "# 任务\n- 理解任务\n").unwrap();

        instance::create(root, "wf", "i1", None, Limits::default()).unwrap();
        next(root, "wf", "i1", false).unwrap(); // 任务理解 (business)
        complete(root, "wf", "i1", "理解产物").unwrap(); // -> 查类型
        next(root, "wf", "i1", false).unwrap(); // 查类型 (decision)
        choose(root, "wf", "i1", "其他", None).unwrap(); // 正向 -> 搜索代码库

        let pf = ProcessFile::read(&root.join(".workflows/wf/instance/i1/process.md")).unwrap();
        assert_eq!(pf.state.loop_count, 0);
    }

    #[test]
    fn choose_loop_back_increments_loop_count() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let wf = root.join(".workflows/wf");
        std::fs::create_dir_all(wf.join("meta-data")).unwrap();
        std::fs::write(
            wf.join("meta-data/flow.json"),
            r#"{
          "nodes": [
            {"id":"start","type":"start","data":{"label":"开始"}},
            {"id":"end","type":"end","data":{"label":"结束"}},
            {"id":"a","type":"business","data":{"label":"调研","nodeRefPath":".nodes/调研.md"}},
            {"id":"b","type":"business","data":{"label":"写方案","nodeRefPath":".nodes/写方案.md"}},
            {"id":"d","type":"decision","data":{"label":"审核","branches":[
              {"id":"ok","name":"通过"},
              {"id":"other","name":"其他"}
            ]}}
          ],
          "edges": [
            {"id":"e1","source":"start","target":"a","type":"default"},
            {"id":"e2","source":"a","target":"b","type":"default"},
            {"id":"e3","source":"b","target":"d","type":"default"},
            {"id":"e4","source":"d","target":"b","branchId":"other","type":"default"},
            {"id":"e5","source":"d","target":"end","branchId":"ok","type":"default"}
          ]
        }"#,
        )
        .unwrap();
        std::fs::write(wf.join("WORKFLOW.md"), "# workflow\n").unwrap();
        std::fs::create_dir_all(root.join(".nodes")).unwrap();
        std::fs::write(root.join(".nodes/调研.md"), "# 调研\n- 调研\n").unwrap();
        std::fs::write(root.join(".nodes/写方案.md"), "# 写方案\n- 写方案\n").unwrap();

        instance::create(root, "wf", "i1", None, Limits::default()).unwrap();
        next(root, "wf", "i1", false).unwrap(); // 调研
        complete(root, "wf", "i1", "调研产物").unwrap(); // -> 写方案
        next(root, "wf", "i1", false).unwrap(); // 写方案
        complete(root, "wf", "i1", "方案产物").unwrap(); // -> 审核
        next(root, "wf", "i1", false).unwrap(); // 审核 (decision)
        choose(root, "wf", "i1", "其他", None).unwrap(); // 回流 -> 写方案

        let pf = ProcessFile::read(&root.join(".workflows/wf/instance/i1/process.md")).unwrap();
        assert_eq!(pf.state.loop_count, 1);
    }
}
