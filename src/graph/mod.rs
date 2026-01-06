//! Graph layout algorithms for the Graph View feature
//! Uses force-directed layout with central gravity for circular distribution (Obsidian-like)

use crate::app::{GraphEdge, GraphNode};
use unicode_width::UnicodeWidthStr;

struct Rng {
    state: u32,
}

impl Rng {
    fn new(seed: u32) -> Self {
        Self { state: seed.max(1) }
    }

    fn next(&mut self) -> f32 {
        self.state = self.state.wrapping_mul(1103515245).wrapping_add(12345);
        ((self.state >> 16) & 0x7fff) as f32 / 32767.0
    }

    fn next_range(&mut self, min: f32, max: f32) -> f32 {
        min + self.next() * (max - min)
    }
}

pub fn apply_force_directed_layout(
    nodes: &mut [GraphNode],
    edges: &[GraphEdge],
    _width: f32,
    _height: f32,
) {
    if nodes.is_empty() {
        return;
    }

    let n = nodes.len();
    if n == 1 {
        nodes[0].x = 50.0;
        nodes[0].y = 25.0;
        nodes[0].home_x = 50.0;
        nodes[0].home_y = 25.0;
        return;
    }

    // Terminal aspect ratio: characters are roughly 2x taller than wide
    // We stretch horizontally to make the circular layout appear circular on screen
    let aspect_ratio = 2.2;

    // Pre-calculate text widths for text-aware spacing
    let text_widths: Vec<f32> = nodes
        .iter()
        .map(|node| node.title.width() as f32)
        .collect();
    let avg_text_width = text_widths.iter().sum::<f32>() / n as f32;

    // Calculate radius based on number of nodes - more nodes = larger circle
    // Scale to ensure nodes are well spread out at any zoom level
    let text_factor = 1.0 + (avg_text_width / 12.0).min(2.0);
    let base_radius = (n as f32).sqrt() * 45.0 * text_factor;

    // Center of the graph
    let center_x = 60.0;
    let center_y = 30.0;

    let mut rng = Rng::new((n as u32 * 31337) ^ 12345);

    // Initialize nodes in a circular/spiral pattern with some randomization
    // This creates the initial circular shape
    for (i, node) in nodes.iter_mut().enumerate() {
        // Golden angle for even distribution (like sunflower seeds)
        let golden_angle = std::f32::consts::PI * (3.0 - (5.0_f32).sqrt());
        let angle = i as f32 * golden_angle;

        // Radius increases with sqrt of index for even area distribution
        let r = base_radius * ((i as f32 + 1.0) / n as f32).sqrt();

        // Add some randomization to avoid perfect patterns
        let r_jitter = rng.next_range(0.8, 1.2);
        let angle_jitter = rng.next_range(-0.2, 0.2);

        let final_r = r * r_jitter;
        let final_angle = angle + angle_jitter;

        // Apply aspect ratio correction for terminal display
        node.x = center_x + final_r * final_angle.cos() * aspect_ratio;
        node.y = center_y + final_r * final_angle.sin();
        node.vx = 0.0;
        node.vy = 0.0;
    }

    // Store initial radii for radial force (maintains circular shape)
    let initial_radii: Vec<f32> = nodes
        .iter()
        .map(|node| {
            let dx = (node.x - center_x) / aspect_ratio;
            let dy = node.y - center_y;
            (dx * dx + dy * dy).sqrt()
        })
        .collect();

    // Force-directed simulation parameters (Obsidian-like)
    let iterations = 200; // More iterations for better convergence
    let initial_temperature = 12.0; // Slightly higher initial temperature

    // Text-aware scaling for forces
    let text_scale = 1.0 + (avg_text_width / 12.0).min(2.0);

    // Forces - very strong repulsion to maintain gaps at all zoom levels
    let repulsion_strength = 2000.0 * text_scale;
    let attraction_strength = 0.008; // Weaker attraction
    let gravity_strength = 0.015; // Weaker gravity for more spread
    let radial_strength = 0.01;

    // Large minimum distances to ensure gaps even at max zoom out
    let ideal_edge_length = 60.0 + avg_text_width * 0.6;
    let base_min_distance = 50.0 + avg_text_width * 0.5;

    for iter in 0..iterations {
        // Temperature decreases over time (simulated annealing)
        let temperature = initial_temperature * (1.0 - iter as f32 / iterations as f32);
        let damping = 0.85 + 0.1 * (iter as f32 / iterations as f32); // Increases over time

        // Reset velocities
        for node in nodes.iter_mut() {
            node.vx = 0.0;
            node.vy = 0.0;
        }

        // Calculate current center of mass
        let (mut cx, mut cy) = (0.0, 0.0);
        for node in nodes.iter() {
            cx += node.x;
            cy += node.y;
        }
        cx /= n as f32;
        cy /= n as f32;

        // Central gravity - pull all nodes toward center of mass
        for node in nodes.iter_mut() {
            let dx = cx - node.x;
            let dy = cy - node.y;
            let dist = (dx * dx + dy * dy).sqrt().max(1.0);

            // Gravity force proportional to distance from center
            let force = gravity_strength * dist;
            node.vx += (dx / dist) * force;
            node.vy += (dy / dist) * force;
        }

        // Radial force - maintain circular shape by pulling nodes toward their ideal radius
        for (i, node) in nodes.iter_mut().enumerate() {
            let dx = (node.x - cx) / aspect_ratio;
            let dy = node.y - cy;
            let current_radius = (dx * dx + dy * dy).sqrt().max(0.1);
            let ideal_radius = initial_radii[i];

            // Pull toward ideal radius
            let radius_diff = current_radius - ideal_radius;
            let force = -radius_diff * radial_strength;

            // Apply force in radial direction
            let nx = dx / current_radius;
            let ny = dy / current_radius;
            node.vx += nx * force * aspect_ratio;
            node.vy += ny * force;
        }

        // Repulsion between all pairs of nodes (Coulomb's law)
        // Text-aware: nodes with longer labels repel more strongly
        for i in 0..n {
            for j in (i + 1)..n {
                let dx = nodes[j].x - nodes[i].x;
                let dy = nodes[j].y - nodes[i].y;
                let dist_sq = (dx * dx + dy * dy).max(1.0);
                let dist = dist_sq.sqrt();

                // Text-aware repulsion: scale by combined text widths
                let combined_width = (text_widths[i] + text_widths[j]) / 2.0;
                let text_factor = 1.0 + (combined_width / 30.0).min(1.0);
                let adjusted_repulsion = repulsion_strength * text_factor;

                // Repulsion force: inversely proportional to distance squared
                let force = adjusted_repulsion / dist_sq;
                let fx = (dx / dist) * force;
                let fy = (dy / dist) * force;

                nodes[i].vx -= fx;
                nodes[i].vy -= fy;
                nodes[j].vx += fx;
                nodes[j].vy += fy;
            }
        }

        // Attraction along edges (spring force)
        for edge in edges {
            if edge.from >= n || edge.to >= n {
                continue;
            }

            let dx = nodes[edge.to].x - nodes[edge.from].x;
            let dy = nodes[edge.to].y - nodes[edge.from].y;
            let dist = (dx * dx + dy * dy).sqrt().max(1.0);

            // Spring force: pull toward ideal length
            let displacement = dist - ideal_edge_length;
            let force = displacement * attraction_strength;
            let fx = (dx / dist) * force;
            let fy = (dy / dist) * force;

            nodes[edge.from].vx += fx;
            nodes[edge.from].vy += fy;
            nodes[edge.to].vx -= fx;
            nodes[edge.to].vy -= fy;
        }

        // Apply velocities with temperature-based limiting and damping
        for node in nodes.iter_mut() {
            // Limit velocity by temperature
            let speed = (node.vx * node.vx + node.vy * node.vy).sqrt();
            if speed > temperature {
                node.vx = (node.vx / speed) * temperature;
                node.vy = (node.vy / speed) * temperature;
            }

            node.x += node.vx * damping;
            node.y += node.vy * damping;
        }

        // Enforce minimum distance between nodes (collision resolution)
        // Text-aware: nodes with longer labels need more space
        // Run multiple passes for better separation
        for _ in 0..3 {
            for i in 0..n {
                for j in (i + 1)..n {
                    let dx = nodes[j].x - nodes[i].x;
                    let dy = nodes[j].y - nodes[i].y;
                    let dist = (dx * dx + dy * dy).sqrt();
                    let half_width_i = text_widths[i] / 2.0;
                    let half_width_j = text_widths[j] / 2.0;
                    let min_distance = base_min_distance + (half_width_i + half_width_j) * 0.6;

                    if dist < min_distance && dist > 0.01 {
                        let overlap = min_distance - dist;
                        let push = (overlap / 2.0 + 1.0) * 1.2;
                        let nx = dx / dist;
                        let ny = dy / dist;

                        nodes[i].x -= nx * push;
                        nodes[i].y -= ny * push;
                        nodes[j].x += nx * push;
                        nodes[j].y += ny * push;
                    }
                }
            }
        }
    }

    // Final collision resolution pass - run multiple times to guarantee no overlaps
    for _ in 0..10 {
        for i in 0..n {
            for j in (i + 1)..n {
                let dx = nodes[j].x - nodes[i].x;
                let dy = nodes[j].y - nodes[i].y;
                let dist = (dx * dx + dy * dy).sqrt();

                let half_width_i = text_widths[i] / 2.0;
                let half_width_j = text_widths[j] / 2.0;
                let min_distance = base_min_distance + (half_width_i + half_width_j) * 0.5;

                if dist < min_distance && dist > 0.01 {
                    let overlap = min_distance - dist;
                    let push = overlap / 2.0 + 2.0; // Strong push
                    let nx = dx / dist;
                    let ny = dy / dist;

                    nodes[i].x -= nx * push;
                    nodes[i].y -= ny * push;
                    nodes[j].x += nx * push;
                    nodes[j].y += ny * push;
                }
            }
        }
    }

    // Final pass: center the graph and normalize positions
    let (mut min_x, mut min_y) = (f32::MAX, f32::MAX);
    let (mut max_x, mut max_y) = (f32::MIN, f32::MIN);

    for node in nodes.iter() {
        min_x = min_x.min(node.x);
        min_y = min_y.min(node.y);
        max_x = max_x.max(node.x);
        max_y = max_y.max(node.y);
    }

    // Center the graph with some padding
    let padding = 15.0;

    for node in nodes.iter_mut() {
        node.x = node.x - min_x + padding;
        node.y = node.y - min_y + padding / 2.0;
        node.home_x = node.x;
        node.home_y = node.y;
    }
}
