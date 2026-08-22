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

    /// 选中 branch 后是否构成环回：目标节点 label 已在 visited（已执行）集合中。
    /// 与分支名称 / description 无关，只看「目标是否回到已访问节点」。
    pub fn is_loop_back(&self, decision_id: &str, branch_id: &str, visited: &[String]) -> bool {
        self.next_node(decision_id, Some(branch_id))
            .ok()
            .and_then(|target_id| self.flow.node(&target_id))
            .map(|n| visited.contains(&n.data.label))
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
    fn loop_back_detection() {
        let f = flow();
        let g = Graph::new(&f);
        // "other" 分支回到节点 a（label "A"），构成环回
        assert!(g.is_loop_back("d", "other", &["A".to_string()]));
        // "ok" 分支到 end，非环回
        assert!(!g.is_loop_back("d", "ok", &["A".to_string()]));
        // 目标 label "A" 不在 visited 中，即使走 "other" 也不算环回
        assert!(!g.is_loop_back("d", "other", &["结束".to_string()]));
        assert_eq!(g.branch_names("d"), vec!["没问题", "其他"]);
    }
}
