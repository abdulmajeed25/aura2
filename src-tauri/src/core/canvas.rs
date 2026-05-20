//! Phase 12: infinite-canvas document format.
//!
//! Canvases are stored as `.canvas` JSON files inside the vault. The shape
//! is intentionally close to Obsidian's so files are portable: `nodes` is a
//! list of cards (file refs or free-form text), `edges` connects two node
//! ids visually. The backend's responsibility is bounded to parsing,
//! validating, and writing the JSON; layout and interaction live in the UI.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum CanvasNode {
    File {
        id: String,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        file: String,
    },
    Text {
        id: String,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        text: String,
    },
}

impl CanvasNode {
    pub fn id(&self) -> &str {
        match self {
            CanvasNode::File { id, .. } | CanvasNode::Text { id, .. } => id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasEdge {
    pub id: String,
    #[serde(rename = "fromNode")]
    pub from_node: String,
    #[serde(rename = "toNode")]
    pub to_node: String,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CanvasDoc {
    #[serde(default)]
    pub nodes: Vec<CanvasNode>,
    #[serde(default)]
    pub edges: Vec<CanvasEdge>,
}

impl CanvasDoc {
    pub fn parse(s: &str) -> serde_json::Result<Self> {
        if s.trim().is_empty() {
            return Ok(Self::default());
        }
        serde_json::from_str(s)
    }

    pub fn serialize_pretty(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    /// Validate that every edge references an existing node id and that node
    /// ids are unique. Returns the offending message rather than panicking.
    pub fn validate(&self) -> Result<(), String> {
        let mut seen = std::collections::HashSet::new();
        for n in &self.nodes {
            if !seen.insert(n.id().to_string()) {
                return Err(format!("duplicate node id: {}", n.id()));
            }
        }
        for e in &self.edges {
            if !seen.contains(&e.from_node) {
                return Err(format!("edge {} references unknown node {}", e.id, e.from_node));
            }
            if !seen.contains(&e.to_node) {
                return Err(format!("edge {} references unknown node {}", e.id, e.to_node));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_canvas() {
        let src = r#"{
            "nodes": [
                {"type":"file","id":"n1","x":0,"y":0,"width":300,"height":200,"file":"Welcome.md"},
                {"type":"text","id":"n2","x":350,"y":0,"width":250,"height":150,"text":"hello"}
            ],
            "edges": [
                {"id":"e1","fromNode":"n1","toNode":"n2"}
            ]
        }"#;
        let doc = CanvasDoc::parse(src).unwrap();
        assert_eq!(doc.nodes.len(), 2);
        assert_eq!(doc.edges.len(), 1);
        doc.validate().unwrap();
    }

    #[test]
    fn empty_string_yields_empty_doc() {
        let doc = CanvasDoc::parse("").unwrap();
        assert!(doc.nodes.is_empty());
        assert!(doc.edges.is_empty());
    }

    #[test]
    fn validate_catches_dangling_edges() {
        let mut doc = CanvasDoc::default();
        doc.edges.push(CanvasEdge {
            id: "e".into(),
            from_node: "missing".into(),
            to_node: "also-missing".into(),
            label: None,
        });
        let err = doc.validate().unwrap_err();
        assert!(err.contains("unknown node"));
    }

    #[test]
    fn validate_catches_duplicate_ids() {
        let doc = CanvasDoc {
            nodes: vec![
                CanvasNode::Text {
                    id: "x".into(),
                    x: 0.0,
                    y: 0.0,
                    width: 100.0,
                    height: 100.0,
                    text: "a".into(),
                },
                CanvasNode::Text {
                    id: "x".into(),
                    x: 10.0,
                    y: 10.0,
                    width: 100.0,
                    height: 100.0,
                    text: "b".into(),
                },
            ],
            edges: vec![],
        };
        let err = doc.validate().unwrap_err();
        assert!(err.contains("duplicate"));
    }

    #[test]
    fn roundtrips_through_pretty_json() {
        let doc = CanvasDoc {
            nodes: vec![CanvasNode::File {
                id: "n1".into(),
                x: 0.0,
                y: 0.0,
                width: 300.0,
                height: 200.0,
                file: "Welcome.md".into(),
            }],
            edges: vec![],
        };
        let s = doc.serialize_pretty().unwrap();
        let back = CanvasDoc::parse(&s).unwrap();
        assert_eq!(back.nodes.len(), 1);
        match &back.nodes[0] {
            CanvasNode::File { file, .. } => assert_eq!(file, "Welcome.md"),
            _ => panic!("expected file node"),
        }
    }
}
