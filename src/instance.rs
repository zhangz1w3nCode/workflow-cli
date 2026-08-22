use crate::graph::Graph;
use crate::model::Flow;
use crate::state::{Limits, ProcessFile, ProcessState, Status};
use std::path::Path;

pub fn create(
    root: &Path,
    workflow: &str,
    instance_id: &str,
    input: Option<&str>,
    limits: Limits,
) -> Result<String, String> {
    let wf_dir = root.join(".workflows").join(workflow);
    let flow_path = wf_dir.join("meta-data").join("flow.json");
    let flow = Flow::from_file(&flow_path)?;
    let graph = Graph::new(&flow);

    let start = flow.start_node().ok_or("工作流缺少 start 节点")?;
    let first_id = graph.next_node(&start.id, None)?;
    let first = flow.node(&first_id).ok_or("首个节点不存在")?;

    let inst_dir = wf_dir.join("instance").join(instance_id);
    std::fs::create_dir_all(&inst_dir).map_err(|e| format!("创建实例目录失败: {e}"))?;

    let wf_md = wf_dir.join("WORKFLOW.md");
    if wf_md.exists() {
        std::fs::copy(&wf_md, inst_dir.join("instance.md"))
            .map_err(|e| format!("复制 instance.md 失败: {e}"))?;
    }

    let state = ProcessState {
        workflow: workflow.to_string(),
        instance_id: instance_id.to_string(),
        initial_input: input.map(|s| s.to_string()),
        status: Status::Idle,
        current: first_id.clone(),
        current_name: first.data.label.clone(),
        current_invoke: crate::state::gen_invoke_id(),
        step: 0,
        loop_count: 0,
        retry_count: 0,
        last_node: None,
        last_invoke: None,
        completed: vec![],
        limits,
    };

    let mermaid = crate::executor::render_mermaid(&flow, &state);
    let pf = ProcessFile {
        state,
        mermaid,
        trace: Vec::new(),
    };
    pf.write(&inst_dir.join("process.md"))?;
    crate::state::log_trace(
        &inst_dir,
        crate::state::TraceLogEntry {
            ts: chrono::Local::now().format("%Y-%m-%d-%H-%M-%S").to_string(),
            command: "instance create".into(),
            node: None,
            invoke: None,
            status: None,
            branch: None,
        },
    )?;

    Ok(instance_id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Limits;

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
    fn create_instance() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        setup(root);
        let id = create(root, "wf", "inst-1", Some("任务"), Limits::default()).unwrap();
        assert_eq!(id, "inst-1");

        let inst = root.join(".workflows/wf/instance/inst-1");
        assert!(inst.join("instance.md").exists());
        let pf = crate::state::ProcessFile::read(&inst.join("process.md")).unwrap();
        assert_eq!(pf.state.status, crate::state::Status::Idle);
        assert_eq!(pf.state.current, "a");
        assert!(pf.state.current_invoke.starts_with("invoke-"));
        assert_eq!(pf.state.initial_input.as_deref(), Some("任务"));
    }
}
