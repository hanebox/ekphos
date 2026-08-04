//! Scalable graph indexing, projection, filtering, and layout.
//!
//! The old graph rebuilt links with repeated linear scans and used all-pairs
//! repulsion for every layout iteration.  This module keeps those concerns
//! separate: a vault-wide index is built once, cheap projections choose what is
//! visible, and layout never runs on the render path.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::io::{BufReader, BufWriter};
use std::path::Path;

use serde::{Deserialize, Serialize};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const LAYOUT_CACHE_VERSION: u32 = 2;
const TERMINAL_X_ASPECT: f32 = 2.0;
pub const GRAPH_MAX_ZOOM: f32 = 2.5;
const GRAPH_MIN_ZOOM: f32 = 0.000_01;

/// Zoom at which the complete graph bounds fit with a one-cell viewport inset.
pub fn fit_zoom_for_bounds(
    graph_width: f32,
    graph_height: f32,
    view_width: f32,
    view_height: f32,
) -> f32 {
    if graph_width <= 0.0 || graph_height <= 0.0 || view_width <= 0.0 || view_height <= 0.0 {
        return GRAPH_MIN_ZOOM;
    }
    let available_width = (view_width - 2.0).max(1.0);
    let available_height = (view_height - 2.0).max(1.0);
    (available_width / graph_width)
        .min(available_height / graph_height)
        .clamp(GRAPH_MIN_ZOOM, GRAPH_MAX_ZOOM)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GraphMode {
    #[default]
    Local,
    Global,
}

impl GraphMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Local => "LOCAL",
            Self::Global => "GLOBAL",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GraphLinkScope {
    #[default]
    All,
    Incoming,
    Outgoing,
}

impl GraphLinkScope {
    pub fn next(self) -> Self {
        match self {
            Self::All => Self::Incoming,
            Self::Incoming => Self::Outgoing,
            Self::Outgoing => Self::All,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Incoming => "in",
            Self::Outgoing => "out",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GraphRelation {
    Root,
    Incoming,
    Outgoing,
    Bidirectional,
    #[default]
    Neutral,
}

#[derive(Debug, Clone)]
pub struct GraphSourceNote {
    pub note_index: usize,
    pub title: String,
    /// Vault-relative path without the `.md` suffix.
    pub path: String,
    pub tags: Vec<String>,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct GraphIndexNode {
    pub note_index: usize,
    pub title: String,
    pub path: String,
    pub tags: Vec<String>,
    pub in_degree: usize,
    pub out_degree: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GraphIndexEdge {
    pub from: usize,
    pub to: usize,
    pub bidirectional: bool,
}

#[derive(Debug, Clone, Default)]
pub struct GraphIndex {
    pub nodes: Vec<GraphIndexNode>,
    pub edges: Vec<GraphIndexEdge>,
    pub outgoing: Vec<Vec<usize>>,
    pub incoming: Vec<Vec<usize>>,
    note_to_node: HashMap<usize, usize>,
    pub fingerprint: u64,
}

#[derive(Debug, Clone)]
pub struct GraphNode {
    /// Index into the vault's `notes` vector. Navigation uses this value.
    pub note_index: usize,
    pub title: String,
    pub full_title: String,
    pub path: String,
    pub x: f32,
    pub y: f32,
    pub home_x: f32,
    pub home_y: f32,
    pub depth: usize,
    pub relation: GraphRelation,
    pub in_degree: usize,
    pub out_degree: usize,
}

impl GraphNode {
    pub fn degree(&self) -> usize {
        self.in_degree + self.out_degree
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphEdge {
    pub from: usize,
    pub to: usize,
    pub bidirectional: bool,
}

#[derive(Debug, Clone, Default)]
pub struct GraphProjection {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub root_node: Option<usize>,
    pub total_nodes: usize,
    pub total_edges: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct GraphLayoutCache {
    version: u32,
    fingerprint: u64,
    positions: Vec<(String, f32, f32)>,
}

pub fn load_layout_cache(
    path: &Path,
    fingerprint: u64,
    nodes: &[GraphNode],
) -> Option<Vec<(usize, f32, f32)>> {
    let file = fs::File::open(path).ok()?;
    let cache: GraphLayoutCache = bincode::deserialize_from(BufReader::new(file)).ok()?;
    if cache.version != LAYOUT_CACHE_VERSION || cache.fingerprint != fingerprint {
        return None;
    }
    let by_path: HashMap<_, _> = cache
        .positions
        .into_iter()
        .map(|(path, x, y)| (path, (x, y)))
        .collect();
    let positions: Vec<_> = nodes
        .iter()
        .filter_map(|node| {
            by_path
                .get(&node.path)
                .filter(|(x, y)| x.is_finite() && y.is_finite())
                .map(|&(x, y)| (node.note_index, x, y))
        })
        .collect();
    (positions.len() == nodes.len()).then_some(positions)
}

pub fn save_layout_cache(path: &Path, fingerprint: u64, nodes: &[GraphNode]) {
    let Some(parent) = path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let cache = GraphLayoutCache {
        version: LAYOUT_CACHE_VERSION,
        fingerprint,
        positions: nodes
            .iter()
            .map(|node| (node.path.clone(), node.x, node.y))
            .collect(),
    };
    let temp_path = path.with_extension("bin.tmp");
    let Ok(file) = fs::File::create(&temp_path) else {
        return;
    };
    if bincode::serialize_into(BufWriter::new(file), &cache).is_ok() {
        let _ = fs::rename(temp_path, path);
    } else {
        let _ = fs::remove_file(temp_path);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FilterKind {
    Text(String),
    Path(String),
    Tag(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FilterTerm {
    exclude: bool,
    kind: FilterKind,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GraphFilter {
    pub query: String,
    terms: Vec<FilterTerm>,
}

impl GraphFilter {
    pub fn parse(query: &str) -> Self {
        let terms = tokenize_filter(query)
            .into_iter()
            .filter_map(|raw| {
                let (exclude, token) = raw
                    .strip_prefix('-')
                    .map(|rest| (true, rest))
                    .unwrap_or((false, raw.as_str()));
                if token.is_empty() {
                    return None;
                }
                let kind = if let Some(tag) = token.strip_prefix('#') {
                    if tag.is_empty() {
                        return None;
                    }
                    FilterKind::Tag(tag.to_lowercase())
                } else if let Some(path) = token.strip_prefix("path:") {
                    if path.is_empty() {
                        return None;
                    }
                    FilterKind::Path(path.to_lowercase())
                } else {
                    FilterKind::Text(token.to_lowercase())
                };
                Some(FilterTerm { exclude, kind })
            })
            .collect();
        Self {
            query: query.to_string(),
            terms,
        }
    }

    pub fn matches(&self, node: &GraphIndexNode) -> bool {
        if self.terms.is_empty() {
            return true;
        }
        let title = node.title.to_lowercase();
        let path = node.path.to_lowercase();
        self.terms.iter().all(|term| {
            let matched = match &term.kind {
                FilterKind::Text(value) => title.contains(value) || path.contains(value),
                FilterKind::Path(value) => path.contains(value),
                FilterKind::Tag(value) => node
                    .tags
                    .iter()
                    .any(|tag| tag.trim_start_matches('#').eq_ignore_ascii_case(value)),
            };
            if term.exclude {
                !matched
            } else {
                matched
            }
        })
    }
}

fn tokenize_filter(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    for ch in query.chars() {
        match ch {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

impl GraphIndex {
    pub fn build(sources: Vec<GraphSourceNote>) -> Self {
        let node_count = sources.len();
        let mut root_titles = HashMap::with_capacity(node_count);
        let mut all_titles = HashMap::with_capacity(node_count);
        let mut paths = HashMap::with_capacity(node_count);

        for (idx, source) in sources.iter().enumerate() {
            let title_key = source.title.to_lowercase();
            all_titles.entry(title_key.clone()).or_insert(idx);
            if !source.path.contains('/') {
                root_titles.entry(title_key).or_insert(idx);
            }
            paths
                .entry(normalize_wiki_path(&source.path))
                .or_insert(idx);
        }

        let mut directed = HashSet::new();
        for (from, source) in sources.iter().enumerate() {
            let mut targets_for_note = HashSet::new();
            for target in extract_wiki_targets(&source.content) {
                let normalized = normalize_wiki_path(&target);
                let to = if normalized.contains('/') {
                    paths.get(&normalized).copied()
                } else {
                    let key = normalized.to_lowercase();
                    root_titles
                        .get(&key)
                        .or_else(|| all_titles.get(&key))
                        .copied()
                };
                if let Some(to) = to {
                    if to != from && targets_for_note.insert(to) {
                        directed.insert((from, to));
                    }
                }
            }
        }

        let mut pairs: HashMap<(usize, usize), (bool, bool)> = HashMap::new();
        for &(from, to) in &directed {
            let key = if from < to { (from, to) } else { (to, from) };
            let entry = pairs.entry(key).or_insert((false, false));
            if from == key.0 {
                entry.0 = true;
            } else {
                entry.1 = true;
            }
        }

        let mut pair_items: Vec<_> = pairs.into_iter().collect();
        pair_items.sort_by_key(|(key, _)| *key);
        let mut edges = Vec::with_capacity(pair_items.len());
        let mut outgoing = vec![Vec::new(); node_count];
        let mut incoming = vec![Vec::new(); node_count];

        for ((low, high), (low_to_high, high_to_low)) in pair_items {
            let edge = if low_to_high && high_to_low {
                outgoing[low].push(high);
                incoming[high].push(low);
                outgoing[high].push(low);
                incoming[low].push(high);
                GraphIndexEdge {
                    from: low,
                    to: high,
                    bidirectional: true,
                }
            } else if low_to_high {
                outgoing[low].push(high);
                incoming[high].push(low);
                GraphIndexEdge {
                    from: low,
                    to: high,
                    bidirectional: false,
                }
            } else {
                outgoing[high].push(low);
                incoming[low].push(high);
                GraphIndexEdge {
                    from: high,
                    to: low,
                    bidirectional: false,
                }
            };
            edges.push(edge);
        }

        for neighbors in outgoing.iter_mut().chain(incoming.iter_mut()) {
            neighbors.sort_unstable();
            neighbors.dedup();
        }

        let nodes: Vec<_> = sources
            .into_iter()
            .enumerate()
            .map(|(idx, source)| GraphIndexNode {
                note_index: source.note_index,
                title: source.title,
                path: source.path,
                tags: source.tags,
                in_degree: incoming[idx].len(),
                out_degree: outgoing[idx].len(),
            })
            .collect();
        let note_to_node = nodes
            .iter()
            .enumerate()
            .map(|(idx, node)| (node.note_index, idx))
            .collect();
        let fingerprint = graph_fingerprint(&nodes, &edges);

        Self {
            nodes,
            edges,
            outgoing,
            incoming,
            note_to_node,
            fingerprint,
        }
    }

    pub fn node_for_note(&self, note_index: usize) -> Option<usize> {
        self.note_to_node.get(&note_index).copied()
    }

    pub fn project(
        &self,
        mode: GraphMode,
        root_note: usize,
        depth_limit: usize,
        scope: GraphLinkScope,
        filter: &GraphFilter,
        show_orphans: bool,
    ) -> GraphProjection {
        let root = self.node_for_note(root_note);
        let mut included = vec![false; self.nodes.len()];
        let mut depths = vec![usize::MAX; self.nodes.len()];
        let mut relations = vec![GraphRelation::Neutral; self.nodes.len()];

        match mode {
            GraphMode::Global => {
                for (idx, node) in self.nodes.iter().enumerate() {
                    let orphan = node.in_degree == 0 && node.out_degree == 0;
                    included[idx] = (show_orphans || !orphan) && filter.matches(node);
                }
            }
            GraphMode::Local => {
                if let Some(root) = root {
                    included[root] = true;
                    depths[root] = 0;
                    relations[root] = GraphRelation::Root;
                    let mut queue = VecDeque::from([root]);
                    while let Some(current) = queue.pop_front() {
                        let next_depth = depths[current].saturating_add(1);
                        if next_depth > depth_limit {
                            continue;
                        }
                        let current_relation = relations[current];
                        let mut visit = |next: usize, relation: GraphRelation| {
                            let merged = merge_relation(relations[next], relation);
                            relations[next] = merged;
                            if depths[next] > next_depth {
                                depths[next] = next_depth;
                                included[next] = true;
                                queue.push_back(next);
                            }
                        };
                        if scope != GraphLinkScope::Incoming {
                            for &next in &self.outgoing[current] {
                                visit(
                                    next,
                                    inherited_relation(current_relation, GraphRelation::Outgoing),
                                );
                            }
                        }
                        if scope != GraphLinkScope::Outgoing {
                            for &next in &self.incoming[current] {
                                visit(
                                    next,
                                    inherited_relation(current_relation, GraphRelation::Incoming),
                                );
                            }
                        }
                    }
                    for (idx, is_included) in included.iter_mut().enumerate() {
                        if idx != root && *is_included && !filter.matches(&self.nodes[idx]) {
                            *is_included = false;
                        }
                    }
                }
            }
        }

        let mut old_to_new = vec![usize::MAX; self.nodes.len()];
        let mut nodes = Vec::new();
        for (idx, indexed) in self.nodes.iter().enumerate() {
            if !included[idx] {
                continue;
            }
            old_to_new[idx] = nodes.len();
            nodes.push(GraphNode {
                note_index: indexed.note_index,
                title: truncate_title(&indexed.title, 28),
                full_title: indexed.title.clone(),
                path: indexed.path.clone(),
                x: 0.0,
                y: 0.0,
                home_x: 0.0,
                home_y: 0.0,
                depth: if mode == GraphMode::Local {
                    depths[idx]
                } else {
                    0
                },
                relation: if mode == GraphMode::Local {
                    relations[idx]
                } else {
                    GraphRelation::Neutral
                },
                in_degree: indexed.in_degree,
                out_degree: indexed.out_degree,
            });
        }

        let mut edges = Vec::new();
        for edge in &self.edges {
            let from = old_to_new[edge.from];
            let to = old_to_new[edge.to];
            if from != usize::MAX && to != usize::MAX {
                edges.push(GraphEdge {
                    from,
                    to,
                    bidirectional: edge.bidirectional,
                });
            }
        }

        GraphProjection {
            root_node: root.and_then(|idx| {
                let projected = old_to_new[idx];
                (projected != usize::MAX).then_some(projected)
            }),
            total_nodes: self.nodes.len(),
            total_edges: self.edges.len(),
            nodes,
            edges,
        }
    }
}

fn inherited_relation(parent: GraphRelation, edge: GraphRelation) -> GraphRelation {
    match parent {
        GraphRelation::Root | GraphRelation::Neutral => edge,
        GraphRelation::Bidirectional => edge,
        existing if existing == edge => existing,
        _ => GraphRelation::Bidirectional,
    }
}

fn merge_relation(existing: GraphRelation, next: GraphRelation) -> GraphRelation {
    match (existing, next) {
        (GraphRelation::Neutral, value) => value,
        (value, GraphRelation::Neutral) => value,
        (left, right) if left == right => left,
        (GraphRelation::Root, _) | (_, GraphRelation::Root) => GraphRelation::Root,
        _ => GraphRelation::Bidirectional,
    }
}

fn normalize_wiki_path(path: &str) -> String {
    path.trim().trim_end_matches(".md").replace('\\', "/")
}

fn extract_wiki_targets(content: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let mut seen = HashSet::new();
    let mut fence: Option<&str> = None;
    for line in content.lines() {
        let trimmed = line.trim_start();
        let marker = if trimmed.starts_with("```") {
            Some("```")
        } else if trimmed.starts_with("~~~") {
            Some("~~~")
        } else {
            None
        };
        if let Some(marker) = marker {
            if fence == Some(marker) {
                fence = None;
            } else if fence.is_none() {
                fence = Some(marker);
            }
            continue;
        }
        if fence.is_some() {
            continue;
        }

        let bytes = line.as_bytes();
        let mut index = 0;
        let mut inline_code = false;
        while index + 1 < bytes.len() {
            if bytes[index] == b'`' {
                inline_code = !inline_code;
                index += 1;
                continue;
            }
            if !inline_code && bytes[index] == b'[' && bytes[index + 1] == b'[' {
                let rest = &line[index + 2..];
                if let Some(end) = rest.find("]]") {
                    let raw = &rest[..end];
                    if !raw.is_empty() && !raw.contains(['[', ']']) {
                        let without_alias = raw.split('|').next().unwrap_or(raw);
                        let target = without_alias
                            .split('#')
                            .next()
                            .unwrap_or(without_alias)
                            .trim();
                        if !target.is_empty() && seen.insert(target.to_string()) {
                            targets.push(target.to_string());
                        }
                    }
                    index += 2 + end + 2;
                    continue;
                }
            }
            index += 1;
        }
    }
    targets
}

fn truncate_title(title: &str, max_width: usize) -> String {
    if title.width() <= max_width {
        return title.to_string();
    }
    let target = max_width.saturating_sub(3);
    let mut result = String::new();
    let mut width = 0;
    for ch in title.chars() {
        let ch_width = ch.width().unwrap_or(1);
        if width + ch_width > target {
            break;
        }
        result.push(ch);
        width += ch_width;
    }
    result.push_str("...");
    result
}

fn graph_fingerprint(nodes: &[GraphIndexNode], edges: &[GraphIndexEdge]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    let mut write = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
    };
    for node in nodes {
        write(node.path.as_bytes());
        write(&[0xff]);
    }
    for edge in edges {
        write(&(edge.from as u64).to_le_bytes());
        write(&(edge.to as u64).to_le_bytes());
        write(&[edge.bidirectional as u8]);
    }
    hash
}

/// Immediate, deterministic Local layout. Incoming and outgoing paths occupy
/// opposing radial fans instead of unbounded straight columns. Large
/// neighborhoods grow into additional arcs, preserving direction while keeping
/// a compact, circular silhouette.
pub fn apply_local_layout(nodes: &mut [GraphNode]) {
    if nodes.is_empty() {
        return;
    }
    for node in nodes
        .iter_mut()
        .filter(|node| node.relation == GraphRelation::Root)
    {
        node.x = 0.0;
        node.y = 0.0;
        node.home_x = 0.0;
        node.home_y = 0.0;
    }

    let max_depth = nodes.iter().map(|node| node.depth).max().unwrap_or(0);
    for relation in [
        GraphRelation::Incoming,
        GraphRelation::Outgoing,
        GraphRelation::Bidirectional,
        GraphRelation::Neutral,
    ] {
        let mut next_radius = 58.0;
        let mut previous_depth = 0usize;
        for depth in 1..=max_depth {
            let mut indices: Vec<usize> = nodes
                .iter()
                .enumerate()
                .filter_map(|(idx, node)| {
                    (node.depth == depth && node.relation == relation).then_some(idx)
                })
                .collect();
            indices.sort_by(|&left, &right| {
                nodes[right]
                    .degree()
                    .cmp(&nodes[left].degree())
                    .then_with(|| nodes[left].path.cmp(&nodes[right].path))
            });
            if indices.is_empty() {
                continue;
            }
            next_radius += depth.saturating_sub(previous_depth + 1) as f32 * 28.0;
            let outer_radius = match relation {
                GraphRelation::Incoming => {
                    layout_radial_fan(nodes, &indices, next_radius, std::f32::consts::PI, 2.15)
                }
                GraphRelation::Outgoing => {
                    layout_radial_fan(nodes, &indices, next_radius, 0.0, 2.15)
                }
                GraphRelation::Bidirectional => {
                    let (top, bottom): (Vec<_>, Vec<_>) = indices
                        .into_iter()
                        .enumerate()
                        .partition(|(rank, _)| rank % 2 == 0);
                    let top: Vec<_> = top.into_iter().map(|(_, idx)| idx).collect();
                    let bottom: Vec<_> = bottom.into_iter().map(|(_, idx)| idx).collect();
                    layout_radial_fan(nodes, &top, next_radius, -std::f32::consts::FRAC_PI_2, 0.82)
                        .max(layout_radial_fan(
                            nodes,
                            &bottom,
                            next_radius,
                            std::f32::consts::FRAC_PI_2,
                            0.82,
                        ))
                }
                GraphRelation::Neutral => layout_radial_fan(
                    nodes,
                    &indices,
                    next_radius,
                    -std::f32::consts::FRAC_PI_2,
                    std::f32::consts::TAU,
                ),
                GraphRelation::Root => next_radius,
            };
            next_radius = outer_radius + 34.0;
            previous_depth = depth;
        }
    }
    normalize_positions(nodes);
}

fn layout_radial_fan(
    nodes: &mut [GraphNode],
    indices: &[usize],
    inner_radius: f32,
    center_angle: f32,
    arc_span: f32,
) -> f32 {
    const NODE_SPACING: f32 = 18.0;
    const RING_GAP: f32 = 18.0;
    if indices.is_empty() {
        return inner_radius;
    }
    let mut offset = 0usize;
    let mut radius = inner_radius;
    while offset < indices.len() {
        let capacity = ((radius * arc_span / NODE_SPACING).floor() as usize).max(4);
        let take = capacity.min(indices.len() - offset);
        let full_circle = arc_span >= std::f32::consts::TAU - 0.01;
        let used_span = if full_circle {
            std::f32::consts::TAU
        } else {
            (((take.saturating_sub(1)) as f32 * NODE_SPACING) / radius).min(arc_span)
        };
        for rank in 0..take {
            let angle = if take == 1 {
                center_angle
            } else if full_circle {
                center_angle + std::f32::consts::TAU * rank as f32 / take as f32
            } else {
                center_angle - used_span / 2.0 + used_span * rank as f32 / (take - 1) as f32
            };
            let idx = indices[offset + rank];
            let x = radius * angle.cos() * TERMINAL_X_ASPECT;
            let y = radius * angle.sin();
            nodes[idx].x = x;
            nodes[idx].y = y;
            nodes[idx].home_x = x;
            nodes[idx].home_y = y;
        }
        offset += take;
        if offset < indices.len() {
            radius += RING_GAP;
        }
    }
    radius
}

/// Cheap deterministic seed shown while a refined Global layout is prepared.
///
/// Parent folders receive stable angular sectors and nodes fill those sectors
/// radially. This gives the first frame a coherent circular silhouette, keeps
/// related notes near each other, and provides a strong prior for the force
/// refinement instead of asking it to recover structure from random points.
pub fn apply_global_seed_layout(nodes: &mut [GraphNode]) {
    if nodes.is_empty() {
        return;
    }

    let mut grouped: HashMap<String, Vec<usize>> = HashMap::new();
    for (idx, node) in nodes.iter().enumerate() {
        let key = visual_group_key(&node.path);
        if let Some(indices) = grouped.get_mut(key) {
            indices.push(idx);
        } else {
            grouped.insert(key.to_string(), vec![idx]);
        }
    }
    let mut groups: Vec<_> = grouped.into_iter().collect();
    groups.sort_by(|(left_key, left), (right_key, right)| {
        right
            .len()
            .cmp(&left.len())
            .then_with(|| left_key.cmp(right_key))
    });

    let group_count = groups.len();
    let outer_radius = (nodes.len() as f32).sqrt() * 18.0;
    let golden_ratio = 0.618_034_f32;
    let mut sector_start = -std::f32::consts::FRAC_PI_2;
    for (_, mut indices) in groups {
        indices.sort_by(|&left, &right| {
            nodes[right]
                .degree()
                .cmp(&nodes[left].degree())
                .then_with(|| nodes[left].path.cmp(&nodes[right].path))
        });
        let sector_span = std::f32::consts::TAU * indices.len() as f32 / nodes.len() as f32;
        let sector_padding = if sector_span < std::f32::consts::TAU - 0.001 {
            (sector_span * 0.06).min(0.10)
        } else {
            0.0
        };
        let usable_span = (sector_span - sector_padding * 2.0).max(sector_span * 0.72);
        let sector_center = sector_start + sector_span / 2.0;
        let denominator = indices.len().saturating_sub(1).max(1) as f32;

        for (rank, idx) in indices.into_iter().enumerate() {
            let radial_fraction = if rank == 0 {
                if group_count == 1 {
                    0.0
                } else {
                    0.055
                }
            } else {
                (rank as f32 / denominator).sqrt().mul_add(0.92, 0.04)
            };
            let angle = if rank == 0 {
                sector_center
            } else {
                let phase = (rank as f32 * golden_ratio).fract();
                sector_start + sector_padding + phase * usable_span
            };
            let radius = outer_radius * radial_fraction;
            nodes[idx].x = radius * angle.cos() * TERMINAL_X_ASPECT;
            nodes[idx].y = radius * angle.sin();
            nodes[idx].home_x = nodes[idx].x;
            nodes[idx].home_y = nodes[idx].y;
        }
        sector_start += sector_span;
    }
    normalize_positions(nodes);
}

fn visual_group_key(path: &str) -> &str {
    let Some((parent, _)) = path.rsplit_once('/') else {
        return "";
    };
    let Some(first_separator) = parent.find('/') else {
        return parent;
    };
    let remainder = &parent[first_separator + 1..];
    remainder
        .find('/')
        .map(|second| &parent[..first_separator + 1 + second])
        .unwrap_or(parent)
}

/// Refine a Global layout. Small graphs use exact repulsion; large graphs use
/// Barnes-Hut approximation, keeping the hot loop near `O(N log N + E)`.
pub fn apply_global_layout(nodes: &mut [GraphNode], edges: &[GraphEdge]) {
    if nodes.len() <= 1 {
        apply_global_seed_layout(nodes);
        return;
    }
    apply_global_seed_layout(nodes);
    let count = nodes.len();
    let iterations = if count <= 256 {
        100
    } else if count <= 2_000 {
        72
    } else {
        56
    };
    let mut velocity = vec![(0.0f32, 0.0f32); count];

    for iteration in 0..iterations {
        velocity.fill((0.0, 0.0));
        if count <= 256 {
            for left in 0..count {
                for right in (left + 1)..count {
                    apply_pair_repulsion(nodes, &mut velocity, left, right, 1500.0);
                }
            }
        } else {
            let tree = QuadTree::build(nodes);
            for idx in 0..count {
                let (fx, fy) = tree.force_on(idx, nodes[idx].x, nodes[idx].y, 0.7, 1500.0);
                velocity[idx].0 += fx;
                velocity[idx].1 += fy;
            }
        }

        let spring_strength = if count > 2_000 { 0.007 } else { 0.012 };
        for edge in edges {
            if edge.from >= count || edge.to >= count {
                continue;
            }
            let dx = nodes[edge.to].x - nodes[edge.from].x;
            let dy = nodes[edge.to].y - nodes[edge.from].y;
            let distance = (dx * dx + dy * dy).sqrt().max(0.01);
            let force = (distance - 42.0) * spring_strength;
            let fx = dx / distance * force;
            let fy = dy / distance * force;
            velocity[edge.from].0 += fx;
            velocity[edge.from].1 += fy;
            velocity[edge.to].0 -= fx;
            velocity[edge.to].1 -= fy;
        }

        let temperature = 10.0 * (1.0 - iteration as f32 / iterations as f32).max(0.08);
        let (center_x, center_y) = center(nodes);
        let anchor_strength = if count > 2_000 { 0.016 } else { 0.010 };
        for idx in 0..count {
            velocity[idx].0 += (center_x - nodes[idx].x) * 0.004;
            velocity[idx].1 += (center_y - nodes[idx].y) * 0.004;
            velocity[idx].0 += (nodes[idx].home_x - nodes[idx].x) * anchor_strength;
            velocity[idx].1 += (nodes[idx].home_y - nodes[idx].y) * anchor_strength;
            let speed = (velocity[idx].0.powi(2) + velocity[idx].1.powi(2)).sqrt();
            if speed > temperature {
                velocity[idx].0 = velocity[idx].0 / speed * temperature;
                velocity[idx].1 = velocity[idx].1 / speed * temperature;
            }
            nodes[idx].x += velocity[idx].0 * 0.88;
            nodes[idx].y += velocity[idx].1 * 0.88;
        }
    }
    resolve_collisions(nodes);
    enforce_circular_aspect(nodes);
    normalize_positions(nodes);
}

/// Expand only the compressed axis so a force-heavy graph cannot degenerate
/// into a horizontal or vertical line. The 2:1 coordinate ratio compensates
/// for terminal cells being roughly twice as tall as they are wide.
fn enforce_circular_aspect(nodes: &mut [GraphNode]) {
    if nodes.len() < 3 {
        return;
    }
    let (center_x, center_y) = center(nodes);
    let (variance_x, variance_y) = nodes.iter().fold((0.0, 0.0), |(x, y), node| {
        (
            x + (node.x - center_x).powi(2),
            y + (node.y - center_y).powi(2),
        )
    });
    let spread_x = (variance_x / nodes.len() as f32).sqrt().max(0.001);
    let spread_y = (variance_y / nodes.len() as f32).sqrt().max(0.001);
    let aspect = spread_x / (spread_y * TERMINAL_X_ASPECT);
    if aspect > 1.0 {
        let scale = aspect.min(3.0);
        for node in nodes {
            node.y = center_y + (node.y - center_y) * scale;
        }
    } else {
        let scale = (1.0 / aspect).min(3.0);
        for node in nodes {
            node.x = center_x + (node.x - center_x) * scale;
        }
    }
}

fn apply_pair_repulsion(
    nodes: &[GraphNode],
    velocity: &mut [(f32, f32)],
    left: usize,
    right: usize,
    strength: f32,
) {
    let mut dx = nodes[right].x - nodes[left].x;
    let mut dy = nodes[right].y - nodes[left].y;
    if dx.abs() + dy.abs() < 0.001 {
        dx = 0.01 * ((left % 7) as f32 + 1.0);
        dy = 0.01 * ((right % 11) as f32 + 1.0);
    }
    let distance_sq = (dx * dx + dy * dy).max(1.0);
    let distance = distance_sq.sqrt();
    let force = strength / distance_sq;
    let fx = dx / distance * force;
    let fy = dy / distance * force;
    velocity[left].0 -= fx;
    velocity[left].1 -= fy;
    velocity[right].0 += fx;
    velocity[right].1 += fy;
}

fn resolve_collisions(nodes: &mut [GraphNode]) {
    const CELL: f32 = 10.0;
    for _ in 0..3 {
        let mut grid: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        for idx in 0..nodes.len() {
            let cell = (
                (nodes[idx].x / CELL).floor() as i32,
                (nodes[idx].y / CELL).floor() as i32,
            );
            for gx in (cell.0 - 1)..=(cell.0 + 1) {
                for gy in (cell.1 - 1)..=(cell.1 + 1) {
                    if let Some(others) = grid.get(&(gx, gy)) {
                        for &other in others {
                            let dx = nodes[idx].x - nodes[other].x;
                            let dy = nodes[idx].y - nodes[other].y;
                            let distance = (dx * dx + dy * dy).sqrt();
                            if distance < 7.0 && distance > 0.001 {
                                let push = (7.0 - distance) * 0.5;
                                let px = dx / distance * push;
                                let py = dy / distance * push;
                                nodes[idx].x += px;
                                nodes[idx].y += py;
                                nodes[other].x -= px;
                                nodes[other].y -= py;
                            }
                        }
                    }
                }
            }
            grid.entry(cell).or_default().push(idx);
        }
    }
}

fn center(nodes: &[GraphNode]) -> (f32, f32) {
    let (x, y) = nodes
        .iter()
        .fold((0.0, 0.0), |(x, y), node| (x + node.x, y + node.y));
    (x / nodes.len() as f32, y / nodes.len() as f32)
}

fn normalize_positions(nodes: &mut [GraphNode]) {
    if nodes.is_empty() {
        return;
    }
    let min_x = nodes
        .iter()
        .map(|node| node.x)
        .fold(f32::INFINITY, f32::min);
    let min_y = nodes
        .iter()
        .map(|node| node.y)
        .fold(f32::INFINITY, f32::min);
    for node in nodes {
        node.x = node.x - min_x + 16.0;
        node.y = node.y - min_y + 8.0;
        node.home_x = node.x;
        node.home_y = node.y;
    }
}

#[derive(Debug)]
struct QuadTree {
    root: Quad,
}

impl QuadTree {
    fn build(nodes: &[GraphNode]) -> Self {
        let min_x = nodes
            .iter()
            .map(|node| node.x)
            .fold(f32::INFINITY, f32::min);
        let max_x = nodes
            .iter()
            .map(|node| node.x)
            .fold(f32::NEG_INFINITY, f32::max);
        let min_y = nodes
            .iter()
            .map(|node| node.y)
            .fold(f32::INFINITY, f32::min);
        let max_y = nodes
            .iter()
            .map(|node| node.y)
            .fold(f32::NEG_INFINITY, f32::max);
        let size = (max_x - min_x).max(max_y - min_y).max(1.0) + 1.0;
        let mut root = Quad::new(min_x - 0.5, min_y - 0.5, size);
        for (idx, node) in nodes.iter().enumerate() {
            root.insert(idx, node.x, node.y, 0);
        }
        Self { root }
    }

    fn force_on(&self, index: usize, x: f32, y: f32, theta: f32, strength: f32) -> (f32, f32) {
        self.root.force_on(index, x, y, theta, strength)
    }
}

#[derive(Debug)]
struct Quad {
    x: f32,
    y: f32,
    size: f32,
    mass: usize,
    center_x: f32,
    center_y: f32,
    point: Option<(usize, f32, f32)>,
    children: Option<Box<[Quad; 4]>>,
}

impl Quad {
    fn new(x: f32, y: f32, size: f32) -> Self {
        Self {
            x,
            y,
            size,
            mass: 0,
            center_x: 0.0,
            center_y: 0.0,
            point: None,
            children: None,
        }
    }

    fn insert(&mut self, index: usize, x: f32, y: f32, depth: usize) {
        let old_mass = self.mass as f32;
        self.mass += 1;
        self.center_x = (self.center_x * old_mass + x) / self.mass as f32;
        self.center_y = (self.center_y * old_mass + y) / self.mass as f32;

        if self.children.is_none() && self.point.is_none() {
            self.point = Some((index, x, y));
            return;
        }
        if self.children.is_none() {
            if depth >= 24 || self.size < 0.001 {
                return;
            }
            self.subdivide();
            if let Some((old_index, old_x, old_y)) = self.point.take() {
                self.insert_child(old_index, old_x, old_y, depth + 1);
            }
        }
        self.insert_child(index, x, y, depth + 1);
    }

    fn subdivide(&mut self) {
        let half = self.size / 2.0;
        self.children = Some(Box::new([
            Quad::new(self.x, self.y, half),
            Quad::new(self.x + half, self.y, half),
            Quad::new(self.x, self.y + half, half),
            Quad::new(self.x + half, self.y + half, half),
        ]));
    }

    fn insert_child(&mut self, index: usize, x: f32, y: f32, depth: usize) {
        let half = self.size / 2.0;
        let right = usize::from(x >= self.x + half);
        let bottom = usize::from(y >= self.y + half) * 2;
        if let Some(children) = &mut self.children {
            children[right + bottom].insert(index, x, y, depth);
        }
    }

    fn force_on(&self, index: usize, x: f32, y: f32, theta: f32, strength: f32) -> (f32, f32) {
        if self.mass == 0 || (self.mass == 1 && self.point.map(|point| point.0) == Some(index)) {
            return (0.0, 0.0);
        }
        let mut dx = x - self.center_x;
        let mut dy = y - self.center_y;
        if dx.abs() + dy.abs() < 0.001 {
            dx = 0.01;
            dy = 0.01;
        }
        let distance_sq = (dx * dx + dy * dy).max(1.0);
        let distance = distance_sq.sqrt();
        if self.children.is_none() || self.size / distance < theta {
            let force = strength * self.mass as f32 / distance_sq;
            return (dx / distance * force, dy / distance * force);
        }
        self.children
            .as_ref()
            .unwrap()
            .iter()
            .fold((0.0, 0.0), |(fx, fy), child| {
                let (child_x, child_y) = child.force_on(index, x, y, theta, strength);
                (fx + child_x, fy + child_y)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(index: usize, path: &str, content: &str) -> GraphSourceNote {
        GraphSourceNote {
            note_index: index,
            title: path.rsplit('/').next().unwrap_or(path).to_string(),
            path: path.to_string(),
            tags: Vec::new(),
            content: content.to_string(),
        }
    }

    #[test]
    fn index_deduplicates_and_marks_reverse_links() {
        let index = GraphIndex::build(vec![
            source(0, "A", "[[B]] [[B#Heading|alias]]"),
            source(1, "B", "[[A]]"),
        ]);
        assert_eq!(
            index.edges,
            vec![GraphIndexEdge {
                from: 0,
                to: 1,
                bidirectional: true
            }]
        );
        assert_eq!(index.nodes[0].out_degree, 1);
        assert_eq!(index.nodes[0].in_degree, 1);
    }

    #[test]
    fn index_ignores_code_unresolved_and_self_links() {
        let index = GraphIndex::build(vec![
            source(0, "A", "`[[B]]`\n```md\n[[B]]\n```\n[[Missing]]\n[[A]]"),
            source(1, "B", ""),
        ]);
        assert!(index.edges.is_empty());
    }

    #[test]
    fn bare_duplicate_title_prefers_vault_root_note() {
        let index = GraphIndex::build(vec![
            source(0, "A", ""),
            source(1, "folder/A", ""),
            source(2, "Source", "[[A]]"),
        ]);
        assert_eq!(
            index.edges,
            vec![GraphIndexEdge {
                from: 2,
                to: 0,
                bidirectional: false,
            }]
        );
    }

    #[test]
    fn local_projection_honors_depth_and_direction() {
        let index = GraphIndex::build(vec![
            source(0, "A", "[[B]]"),
            source(1, "B", "[[C]]"),
            source(2, "C", ""),
            source(3, "D", "[[A]]"),
        ]);
        let filter = GraphFilter::default();
        let out = index.project(
            GraphMode::Local,
            0,
            2,
            GraphLinkScope::Outgoing,
            &filter,
            true,
        );
        assert_eq!(
            out.nodes
                .iter()
                .map(|node| node.full_title.as_str())
                .collect::<Vec<_>>(),
            vec!["A", "B", "C"]
        );
        let incoming = index.project(
            GraphMode::Local,
            0,
            1,
            GraphLinkScope::Incoming,
            &filter,
            true,
        );
        assert_eq!(
            incoming
                .nodes
                .iter()
                .map(|node| node.full_title.as_str())
                .collect::<Vec<_>>(),
            vec!["A", "D"]
        );
    }

    #[test]
    fn metadata_filter_supports_path_tag_quotes_and_exclusion() {
        let mut note = source(0, "areas/machine learning/Project", "");
        note.tags = vec!["research".to_string()];
        let index = GraphIndex::build(vec![note, source(1, "archive/Project old", "")]);
        let filter = GraphFilter::parse("project #research path:\"machine learning\" -archive");
        assert!(filter.matches(&index.nodes[0]));
        assert!(!filter.matches(&index.nodes[1]));
    }

    #[test]
    fn title_truncation_respects_unicode_display_width() {
        let truncated = truncate_title("日本語の長いノートタイトルです", 18);
        assert!(truncated.width() <= 18);
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn local_layout_places_incoming_left_and_outgoing_right() {
        let index = GraphIndex::build(vec![
            source(0, "Root", "[[Out]]"),
            source(1, "Out", ""),
            source(2, "In", "[[Root]]"),
        ]);
        let mut projection = index.project(
            GraphMode::Local,
            0,
            1,
            GraphLinkScope::All,
            &GraphFilter::default(),
            true,
        );
        apply_local_layout(&mut projection.nodes);
        let root_x = projection
            .nodes
            .iter()
            .find(|node| node.relation == GraphRelation::Root)
            .unwrap()
            .x;
        let in_x = projection
            .nodes
            .iter()
            .find(|node| node.relation == GraphRelation::Incoming)
            .unwrap()
            .x;
        let out_x = projection
            .nodes
            .iter()
            .find(|node| node.relation == GraphRelation::Outgoing)
            .unwrap()
            .x;
        assert!(in_x < root_x);
        assert!(out_x > root_x);
    }

    #[test]
    fn large_local_neighborhood_uses_radial_fans_instead_of_lines() {
        let outgoing_links = (0..80)
            .map(|idx| format!("[[Out{idx:03}]]"))
            .collect::<Vec<_>>()
            .join(" ");
        let mut sources = vec![source(0, "Root", &outgoing_links)];
        sources.extend((0..80).map(|idx| source(idx + 1, &format!("Out{idx:03}"), "")));
        sources.extend((0..80).map(|idx| source(idx + 81, &format!("In{idx:03}"), "[[Root]]")));
        let index = GraphIndex::build(sources);
        let mut projection = index.project(
            GraphMode::Local,
            0,
            1,
            GraphLinkScope::All,
            &GraphFilter::default(),
            true,
        );
        apply_local_layout(&mut projection.nodes);

        let root = projection
            .nodes
            .iter()
            .find(|node| node.relation == GraphRelation::Root)
            .unwrap();
        let incoming: Vec<_> = projection
            .nodes
            .iter()
            .filter(|node| node.relation == GraphRelation::Incoming)
            .collect();
        let outgoing: Vec<_> = projection
            .nodes
            .iter()
            .filter(|node| node.relation == GraphRelation::Outgoing)
            .collect();
        assert!(incoming.iter().all(|node| node.x < root.x));
        assert!(outgoing.iter().all(|node| node.x > root.x));
        let incoming_x_bands: HashSet<_> =
            incoming.iter().map(|node| node.x.round() as i32).collect();
        let outgoing_x_bands: HashSet<_> =
            outgoing.iter().map(|node| node.x.round() as i32).collect();
        assert!(incoming_x_bands.len() > 8);
        assert!(outgoing_x_bands.len() > 8);
    }

    #[test]
    fn large_layout_is_finite_and_deterministic() {
        let sources: Vec<_> = (0..300)
            .map(|idx| {
                source(
                    idx,
                    &format!("N{idx}"),
                    &format!("[[N{}]]", (idx + 1) % 300),
                )
            })
            .collect();
        let index = GraphIndex::build(sources);
        let mut first = index.project(
            GraphMode::Global,
            0,
            1,
            GraphLinkScope::All,
            &GraphFilter::default(),
            true,
        );
        let mut second = first.clone();
        apply_global_layout(&mut first.nodes, &first.edges);
        apply_global_layout(&mut second.nodes, &second.edges);
        assert!(first
            .nodes
            .iter()
            .all(|node| node.x.is_finite() && node.y.is_finite()));
        for (left, right) in first.nodes.iter().zip(second.nodes.iter()) {
            assert!((left.x - right.x).abs() < 0.001);
            assert!((left.y - right.y).abs() < 0.001);
        }
        let (center_x, center_y) = center(&first.nodes);
        let (variance_x, variance_y) = first.nodes.iter().fold((0.0, 0.0), |acc, node| {
            (
                acc.0 + (node.x - center_x).powi(2),
                acc.1 + (node.y - center_y).powi(2),
            )
        });
        let visual_aspect = variance_x.sqrt() / (variance_y.sqrt() * TERMINAL_X_ASPECT);
        assert!((0.95..=1.05).contains(&visual_aspect));
    }

    #[test]
    fn fit_zoom_is_bounded_and_keeps_complete_bounds_visible() {
        let zoom = fit_zoom_for_bounds(1_000.0, 500.0, 100.0, 50.0);
        assert!((zoom - 0.096).abs() < 0.0001);
        assert!(1_000.0 * zoom <= 98.0);
        assert!(500.0 * zoom <= 48.0);
        assert_eq!(
            fit_zoom_for_bounds(1.0, 1.0, 1_000.0, 1_000.0),
            GRAPH_MAX_ZOOM
        );
    }

    #[test]
    fn local_filter_keeps_the_root_for_context() {
        let index = GraphIndex::build(vec![
            source(0, "Root", "[[Matching]] [[Hidden]]"),
            source(1, "Matching", ""),
            source(2, "Hidden", ""),
        ]);
        let projection = index.project(
            GraphMode::Local,
            0,
            1,
            GraphLinkScope::All,
            &GraphFilter::parse("matching"),
            true,
        );
        assert_eq!(
            projection
                .nodes
                .iter()
                .map(|node| node.full_title.as_str())
                .collect::<Vec<_>>(),
            vec!["Root", "Matching"]
        );
        assert_eq!(projection.edges.len(), 1);
    }

    #[test]
    fn global_orphan_toggle_does_not_change_connected_nodes() {
        let index = GraphIndex::build(vec![
            source(0, "A", "[[B]]"),
            source(1, "B", ""),
            source(2, "Orphan", ""),
        ]);
        let filter = GraphFilter::default();
        let shown = index.project(GraphMode::Global, 0, 1, GraphLinkScope::All, &filter, true);
        let hidden = index.project(GraphMode::Global, 0, 1, GraphLinkScope::All, &filter, false);
        assert_eq!(shown.nodes.len(), 3);
        assert_eq!(hidden.nodes.len(), 2);
        assert_eq!(hidden.edges.len(), 1);
    }

    #[test]
    fn layout_cache_round_trips_and_rejects_other_fingerprints() {
        let index = GraphIndex::build(vec![source(0, "A", "[[B]]"), source(1, "B", "")]);
        let mut projection = index.project(
            GraphMode::Global,
            0,
            1,
            GraphLinkScope::All,
            &GraphFilter::default(),
            true,
        );
        apply_global_seed_layout(&mut projection.nodes);
        let path =
            std::env::temp_dir().join(format!("ekphos-graph-cache-{}.bin", std::process::id()));
        save_layout_cache(&path, index.fingerprint, &projection.nodes);
        let loaded = load_layout_cache(&path, index.fingerprint, &projection.nodes).unwrap();
        assert_eq!(loaded.len(), 2);
        assert!(
            load_layout_cache(&path, index.fingerprint.wrapping_add(1), &projection.nodes)
                .is_none()
        );
        std::fs::write(&path, b"not a graph layout cache").unwrap();
        assert!(load_layout_cache(&path, index.fingerprint, &projection.nodes).is_none());
        let _ = std::fs::remove_file(path);
    }

    /// Run with `cargo test graph_large_vault_benchmark --release -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn graph_large_vault_benchmark() {
        let note_count = 10_000usize;
        let sources: Vec<_> = (0..note_count)
            .map(|idx| {
                let links = (1..=5)
                    .map(|offset| format!("[[N{}]]", (idx + offset) % note_count))
                    .collect::<Vec<_>>()
                    .join(" ");
                source(idx, &format!("N{idx}"), &links)
            })
            .collect();
        let started = std::time::Instant::now();
        let index = GraphIndex::build(sources);
        let index_elapsed = started.elapsed();
        assert_eq!(index.nodes.len(), note_count);
        assert!(index.edges.len() >= 49_900);

        let local_started = std::time::Instant::now();
        let mut local = index.project(
            GraphMode::Local,
            0,
            2,
            GraphLinkScope::All,
            &GraphFilter::default(),
            true,
        );
        apply_local_layout(&mut local.nodes);
        let local_elapsed = local_started.elapsed();

        let global_started = std::time::Instant::now();
        let mut global = index.project(
            GraphMode::Global,
            0,
            1,
            GraphLinkScope::All,
            &GraphFilter::default(),
            true,
        );
        apply_global_layout(&mut global.nodes, &global.edges);
        let global_elapsed = global_started.elapsed();
        eprintln!(
            "10k/50k graph: index={index_elapsed:?}, local={local_elapsed:?}, global={global_elapsed:?}"
        );
    }
}
