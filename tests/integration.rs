use std::path::Path;
use std::process::Command;

fn workflow_bin() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_workflow"))
}

fn run(root: &Path, args: &[&str]) -> (bool, String, String) {
    let out = Command::new(workflow_bin())
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    (out.status.success(), String::from_utf8_lossy(&out.stdout).to_string(), String::from_utf8_lossy(&out.stderr).to_string())
}

fn subdir_count(path: &Path) -> usize {
    std::fs::read_dir(path)
        .map(|rd| rd.filter_map(|e| e.ok()).filter(|e| e.path().is_dir()).count())
        .unwrap_or(0)
}

fn setup(root: &Path) {
    let wf = root.join(".workflows/wf");
    std::fs::create_dir_all(wf.join("meta-data")).unwrap();
    std::fs::write(wf.join("meta-data/flow.json"), r#"{
      "nodes": [
        {"id":"start","type":"start","data":{"label":"开始"}},
        {"id":"end","type":"end","data":{"label":"结束"}},
        {"id":"a","type":"business","data":{"label":"任务理解","nodeRefPath":".nodes/任务理解.md"}},
        {"id":"b","type":"business","data":{"label":"任务调研","nodeRefPath":".nodes/任务调研.md"}},
        {"id":"d","type":"decision","data":{"label":"审核","condition":"前置产物","branches":[
          {"id":"ok","name":"没问题"},
          {"id":"other","name":"其他","description":"均不符合"}
        ]}}
      ],
      "edges": [
        {"id":"e1","source":"start","target":"a","type":"default"},
        {"id":"e2","source":"a","target":"b","type":"default"},
        {"id":"e3","source":"b","target":"d","type":"default"},
        {"id":"e4","source":"d","target":"end","branchId":"ok","type":"default"},
        {"id":"e5","source":"d","target":"b","branchId":"other","type":"default"}
      ]
    }"#).unwrap();
    std::fs::write(wf.join("WORKFLOW.md"), "# wf\n").unwrap();
    std::fs::create_dir_all(root.join(".nodes")).unwrap();
    std::fs::write(root.join(".nodes/任务理解.md"), "# 任务\n- 理解\n").unwrap();
    std::fs::write(root.join(".nodes/任务调研.md"), "# 任务\n- 调研\n").unwrap();
}

#[test]
fn linear_run_to_completion() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (ok, id, _) = run(root, &["instance", "wf"]);
    assert!(ok);
    let id = id.trim();

    let (ok, out, _) = run(root, &["next", "--instance", id]);
    assert!(ok && out.contains("- 理解"));

    let (ok, _, _) = run(root, &["complete", "--instance", id, "--output", "理解产物"]);
    assert!(ok);

    let (ok, out, _) = run(root, &["next", "--instance", id]);
    assert!(ok && out.contains("- 调研"));

    let (ok, _, _) = run(root, &["complete", "--instance", id, "--output", "调研产物"]);
    assert!(ok);

    let (ok, out, _) = run(root, &["next", "--instance", id]);
    assert!(ok && out.contains("可选分支"));

    let (ok, _, _) = run(root, &["choose", "--instance", id, "--branch", "没问题"]);
    assert!(ok);

    let (ok, out, _) = run(root, &["next", "--instance", id]);
    assert!(ok && out.contains("已完成"));
}

#[test]
fn loop_reentry_increments_invoke() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim().to_string();

    run(root, &["next", "--instance", &id]);
    run(root, &["complete", "--instance", &id, "--output", "a1"]);
    run(root, &["next", "--instance", &id]);
    run(root, &["complete", "--instance", &id, "--output", "b1"]);
    run(root, &["next", "--instance", &id]);
    run(root, &["choose", "--instance", &id, "--branch", "其他"]);

    run(root, &["next", "--instance", &id]);
    let base = root.join(".workflows/wf/instance").join(&id).join("artifacts");
    let dir = base.join("任务调研");
    assert_eq!(subdir_count(&dir), 1);

    run(root, &["complete", "--instance", &id, "--output", "b2"]);
    assert_eq!(subdir_count(&dir), 2);
}

#[test]
fn complete_without_output_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);
    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim().to_string();
    run(root, &["next", "--instance", &id]);
    let (ok, _, err) = run(root, &["complete", "--instance", &id, "--output", ""]);
    assert!(!ok);
    assert!(err.contains("提供产物"));
}

#[test]
fn illegal_transition_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);
    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim().to_string();
    let (ok, _, err) = run(root, &["complete", "--instance", &id, "--output", "x"]);
    assert!(!ok);
    assert!(err.contains("无执行中"));
}

#[test]
fn artifact_list_shows_completed_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (ok, id, _) = run(root, &["instance", "wf"]);
    assert!(ok);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(root, &["complete", "--instance", id, "--output", "理解产物内容"]);
    run(root, &["next", "--instance", id]);
    run(root, &["complete", "--instance", id, "--output", "调研产物内容"]);

    let (ok, out, err) = run(root, &["artifact", "list", "--instance", id]);
    assert!(ok, "artifact list failed: {err}");
    assert!(out.contains("任务理解"), "应包含节点 任务理解: {out}");
    assert!(out.contains("任务调研"), "应包含节点 任务调研: {out}");
    assert!(out.contains("detail"), "应包含类型 detail: {out}");
    assert!(out.contains("completed"), "应包含状态 completed: {out}");
}

#[test]
fn artifact_view_by_node() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(root, &["complete", "--instance", id, "--output", "理解产物内容"]);
    run(root, &["next", "--instance", id]);
    run(root, &["complete", "--instance", id, "--output", "调研产物内容"]);

    let (ok, out, err) = run(root, &["artifact", "view", "--instance", id, "--node", "任务理解"]);
    assert!(ok, "view --node failed: {err}");
    assert!(out.contains("理解产物内容"), "应包含产物内容: {out}");

    let (ok, out, _) = run(root, &["artifact", "view", "--instance", id, "--node", "任务调研"]);
    assert!(ok);
    assert!(out.contains("调研产物内容"));
}

#[test]
fn artifact_view_by_invoke() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(root, &["complete", "--instance", id, "--output", "理解产物内容"]);

    let (_, list_out, _) = run(root, &["artifact", "list", "--instance", id]);
    let invoke_id = list_out
        .lines()
        .find(|l| l.contains("任务理解"))
        .and_then(|l| l.split('|').nth(3))
        .map(|s| s.trim().to_string())
        .expect("应找到任务理解的 invoke-id");
    assert!(invoke_id.starts_with("invoke-"));

    let (ok, out, err) = run(root, &["artifact", "view", "--instance", id, "--invoke", &invoke_id]);
    assert!(ok, "view --invoke failed: {err}");
    assert!(out.contains("理解产物内容"), "应包含产物内容: {out}");
}

#[test]
fn artifact_view_missing_node_errors() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    let (ok, _, err) = run(root, &["artifact", "view", "--instance", id, "--node", "不存在的节点"]);
    assert!(!ok);
    assert!(err.contains("未找到节点"));
}

#[test]
fn artifact_view_no_args_errors() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    let (ok, _, err) = run(root, &["artifact", "view", "--instance", id]);
    assert!(!ok);
    assert!(err.contains("请通过 --node 或 --invoke"));
}
