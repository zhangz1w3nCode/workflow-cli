use std::io::Write;
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
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

fn subdir_count(path: &Path) -> usize {
    std::fs::read_dir(path)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .count()
        })
        .unwrap_or(0)
}

fn setup(root: &Path) {
    let wf = root.join(".workflows/wf");
    std::fs::create_dir_all(wf.join("meta-data")).unwrap();
    std::fs::write(
        wf.join("meta-data/flow.json"),
        r#"{
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
    }"#,
    )
    .unwrap();
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

    let (ok, _, _) = run(
        root,
        &["complete", "--instance", id, "--output", "理解产物"],
    );
    assert!(ok);

    let (ok, out, _) = run(root, &["next", "--instance", id]);
    assert!(ok && out.contains("- 调研"));

    let (ok, _, _) = run(
        root,
        &["complete", "--instance", id, "--output", "调研产物"],
    );
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
    let base = root
        .join(".workflows/wf/instance")
        .join(&id)
        .join("artifacts");
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
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物内容"],
    );
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "调研产物内容"],
    );

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
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物内容"],
    );
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "调研产物内容"],
    );

    let (ok, out, err) = run(
        root,
        &["artifact", "view", "--instance", id, "--node", "任务理解"],
    );
    assert!(ok, "view --node failed: {err}");
    assert!(out.contains("理解产物内容"), "应包含产物内容: {out}");

    let (ok, out, _) = run(
        root,
        &["artifact", "view", "--instance", id, "--node", "任务调研"],
    );
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
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物内容"],
    );

    let (_, list_out, _) = run(root, &["artifact", "list", "--instance", id]);
    let invoke_id = list_out
        .lines()
        .find(|l| l.contains("任务理解"))
        .and_then(|l| l.split('|').nth(3))
        .map(|s| s.trim().to_string())
        .expect("应找到任务理解的 invoke-id");
    assert!(invoke_id.starts_with("invoke-"));

    let (ok, out, err) = run(
        root,
        &["artifact", "view", "--instance", id, "--invoke", &invoke_id],
    );
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

    let (ok, _, err) = run(
        root,
        &[
            "artifact",
            "view",
            "--instance",
            id,
            "--node",
            "不存在的节点",
        ],
    );
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

#[test]
fn artifact_search_finds_keyword_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物内容"],
    );
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "调研产物内容"],
    );

    let (ok, out, err) = run(
        root,
        &["artifact", "search", "--instance", id, "--keyword", "调研"],
    );
    assert!(ok, "search failed: {err}");
    assert!(out.contains("任务调研"), "应包含节点 任务调研: {out}");
    assert!(!out.contains("任务理解"), "不应包含节点 任务理解: {out}");
}

#[test]
fn artifact_timeline_shows_all_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf", "--input", "测试任务"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物内容"],
    );
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "调研产物内容"],
    );
    run(root, &["next", "--instance", id]);
    run(root, &["choose", "--instance", id, "--branch", "没问题"]);

    let (ok, out, err) = run(root, &["artifact", "timeline", "--instance", id]);
    assert!(ok, "timeline failed: {err}");
    assert!(out.contains("理解产物内容"), "应包含产物内容: {out}");
    assert!(out.contains("调研产物内容"), "应包含产物内容: {out}");
    assert!(out.contains("执行时间线"), "应包含时间线标题: {out}");
    assert!(out.contains("产物详情"), "应包含产物详情标题: {out}");
    assert!(out.contains("测试任务"), "应包含初始任务: {out}");
}

#[test]
fn artifact_diff_shows_changes_between_executions() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物"],
    );
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "第一次调研结果"],
    );
    run(root, &["next", "--instance", id]);
    run(root, &["choose", "--instance", id, "--branch", "其他"]);
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "第二次调研结果"],
    );

    let (ok, out, err) = run(
        root,
        &["artifact", "diff", "--instance", id, "--node", "任务调研"],
    );
    assert!(ok, "diff failed: {err}");
    assert!(out.contains("- 第一次调研结果"), "应显示删除行: {out}");
    assert!(out.contains("+ 第二次调研结果"), "应显示新增行: {out}");
}

#[test]
fn artifact_diff_single_execution_errors() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物"],
    );

    let (ok, _, err) = run(
        root,
        &["artifact", "diff", "--instance", id, "--node", "任务理解"],
    );
    assert!(!ok);
    assert!(err.contains("仅执行 1 次"));
}

#[test]
fn artifact_diff_context_limit_mode() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    let old = "aaa\nbbb\nccc\nddd\neee\nfff\nggg\nhhh\niii\njjj";
    let new = "aaa\nbbb\nccc\nddd\neee-x\nfff\nggg\nhhh\niii\njjj";

    run(root, &["next", "--instance", id]);
    run(root, &["complete", "--instance", id, "--output", "理解"]);
    run(root, &["next", "--instance", id]);
    run(root, &["complete", "--instance", id, "--output", old]);
    run(root, &["next", "--instance", id]);
    run(root, &["choose", "--instance", id, "--branch", "其他"]);
    run(root, &["next", "--instance", id]);
    run(root, &["complete", "--instance", id, "--output", new]);

    let (ok, out, err) = run(
        root,
        &[
            "artifact",
            "diff",
            "--instance",
            id,
            "--node",
            "任务调研",
            "--context",
            "1",
        ],
    );
    assert!(ok, "diff --context 1 failed: {err}");
    assert!(out.contains("- eee"), "应显示删除行: {out}");
    assert!(out.contains("+ eee-x"), "应显示新增行: {out}");
    assert!(out.contains("ddd"), "应显示变更前1行: {out}");
    assert!(out.contains("fff"), "应显示变更后1行: {out}");
    assert!(!out.contains("aaa"), "不应显示远离变更的行: {out}");
    assert!(!out.contains("jjj"), "不应显示远离变更的行: {out}");

    let (ok, out, _) = run(
        root,
        &[
            "artifact",
            "diff",
            "--instance",
            id,
            "--node",
            "任务调研",
            "--full",
        ],
    );
    assert!(ok);
    assert!(out.contains("aaa"), "--full 应显示全部行: {out}");
    assert!(out.contains("jjj"), "--full 应显示全部行: {out}");
}

#[test]
fn context_set_and_get() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    let (ok, _, err) = run(
        root,
        &[
            "context",
            "set",
            "--instance",
            id,
            "--topic",
            "调研",
            "--content",
            "完成了项目调研",
        ],
    );
    assert!(ok, "context set failed: {err}");

    let (ok, out, err) = run(root, &["context", "get", "--instance", id]);
    assert!(ok, "context get failed: {err}");
    assert!(out.contains("调研"), "应包含 topic: {out}");
    assert!(out.contains("完成了项目调研"), "应包含 content: {out}");

    // 追加第二条
    let (ok, _, _) = run(
        root,
        &[
            "context",
            "set",
            "--instance",
            id,
            "--topic",
            "方案",
            "--content",
            "设计了方案",
        ],
    );
    assert!(ok);

    let (ok, out, _) = run(root, &["context", "get", "--instance", id]);
    assert!(ok);
    assert!(out.contains("调研"), "应保留第一条: {out}");
    assert!(out.contains("方案"), "应包含第二条: {out}");
    assert!(out.contains("完成了项目调研"), "应保留第一条内容: {out}");
    assert!(out.contains("设计了方案"), "应包含第二条内容: {out}");
}

#[test]
fn context_get_empty() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    let (ok, out, _) = run(root, &["context", "get", "--instance", id]);
    assert!(ok);
    assert!(out.contains("无暂存上下文"));
}

#[test]
fn timeline_includes_context() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf", "--input", "测试任务"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物"],
    );
    run(
        root,
        &[
            "context",
            "set",
            "--instance",
            id,
            "--topic",
            "阶段1",
            "--content",
            "完成了理解阶段",
        ],
    );

    let (ok, out, err) = run(root, &["artifact", "timeline", "--instance", id]);
    assert!(ok, "timeline failed: {err}");
    assert!(out.contains("上下文"), "应包含上下文标题: {out}");
    assert!(out.contains("阶段1"), "应包含 topic: {out}");
    assert!(out.contains("完成了理解阶段"), "应包含 content: {out}");
}

#[test]
fn artifact_list_json_output() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物内容"],
    );

    let (ok, out, err) = run(root, &["artifact", "list", "--instance", id, "--json"]);
    assert!(ok, "json output failed: {err}");
    assert!(
        out.contains("\"artifacts\""),
        "应包含 artifacts 字段: {out}"
    );
    assert!(out.contains("\"node\""), "应包含 node 字段: {out}");
    assert!(out.contains("\"invoke\""), "应包含 invoke 字段: {out}");
}

#[test]
fn artifact_timeline_json_output() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf", "--input", "测试任务"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物内容"],
    );
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "调研产物内容"],
    );

    let (ok, out, err) = run(root, &["artifact", "timeline", "--instance", id, "--json"]);
    assert!(ok, "json output failed: {err}");
    assert!(out.contains("\"timeline\""), "应包含 timeline 字段: {out}");
    assert!(out.contains("\"content\""), "应包含 content 字段: {out}");
    assert!(
        out.contains("\"initial_input\""),
        "应包含 initial_input 字段: {out}"
    );
    assert!(out.contains("理解产物内容"), "应包含产物内容: {out}");
}

#[test]
fn artifact_view_json_output() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物内容"],
    );

    let (ok, out, err) = run(
        root,
        &[
            "artifact",
            "view",
            "--instance",
            id,
            "--node",
            "任务理解",
            "--json",
        ],
    );
    assert!(ok, "view --json failed: {err}");
    assert!(
        out.contains("\"artifacts\""),
        "应包含 artifacts 字段: {out}"
    );
    assert!(out.contains("\"content\""), "应包含 content 字段: {out}");
    assert!(out.contains("\"order\""), "应包含 order 字段: {out}");
    assert!(out.contains("\"node\""), "应包含 node 字段: {out}");
    assert!(out.contains("\"invoke\""), "应包含 invoke 字段: {out}");
    assert!(out.contains("理解产物内容"), "应包含产物内容: {out}");
}

#[test]
fn artifact_search_json_output() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物内容"],
    );
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "调研产物内容"],
    );

    let (ok, out, err) = run(
        root,
        &[
            "artifact",
            "search",
            "--instance",
            id,
            "--keyword",
            "调研",
            "--json",
        ],
    );
    assert!(ok, "search --json failed: {err}");
    assert!(out.contains("\"results\""), "应包含 results 字段: {out}");
    assert!(out.contains("\"keyword\""), "应包含 keyword 字段: {out}");
    assert!(out.contains("任务调研"), "应包含节点 任务调研: {out}");
    assert!(!out.contains("任务理解"), "不应包含节点 任务理解: {out}");
}

#[test]
fn artifact_diff_json_output() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物"],
    );
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "第一次调研结果"],
    );
    run(root, &["next", "--instance", id]);
    run(root, &["choose", "--instance", id, "--branch", "其他"]);
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "第二次调研结果"],
    );

    let (ok, out, err) = run(
        root,
        &[
            "artifact",
            "diff",
            "--instance",
            id,
            "--node",
            "任务调研",
            "--json",
        ],
    );
    assert!(ok, "diff --json failed: {err}");
    assert!(out.contains("\"diffs\""), "应包含 diffs 字段: {out}");
    assert!(
        out.contains("\"from_invoke\""),
        "应包含 from_invoke 字段: {out}"
    );
    assert!(
        out.contains("\"to_invoke\""),
        "应包含 to_invoke 字段: {out}"
    );
    assert!(out.contains("\"changes\""), "应包含 changes 字段: {out}");
    assert!(out.contains("\"added\""), "应包含 added 类型: {out}");
    assert!(out.contains("\"removed\""), "应包含 removed 类型: {out}");
}

#[test]
fn context_get_json_output() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    let (ok, out, err) = run(root, &["context", "get", "--instance", id, "--json"]);
    assert!(ok, "context get --json empty failed: {err}");
    assert!(out.contains("\"context\""), "应包含 context 字段: {out}");
    assert!(out.contains("null"), "空 context 应为 null: {out}");

    run(
        root,
        &[
            "context",
            "set",
            "--instance",
            id,
            "--topic",
            "阶段1",
            "--content",
            "完成了理解阶段",
        ],
    );

    let (ok, out, err) = run(root, &["context", "get", "--instance", id, "--json"]);
    assert!(ok, "context get --json failed: {err}");
    assert!(out.contains("阶段1"), "应包含 topic: {out}");
    assert!(out.contains("完成了理解阶段"), "应包含 content: {out}");
}

#[test]
fn artifact_search_no_match() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物内容"],
    );

    let (ok, out, err) = run(
        root,
        &[
            "artifact",
            "search",
            "--instance",
            id,
            "--keyword",
            "不存在的关键词",
        ],
    );
    assert!(ok, "search no-match failed: {err}");
    assert!(!out.contains("任务理解"), "不应包含命中节点: {out}");

    let (ok, out, err) = run(
        root,
        &[
            "artifact",
            "search",
            "--instance",
            id,
            "--keyword",
            "不存在的关键词",
            "--json",
        ],
    );
    assert!(ok, "search no-match json failed: {err}");
    assert!(out.contains("\"results\""), "应包含 results 字段: {out}");
    assert!(out.contains("[]"), "空 results 应为空数组: {out}");
}

#[test]
fn artifact_view_invoke_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物内容"],
    );

    let (ok, _, err) = run(
        root,
        &[
            "artifact",
            "view",
            "--instance",
            id,
            "--invoke",
            "invoke-nonexistent",
        ],
    );
    assert!(!ok);
    assert!(err.contains("未找到执行ID"), "应报错未找到执行ID: {err}");
}

#[test]
fn artifact_list_empty_instance() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    let (ok, out, err) = run(root, &["artifact", "list", "--instance", id]);
    assert!(ok, "list empty failed: {err}");
    assert!(out.contains("节点"), "应包含表头: {out}");
    assert!(!out.contains("任务理解"), "空实例不应有节点: {out}");
}

#[test]
fn artifact_list_loop_back_multiple_executions() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物"],
    );
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "第一次调研"],
    );
    run(root, &["next", "--instance", id]);
    run(root, &["choose", "--instance", id, "--branch", "其他"]);
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "第二次调研"],
    );

    let (ok, out, err) = run(root, &["artifact", "list", "--instance", id]);
    assert!(ok, "list loop-back failed: {err}");
    let count = out.matches("任务调研").count();
    assert!(count >= 2, "任务调研应出现至少2次，实际{}次: {out}", count);
}

#[test]
fn artifact_view_loop_back_returns_all() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物"],
    );
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "第一次调研"],
    );
    run(root, &["next", "--instance", id]);
    run(root, &["choose", "--instance", id, "--branch", "其他"]);
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "第二次调研"],
    );

    let (ok, out, err) = run(
        root,
        &["artifact", "view", "--instance", id, "--node", "任务调研"],
    );
    assert!(ok, "view loop-back failed: {err}");
    assert!(out.contains("第一次调研"), "应包含第一次产物: {out}");
    assert!(out.contains("第二次调研"), "应包含第二次产物: {out}");
    assert!(out.contains("[1/2]"), "应显示 [1/2] 标记: {out}");
    assert!(out.contains("[2/2]"), "应显示 [2/2] 标记: {out}");
}

#[test]
fn artifact_view_invoke_precedence() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物"],
    );
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "第一次调研"],
    );
    run(root, &["next", "--instance", id]);
    run(root, &["choose", "--instance", id, "--branch", "其他"]);
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "第二次调研"],
    );

    let (_, list_out, _) = run(root, &["artifact", "list", "--instance", id]);
    let first_invoke = list_out
        .lines()
        .find(|l| l.contains("任务调研"))
        .and_then(|l| l.split('|').nth(3))
        .map(|s| s.trim().to_string())
        .expect("应找到任务调研的 invoke-id");

    let (ok, out, err) = run(
        root,
        &[
            "artifact",
            "view",
            "--instance",
            id,
            "--node",
            "任务调研",
            "--invoke",
            &first_invoke,
        ],
    );
    assert!(ok, "view precedence failed: {err}");
    assert!(out.contains("第一次调研"), "应包含第一次产物: {out}");
    assert!(
        !out.contains("第二次调研"),
        "不应包含第二次（invoke 精确匹配优先）: {out}"
    );
}

#[test]
fn artifact_view_no_artifact_content_null() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物"],
    );
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "调研产物"],
    );
    run(root, &["next", "--instance", id]);

    let (ok, out, err) = run(
        root,
        &[
            "artifact",
            "view",
            "--instance",
            id,
            "--node",
            "审核",
            "--json",
        ],
    );
    assert!(ok, "view no-artifact --json failed: {err}");
    assert!(out.contains("null"), "无产物 content 应为 null: {out}");
    assert!(out.contains("none"), "type 应为 none: {out}");
}

#[test]
fn artifact_search_loop_back_multiple() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物"],
    );
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "调研关键词A"],
    );
    run(root, &["next", "--instance", id]);
    run(root, &["choose", "--instance", id, "--branch", "其他"]);
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "调研关键词B"],
    );

    let (ok, out, err) = run(
        root,
        &[
            "artifact",
            "search",
            "--instance",
            id,
            "--keyword",
            "调研关键词",
        ],
    );
    assert!(ok, "search loop-back failed: {err}");
    let count = out.matches("任务调研").count();
    assert!(count >= 2, "任务调研应出现至少2次，实际{}次: {out}", count);
    assert!(
        !out.contains("任务理解"),
        "不应包含任务理解（产物不含关键词）: {out}"
    );
}

#[test]
fn artifact_timeline_no_context() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物"],
    );

    let (ok, out, err) = run(root, &["artifact", "timeline", "--instance", id]);
    assert!(ok, "timeline no-context failed: {err}");
    assert!(out.contains("执行时间线"), "应包含时间线: {out}");
    assert!(
        !out.contains("## 上下文"),
        "无 context 时不应包含上下文段: {out}"
    );
}

#[test]
fn artifact_diff_no_changes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物"],
    );
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "相同内容"],
    );
    run(root, &["next", "--instance", id]);
    run(root, &["choose", "--instance", id, "--branch", "其他"]);
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "相同内容"],
    );

    let (ok, out, err) = run(
        root,
        &["artifact", "diff", "--instance", id, "--node", "任务调研"],
    );
    assert!(ok, "diff no-changes failed: {err}");
    assert!(out.contains("无变更"), "相同产物应显示无变更: {out}");
}

#[test]
fn artifact_diff_full_json() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    let old = "aaa\nbbb\nccc\nddd\neee\nfff\nggg\nhhh\niii\njjj";
    let new = "aaa\nbbb\nccc\nddd\neee-x\nfff\nggg\nhhh\niii\njjj";
    run(root, &["next", "--instance", id]);
    run(root, &["complete", "--instance", id, "--output", "理解"]);
    run(root, &["next", "--instance", id]);
    run(root, &["complete", "--instance", id, "--output", old]);
    run(root, &["next", "--instance", id]);
    run(root, &["choose", "--instance", id, "--branch", "其他"]);
    run(root, &["next", "--instance", id]);
    run(root, &["complete", "--instance", id, "--output", new]);

    let (ok, out, err) = run(
        root,
        &[
            "artifact",
            "diff",
            "--instance",
            id,
            "--node",
            "任务调研",
            "--full",
            "--json",
        ],
    );
    assert!(ok, "diff --full --json failed: {err}");
    assert!(out.contains("aaa"), "--full 应包含全部行 aaa: {out}");
    assert!(out.contains("jjj"), "--full 应包含全部行 jjj: {out}");
    assert!(out.contains("eee-x"), "应包含变更行: {out}");
    assert!(out.contains("added"), "应包含 added 类型: {out}");
}

#[test]
fn context_multiple_entries_in_timeline() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物"],
    );
    run(
        root,
        &[
            "context",
            "set",
            "--instance",
            id,
            "--topic",
            "阶段1",
            "--content",
            "完成理解",
        ],
    );
    run(
        root,
        &[
            "context",
            "set",
            "--instance",
            id,
            "--topic",
            "阶段2",
            "--content",
            "开始调研",
        ],
    );

    let (ok, out, err) = run(root, &["artifact", "timeline", "--instance", id]);
    assert!(ok, "timeline multiple-context failed: {err}");
    assert!(out.contains("阶段1"), "应包含第一条 topic: {out}");
    assert!(out.contains("完成理解"), "应包含第一条 content: {out}");
    assert!(out.contains("阶段2"), "应包含第二条 topic: {out}");
    assert!(out.contains("开始调研"), "应包含第二条 content: {out}");
}

#[test]
fn search_empty_keyword_errors() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    let (ok, _, err) = run(
        root,
        &["artifact", "search", "--instance", id, "--keyword", ""],
    );
    assert!(!ok);
    assert!(err.contains("不能为空"), "应报错关键词不能为空: {err}");
}

#[test]
fn context_set_empty_topic_errors() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    let (ok, _, err) = run(
        root,
        &[
            "context",
            "set",
            "--instance",
            id,
            "--topic",
            "",
            "--content",
            "内容",
        ],
    );
    assert!(!ok);
    assert!(
        err.contains("topic 不能为空"),
        "应报错 topic 不能为空: {err}"
    );
}

#[test]
fn context_set_empty_content_errors() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    let (ok, _, err) = run(
        root,
        &[
            "context",
            "set",
            "--instance",
            id,
            "--topic",
            "主题",
            "--content",
            "",
        ],
    );
    assert!(!ok);
    assert!(
        err.contains("content 不能为空"),
        "应报错 content 不能为空: {err}"
    );
}

#[test]
fn error_artifact_coverage() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["fail", "--instance", id, "--reason", "执行失败原因"],
    );

    let (ok, out, err) = run(root, &["artifact", "list", "--instance", id]);
    assert!(ok, "list after fail failed: {err}");
    assert!(out.contains("error"), "应包含 error 类型: {out}");

    let (ok, out, err) = run(
        root,
        &["artifact", "view", "--instance", id, "--node", "任务理解"],
    );
    assert!(ok, "view after fail failed: {err}");
    assert!(out.contains("执行失败原因"), "应包含 error 内容: {out}");
}

#[test]
fn list_with_end_node_invoke_dash() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物"],
    );
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "调研产物"],
    );
    run(root, &["next", "--instance", id]);
    run(root, &["choose", "--instance", id, "--branch", "没问题"]);
    run(root, &["next", "--instance", id]);

    let (ok, out, err) = run(root, &["artifact", "list", "--instance", id]);
    assert!(ok, "list with end failed: {err}");
    assert!(out.contains("结束"), "应包含 end 节点: {out}");
    assert!(out.contains("none"), "end 节点 type 应为 none: {out}");
}

#[test]
fn view_no_artifact_text_output() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物"],
    );
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "调研产物"],
    );
    run(root, &["next", "--instance", id]);

    let (ok, out, err) = run(
        root,
        &["artifact", "view", "--instance", id, "--node", "审核"],
    );
    assert!(ok, "view no-artifact text failed: {err}");
    assert!(out.contains("无产物"), "应显示无产物: {out}");
}

#[test]
fn timeline_json_with_none_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物"],
    );
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "调研产物"],
    );
    run(root, &["next", "--instance", id]);

    let (ok, out, err) = run(root, &["artifact", "timeline", "--instance", id, "--json"]);
    assert!(ok, "timeline json with none failed: {err}");
    assert!(out.contains("null"), "审核节点 content 应为 null: {out}");
}

#[test]
fn timeline_text_with_none_artifact() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物"],
    );
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "调研产物"],
    );
    run(root, &["next", "--instance", id]);

    let (ok, out, err) = run(root, &["artifact", "timeline", "--instance", id]);
    assert!(ok, "timeline text with none failed: {err}");
    assert!(out.contains("无产物"), "应显示无产物: {out}");
}

#[test]
fn diff_separator_coverage() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    let old = "aaa\nbbb\nccc\nddd\neee\nfff\nggg";
    let new = "aaa\nbbb-x\nccc\nddd\neee\nfff\nggg-x";
    run(root, &["next", "--instance", id]);
    run(root, &["complete", "--instance", id, "--output", "理解"]);
    run(root, &["next", "--instance", id]);
    run(root, &["complete", "--instance", id, "--output", old]);
    run(root, &["next", "--instance", id]);
    run(root, &["choose", "--instance", id, "--branch", "其他"]);
    run(root, &["next", "--instance", id]);
    run(root, &["complete", "--instance", id, "--output", new]);

    let (ok, out, err) = run(
        root,
        &[
            "artifact",
            "diff",
            "--instance",
            id,
            "--node",
            "任务调研",
            "--context",
            "1",
            "--json",
        ],
    );
    assert!(ok, "diff separator json failed: {err}");
    assert!(out.contains("separator"), "应包含 separator 类型: {out}");

    let (ok, out, err) = run(
        root,
        &[
            "artifact",
            "diff",
            "--instance",
            id,
            "--node",
            "任务调研",
            "--context",
            "1",
        ],
    );
    assert!(ok, "diff separator text failed: {err}");
    assert!(out.contains("..."), "应包含 separator 标记: {out}");
}

#[test]
fn diff_multiple_pairs_newline() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(root, &["complete", "--instance", id, "--output", "理解"]);
    run(root, &["next", "--instance", id]);
    run(root, &["complete", "--instance", id, "--output", "第一次"]);
    run(root, &["next", "--instance", id]);
    run(root, &["choose", "--instance", id, "--branch", "其他"]);
    run(root, &["next", "--instance", id]);
    run(root, &["complete", "--instance", id, "--output", "第二次"]);
    run(root, &["next", "--instance", id]);
    run(root, &["choose", "--instance", id, "--branch", "其他"]);
    run(root, &["next", "--instance", id]);
    run(root, &["complete", "--instance", id, "--output", "第三次"]);

    let (ok, out, err) = run(
        root,
        &["artifact", "diff", "--instance", id, "--node", "任务调研"],
    );
    assert!(ok, "diff multiple pairs failed: {err}");
    assert!(out.contains("第一次"), "应包含第一次: {out}");
    assert!(out.contains("第二次"), "应包含第二次: {out}");
    assert!(out.contains("第三次"), "应包含第三次: {out}");
}

#[test]
fn diff_max_lines_exceeded() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    let big: String = (0..5001)
        .map(|i| format!("line-{i}"))
        .collect::<Vec<_>>()
        .join("\n");
    run(root, &["next", "--instance", id]);
    run(root, &["complete", "--instance", id, "--output", "理解"]);
    run(root, &["next", "--instance", id]);
    run(root, &["complete", "--instance", id, "--output", &big]);
    run(root, &["next", "--instance", id]);
    run(root, &["choose", "--instance", id, "--branch", "其他"]);
    run(root, &["next", "--instance", id]);
    run(root, &["complete", "--instance", id, "--output", &big]);

    let (ok, _, err) = run(
        root,
        &["artifact", "diff", "--instance", id, "--node", "任务调研"],
    );
    assert!(!ok);
    assert!(err.contains("超过"), "应报错行数超限: {err}");
}

#[test]
fn timeline_context_no_trailing_newline() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物"],
    );

    let ctx_path = root
        .join(".workflows/wf/instance")
        .join(id)
        .join("context.md");
    std::fs::create_dir_all(ctx_path.parent().unwrap()).unwrap();
    std::fs::write(&ctx_path, "## 手动上下文\n\n无尾随换行").unwrap();

    let (ok, out, err) = run(root, &["artifact", "timeline", "--instance", id]);
    assert!(ok, "timeline no-trailing-newline failed: {err}");
    assert!(out.contains("无尾随换行"), "应包含 context 内容: {out}");
}

#[test]
fn view_empty_detail_md() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物"],
    );

    let artifacts_dir = root
        .join(".workflows/wf/instance")
        .join(id)
        .join("artifacts")
        .join("任务理解");
    let invoke_dir: std::path::PathBuf = std::fs::read_dir(&artifacts_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.path().is_dir())
        .unwrap()
        .path();
    std::fs::write(invoke_dir.join("detail.md"), "").unwrap();

    let (ok, out, err) = run(
        root,
        &[
            "artifact",
            "view",
            "--instance",
            id,
            "--node",
            "任务理解",
            "--json",
        ],
    );
    assert!(ok, "view empty detail failed: {err}");
    assert!(
        out.contains("null"),
        "空 detail.md content 应为 null: {out}"
    );
}

#[test]
fn search_and_timeline_with_end_node() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物"],
    );
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "调研产物"],
    );
    run(root, &["next", "--instance", id]);
    run(root, &["choose", "--instance", id, "--branch", "没问题"]);
    run(root, &["next", "--instance", id]);

    let (ok, _, err) = run(
        root,
        &["artifact", "search", "--instance", id, "--keyword", "产物"],
    );
    assert!(ok, "search with end failed: {err}");

    let (ok, out, err) = run(root, &["artifact", "timeline", "--instance", id]);
    assert!(ok, "timeline with end failed: {err}");
    assert!(out.contains("结束"), "应包含 end 节点: {out}");
}

#[test]
fn view_empty_error_md() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(root, &["fail", "--instance", id, "--reason", "失败原因"]);

    let artifacts_dir = root
        .join(".workflows/wf/instance")
        .join(id)
        .join("artifacts")
        .join("任务理解");
    let invoke_dir: std::path::PathBuf = std::fs::read_dir(&artifacts_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.path().is_dir())
        .unwrap()
        .path();
    std::fs::write(invoke_dir.join("error.md"), "").unwrap();

    let (ok, out, err) = run(
        root,
        &[
            "artifact",
            "view",
            "--instance",
            id,
            "--node",
            "任务理解",
            "--json",
        ],
    );
    assert!(ok, "view empty error failed: {err}");
    assert!(out.contains("null"), "空 error.md content 应为 null: {out}");
}

#[test]
fn trace_jsonl_created_on_instance_create() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    let trace_path = root
        .join(".workflows/wf/instance")
        .join(id)
        .join("trace/trace.jsonl");
    assert!(
        trace_path.exists(),
        "trace.jsonl should exist after instance creation"
    );

    let content = std::fs::read_to_string(&trace_path).unwrap();
    assert!(
        content.contains("\"command\":\"instance create\""),
        "trace.jsonl should contain instance create entry: {content}"
    );
}

#[test]
fn trace_jsonl_records_workflow_commands() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物"],
    );

    let trace_path = root
        .join(".workflows/wf/instance")
        .join(id)
        .join("trace/trace.jsonl");
    let content = std::fs::read_to_string(&trace_path).unwrap();
    let lines: Vec<&str> = content.lines().filter(|l| !l.is_empty()).collect();

    assert!(
        lines.len() >= 3,
        "trace.jsonl should have at least 3 entries, got {}: {content}",
        lines.len()
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains("\"command\":\"instance create\"")),
        "should have instance create"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains("\"command\":\"next\"") && l.contains("\"status\":\"active\"")),
        "should have next active"
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains("\"command\":\"complete\"")
                && l.contains("\"status\":\"completed\"")),
        "should have complete completed"
    );
}

#[test]
fn trace_jsonl_records_readonly_commands() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物"],
    );
    run(root, &["status", "--instance", id]);
    run(root, &["artifact", "list", "--instance", id]);

    let trace_path = root
        .join(".workflows/wf/instance")
        .join(id)
        .join("trace/trace.jsonl");
    let content = std::fs::read_to_string(&trace_path).unwrap();

    assert!(
        content.contains("\"command\":\"status\""),
        "should have status command: {content}"
    );
    assert!(
        content.contains("\"command\":\"artifact list\""),
        "should have artifact list command: {content}"
    );
}

#[test]
fn trace_jsonl_records_choose_with_branch() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物"],
    );
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "调研产物"],
    );
    run(root, &["next", "--instance", id]);
    run(root, &["choose", "--instance", id, "--branch", "其他"]);

    let trace_path = root
        .join(".workflows/wf/instance")
        .join(id)
        .join("trace/trace.jsonl");
    let content = std::fs::read_to_string(&trace_path).unwrap();

    assert!(
        content.contains("\"command\":\"choose\""),
        "should have choose command: {content}"
    );
    assert!(
        content.contains("\"branch\":\"其他\""),
        "should have branch info: {content}"
    );
}

#[test]
fn trace_jsonl_records_fail_command() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["fail", "--instance", id, "--reason", "执行失败原因"],
    );

    let trace_path = root
        .join(".workflows/wf/instance")
        .join(id)
        .join("trace/trace.jsonl");
    let content = std::fs::read_to_string(&trace_path).unwrap();

    assert!(
        content.contains("\"command\":\"fail\"") && content.contains("\"status\":\"failed\""),
        "should have fail command with failed status: {content}"
    );
}

#[test]
fn artifact_list_reads_from_trace_jsonl() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物内容"],
    );
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "调研产物内容"],
    );

    let (ok, out, err) = run(root, &["artifact", "list", "--instance", id]);
    assert!(ok, "artifact list failed: {err}");
    assert!(
        out.contains("任务理解"),
        "should contain node 任务理解: {out}"
    );
    assert!(
        out.contains("任务调研"),
        "should contain node 任务调研: {out}"
    );
    assert!(
        out.contains("completed"),
        "should contain completed status: {out}"
    );
    assert!(out.contains("detail"), "should contain detail type: {out}");
}

#[test]
fn artifact_list_fallback_without_trace_jsonl() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物内容"],
    );
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "调研产物内容"],
    );

    let trace_dir = root.join(".workflows/wf/instance").join(id).join("trace");
    std::fs::remove_dir_all(&trace_dir).unwrap();

    let (ok, out, err) = run(root, &["artifact", "list", "--instance", id]);
    assert!(ok, "artifact list fallback failed: {err}");
    assert!(
        out.contains("任务理解"),
        "should contain node 任务理解 (from process.md fallback): {out}"
    );
    assert!(
        out.contains("任务调研"),
        "should contain node 任务调研 (from process.md fallback): {out}"
    );
    assert!(
        out.contains("completed"),
        "should contain completed status: {out}"
    );
}

#[test]
fn trace_jsonl_end_node_completed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物"],
    );
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "调研产物"],
    );
    run(root, &["next", "--instance", id]);
    run(root, &["choose", "--instance", id, "--branch", "没问题"]);
    run(root, &["next", "--instance", id]);

    let trace_path = root
        .join(".workflows/wf/instance")
        .join(id)
        .join("trace/trace.jsonl");
    let content = std::fs::read_to_string(&trace_path).unwrap();

    assert!(
        content.contains("\"command\":\"next\"") && content.contains("\"status\":\"completed\""),
        "should have next completed (end node): {content}"
    );
    assert!(
        content.contains("\"invoke\":\"-\""),
        "should have invoke dash for end node: {content}"
    );
}

#[test]
fn trace_jsonl_readonly_only_falls_back_to_process_md() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物"],
    );
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "调研产物"],
    );

    let trace_path = root
        .join(".workflows/wf/instance")
        .join(id)
        .join("trace/trace.jsonl");
    std::fs::write(
        &trace_path,
        "{\"ts\":\"2026-08-22-20-00-00\",\"command\":\"status\"}\n",
    )
    .unwrap();

    let (ok, out, err) = run(root, &["artifact", "list", "--instance", id]);
    assert!(ok, "artifact list with readonly-only jsonl failed: {err}");
    assert!(
        out.contains("任务理解"),
        "should contain node 任务理解 (fallback when jsonl has no workflow entries): {out}"
    );
    assert!(
        out.contains("任务调研"),
        "should contain node 任务调研 (fallback when jsonl has no workflow entries): {out}"
    );
}

#[test]
fn trace_jsonl_partial_history_merges_with_process_md() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物"],
    );
    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "调研产物"],
    );

    let trace_path = root
        .join(".workflows/wf/instance")
        .join(id)
        .join("trace/trace.jsonl");
    let partial = "{\"ts\":\"2026-08-22-20-00-00\",\"command\":\"next\",\"node\":\"任务理解\",\"invoke\":\"invoke-partial\",\"status\":\"active\"}\n";
    std::fs::write(&trace_path, partial).unwrap();

    let (ok, out, err) = run(root, &["artifact", "list", "--instance", id]);
    assert!(ok, "artifact list with partial jsonl failed: {err}");
    assert!(
        out.contains("任务理解"),
        "should contain node 任务理解 (merged from process.md): {out}"
    );
    assert!(
        out.contains("任务调研"),
        "should contain node 任务调研 (merged from process.md): {out}"
    );
    assert!(
        out.contains("invoke-partial"),
        "should contain jsonl-only entry invoke-partial: {out}"
    );
}

#[test]
fn process_md_trace_derived_from_trace_jsonl() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    setup(root);

    let (_, id, _) = run(root, &["instance", "wf"]);
    let id = id.trim();

    run(root, &["next", "--instance", id]);
    run(
        root,
        &["complete", "--instance", id, "--output", "理解产物"],
    );

    let trace_path = root
        .join(".workflows/wf/instance")
        .join(id)
        .join("trace/trace.jsonl");
    let fake_entry =
        "{\"ts\":\"2026-01-01\",\"command\":\"next\",\"node\":\"DECOUPLED_TEST\",\"invoke\":\"invoke-decoupled\",\"status\":\"active\"}\n";
    std::fs::OpenOptions::new()
        .append(true)
        .open(&trace_path)
        .unwrap()
        .write_all(fake_entry.as_bytes())
        .unwrap();

    let (ok, out, err) = run(root, &["artifact", "list", "--instance", id]);
    assert!(ok, "artifact list after jsonl modify failed: {err}");
    assert!(
        out.contains("DECOUPLED_TEST"),
        "artifact list should show DECOUPLED_TEST from trace.jsonl: {out}"
    );

    run(root, &["next", "--instance", id]);

    let process_md = std::fs::read_to_string(
        root.join(".workflows/wf/instance")
            .join(id)
            .join("process.md"),
    )
    .unwrap();
    assert!(
        process_md.contains("DECOUPLED_TEST"),
        "process.md trace table should be derived from trace.jsonl (should contain DECOUPLED_TEST): {process_md}"
    );
}
