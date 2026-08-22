use std::path::Path;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Flow {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Node {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub data: NodeData,
}

#[derive(Debug, Deserialize, Clone)]
pub struct NodeData {
    pub label: String,
    #[serde(rename = "nodeRefPath", default)]
    pub node_ref_path: Option<String>,
    #[serde(default)]
    pub condition: Option<String>,
    #[serde(default)]
    pub branches: Vec<Branch>,
    #[serde(default)]
    pub content: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Branch {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Edge {
    pub source: String,
    pub target: String,
    #[serde(rename = "branchId", default)]
    pub branch_id: Option<String>,
}

impl Flow {
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("读取 flow.json 失败 {}: {e}", path.display()))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("解析 flow.json 失败: {e}"))
    }

    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn start_node(&self) -> Option<&Node> {
        self.nodes.iter().find(|n| n.node_type == "start")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLOW: &str = r#"{
      "nodes": [
        {"id": "start-1", "type": "start", "data": {"label": "开始"}},
        {"id": "end-1", "type": "end", "data": {"label": "结束"}},
        {"id": "b-1", "type": "business", "data": {"label": "任务理解", "nodeRefPath": ".nodes/任务理解.md"}},
        {"id": "d-1", "type": "decision", "data": {"label": "审核", "condition": "前置产物", "branches": [
          {"id": "br-1", "name": "没问题"},
          {"id": "br-2", "name": "其他", "description": "均不符合上述分类的进入本分支"}
        ]}}
      ],
      "edges": [
        {"id": "e1", "source": "start-1", "target": "b-1", "type": "default"},
        {"id": "e2", "source": "b-1", "target": "d-1", "type": "default"},
        {"id": "e3", "source": "d-1", "target": "end-1", "branchId": "br-1", "type": "default"}
      ]
    }"#;

    #[test]
    fn parse_valid_flow() {
        let flow: Flow = serde_json::from_str(FLOW).unwrap();
        assert_eq!(flow.nodes.len(), 4);
        assert_eq!(flow.edges.len(), 3);
        assert_eq!(flow.start_node().unwrap().id, "start-1");
    }

    #[test]
    fn node_lookup() {
        let flow: Flow = serde_json::from_str(FLOW).unwrap();
        let d = flow.node("d-1").unwrap();
        assert_eq!(d.data.branches.len(), 2);
        assert_eq!(d.data.branches[1].description.as_deref(), Some("均不符合上述分类的进入本分支"));
    }

    #[test]
    fn missing_optional_fields() {
        let json = r#"{"nodes":[{"id":"s","type":"start","data":{"label":"开始"}}],"edges":[]}"#;
        let flow: Flow = serde_json::from_str(json).unwrap();
        let n = &flow.nodes[0];
        assert_eq!(n.data.node_ref_path, None);
        assert_eq!(n.data.condition, None);
        assert!(n.data.branches.is_empty());
    }
}
