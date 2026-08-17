use std::path::Path;
use serde::{Deserialize, Serialize};

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
    format!("invoke-{}-{:03}", now.format("%Y%m%d-%H%M%S"), now.timestamp_subsec_millis())
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Limits {
    pub max_steps: usize,
    pub max_loop: usize,
    pub max_retry: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { max_steps: 100, max_loop: 10, max_retry: 2 }
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

#[derive(Debug, Clone)]
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
                let after = body[idx + marker.len()..].trim_start_matches('\n').to_string();
                let mermaid = raw.strip_prefix("## 流程进度").map(|s| s.trim().to_string()).unwrap_or(raw);
                (mermaid, after)
            }
            None => (String::new(), body),
        };
        let state: ProcessState = serde_yaml::from_str(&yaml_str)
            .map_err(|e| format!("解析 process.md frontmatter 失败: {e}"))?;
        let trace = parse_trace_table(&trace_text);
        Ok(ProcessFile { state, mermaid, trace })
    }

    pub fn write(&self, path: &Path) -> Result<(), String> {
        let yaml = serde_yaml::to_string(&self.state)
            .map_err(|e| format!("序列化状态失败: {e}"))?;
        let trace_table = render_trace_table(&self.trace);
        let body = format!("## 流程进度\n\n{}\n\n## 执行轨迹\n\n{}", self.mermaid, trace_table);
        let content = format!("---\n{yaml}---\n\n{body}\n");
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, content).map_err(|e| format!("写入失败: {e}"))?;
        std::fs::rename(&tmp, path).map_err(|e| format!("替换失败: {e}"))?;
        Ok(())
    }

    pub fn append_trace(&mut self, status: &str, node: &str, invoke: &str, branch: Option<&str>) {
        if let Some(e) = self.trace.iter_mut().find(|e| e.node == node && e.invoke == invoke) {
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

fn render_trace_table(trace: &[TraceEvent]) -> String {
    let mut s = String::from("| # | 状态 | 节点 | 节点执行ID | 执行时间 |\n|---|------|------|-----------|---------|\n");
    for (i, e) in trace.iter().enumerate() {
        let node_display = match &e.branch {
            Some(b) => format!("{}({})", e.node, b),
            None => e.node.clone(),
        };
        s.push_str(&format!("| {} | {} | {} | {} | {} |\n", i + 1, e.status, node_display, e.invoke, e.time));
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
        let mut pf = ProcessFile { state: sample_state(), mermaid: "```mermaid\nflowchart TD\n```".into(), trace: Vec::new() };
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
}
