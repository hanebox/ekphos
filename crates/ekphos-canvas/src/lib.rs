//! Pure parser and validation model for the JSON Canvas 1.0 format.

use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanvasColor {
    Preset(u8),
    Hex(String),
    Custom(String),
}

impl CanvasColor {
    pub fn parse(value: String) -> Self {
        if value.len() == 1 {
            if let Some(number @ 1..=6) = value.chars().next().and_then(|character| character.to_digit(10)) {
                return Self::Preset(number as u8);
            }
        }
        if value.starts_with('#') {
            Self::Hex(value)
        } else {
            Self::Custom(value)
        }
    }

    pub fn as_str(&self) -> String {
        match self {
            Self::Preset(value) => value.to_string(),
            Self::Hex(value) | Self::Custom(value) => value.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Canvas {
    pub nodes: Vec<CanvasNode>,
    pub edges: Vec<CanvasEdge>,
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CanvasNode {
    pub id: String,
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
    pub color: Option<CanvasColor>,
    pub kind: CanvasNodeKind,
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CanvasNodeKind {
    Text { text: String },
    File { file: String, subpath: Option<String> },
    Link { url: String },
    Group { label: Option<String>, background: Option<String>, background_style: Option<String> },
    Unknown { kind: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CanvasEdge {
    pub id: String,
    pub from_node: String,
    pub from_side: Option<CanvasSide>,
    pub from_end: CanvasEnd,
    pub to_node: String,
    pub to_side: Option<CanvasSide>,
    pub to_end: CanvasEnd,
    pub color: Option<CanvasColor>,
    pub label: Option<String>,
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasSide {
    Top,
    Right,
    Bottom,
    Left,
}

impl CanvasSide {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "top" => Some(Self::Top),
            "right" => Some(Self::Right),
            "bottom" => Some(Self::Bottom),
            "left" => Some(Self::Left),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasEnd {
    None,
    Arrow,
}

impl CanvasEnd {
    fn parse(value: Option<&str>, default: Self) -> Self {
        match value {
            Some("arrow") => Self::Arrow,
            Some("none") => Self::None,
            _ => default,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanvasDiagnostic {
    DuplicateNodeId(String),
    DuplicateEdgeId(String),
    NonPositiveNodeSize(String),
    DanglingEdge { edge: String, node: String },
    UnknownNodeType { node: String, kind: String },
    UnknownSide { edge: String, side: String },
}

impl fmt::Display for CanvasDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateNodeId(id) => write!(formatter, "duplicate node id '{id}'"),
            Self::DuplicateEdgeId(id) => write!(formatter, "duplicate edge id '{id}'"),
            Self::NonPositiveNodeSize(id) => write!(formatter, "node '{id}' has a non-positive size"),
            Self::DanglingEdge { edge, node } => write!(formatter, "edge '{edge}' references missing node '{node}'"),
            Self::UnknownNodeType { node, kind } => write!(formatter, "node '{node}' has unsupported type '{kind}'"),
            Self::UnknownSide { edge, side } => write!(formatter, "edge '{edge}' has unsupported side '{side}'"),
        }
    }
}

#[derive(Debug)]
pub enum CanvasError {
    Json(serde_json::Error),
}

impl fmt::Display for CanvasError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for CanvasError {}

#[derive(Debug, Deserialize)]
struct RawCanvas {
    #[serde(default)]
    nodes: Vec<RawNode>,
    #[serde(default)]
    edges: Vec<RawEdge>,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct RawNode {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    x: i64,
    y: i64,
    width: i64,
    height: i64,
    color: Option<String>,
    text: Option<String>,
    file: Option<String>,
    subpath: Option<String>,
    url: Option<String>,
    label: Option<String>,
    background: Option<String>,
    #[serde(rename = "backgroundStyle")]
    background_style: Option<String>,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct RawEdge {
    id: String,
    #[serde(rename = "fromNode")]
    from_node: String,
    #[serde(rename = "fromSide")]
    from_side: Option<String>,
    #[serde(rename = "fromEnd")]
    from_end: Option<String>,
    #[serde(rename = "toNode")]
    to_node: String,
    #[serde(rename = "toSide")]
    to_side: Option<String>,
    #[serde(rename = "toEnd")]
    to_end: Option<String>,
    color: Option<String>,
    label: Option<String>,
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

pub fn parse_canvas(source: &str) -> Result<(Canvas, Vec<CanvasDiagnostic>), CanvasError> {
    let raw: RawCanvas = serde_json::from_str(source).map_err(CanvasError::Json)?;
    let mut diagnostics = Vec::new();
    let mut node_ids = HashSet::new();
    let nodes = raw
        .nodes
        .into_iter()
        .map(|node| {
            if !node_ids.insert(node.id.clone()) {
                diagnostics.push(CanvasDiagnostic::DuplicateNodeId(node.id.clone()));
            }
            if node.width <= 0 || node.height <= 0 {
                diagnostics.push(CanvasDiagnostic::NonPositiveNodeSize(node.id.clone()));
            }
            let kind = match node.kind.as_str() {
                "text" => CanvasNodeKind::Text { text: node.text.unwrap_or_default() },
                "file" => CanvasNodeKind::File { file: node.file.unwrap_or_default(), subpath: node.subpath },
                "link" => CanvasNodeKind::Link { url: node.url.unwrap_or_default() },
                "group" => CanvasNodeKind::Group { label: node.label, background: node.background, background_style: node.background_style },
                kind => {
                    diagnostics.push(CanvasDiagnostic::UnknownNodeType { node: node.id.clone(), kind: kind.to_string() });
                    CanvasNodeKind::Unknown { kind: kind.to_string() }
                }
            };
            CanvasNode { id: node.id, x: node.x, y: node.y, width: node.width, height: node.height, color: node.color.map(CanvasColor::parse), kind, extra: node.extra }
        })
        .collect();

    let mut edge_ids = HashSet::new();
    let edges = raw
        .edges
        .into_iter()
        .map(|edge| {
            if !edge_ids.insert(edge.id.clone()) {
                diagnostics.push(CanvasDiagnostic::DuplicateEdgeId(edge.id.clone()));
            }
            for referenced in [&edge.from_node, &edge.to_node] {
                if !node_ids.contains(referenced) {
                    diagnostics.push(CanvasDiagnostic::DanglingEdge { edge: edge.id.clone(), node: referenced.clone() });
                }
            }
            for raw_side in [edge.from_side.as_deref(), edge.to_side.as_deref()].into_iter().flatten() {
                if CanvasSide::parse(raw_side).is_none() {
                    diagnostics.push(CanvasDiagnostic::UnknownSide { edge: edge.id.clone(), side: raw_side.to_string() });
                }
            }
            CanvasEdge {
                id: edge.id,
                from_node: edge.from_node,
                from_side: edge.from_side.as_deref().and_then(CanvasSide::parse),
                from_end: CanvasEnd::parse(edge.from_end.as_deref(), CanvasEnd::None),
                to_node: edge.to_node,
                to_side: edge.to_side.as_deref().and_then(CanvasSide::parse),
                to_end: CanvasEnd::parse(edge.to_end.as_deref(), CanvasEnd::Arrow),
                color: edge.color.map(CanvasColor::parse),
                label: edge.label,
                extra: edge.extra,
            }
        })
        .collect();
    Ok((Canvas { nodes, edges, extra: raw.extra }, diagnostics))
}

impl Canvas {
    pub fn bounds(&self) -> Option<(i64, i64, i64, i64)> {
        let mut nodes = self.nodes.iter();
        let first = nodes.next()?;
        let mut bounds = (first.x, first.y, first.x.saturating_add(first.width), first.y.saturating_add(first.height));
        for node in nodes {
            bounds.0 = bounds.0.min(node.x);
            bounds.1 = bounds.1.min(node.y);
            bounds.2 = bounds.2.max(node.x.saturating_add(node.width));
            bounds.3 = bounds.3.max(node.y.saturating_add(node.height));
        }
        Some(bounds)
    }

    /// Serialize a JSON Canvas 1.0 document while retaining extension fields
    /// that Ekphos does not interpret.
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        let mut root = json_object(&self.extra);
        root.insert("nodes".to_string(), serde_json::Value::Array(self.nodes.iter().map(node_json).collect()));
        root.insert("edges".to_string(), serde_json::Value::Array(self.edges.iter().map(edge_json).collect()));
        serde_json::to_string_pretty(&serde_json::Value::Object(root))
    }
}

fn json_object(extra: &BTreeMap<String, serde_json::Value>) -> serde_json::Map<String, serde_json::Value> {
    extra.iter().map(|(key, value)| (key.clone(), value.clone())).collect()
}

fn node_json(node: &CanvasNode) -> serde_json::Value {
    let mut object = json_object(&node.extra);
    object.insert("id".to_string(), node.id.clone().into());
    object.insert("type".to_string(), node_kind_name(&node.kind).into());
    object.insert("x".to_string(), node.x.into());
    object.insert("y".to_string(), node.y.into());
    object.insert("width".to_string(), node.width.into());
    object.insert("height".to_string(), node.height.into());
    if let Some(color) = &node.color {
        object.insert("color".to_string(), color.as_str().into());
    }
    match &node.kind {
        CanvasNodeKind::Text { text } => {
            object.insert("text".to_string(), text.clone().into());
        }
        CanvasNodeKind::File { file, subpath } => {
            object.insert("file".to_string(), file.clone().into());
            if let Some(subpath) = subpath {
                object.insert("subpath".to_string(), subpath.clone().into());
            }
        }
        CanvasNodeKind::Link { url } => {
            object.insert("url".to_string(), url.clone().into());
        }
        CanvasNodeKind::Group { label, background, background_style } => {
            if let Some(label) = label {
                object.insert("label".to_string(), label.clone().into());
            }
            if let Some(background) = background {
                object.insert("background".to_string(), background.clone().into());
            }
            if let Some(background_style) = background_style {
                object.insert("backgroundStyle".to_string(), background_style.clone().into());
            }
        }
        CanvasNodeKind::Unknown { .. } => {}
    }
    serde_json::Value::Object(object)
}

fn edge_json(edge: &CanvasEdge) -> serde_json::Value {
    let mut object = json_object(&edge.extra);
    object.insert("id".to_string(), edge.id.clone().into());
    object.insert("fromNode".to_string(), edge.from_node.clone().into());
    object.insert("toNode".to_string(), edge.to_node.clone().into());
    if let Some(side) = edge.from_side {
        object.insert("fromSide".to_string(), side_name(side).into());
    }
    if edge.from_end != CanvasEnd::None {
        object.insert("fromEnd".to_string(), end_name(edge.from_end).into());
    }
    if let Some(side) = edge.to_side {
        object.insert("toSide".to_string(), side_name(side).into());
    }
    if edge.to_end != CanvasEnd::Arrow {
        object.insert("toEnd".to_string(), end_name(edge.to_end).into());
    }
    if let Some(color) = &edge.color {
        object.insert("color".to_string(), color.as_str().into());
    }
    if let Some(label) = &edge.label {
        object.insert("label".to_string(), label.clone().into());
    }
    serde_json::Value::Object(object)
}

fn node_kind_name(kind: &CanvasNodeKind) -> &str {
    match kind {
        CanvasNodeKind::Text { .. } => "text",
        CanvasNodeKind::File { .. } => "file",
        CanvasNodeKind::Link { .. } => "link",
        CanvasNodeKind::Group { .. } => "group",
        CanvasNodeKind::Unknown { kind } => kind,
    }
}

fn side_name(side: CanvasSide) -> &'static str {
    match side {
        CanvasSide::Top => "top",
        CanvasSide::Right => "right",
        CanvasSide::Bottom => "bottom",
        CanvasSide::Left => "left",
    }
}

fn end_name(end: CanvasEnd) -> &'static str {
    match end {
        CanvasEnd::None => "none",
        CanvasEnd::Arrow => "arrow",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_json_canvas_node_and_edge_type() {
        let source = r##"{
          "nodes": [
            {"id":"t","type":"text","x":-10,"y":0,"width":200,"height":100,"text":"# Hello","color":"1"},
            {"id":"f","type":"file","x":220,"y":0,"width":200,"height":100,"file":"Note.md","subpath":"#Part"},
            {"id":"l","type":"link","x":0,"y":120,"width":200,"height":100,"url":"https://example.test","color":"#123456"},
            {"id":"g","type":"group","x":-20,"y":-20,"width":500,"height":300,"label":"Group","background":"bg.png","backgroundStyle":"cover"}
          ],
          "edges": [
            {"id":"e","fromNode":"t","fromSide":"right","toNode":"f","toSide":"left","toEnd":"arrow","label":"next"}
          ],
          "vendorKey": true
        }"##;
        let (canvas, diagnostics) = parse_canvas(source).unwrap();
        assert!(diagnostics.is_empty());
        assert_eq!(canvas.nodes.len(), 4);
        assert_eq!(canvas.edges.len(), 1);
        assert!(matches!(canvas.nodes[0].kind, CanvasNodeKind::Text { .. }));
        assert_eq!(canvas.bounds(), Some((-20, -20, 480, 280)));
        assert_eq!(canvas.extra.get("vendorKey"), Some(&serde_json::Value::Bool(true)));
    }

    #[test]
    fn malformed_references_are_diagnostics_not_panics() {
        let source = r#"{
          "nodes": [{"id":"a","type":"future","x":0,"y":0,"width":0,"height":1}],
          "edges": [{"id":"e","fromNode":"a","fromSide":"diagonal","toNode":"missing"}]
        }"#;
        let (_, diagnostics) = parse_canvas(source).unwrap();
        assert!(diagnostics.iter().any(|diagnostic| matches!(diagnostic, CanvasDiagnostic::UnknownNodeType { .. })));
        assert!(diagnostics.iter().any(|diagnostic| matches!(diagnostic, CanvasDiagnostic::NonPositiveNodeSize(_))));
        assert!(diagnostics.iter().any(|diagnostic| matches!(diagnostic, CanvasDiagnostic::DanglingEdge { .. })));
        assert!(diagnostics.iter().any(|diagnostic| matches!(diagnostic, CanvasDiagnostic::UnknownSide { .. })));
    }

    #[test]
    fn invalid_json_reports_the_source_location() {
        let error = parse_canvas("{\n  nope").unwrap_err().to_string();
        assert!(error.contains("line 2"), "{error}");
    }

    #[test]
    fn edited_documents_round_trip_with_extension_fields() {
        let source = r##"{
          "nodes": [{"id":"a","type":"text","x":0,"y":0,"width":200,"height":100,"text":"Hello","vendorNode":7}],
          "edges": [],
          "vendorRoot": {"enabled":true}
        }"##;
        let (mut canvas, diagnostics) = parse_canvas(source).unwrap();
        assert!(diagnostics.is_empty());
        canvas.nodes[0].x = 120;
        canvas.edges.push(CanvasEdge {
            id: "edge-1".to_string(),
            from_node: "a".to_string(),
            from_side: Some(CanvasSide::Right),
            from_end: CanvasEnd::None,
            to_node: "a".to_string(),
            to_side: Some(CanvasSide::Left),
            to_end: CanvasEnd::Arrow,
            color: Some(CanvasColor::Preset(4)),
            label: Some("loop".to_string()),
            extra: BTreeMap::from([("vendorEdge".to_string(), serde_json::Value::Bool(true))]),
        });

        let serialized = canvas.to_json_pretty().unwrap();
        let (round_tripped, diagnostics) = parse_canvas(&serialized).unwrap();
        assert!(diagnostics.is_empty());
        assert_eq!(round_tripped.nodes[0].x, 120);
        assert_eq!(round_tripped.nodes[0].extra.get("vendorNode"), Some(&serde_json::json!(7)));
        assert_eq!(round_tripped.edges[0].from_side, Some(CanvasSide::Right));
        assert_eq!(round_tripped.edges[0].to_end, CanvasEnd::Arrow);
        assert_eq!(round_tripped.edges[0].extra.get("vendorEdge"), Some(&serde_json::Value::Bool(true)));
        assert_eq!(round_tripped.extra.get("vendorRoot"), Some(&serde_json::json!({"enabled": true})));
    }
}
