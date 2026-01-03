//! Graph layout algorithms for the Graph View feature
//! Uses a force-directed layout for organic, evenly-spread visualization

use crate::app::{GraphNode, GraphEdge};

const REPULSION_STRENGTH: f32 = 1500.0;
const ATTRACTION_STRENGTH: f32 = 0.05; 
const CENTER_GRAVITY: f32 = 0.01;       
const DAMPING: f32 = 0.85;              
const NODE_HEIGHT: f32 = 3.0;           
const NODE_PADDING: f32 = 4.0;          
const ITERATIONS: usize = 200;          

pub fn apply_force_directed_layout(
    nodes: &mut [GraphNode],
    edges: &[GraphEdge],
    width: f32,
    height: f32,
) {
    if nodes.is_empty() {
        return;
    }

    let n = nodes.len();

    let cols = (n as f32).sqrt().ceil() as usize;
    let spacing_x = 25.0;
    let spacing_y = 8.0;

    for (i, node) in nodes.iter_mut().enumerate() {
        let col = i % cols;
        let row = i / cols;
        node.x = width / 4.0 + col as f32 * spacing_x;
        node.y = height / 4.0 + row as f32 * spacing_y;
        node.vx = 0.0;
        node.vy = 0.0;
    }

    let center_x = width / 2.0;
    let center_y = height / 2.0;

    // Run force simulation
    for iteration in 0..ITERATIONS {
        let temperature = 1.0 - (iteration as f32 / ITERATIONS as f32) * 0.8;
        let mut forces: Vec<(f32, f32)> = vec![(0.0, 0.0); n];

        for i in 0..n {
            for j in (i + 1)..n {
                let node_i = &nodes[i];
                let node_j = &nodes[j];
                let ci_x = node_i.x + node_i.width as f32 / 2.0;
                let ci_y = node_i.y + NODE_HEIGHT / 2.0;
                let cj_x = node_j.x + node_j.width as f32 / 2.0;
                let cj_y = node_j.y + NODE_HEIGHT / 2.0;
                let dx = cj_x - ci_x;
                let dy = cj_y - ci_y;
                let dist_sq = dx * dx + dy * dy;
                let dist = dist_sq.sqrt().max(1.0);

                let min_dist_x = (node_i.width + node_j.width) as f32 / 2.0 + NODE_PADDING;
                let min_dist_y = NODE_HEIGHT + NODE_PADDING;
                let min_dist = (min_dist_x * min_dist_x + min_dist_y * min_dist_y).sqrt();
                let effective_dist = dist.max(min_dist * 0.5);
                let force = REPULSION_STRENGTH / (effective_dist * effective_dist);

                let fx = (dx / dist) * force;
                let fy = (dy / dist) * force;

                forces[i].0 -= fx;
                forces[i].1 -= fy;
                forces[j].0 += fx;
                forces[j].1 += fy;
                if dist < min_dist {
                    let overlap = min_dist - dist;
                    let push = overlap * 0.5;
                    let push_x = (dx / dist) * push;
                    let push_y = (dy / dist) * push;

                    forces[i].0 -= push_x;
                    forces[i].1 -= push_y;
                    forces[j].0 += push_x;
                    forces[j].1 += push_y;
                }
            }
        }

        for edge in edges {
            if edge.from >= n || edge.to >= n {
                continue;
            }

            let node_from = &nodes[edge.from];
            let node_to = &nodes[edge.to];

            let from_cx = node_from.x + node_from.width as f32 / 2.0;
            let from_cy = node_from.y + NODE_HEIGHT / 2.0;
            let to_cx = node_to.x + node_to.width as f32 / 2.0;
            let to_cy = node_to.y + NODE_HEIGHT / 2.0;

            let dx = to_cx - from_cx;
            let dy = to_cy - from_cy;
            let dist = (dx * dx + dy * dy).sqrt().max(1.0);

            let ideal_dist = (node_from.width + node_to.width) as f32 / 2.0 + NODE_PADDING + 8.0;
            let displacement = dist - ideal_dist;

            let fx = (dx / dist) * displacement * ATTRACTION_STRENGTH;
            let fy = (dy / dist) * displacement * ATTRACTION_STRENGTH;

            forces[edge.from].0 += fx;
            forces[edge.from].1 += fy;
            forces[edge.to].0 -= fx;
            forces[edge.to].1 -= fy;
        }

        for i in 0..n {
            let node_cx = nodes[i].x + nodes[i].width as f32 / 2.0;
            let node_cy = nodes[i].y + NODE_HEIGHT / 2.0;
            let dx = center_x - node_cx;
            let dy = center_y - node_cy;
            forces[i].0 += dx * CENTER_GRAVITY;
            forces[i].1 += dy * CENTER_GRAVITY;
        }

        for i in 0..n {
            nodes[i].vx = (nodes[i].vx + forces[i].0) * DAMPING * temperature;
            nodes[i].vy = (nodes[i].vy + forces[i].1) * DAMPING * temperature;

            let speed = (nodes[i].vx * nodes[i].vx + nodes[i].vy * nodes[i].vy).sqrt();
            let max_speed = 8.0 * temperature;
            if speed > max_speed {
                nodes[i].vx = nodes[i].vx / speed * max_speed;
                nodes[i].vy = nodes[i].vy / speed * max_speed;
            }

            nodes[i].x += nodes[i].vx;
            nodes[i].y += nodes[i].vy;
        }
    }

    for _ in 0..10 {
        for i in 0..n {
            for j in (i + 1)..n {
                let (left, right) = nodes.split_at_mut(j);
                let node_i = &mut left[i];
                let node_j = &mut right[0];

                let ci_x = node_i.x + node_i.width as f32 / 2.0;
                let ci_y = node_i.y + NODE_HEIGHT / 2.0;
                let cj_x = node_j.x + node_j.width as f32 / 2.0;
                let cj_y = node_j.y + NODE_HEIGHT / 2.0;

                let dx = cj_x - ci_x;
                let dy = cj_y - ci_y;
                let dist = (dx * dx + dy * dy).sqrt().max(0.1);

                let min_dist_x = (node_i.width + node_j.width) as f32 / 2.0 + NODE_PADDING;
                let min_dist_y = NODE_HEIGHT + NODE_PADDING;
                let min_dist = (min_dist_x * min_dist_x + min_dist_y * min_dist_y).sqrt();

                if dist < min_dist {
                    let overlap = (min_dist - dist) / 2.0;
                    let push_x = (dx / dist) * overlap;
                    let push_y = (dy / dist) * overlap;

                    node_i.x -= push_x;
                    node_i.y -= push_y;
                    node_j.x += push_x;
                    node_j.y += push_y;
                }
            }
        }
    }

    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;

    for node in nodes.iter() {
        min_x = min_x.min(node.x);
        min_y = min_y.min(node.y);
    }

    let padding = 5.0;
    for node in nodes.iter_mut() {
        node.x = node.x - min_x + padding;
        node.y = node.y - min_y + padding;
    }
}
