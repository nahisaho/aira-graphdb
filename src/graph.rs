use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub type NodeId = String;
pub type EdgeId = String;
pub type Properties = HashMap<String, Value>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    String(String),
    Int64(i64),
    Float64(f64),
    Bool(bool),
    Bytes(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: NodeId,
    pub labels: Vec<String>,
    pub properties: Properties,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: EdgeId,
    pub from: NodeId,
    pub to: NodeId,
    pub edge_type: String,
    pub properties: Properties,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct InMemoryGraphStore {
    nodes: HashMap<NodeId, GraphNode>,
    edges: HashMap<EdgeId, GraphEdge>,
    next_node_seq: u64,
    next_edge_seq: u64,
}

impl InMemoryGraphStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn create_node(&mut self, labels: Vec<String>, properties: Properties) -> GraphNode {
        self.next_node_seq += 1;
        let id = format!("n{}", self.next_node_seq);
        let node = GraphNode {
            id: id.clone(),
            labels,
            properties,
        };
        self.nodes.insert(id.clone(), node.clone());
        node
    }

    pub fn get_node(&self, node_id: &str) -> Option<&GraphNode> {
        self.nodes.get(node_id)
    }

    pub fn get_node_mut(&mut self, node_id: &str) -> Option<&mut GraphNode> {
        self.nodes.get_mut(node_id)
    }

    pub fn update_node(
        &mut self,
        node_id: &str,
        labels: Vec<String>,
        properties: Properties,
    ) -> Option<GraphNode> {
        let node = self.nodes.get_mut(node_id)?;
        node.labels = labels;
        node.properties = properties;
        Some(node.clone())
    }

    pub fn delete_node(&mut self, node_id: &str) -> bool {
        self.nodes.remove(node_id).is_some()
    }

    pub fn create_edge(
        &mut self,
        from: &str,
        to: &str,
        edge_type: String,
        properties: Properties,
    ) -> Option<GraphEdge> {
        if !self.nodes.contains_key(from) || !self.nodes.contains_key(to) {
            return None;
        }
        self.next_edge_seq += 1;
        let id = format!("e{}", self.next_edge_seq);
        let edge = GraphEdge {
            id: id.clone(),
            from: from.to_string(),
            to: to.to_string(),
            edge_type,
            properties,
        };
        self.edges.insert(id.clone(), edge.clone());
        Some(edge)
    }

    pub fn get_edge(&self, edge_id: &str) -> Option<&GraphEdge> {
        self.edges.get(edge_id)
    }

    pub fn update_edge(
        &mut self,
        edge_id: &str,
        edge_type: String,
        properties: Properties,
    ) -> Option<GraphEdge> {
        let edge = self.edges.get_mut(edge_id)?;
        edge.edge_type = edge_type;
        edge.properties = properties;
        Some(edge.clone())
    }

    pub fn delete_edge(&mut self, edge_id: &str) -> bool {
        self.edges.remove(edge_id).is_some()
    }

    pub fn has_node(&self, node_id: &str) -> bool {
        self.nodes.contains_key(node_id)
    }

    pub fn list_nodes(&self) -> Vec<GraphNode> {
        let mut nodes: Vec<GraphNode> = self.nodes.values().cloned().collect();
        nodes.sort_by(|a, b| a.id.cmp(&b.id));
        nodes
    }

    pub fn find_node_by_label_and_props(
        &self,
        label: Option<&str>,
        props: &Properties,
    ) -> Option<GraphNode> {
        self.nodes.values().find_map(|node| {
            if let Some(label) = label
                && !node.labels.iter().any(|l| l == label)
            {
                return None;
            }
            let all_match = props.iter().all(|(k, v)| node.properties.get(k) == Some(v));
            if all_match { Some(node.clone()) } else { None }
        })
    }

    pub fn edges_touching_node(&self, node_id: &str) -> Vec<EdgeId> {
        self.edges
            .values()
            .filter(|edge| edge.from == node_id || edge.to == node_id)
            .map(|edge| edge.id.clone())
            .collect()
    }

    pub fn delete_node_detach(&mut self, node_id: &str) -> bool {
        let touching = self.edges_touching_node(node_id);
        for edge_id in touching {
            self.edges.remove(&edge_id);
        }
        self.delete_node(node_id)
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    pub fn list_edges(&self) -> Vec<GraphEdge> {
        let mut edges: Vec<GraphEdge> = self.edges.values().cloned().collect();
        edges.sort_by(|a, b| a.id.cmp(&b.id));
        edges
    }

    pub fn rename_label(&mut self, from: &str, to: &str) -> usize {
        let mut updated = 0usize;
        for node in self.nodes.values_mut() {
            let mut changed = false;
            for label in &mut node.labels {
                if label == from {
                    *label = to.to_string();
                    changed = true;
                }
            }
            if changed {
                updated += 1;
            }
        }
        updated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn props(key: &str, value: Value) -> Properties {
        let mut p = Properties::new();
        p.insert(key.to_string(), value);
        p
    }

    #[test]
    fn node_crud_works() {
        let mut store = InMemoryGraphStore::new();
        let created = store.create_node(
            vec!["Paper".to_string()],
            props("title", Value::String("MemGraphRAG".to_string())),
        );
        assert_eq!(store.node_count(), 1);

        let fetched = store.get_node(&created.id).expect("node must exist");
        assert_eq!(fetched.labels, vec!["Paper".to_string()]);

        let updated = store
            .update_node(
                &created.id,
                vec!["Paper".to_string(), "Benchmark".to_string()],
                props("year", Value::Int64(2026)),
            )
            .expect("node update must succeed");
        assert_eq!(updated.labels.len(), 2);

        assert!(store.delete_node(&created.id));
        assert_eq!(store.node_count(), 0);
    }

    #[test]
    fn edge_crud_works() {
        let mut store = InMemoryGraphStore::new();
        let n1 = store.create_node(
            vec!["Author".to_string()],
            props("name", Value::String("A".into())),
        );
        let n2 = store.create_node(
            vec!["Paper".to_string()],
            props("title", Value::String("B".into())),
        );

        let created = store
            .create_edge(&n1.id, &n2.id, "WROTE".to_string(), Properties::new())
            .expect("edge creation must succeed");
        assert_eq!(store.edge_count(), 1);

        let fetched = store.get_edge(&created.id).expect("edge must exist");
        assert_eq!(fetched.edge_type, "WROTE");

        let updated = store
            .update_edge(
                &created.id,
                "CONTRIBUTED_TO".to_string(),
                props("weight", Value::Float64(0.8)),
            )
            .expect("edge update must succeed");
        assert_eq!(updated.edge_type, "CONTRIBUTED_TO");

        assert!(store.delete_edge(&created.id));
        assert_eq!(store.edge_count(), 0);
    }

    #[test]
    fn edge_creation_requires_existing_nodes() {
        let mut store = InMemoryGraphStore::new();
        let result = store.create_edge("n1", "n2", "REL".to_string(), Properties::new());
        assert!(result.is_none());
    }

    #[test]
    fn detach_delete_removes_connected_edges() {
        let mut store = InMemoryGraphStore::new();
        let n1 = store.create_node(vec!["Node".to_string()], Properties::new());
        let n2 = store.create_node(vec!["Node".to_string()], Properties::new());
        let edge = store
            .create_edge(&n1.id, &n2.id, "LINK".to_string(), Properties::new())
            .expect("edge create");
        assert!(store.get_edge(&edge.id).is_some());

        assert!(store.delete_node_detach(&n1.id));
        assert!(store.get_edge(&edge.id).is_none());
    }

    #[test]
    fn finds_node_by_label_and_props() {
        let mut store = InMemoryGraphStore::new();
        let mut props1 = Properties::new();
        props1.insert("name".to_string(), Value::String("alice".to_string()));
        store.create_node(vec!["Author".to_string()], props1);

        let mut find = Properties::new();
        find.insert("name".to_string(), Value::String("alice".to_string()));
        let hit = store.find_node_by_label_and_props(Some("Author"), &find);
        assert!(hit.is_some());
    }
}
