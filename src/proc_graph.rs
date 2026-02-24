//! Shared procedural helpers for CHOP/SOP/TOP-camera graph nodes.

use crate::chop::{ChopLfoNode, ChopMathMode, ChopMathNode, ChopRemapNode, ChopWave};
use crate::node::GraphTimeInput;
use crate::sop::{SopCircleNode, SopSphereNode, TopCameraRenderNode};

/// Runtime SOP primitive payload evaluated from one SOP node.
#[derive(Clone, Copy, Debug)]
pub enum SopPrimitive {
    Circle(SopCircleNode),
    Sphere(SopSphereNode),
}

/// Evaluate one LFO channel node at the current graph time.
pub fn eval_chop_lfo(node: ChopLfoNode, time: Option<GraphTimeInput>) -> f32 {
    let t = time.map(|sample| sample.normalized).unwrap_or(0.0);
    let phase = t * node.frequency + node.phase;
    let wave = match node.wave {
        ChopWave::Sine => (phase * std::f32::consts::TAU).sin(),
        ChopWave::Triangle => 1.0 - ((phase.fract() - 0.5).abs() * 4.0),
        ChopWave::Saw => phase.fract() * 2.0 - 1.0,
    };
    node.offset + wave * node.amplitude
}

/// Evaluate one math channel node from input channels and constants.
pub fn eval_chop_math(node: ChopMathNode, a: f32, b: Option<f32>) -> f32 {
    let rhs = b.unwrap_or(node.value);
    match node.mode {
        ChopMathMode::Add => a + rhs,
        ChopMathMode::Multiply => a * rhs,
        ChopMathMode::Min => a.min(rhs),
        ChopMathMode::Max => a.max(rhs),
        ChopMathMode::Mix => a + (rhs - a) * node.blend.clamp(0.0, 1.0),
    }
}

/// Remap one channel value from input range to output range.
pub fn eval_chop_remap(node: ChopRemapNode, value: f32) -> f32 {
    let denom = (node.in_max - node.in_min).abs().max(1e-6);
    let mut t = (value - node.in_min) / denom;
    if node.clamp {
        t = t.clamp(0.0, 1.0);
    }
    node.out_min + (node.out_max - node.out_min) * t
}

/// Render one SOP primitive through a simple camera model into `out`.
pub fn render_top_camera(
    primitive: SopPrimitive,
    node: TopCameraRenderNode,
    channel_mod: Option<f32>,
    width: u32,
    height: u32,
    out: &mut [f32],
) {
    let width_f = width.max(1) as f32;
    let height_f = height.max(1) as f32;
    let modulation = channel_mod.unwrap_or(1.0).clamp(0.2, 3.0);
    let zoom = (node.zoom * modulation).clamp(0.2, 4.0);
    let cos_r = node.rotate.cos();
    let sin_r = node.rotate.sin();

    for y in 0..height {
        for x in 0..width {
            let i = (x + y * width) as usize;
            let ux = x as f32 / width_f - 0.5;
            let uy = y as f32 / height_f - 0.5;

            let px = (ux - node.pan_x) / zoom;
            let py = (uy - node.pan_y) / zoom;
            let rx = px * cos_r - py * sin_r;
            let ry = px * sin_r + py * cos_r;

            let mut value = match primitive {
                SopPrimitive::Circle(circle) => sample_circle(circle, rx, ry),
                SopPrimitive::Sphere(sphere) => sample_sphere(sphere, rx, ry),
            };

            value = (value * node.exposure.max(0.0))
                .max(0.0)
                .powf(1.0 / node.gamma.max(0.2));
            if node.invert {
                value = 1.0 - value;
            }
            out[i] = value.clamp(0.0, 1.0);
        }
    }
}

fn sample_circle(circle: SopCircleNode, x: f32, y: f32) -> f32 {
    let dx = x - circle.center_x;
    let dy = y - circle.center_y;
    let distance = (dx * dx + dy * dy).sqrt();
    let radius = circle.radius.max(0.01);
    let feather = circle.feather.max(1e-4);
    smoothstep(radius + feather, radius - feather, distance)
}

fn sample_sphere(sphere: SopSphereNode, x: f32, y: f32) -> f32 {
    let dx = x - sphere.center_x;
    let dy = y - sphere.center_y;
    let radius = sphere.radius.max(0.01);
    let rr = radius * radius;
    let dist2 = dx * dx + dy * dy;
    if dist2 > rr {
        return 0.0;
    }

    let z = (rr - dist2).sqrt();
    let inv_r = 1.0 / radius;
    let nx = dx * inv_r;
    let ny = dy * inv_r;
    let nz = z * inv_r;

    let mut lx = sphere.light_x;
    let mut ly = sphere.light_y;
    let mut lz = 1.0;
    let len = (lx * lx + ly * ly + lz * lz).sqrt().max(1e-6);
    lx /= len;
    ly /= len;
    lz /= len;

    let diffuse = (nx * lx + ny * ly + nz * lz).max(0.0);
    (sphere.ambient.clamp(0.0, 1.0) + (1.0 - sphere.ambient.clamp(0.0, 1.0)) * diffuse)
        .clamp(0.0, 1.0)
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    if (edge1 - edge0).abs() < f32::EPSILON {
        return if x >= edge0 { 1.0 } else { 0.0 };
    }
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
