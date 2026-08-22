use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Idle,
    Executing,
    AwaitingChoice,
    Completed,
    Aborted,
}

impl Status {
    pub fn as_str(&self) -> &'static str {
        match self {
            Status::Idle => "idle",
            Status::Executing => "executing",
            Status::AwaitingChoice => "awaiting_choice",
            Status::Completed => "completed",
            Status::Aborted => "aborted",
        }
    }
}

pub fn gen_invoke_id() -> String {
    let now = chrono::Local::now();
    format!(
        "invoke-{}-{:03}",
        now.format("%Y%m%d-%H%M%S"),
        now.timestamp_subsec_millis()
    )
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Limits {
    pub max_steps: usize,
    pub max_loop: usize,
    pub max_retry: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_steps: 100,
            max_loop: 10,
            max_retry: 2,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProcessState {
    pub workflow: String,
    pub instance_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_input: Option<String>,
    pub status: Status,
    pub current: String,
    pub current_name: String,
    pub current_invoke: String,
    pub step: usize,
    pub loop_count: usize,
    pub retry_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_node: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_invoke: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub completed: Vec<String>,
    pub limits: Limits,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEvent {
    pub status: String,
    pub node: String,
    pub invoke: String,
    pub branch: Option<String>,
    pub time: String,
}

pub struct ProcessFile {
    pub state: ProcessState,
    pub mermaid: String,
    pub trace: Vec<TraceEvent>,
}

impl ProcessFile {
    pub fn read(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("读取 process.md 失败 {}: {e}", path.display()))?;
        let mut lines = content.lines();
        if lines.next() != Some("---") {
            return Err("process.md 缺少 frontmatter 开头 ---".into());
        }
        let mut yaml_lines = Vec::new();
        for line in lines.by_ref() {
            if line == "---" {
                break;
            }
            yaml_lines.push(line);
        }
        let yaml_str = yaml_lines.join("\n");
        let body = lines.collect::<Vec<_>>().join("\n");
        let marker = "## 执行轨迹";
        let (mermaid, trace_text) = match body.find(marker) {
            Some(idx) => {
                let raw = body[..idx].trim().to_string();
                let after = body[idx + marker.len()..]
                    .trim_start_matches('\n')
                    .to_string();
                let mermaid = raw
                    .strip_prefix("## 流程进度")
                    .map(|s| s.trim().to_string())
                    .unwrap_or(raw);
                (mermaid, after)
            }
            None => (String::new(), body),
        };
        let state: ProcessState = serde_yaml::from_str(&yaml_str)
            .map_err(|e| format!("解析 process.md frontmatter 失败: {e}"))?;
        let trace = parse_trace_table(&trace_text);
        Ok(ProcessFile {
            state,
            mermaid,
            trace,
        })
    }

    pub fn write(&self, path: &Path) -> Result<(), String> {
        let yaml =
            serde_yaml::to_string(&self.state).map_err(|e| format!("序列化状态失败: {e}"))?;
        let trace_table = render_trace_table(&self.trace);
        let body = format!(
            "## 流程进度\n\n{}\n\n## 执行轨迹\n\n{}",
            self.mermaid, trace_table
        );
        let content = format!("---\n{yaml}---\n\n{body}\n");
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, content).map_err(|e| format!("写入失败: {e}"))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("替换失败: {e}"))?;
        Ok(())
    }

    pub fn append_trace(&mut self, status: &str, node: &str, invoke: &str, branch: Option<&str>) {
        if let Some(e) = self
            .trace
            .iter_mut()
            .find(|e| e.node == node && e.invoke == invoke)
        {
            e.status = status.to_string();
            if branch.is_some() {
                e.branch = branch.map(|s| s.to_string());
            }
        } else {
            let time = chrono::Local::now().format("%Y-%m-%d-%H-%M-%S").to_string();
            self.trace.push(TraceEvent {
                status: status.to_string(),
                node: node.to_string(),
                invoke: invoke.to_string(),
                branch: branch.map(|s| s.to_string()),
                time,
            });
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TraceLogEntry {
    pub ts: String,
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invoke: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

pub fn trace_jsonl_path(inst_dir: &Path) -> PathBuf {
    inst_dir.join("trace").join("trace.jsonl")
}

pub fn write_trace_log(path: &Path, entry: &TraceLogEntry) -> Result<(), String> {
    use std::io::Write;
    let line = serde_json::to_string(entry).map_err(|e| format!("序列化 trace 日志失败: {e}"))?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("打开 trace.jsonl 失败: {e}"))?;
    file.write_all(format!("{line}\n").as_bytes())
        .map_err(|e| format!("写入 trace.jsonl 失败: {e}"))?;
    Ok(())
}

pub fn log_trace(inst_dir: &Path, entry: TraceLogEntry) -> Result<(), String> {
    let trace_dir = inst_dir.join("trace");
    std::fs::create_dir_all(&trace_dir).map_err(|e| format!("创建 trace 目录失败: {e}"))?;
    write_trace_log(&trace_dir.join("trace.jsonl"), &entry)
}

pub fn read_trace_jsonl(path: &Path) -> Vec<TraceLogEntry> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    content
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(|l| serde_json::from_str::<TraceLogEntry>(l).ok())
        .collect()
}

pub fn reconstruct_trace_from_jsonl(entries: &[TraceLogEntry]) -> Vec<TraceEvent> {
    let mut trace: Vec<TraceEvent> = Vec::new();
    for entry in entries {
        if let (Some(node), Some(invoke), Some(status)) =
            (&entry.node, &entry.invoke, &entry.status)
        {
            if let Some(e) = trace
                .iter_mut()
                .find(|e| &e.node == node && &e.invoke == invoke)
            {
                e.status = status.clone();
                if entry.branch.is_some() {
                    e.branch = entry.branch.clone();
                }
            } else {
                trace.push(TraceEvent {
                    status: status.clone(),
                    node: node.clone(),
                    invoke: invoke.clone(),
                    branch: entry.branch.clone(),
                    time: entry.ts.clone(),
                });
            }
        }
    }
    trace
}

pub fn merge_trace(base: &mut Vec<TraceEvent>, updates: Vec<TraceEvent>) {
    for event in updates {
        if let Some(e) = base
            .iter_mut()
            .find(|e| e.node == event.node && e.invoke == event.invoke)
        {
            e.status = event.status;
            if event.branch.is_some() {
                e.branch = event.branch;
            }
        } else {
            base.push(event);
        }
    }
}

fn render_trace_table(trace: &[TraceEvent]) -> String {
    let mut s = String::from(
        "| # | 状态 | 节点 | 节点执行ID | 执行时间 |\n|---|------|------|-----------|---------|\n",
    );
    for (i, e) in trace.iter().enumerate() {
        let node_display = match &e.branch {
            Some(b) => format!("{}({})", e.node, b),
            None => e.node.clone(),
        };
        s.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            i + 1,
            e.status,
            node_display,
            e.invoke,
            e.time
        ));
    }
    s
}

fn parse_trace_table(text: &str) -> Vec<TraceEvent> {
    let mut events = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = line.split('|').map(|s| s.trim()).collect();
        if cells.len() < 6 {
            continue;
        }
        let num = cells[1];
        if num.is_empty() || num == "#" || num.starts_with('-') {
            continue;
        }
        let (node, branch) = parse_node_cell(cells[3]);
        events.push(TraceEvent {
            status: cells[2].to_string(),
            node,
            invoke: cells[4].to_string(),
            branch,
            time: cells[5].to_string(),
        });
    }
    events
}

fn parse_node_cell(cell: &str) -> (String, Option<String>) {
    if let Some(open) = cell.find('(') {
        if cell.ends_with(')') {
            let node = cell[..open].to_string();
            let branch = cell[open + 1..cell.len() - 1].to_string();
            return (node, Some(branch));
        }
    }
    (cell.to_string(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_state() -> ProcessState {
        ProcessState {
            workflow: "wf".into(),
            instance_id: "id".into(),
            initial_input: Some("任务".into()),
            status: Status::Idle,
            current: "b-1".into(),
            current_name: "任务理解".into(),
            current_invoke: "invoke-20260818-000001".into(),
            step: 0,
            loop_count: 0,
            retry_count: 0,
            last_node: None,
            last_invoke: None,
            completed: vec![],
            limits: Limits::default(),
        }
    }

    #[test]
    fn roundtrip_write_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("process.md");
        let mut pf = ProcessFile {
            state: sample_state(),
            mermaid: "```mermaid\nflowchart TD\n```".into(),
            trace: Vec::new(),
        };
        pf.append_trace("completed", "任务理解", "invoke-1", None);
        pf.write(&path).unwrap();

        let read = ProcessFile::read(&path).unwrap();
        assert_eq!(read.state.workflow, "wf");
        assert_eq!(read.state.initial_input.as_deref(), Some("任务"));
        assert_eq!(read.state.status, Status::Idle);
        assert_eq!(read.state.current_invoke, "invoke-20260818-000001");
        assert_eq!(read.trace.len(), 1);
        assert_eq!(read.trace[0].status, "completed");
        assert_eq!(read.trace[0].node, "任务理解");
        assert_eq!(read.trace[0].invoke, "invoke-1");
    }

    #[test]
    fn default_limits() {
        let l = Limits::default();
        assert_eq!(l.max_steps, 100);
        assert_eq!(l.max_loop, 10);
        assert_eq!(l.max_retry, 2);
    }

    #[test]
    fn status_as_str() {
        assert_eq!(Status::Idle.as_str(), "idle");
        assert_eq!(Status::Executing.as_str(), "executing");
        assert_eq!(Status::AwaitingChoice.as_str(), "awaiting_choice");
        assert_eq!(Status::Completed.as_str(), "completed");
        assert_eq!(Status::Aborted.as_str(), "aborted");
    }
    #[test]
    fn trace_log_entry_serialization() {
        let entry = TraceLogEntry {
            ts: "2026-08-22-19-50-04".into(),
            command: "next".into(),
            node: Some("任务理解".into()),
            invoke: Some("invoke-1".into()),
            status: Some("active".into()),
            branch: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"command\":\"next\""));
        assert!(json.contains("\"node\":\"任务理解\""));
        assert!(!json.contains("branch"));

        let entry2 = TraceLogEntry {
            ts: "2026-08-22-19-50-04".into(),
            command: "status".into(),
            node: None,
            invoke: None,
            status: None,
            branch: None,
        };
        let json2 = serde_json::to_string(&entry2).unwrap();
        assert!(json2.contains("\"command\":\"status\""));
        assert!(!json2.contains("node"));
        assert!(!json2.contains("invoke"));
    }

    #[test]
    fn trace_log_write_read_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trace.jsonl");

        let entry1 = TraceLogEntry {
            ts: "2026-08-22-19-50-04".into(),
            command: "next".into(),
            node: Some("任务理解".into()),
            invoke: Some("invoke-1".into()),
            status: Some("active".into()),
            branch: None,
        };
        let entry2 = TraceLogEntry {
            ts: "2026-08-22-19-50-05".into(),
            command: "complete".into(),
            node: Some("任务理解".into()),
            invoke: Some("invoke-1".into()),
            status: Some("completed".into()),
            branch: None,
        };

        write_trace_log(&path, &entry1).unwrap();
        write_trace_log(&path, &entry2).unwrap();

        let entries = read_trace_jsonl(&path);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].command, "next");
        assert_eq!(entries[0].status.as_deref(), Some("active"));
        assert_eq!(entries[1].command, "complete");
        assert_eq!(entries[1].status.as_deref(), Some("completed"));
    }

    #[test]
    fn reconstruct_trace_upsert_semantics() {
        let entries = vec![
            TraceLogEntry {
                ts: "2026-08-22-19-50-04".into(),
                command: "next".into(),
                node: Some("任务理解".into()),
                invoke: Some("invoke-1".into()),
                status: Some("active".into()),
                branch: None,
            },
            TraceLogEntry {
                ts: "2026-08-22-19-50-05".into(),
                command: "complete".into(),
                node: Some("任务理解".into()),
                invoke: Some("invoke-1".into()),
                status: Some("completed".into()),
                branch: None,
            },
            TraceLogEntry {
                ts: "2026-08-22-19-50-06".into(),
                command: "status".into(),
                node: None,
                invoke: None,
                status: None,
                branch: None,
            },
            TraceLogEntry {
                ts: "2026-08-22-19-50-07".into(),
                command: "next".into(),
                node: Some("任务调研".into()),
                invoke: Some("invoke-2".into()),
                status: Some("active".into()),
                branch: None,
            },
        ];

        let trace = reconstruct_trace_from_jsonl(&entries);
        assert_eq!(trace.len(), 2);
        assert_eq!(trace[0].node, "任务理解");
        assert_eq!(trace[0].status, "completed");
        assert_eq!(trace[0].time, "2026-08-22-19-50-04");
        assert_eq!(trace[1].node, "任务调研");
        assert_eq!(trace[1].status, "active");
    }

    #[test]
    fn reconstruct_trace_with_branch() {
        let entries = vec![TraceLogEntry {
            ts: "2026-08-22-19-50-04".into(),
            command: "choose".into(),
            node: Some("审核".into()),
            invoke: Some("invoke-1".into()),
            status: Some("completed".into()),
            branch: Some("没问题".into()),
        }];

        let trace = reconstruct_trace_from_jsonl(&entries);
        assert_eq!(trace.len(), 1);
        assert_eq!(trace[0].branch.as_deref(), Some("没问题"));
    }
}
