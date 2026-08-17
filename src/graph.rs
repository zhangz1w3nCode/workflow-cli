use crate::model::{Edge, Flow};

pub struct Graph<'a> {
    flow: &'a Flow,
}

impl<'a> Graph<'a> {
    pub fn new(flow: &'a Flow) -> Self {
        Graph { flow }
    }

    fn outgoing(&self, node_id: &str) -> Vec<&'a Edge> {
        self.flow.edges.iter().filter(|e| e.source == node_id).collect()
    }

    pub fn next_node(&self, current_id: &str, branch_id: Option<&str>) -> Result<String, String> {
        let node = self.flow.node(current_id)
            .ok_or_else(|| format!("节点不存在: {current_id}"))?;

        match node.node_type.as_str() {
            "end" => Err("当前已是结束节点".to_string()),
            "decision" => {
                let bid = branch_id.ok_or_else(|| "decision 节点需要分支选择".to_string())?;
                self.outgoing(current_id).iter()
                    .find(|e| e.branch_id.as_deref() == Some(bid))
                    .map(|e| e.target.clone())
                    .ok_or_else(|| format!("分支 {bid} 不存在"))
            }
            _ => {
                let outs = self.outgoing(current_id);
                match outs.len() {
                    1 => Ok(outs[0].target.clone()),
                    0 => Err(format!("节点 {current_id} 无出边")),
                    _ => Err(format!("business 节点 {current_id} 存在多条出边")),
                }
            }
        }
    }

    pub fn is_catch_all_branch(&self, decision_id: &str, branch_name: &str) -> bool {
        self.flow.node(decision_id)
            .map(|n| n.data.branches.iter()
                .find(|b| b.name == branch_name)
                .map(|b| b.description.is_some())
                .unwrap_or(false))
            .unwrap_or(false)
    }

    pub fn branch_names(&self, decision_id: &str) -> Vec<String> {
        self.flow.node(decision_id)
            .map(|n| n.data.branches.iter().map(|b| b.name.clone()).collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Flow;

    fn flow() -> Flow {
        serde_json::from_str(r#"{
          "nodes": [
            {"id":"start","type":"start","data":{"label":"开始"}},
            {"id":"end","type":"end","data":{"label":"结束"}},
            {"id":"a","type":"business","data":{"label":"A"}},
            {"id":"d","type":"decision","data":{"label":"D","branches":[
              {"id":"ok","name":"没问题"},
              {"id":"other","name":"其他","description":"均不符合"}
            ]}}
          ],
          "edges": [
            {"id":"e1","source":"start","target":"a","type":"default"},
            {"id":"e2","source":"a","target":"d","type":"default"},
            {"id":"e3","source":"d","target":"end","branchId":"ok","type":"default"},
            {"id":"e4","source":"d","target":"a","branchId":"other","type":"default"}
          ]
        }"#).unwrap()
    }

    #[test]
    fn business_single_out() {
        let f = flow();
        let g = Graph::new(&f);
        assert_eq!(g.next_node("a", None).unwrap(), "d");
    }

    #[test]
    fn decision_branch_match() {
        let f = flow();
        let g = Graph::new(&f);
        assert_eq!(g.next_node("d", Some("ok")).unwrap(), "end");
        assert_eq!(g.next_node("d", Some("other")).unwrap(), "a");
    }

    #[test]
    fn decision_requires_branch() {
        let f = flow();
        let g = Graph::new(&f);
        assert!(g.next_node("d", None).is_err());
    }

    #[test]
    fn catch_all_detection() {
        let f = flow();
        let g = Graph::new(&f);
        assert!(!g.is_catch_all_branch("d", "没问题"));
        assert!(g.is_catch_all_branch("d", "其他"));
        assert_eq!(g.branch_names("d"), vec!["没问题", "其他"]);
    }
}
