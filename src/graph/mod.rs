//! Graph layout algorithms for the Graph View feature
//! Uses a hierarchical tree layout for clean visualization

use std::collections::{HashMap, HashSet, VecDeque};
use crate::app::{GraphNode, GraphEdge};

const NODE_HEIGHT: f32 = 4.0;
const HORIZONTAL_SPACING: f32 = 4.0;
const VERTICAL_SPACING: f32 = 6.0;

pub fn apply_force_directed_layout(
    nodes: &mut [GraphNode],
    edges: &[GraphEdge],
    _width: f32,
    _height: f32,
) {
    if nodes.is_empty() {
        return;
    }

    let mut children: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut has_parent: HashSet<usize> = HashSet::new();

    for edge in edges {
        if edge.from < nodes.len() && edge.to < nodes.len() {
            children.entry(edge.from).or_default().push(edge.to);
            has_parent.insert(edge.to);
        }
    }

    let mut roots: Vec<usize> = (0..nodes.len())
        .filter(|i| !has_parent.contains(i))
        .collect();

    if roots.is_empty() {
        let mut out_degree: Vec<(usize, usize)> = (0..nodes.len())
            .map(|i| (i, children.get(&i).map(|c| c.len()).unwrap_or(0)))
            .collect();
        out_degree.sort_by(|a, b| b.1.cmp(&a.1));
        roots = vec![out_degree[0].0];
    }

    let mut levels: Vec<i32> = vec![-1; nodes.len()];
    let mut queue: VecDeque<usize> = VecDeque::new();

    for &root in &roots {
        if levels[root] == -1 {
            levels[root] = 0;
            queue.push_back(root);
        }
    }

    while let Some(node) = queue.pop_front() {
        if let Some(node_children) = children.get(&node) {
            for &child in node_children {
                if levels[child] == -1 {
                    levels[child] = levels[node] + 1;
                    queue.push_back(child);
                }
            }
        }
    }

    let mut current_level = 0;
    for i in 0..nodes.len() {
        if levels[i] == -1 {
            levels[i] = current_level;
            current_level = (current_level + 1) % 3;
        }
    }

    let max_level = levels.iter().max().copied().unwrap_or(0);
    let mut level_nodes: Vec<Vec<usize>> = vec![Vec::new(); (max_level + 1) as usize];

    for (i, &level) in levels.iter().enumerate() {
        if level >= 0 {
            level_nodes[level as usize].push(i);
        }
    }

    for (level, node_indices) in level_nodes.iter().enumerate() {
        let y = 10.0 + level as f32 * (NODE_HEIGHT + VERTICAL_SPACING);
        let mut x = 10.0;

        for &node_idx in node_indices.iter() {
            nodes[node_idx].x = x;
            nodes[node_idx].y = y;
            nodes[node_idx].vx = 0.0;
            nodes[node_idx].vy = 0.0;
            x += nodes[node_idx].width as f32 + HORIZONTAL_SPACING;
        }
    }

    for level in 1..=max_level as usize {
        for &node_idx in &level_nodes[level] {
            let mut parent_x_sum = 0.0;
            let mut parent_count = 0;

            for edge in edges {
                if edge.to == node_idx && levels[edge.from] < levels[node_idx] {
                    parent_x_sum += nodes[edge.from].x;
                    parent_count += 1;
                }
            }

            if parent_count > 0 {
                let _preferred_x = parent_x_sum / parent_count as f32;
            }
        }
    }
}
