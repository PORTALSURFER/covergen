//! Generative strategy module used for CPU-side procedural rendering paths.
//!
//! The main application primarily renders through GPU compute shaders. This module
//! adds a secondary family of CPU generators to diversify outputs with non-fractal
//! algorithms and to reduce over-convergence toward one visual style.

use crate::XorShift32;
use std::f32::consts::TAU;

/// CPU generator strategies that can replace GPU-based render layers.
#[derive(Clone, Copy, Debug)]
pub enum CpuStrategy {
    /// Sobel edge detector rendered from layered noise.
    EdgeSobel,
    /// Laplacian-like edge detector rendered from layered noise.
    EdgeLaplacian,
    /// Recursive maze carve in a low-resolution grid, upscaled to final resolution.
    Maze,
    /// Gray-Scott reaction-diffusion simulation.
    ReactionDiffusion,
    /// L-system turtle growth.
    LSystem,
    /// Folded/noisy procedural field.
    ProceduralNoise,
    /// Cellular automata with evolving binary rules.
    CellularAutomata,
    /// Particle flow accumulation in a synthetic vector field.
    ParticleFlow,
    /// Voronoi-style ridge map.
    Voronoi,
    /// Sparse edge list inspired triangulation-like structure.
    Delaunay,
    /// IFS-like iterated point fractal.
    IteratedFractal,
    /// Chaotic strange-attractor trajectory density.
    StrangeAttractor,
    /// Radial-wave-like interference pattern.
    RadialWave,
    /// Recursive folding map with multiple map iterations.
    RecursiveFold,
    /// Attractor trajectory blended from two chaotic updates.
    AttractorHybrid,
}

impl CpuStrategy {
    /// Total number of CPU strategies available.
    fn count() -> u32 {
        15
    }

    /// Creates a strategy from an arbitrary value.
    pub fn from_u32(value: u32) -> Self {
        match value % Self::count() {
            0 => Self::EdgeSobel,
            1 => Self::EdgeLaplacian,
            2 => Self::Maze,
            3 => Self::ReactionDiffusion,
            4 => Self::LSystem,
            5 => Self::ProceduralNoise,
            6 => Self::CellularAutomata,
            7 => Self::ParticleFlow,
            8 => Self::Voronoi,
            9 => Self::Delaunay,
            10 => Self::IteratedFractal,
            11 => Self::StrangeAttractor,
            12 => Self::RadialWave,
            13 => Self::RecursiveFold,
            _ => Self::AttractorHybrid,
        }
    }

    /// Human-readable strategy name.
    pub fn label(self) -> &'static str {
        match self {
            Self::EdgeSobel => "edge-sobel",
            Self::EdgeLaplacian => "edge-laplacian",
            Self::Maze => "maze",
            Self::ReactionDiffusion => "reaction-diffusion",
            Self::LSystem => "l-system",
            Self::ProceduralNoise => "procedural-noise",
            Self::CellularAutomata => "cellular-automata",
            Self::ParticleFlow => "particle-flow",
            Self::Voronoi => "voronoi",
            Self::Delaunay => "delaunay",
            Self::IteratedFractal => "iterated-fractal",
            Self::StrangeAttractor => "strange-attractor",
            Self::RadialWave => "radial-wave",
            Self::RecursiveFold => "recursive-fold",
            Self::AttractorHybrid => "attractor-hybrid",
        }
    }
}

/// Rendering path selected for a layer.
#[derive(Clone, Copy, Debug)]
pub enum RenderStrategy {
    /// Route layer to a GPU compute style.
    Gpu(u32),
    /// Route layer to a CPU strategy.
    Cpu(CpuStrategy),
}

impl RenderStrategy {
    /// Human-readable strategy name.
    pub fn label(self) -> &'static str {
        match self {
            Self::Gpu(style) => crate::ArtStyle::from_u32(style).label(),
            Self::Cpu(kind) => kind.label(),
        }
    }
}

/// Strategy-specific tuning for downstream post-processing.
#[derive(Clone, Copy, Debug)]
pub struct StrategyProfile {
    /// Multiplier applied to the probability of dynamic filtering.
    pub filter_bias: f32,
    /// Multiplier applied to the probability of gradient maps.
    pub gradient_bias: f32,
    /// If true, force additional structure-preserving operations.
    pub force_detail: bool,
}

/// Returns a render strategy for the next layer.
pub fn pick_render_strategy(rng: &mut XorShift32, fast: bool) -> RenderStrategy {
    let strategy_roll = rng.next_f32();
    let gpu_chance = if fast { 0.44 } else { 0.34 };

    if strategy_roll < gpu_chance {
        return RenderStrategy::Gpu(crate::ArtStyle::from_u32(rng.next_u32()).as_u32());
    }

    let cpu_bias = if fast { 21 } else { 15 };
    RenderStrategy::Cpu(CpuStrategy::from_u32(rng.next_u32() + cpu_bias))
}

/// Returns post-processing guidance for a strategy.
pub fn strategy_profile(strategy: RenderStrategy) -> StrategyProfile {
    match strategy {
        RenderStrategy::Gpu(_) => StrategyProfile {
            filter_bias: 1.0,
            gradient_bias: 1.0,
            force_detail: false,
        },
        RenderStrategy::Cpu(kind) => match kind {
            CpuStrategy::ReactionDiffusion
            | CpuStrategy::StrangeAttractor
            | CpuStrategy::IteratedFractal
            | CpuStrategy::LSystem
            | CpuStrategy::Maze
            | CpuStrategy::ParticleFlow => StrategyProfile {
                filter_bias: 0.20,
                gradient_bias: 0.24,
                force_detail: true,
            },
            CpuStrategy::Voronoi
            | CpuStrategy::Delaunay
            | CpuStrategy::EdgeSobel
            | CpuStrategy::EdgeLaplacian => StrategyProfile {
                filter_bias: 0.30,
                gradient_bias: 0.22,
                force_detail: true,
            },
            CpuStrategy::RadialWave | CpuStrategy::RecursiveFold => StrategyProfile {
                filter_bias: 0.22,
                gradient_bias: 0.20,
                force_detail: true,
            },
            CpuStrategy::CellularAutomata | CpuStrategy::ProceduralNoise => StrategyProfile {
                filter_bias: 0.44,
                gradient_bias: 0.38,
                force_detail: false,
            },
            CpuStrategy::AttractorHybrid => StrategyProfile {
                filter_bias: 0.18,
                gradient_bias: 0.24,
                force_detail: true,
            },
        },
    }
}

/// Render a CPU strategy for a layer and return a normalized gray buffer.
pub fn render_cpu_strategy(
    strategy: CpuStrategy,
    width: u32,
    height: u32,
    seed: u32,
    fast: bool,
) -> Vec<f32> {
    let mut rng = XorShift32::new(seed ^ 0x9e37_79b9);
    match strategy {
        CpuStrategy::EdgeSobel => render_edge_field(width, height, &mut rng, true),
        CpuStrategy::EdgeLaplacian => render_edge_field(width, height, &mut rng, false),
        CpuStrategy::Maze => render_maze_field(width, height, &mut rng),
        CpuStrategy::ReactionDiffusion => render_reaction_diffusion(width, height, &mut rng, fast),
        CpuStrategy::LSystem => render_lsystem(width, height, &mut rng),
        CpuStrategy::ProceduralNoise => render_noise_field(width, height, &mut rng),
        CpuStrategy::CellularAutomata => render_cellular_automata(width, height, &mut rng, fast),
        CpuStrategy::ParticleFlow => render_particle_flow(width, height, &mut rng, fast),
        CpuStrategy::Voronoi => render_voronoi(width, height, &mut rng),
        CpuStrategy::Delaunay => render_delaunay(width, height, &mut rng),
        CpuStrategy::IteratedFractal => render_iterated_fractal(width, height, &mut rng, fast),
        CpuStrategy::StrangeAttractor => render_strange_attractor(width, height, &mut rng, fast),
        CpuStrategy::RadialWave => render_radial_wave(width, height, &mut rng),
        CpuStrategy::RecursiveFold => render_recursive_fold(width, height, &mut rng, fast),
        CpuStrategy::AttractorHybrid => render_attractor_hybrid(width, height, &mut rng, fast),
    }
}

fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

fn hash_u32(mut value: u32) -> u32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x85eb_ca6b);
    value ^= value >> 13;
    value = value.wrapping_mul(0xc2b2_ae35);
    value ^= value >> 16;
    value
}

fn mix_hash(a: u32, b: u32, seed: u32) -> u32 {
    hash_u32(a ^ (b.rotate_left(11)).wrapping_add(seed.wrapping_mul(0x9e37_79b9)))
}

fn hash01(value: u32, seed: u32) -> f32 {
    (mix_hash(value, value.rotate_left(13), seed) as f32) / (u32::MAX as f32)
}

fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let xf = x.floor();
    let yf = y.floor();
    let xq = xf.rem_euclid(8192.0);
    let yq = yf.rem_euclid(8192.0);
    let x0 = xq as i32;
    let y0 = yq as i32;
    let fx = x - xf;
    let fy = y - yf;

    let a = hash01(x0 as u32, seed ^ (y0 as u32));
    let b = hash01((x0 + 1) as u32, seed ^ (y0 as u32));
    let c = hash01(x0 as u32, seed ^ ((y0 + 1) as u32));
    let d = hash01((x0 + 1) as u32, seed ^ ((y0 + 1) as u32));
    let nx0 = a + (b - a) * fx;
    let nx1 = c + (d - c) * fx;
    nx0 + (nx1 - nx0) * fy
}

fn noise_field(width: u32, height: u32, rng: &mut XorShift32, octaves: u32) -> Vec<f32> {
    let width_f = width as f32;
    let height_f = height as f32;
    let seed = rng.next_u32();
    let mut out = vec![0.0f32; (width * height) as usize];
    for y in 0..height {
        for x in 0..width {
            let px = x as f32 / width_f;
            let py = y as f32 / height_f;
            let mut frequency = 1.2 + rng.next_f32() * 1.2;
            let mut amp = 1.0;
            let mut value = 0.0;
            let mut norm = 0.0;
            let mut i = 0;

            while i < octaves {
                value += value_noise(
                    px * frequency * width_f * 0.002 + (seed % 8192) as f32,
                    py * frequency * height_f * 0.002 + (seed % 4096) as f32,
                    seed + i,
                ) * amp;
                norm += amp;
                amp *= 0.5;
                frequency *= 2.0;
                i += 1;
            }

            let idx = (y * width + x) as usize;
            out[idx] = if norm > 0.0 { value / norm } else { 0.5 };
        }
    }
    normalize(&mut out);
    out
}

fn normalize(src: &mut [f32]) {
    if src.is_empty() {
        return;
    }

    let mut min = 1.0f32;
    let mut max = 0.0f32;
    for &value in src.iter() {
        min = min.min(value);
        max = max.max(value);
    }
    let span = (max - min).max(1e-8);
    for value in src.iter_mut() {
        *value = ((*value - min) / span).clamp(0.0, 1.0);
    }
}

fn resize_nearest(src: &[f32], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<f32> {
    let mut out = vec![0.0f32; (dst_w * dst_h) as usize];
    let src_w = src_w.max(1) as f32;
    let src_h = src_h.max(1) as f32;
    for y in 0..dst_h {
        let sy = ((y as f32 / dst_h as f32) * src_h)
            .floor()
            .clamp(0.0, src_h - 1.0) as usize;
        for x in 0..dst_w {
            let sx = ((x as f32 / dst_w as f32) * src_w)
                .floor()
                .clamp(0.0, src_w - 1.0) as usize;
            out[(y * dst_w + x) as usize] = src[sy * src_w as usize + sx];
        }
    }
    out
}

fn resize_bilinear(src: &[f32], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<f32> {
    let mut out = vec![0.0f32; (dst_w * dst_h) as usize];
    let src_w = src_w.max(1) as f32;
    let src_h = src_h.max(1) as f32;
    let width = dst_w as usize;
    for y in 0..dst_h {
        let yf = (y as f32 / dst_h as f32) * src_h;
        let y0 = yf.floor() as usize;
        let y1 = (y0 + 1).min((src_h as usize).saturating_sub(1));
        let wy = yf - y0 as f32;
        for x in 0..dst_w {
            let xf = (x as f32 / dst_w as f32) * src_w;
            let x0 = xf.floor() as usize;
            let x1 = (x0 + 1).min((src_w as usize).saturating_sub(1));
            let wx = xf - x0 as f32;

            let a = src[y0 * src_w as usize + x0];
            let b = src[y0 * src_w as usize + x1];
            let c = src[y1 * src_w as usize + x0];
            let d = src[y1 * src_w as usize + x1];
            let top = a * (1.0 - wx) + b * wx;
            let bot = c * (1.0 - wx) + d * wx;
            let idx = y as usize * width + x as usize;
            out[idx] = top * (1.0 - wy) + bot * wy;
        }
    }
    out
}

fn sample_nearest(src: &[f32], width: u32, height: u32, x: i32, y: i32) -> f32 {
    let x = x.clamp(0, width as i32 - 1);
    let y = y.clamp(0, height as i32 - 1);
    src[y as usize * width as usize + x as usize]
}

fn draw_point(
    x: i32,
    y: i32,
    width: usize,
    height: usize,
    radius: i32,
    value: f32,
    out: &mut [f32],
) {
    for dy in -radius..=radius {
        for dx in -radius..=radius {
            let nx = x + dx;
            let ny = y + dy;
            if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                continue;
            }
            let dist = ((dx * dx + dy * dy) as f32).sqrt();
            let tap = if radius > 0 {
                (1.0 - (dist / (radius as f32 + 0.1))).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let idx = ny as usize * width + nx as usize;
            out[idx] = clamp01(out[idx] + value * tap);
        }
    }
}

fn draw_line(
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    width: usize,
    height: usize,
    value: f32,
    out: &mut [f32],
) {
    let mut x0 = x0;
    let mut y0 = y0;
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        draw_point(x0, y0, width, height, 1, value, out);
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

fn render_edge_field(width: u32, height: u32, rng: &mut XorShift32, sobel: bool) -> Vec<f32> {
    let base = noise_field(width, height, rng, 4);
    let mut out = vec![0.0f32; base.len()];
    for y in 0..height as i32 {
        for x in 0..width as i32 {
            let c = sample_nearest(&base, width, height, x, y);
            let n = sample_nearest(&base, width, height, x, y - 1);
            let s = sample_nearest(&base, width, height, x, y + 1);
            let e = sample_nearest(&base, width, height, x + 1, y);
            let wv = sample_nearest(&base, width, height, x - 1, y);
            let ne = sample_nearest(&base, width, height, x + 1, y - 1);
            let nw = sample_nearest(&base, width, height, x - 1, y - 1);
            let se = sample_nearest(&base, width, height, x + 1, y + 1);
            let sw = sample_nearest(&base, width, height, x - 1, y + 1);

            let value = if sobel {
                let gx = -ne - 2.0 * e - se + nw + 2.0 * wv + sw;
                let gy = -nw - 2.0 * n - ne + sw + 2.0 * s + se;
                (gx * gx + gy * gy).sqrt()
            } else {
                let m = 4.0 * c - (n + s + e + wv);
                m.abs() * 1.7
            };
            out[(y as usize * width as usize) + x as usize] = value;
        }
    }
    normalize(&mut out);
    for value in out.iter_mut() {
        *value = (*value * (0.8 + rng.next_f32() * 0.5)).clamp(0.0, 1.0);
    }
    normalize(&mut out);
    out
}

fn render_maze_field(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let maze_w = (width / 6).max(56).min(168);
    let maze_h = (height / 6).max(56).min(168);
    let maze_grid_w = (maze_w | 1) as usize;
    let maze_grid_h = (maze_h | 1) as usize;
    let cell_w = (maze_grid_w / 2) as usize;
    let cell_h = (maze_grid_h / 2) as usize;

    let mut walls = vec![true; maze_grid_w * maze_grid_h];
    let mut visited = vec![false; cell_w * cell_h];

    let to_cell = |cx: usize, cy: usize| -> usize { cy * cell_w + cx };
    let to_wall = |wx: usize, wy: usize| -> usize { wy * maze_grid_w + wx };
    let mut stack = Vec::with_capacity(cell_w * cell_h);
    let start_x = rng.next_u32() as usize % cell_w;
    let start_y = rng.next_u32() as usize % cell_h;
    visited[to_cell(start_x, start_y)] = true;
    stack.push((start_x, start_y));
    walls[to_wall(start_x * 2 + 1, start_y * 2 + 1)] = false;

    while let Some(&(cx, cy)) = stack.last() {
        let mut options = Vec::with_capacity(4);
        if cx > 0 && !visited[to_cell(cx - 1, cy)] {
            options.push((cx - 1, cy, -2i32, 0i32));
        }
        if cx + 1 < cell_w && !visited[to_cell(cx + 1, cy)] {
            options.push((cx + 1, cy, 2i32, 0i32));
        }
        if cy > 0 && !visited[to_cell(cx, cy - 1)] {
            options.push((cx, cy - 1, 0i32, -2i32));
        }
        if cy + 1 < cell_h && !visited[to_cell(cx, cy + 1)] {
            options.push((cx, cy + 1, 0i32, 2i32));
        }

        if options.is_empty() {
            stack.pop();
            continue;
        }

        let pick = options[rng.next_u32() as usize % options.len()];
        let nx = pick.0;
        let ny = pick.1;
        let wx = cx as i32 * 2 + 1 + pick.2;
        let wy = cy as i32 * 2 + 1 + pick.3;
        let wx = wx as usize;
        let wy = wy as usize;

        visited[to_cell(nx, ny)] = true;
        walls[to_wall(wx, wy)] = false;
        walls[to_wall(nx * 2 + 1, ny * 2 + 1)] = false;
        stack.push((nx, ny));
    }

    let mut out = vec![0.0f32; (width * height) as usize];
    for y in 0..height {
        for x in 0..width {
            let wx = ((x as f32 / width as f32) * (maze_grid_w as f32 - 1.0)).floor() as usize;
            let wy = ((y as f32 / height as f32) * (maze_grid_h as f32 - 1.0)).floor() as usize;
            let idx = (y * width + x) as usize;
            out[idx] = if walls[wy * maze_grid_w + wx] {
                0.05 + 0.1 * rng.next_f32()
            } else {
                0.75 + 0.22 * rng.next_f32()
            };
        }
    }
    normalize(&mut out);
    out
}

fn render_reaction_diffusion(
    width: u32,
    height: u32,
    rng: &mut XorShift32,
    fast: bool,
) -> Vec<f32> {
    let sim_w = (width / 2).max(128);
    let sim_h = (height / 2).max(128);
    let iterations: usize = if fast { 420 } else { 900 };
    let mut u = vec![1.0f32; (sim_w * sim_h) as usize];
    let mut v = vec![0.0f32; (sim_w * sim_h) as usize];
    let mut un = vec![0.0f32; (sim_w * sim_h) as usize];
    let mut vn = vec![0.0f32; (sim_w * sim_h) as usize];

    let mut seed_rng = XorShift32::new(rng.next_u32() ^ 0x1234_5678);
    for v_cell in v.iter_mut() {
        *v_cell = if seed_rng.next_f32() < 0.035 {
            0.9
        } else {
            0.0
        };
    }

    let du = 0.16;
    let dv = 0.08;
    let dt = 1.0;
    let f = 0.034 + rng.next_f32() * 0.018;
    let k = 0.058 + rng.next_f32() * 0.018;
    let w = sim_w as usize;
    let h = sim_h as usize;

    let mut step = 0usize;
    while step < iterations {
        let mut y = 1usize;
        while y < h - 1 {
            let mut x = 1usize;
            while x < w - 1 {
                let idx = y * w + x;
                let u0 = u[idx];
                let v0 = v[idx];
                let lap_u = u[idx - 1] + u[idx + 1] + u[idx - w] + u[idx + w] - 4.0 * u0;
                let lap_v = v[idx - 1] + v[idx + 1] + v[idx - w] + v[idx + w] - 4.0 * v0;
                let uvv = u0 * v0 * v0;
                un[idx] = (u0 + (du * lap_u - uvv + f * (1.0 - u0)) * dt).clamp(0.0, 1.0);
                vn[idx] = (v0 + (dv * lap_v + uvv - (f + k) * v0) * dt).clamp(0.0, 1.0);
                x += 1;
            }
            y += 1;
        }
        std::mem::swap(&mut u, &mut un);
        std::mem::swap(&mut v, &mut vn);
        step += 1;
    }

    let mut out = v;
    normalize(&mut out);
    resize_nearest(&out, sim_w, sim_h, width, height)
}

fn render_lsystem(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim = (width / 2).max(160);
    let mut state = vec!['X'];
    let iterations = (3 + rng.next_u32() % 3) as usize;

    let rules = [
        ('X', "F[+X][-X]F"),
        ('F', "F[+F]F[-F]F"),
        ('X', "FF+[+X]-[-X]F"),
    ];

    let mut i = 0;
    while i < iterations {
        let mut next = Vec::with_capacity(state.len() * 2);
        for ch in state.iter().copied() {
            let mut replaced = false;
            for &(symbol, replacement) in &rules {
                if ch == symbol {
                    next.extend(replacement.chars());
                    replaced = true;
                    break;
                }
            }
            if !replaced {
                next.push(ch);
            }
            if next.len() > 60_000 {
                break;
            }
        }
        if next.is_empty() {
            break;
        }
        state = next;
        i += 1;
    }

    let mut canvas = vec![0.0f32; (sim * sim) as usize];
    let mut stack = Vec::new();
    let mut x = sim as f32 * 0.5;
    let mut y = sim as f32 * 0.98;
    let mut angle = -std::f32::consts::FRAC_PI_2;
    let mut step = (sim as f32 / 170.0) * (0.5 + rng.next_f32() * 0.8);
    let delta = (16.0 + rng.next_f32() * 34.0).to_radians();
    let branch_shrink = 0.997 + rng.next_f32() * 0.003;

    for token in state {
        match token {
            'F' => {
                let nx = x + step * angle.cos();
                let ny = y + step * angle.sin();
                draw_line(
                    x.round() as i32,
                    y.round() as i32,
                    nx.round() as i32,
                    ny.round() as i32,
                    sim as usize,
                    sim as usize,
                    0.9,
                    &mut canvas,
                );
                x = nx;
                y = ny;
            }
            '+' => angle += delta,
            '-' => angle -= delta,
            '[' => stack.push((x, y, angle)),
            ']' => {
                if let Some(state) = stack.pop() {
                    x = state.0;
                    y = state.1;
                    angle = state.2;
                }
            }
            _ => {}
        }
        if step > 0.3 {
            step *= branch_shrink;
        }
    }

    let mut out = canvas;
    normalize(&mut out);
    resize_nearest(&out, sim, sim, width, height)
}

fn render_noise_field(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let mut out = noise_field(width, height, rng, 6);
    for value in out.iter_mut() {
        let folded = (*value * 2.0 - 1.0).abs();
        *value = (1.0 - folded).powf(1.0 + rng.next_f32() * 1.3);
    }
    normalize(&mut out);
    out
}

fn render_radial_wave(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let mut out = vec![0.0f32; (width * height) as usize];
    let width_f = width.max(1) as f32;
    let height_f = height.max(1) as f32;
    let cx = (rng.next_f32() - 0.5) * 0.2;
    let cy = (rng.next_f32() - 0.5) * 0.2;
    let base_freq = 2.0 + rng.next_f32() * 6.0;
    let bands = 2 + (rng.next_u32() % 4) as u32;
    let seed = rng.next_u32();

    for y in 0..height as i32 {
        for x in 0..width as i32 {
            let nx = (x as f32 / width_f) * 2.0 - 1.0 + cx;
            let ny = (y as f32 / height_f) * 2.0 - 1.0 + cy;
            let angle = ny.atan2(nx);
            let radius = (nx * nx + ny * ny).sqrt().max(1e-6);
            let mut value = 0.0f32;
            let mut i = 0u32;
            while i < bands {
                let tone = 0.30 + (i as f32) * 0.18;
                let frequency = base_freq * (1.0 + i as f32 * 0.33);
                let wave =
                    (radius * frequency * TAU + angle * (0.75 + i as f32 * 0.22) + angle * 2.8)
                        .sin()
                        .abs();
                let orbital = (angle * frequency * 1.1).cos().abs();
                let shell = (1.0 - (radius * tone).clamp(0.0, 1.0)).clamp(0.0, 1.0);
                let noise = value_noise(
                    nx * 4.5 + 0.5 * i as f32 + seed as f32 / 16_777_216.0,
                    ny * 4.5 - 0.5 * i as f32 + seed as f32 / 32_768.0,
                    seed + i,
                );
                value += (wave * shell + orbital * 0.35 + noise * 0.12) / (1.0 + i as f32 * 0.8);
                i += 1;
            }
            out[(y as usize * width as usize) + x as usize] = value;
        }
    }

    normalize(&mut out);
    out
}

fn render_recursive_fold(width: u32, height: u32, rng: &mut XorShift32, fast: bool) -> Vec<f32> {
    let mut out = vec![0.0f32; (width * height) as usize];
    let width_f = width.max(1) as f32;
    let height_f = height.max(1) as f32;
    let fold_center_x = (rng.next_f32() - 0.5) * 0.35;
    let fold_center_y = (rng.next_f32() - 0.5) * 0.35;
    let base_rot = rng.next_f32() * TAU;
    let base_scale = if fast { 0.86 } else { 0.78 };
    let iterations = if fast { 26 } else { 44 };
    let seed = rng.next_u32();

    for y in 0..height as i32 {
        for x in 0..width as i32 {
            let mut px = (x as f32 / width_f) * 2.0 - 1.0 + fold_center_x;
            let mut py = (y as f32 / height_f) * 2.0 - 1.0 + fold_center_y;
            let mut field = 0.0f32;
            let mut loop_idx = 0u32;
            while loop_idx < iterations {
                let step = loop_idx as f32 / iterations as f32;
                let angle = base_rot + 0.85 + step * 2.3;
                let c = angle.cos();
                let s = angle.sin();
                let tx = px * c - py * s;
                let ty = px * s + py * c;
                let cx = 0.48 + 0.18 * (seed as f32 * 0.000001 + step).sin();
                let cy = 0.34 + 0.16 * (seed as f32 * 0.000002 - step).cos();
                px = (tx.abs() - cx) * (1.0 - 0.55 * step);
                py = (ty.abs() - cy) * (1.0 - 0.47 * step);
                let n = value_noise(
                    px * 3.5 + step * 1.7 + seed as f32,
                    py * 3.5 - step * 1.3 - seed as f32,
                    seed + loop_idx,
                );
                let magnitude = (px * px + py * py).sqrt();
                let fold = (1.0 / (1.0 + magnitude)).clamp(0.0, 1.0);
                field += (fold * 0.72 + 0.28 * n) * (base_scale + 0.22 * (1.0 - step));
                field = field.clamp(0.0, 1.0);
                px = px * base_scale + (n - 0.5) * 0.25 * (1.0 - step);
                py = py * base_scale + (0.5 - n) * 0.25 * (1.0 - step);
                loop_idx += 1;
            }
            out[(y as usize * width as usize) + x as usize] = field;
        }
    }

    normalize(&mut out);
    out
}

fn render_cellular_automata(width: u32, height: u32, rng: &mut XorShift32, fast: bool) -> Vec<f32> {
    let sim_w = (width / 2).max(128);
    let sim_h = (height / 2).max(128);
    let n = (sim_w * sim_h) as usize;
    let mut state = vec![0u8; n];
    let mut next = vec![0u8; n];
    for cell in state.iter_mut() {
        *cell = if rng.next_f32() < 0.45 { 1 } else { 0 };
    }
    let w = sim_w as i32;
    let h = sim_h as i32;
    let iters = if fast { 46 } else { 84 };
    let birth_min = 3 + (rng.next_u32() % 2) as i32;
    let birth_max = birth_min + 2;
    let survive_min = 2;
    let survive_max = if fast { 4 } else { 5 };

    for _ in 0..iters {
        let mut y = 0i32;
        while y < h {
            let mut x = 0i32;
            while x < w {
                let mut neighbors = 0i32;
                let mut dy = -1i32;
                while dy <= 1 {
                    let mut dx = -1i32;
                    while dx <= 1 {
                        if dx != 0 || dy != 0 {
                            let sx = (x + dx).rem_euclid(w) as usize;
                            let sy = (y + dy).rem_euclid(h) as usize;
                            neighbors += state[sy * w as usize + sx] as i32;
                        }
                        dx += 1;
                    }
                    dy += 1;
                }
                let idx = y as usize * w as usize + x as usize;
                next[idx] = match state[idx] {
                    1 if (survive_min..=survive_max).contains(&neighbors) => 1,
                    0 if (birth_min..=birth_max).contains(&neighbors) => 1,
                    _ => 0,
                };
                x += 1;
            }
            y += 1;
        }
        std::mem::swap(&mut state, &mut next);
    }

    let mut out = vec![0.0f32; n];
    for i in 0..n {
        out[i] = state[i] as f32;
    }
    normalize(&mut out);
    resize_nearest(&out, sim_w, sim_h, width, height)
}

fn render_particle_flow(width: u32, height: u32, rng: &mut XorShift32, fast: bool) -> Vec<f32> {
    let sim_w = (width / 2).max(128);
    let sim_h = (height / 2).max(128);
    let mut density = vec![0.0f32; (sim_w * sim_h) as usize];
    let steps = if fast { 92 } else { 184 };
    let particles = ((sim_w * sim_h) / if fast { 18 } else { 12 }).max(1400);
    let flow_scale = 0.0027 + rng.next_f32() * 0.0016;
    let speed = 0.45 + rng.next_f32() * 1.45;
    let seed = rng.next_u32();

    for p in 0..particles {
        let mut x = ((seed.wrapping_add(p).wrapping_mul(1_103_515_245)) % sim_w) as f32 + 0.5;
        let mut y = ((seed.wrapping_add(p).wrapping_mul(3_456_789)) % sim_h) as f32 + 0.5;
        let mut i = 0;
        while i < steps {
            let angle = value_noise(x * flow_scale, y * flow_scale, seed ^ p) * TAU;
            x = (x + angle.cos() * speed + sim_w as f32).rem_euclid(sim_w as f32);
            y = (y + angle.sin() * speed + sim_h as f32).rem_euclid(sim_h as f32);
            let ix = x as usize;
            let iy = y as usize;
            let idx = iy * sim_w as usize + ix;
            density[idx] += 1.0;
            if i % 2 == 0 {
                draw_point(
                    ix as i32,
                    iy as i32,
                    sim_w as usize,
                    sim_h as usize,
                    1,
                    0.8,
                    &mut density,
                );
            }
            i += 1;
        }
    }

    normalize(&mut density);
    let mut blur = if fast {
        resize_bilinear(&density, sim_w, sim_h, width, height)
    } else {
        resize_nearest(&density, sim_w, sim_h, width, height)
    };
    for value in blur.iter_mut() {
        *value = clamp01(value.powf(0.86));
    }
    normalize(&mut blur);
    blur
}

fn render_voronoi(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim_w = (width / 2).max(128);
    let sim_h = (height / 2).max(128);
    let site_count = 14 + (rng.next_u32() % 26) as usize;
    let mut sites = Vec::with_capacity(site_count);
    for _ in 0..site_count {
        sites.push((rng.next_f32(), rng.next_f32()));
    }

    let mut out = vec![0.0f32; (sim_w * sim_h) as usize];
    let mut max_diff = 0.0f32;
    for y in 0..sim_h {
        for x in 0..sim_w {
            let fx = x as f32 / sim_w as f32;
            let fy = y as f32 / sim_h as f32;
            let mut nearest = f32::INFINITY;
            let mut second = f32::INFINITY;
            for &(sx, sy) in &sites {
                let dx = fx - sx;
                let dy = fy - sy;
                let d = dx * dx + dy * dy;
                if d < nearest {
                    second = nearest;
                    nearest = d;
                } else if d < second {
                    second = d;
                }
            }
            let gap = (second.sqrt() - nearest.sqrt()).abs();
            max_diff = max_diff.max(gap);
            out[(y * sim_w + x) as usize] = gap;
        }
    }
    if max_diff > 0.0 {
        for value in out.iter_mut() {
            *value = *value / max_diff;
        }
    }
    normalize(&mut out);
    resize_bilinear(&out, sim_w, sim_h, width, height)
}

fn render_delaunay(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim_w = (width / 2).max(128);
    let sim_h = (height / 2).max(128);
    let point_count = 10 + (rng.next_u32() % 20) as usize;
    let mut points: Vec<(i32, i32)> = Vec::with_capacity(point_count);
    for _ in 0..point_count {
        points.push((
            rng.next_u32() as i32 % sim_w as i32,
            rng.next_u32() as i32 % sim_h as i32,
        ));
    }
    let mut out = vec![0.05f32; (sim_w * sim_h) as usize];
    let mut i = 0usize;
    while i < points.len() {
        let (x0, y0) = points[i];
        let mut distances = Vec::with_capacity(points.len().saturating_sub(1));
        let mut j = 0usize;
        while j < points.len() {
            if i != j {
                let (x1, y1) = points[j];
                let dx = (x0 - x1) as f32;
                let dy = (y0 - y1) as f32;
                distances.push((dx * dx + dy * dy, x1, y1));
            }
            j += 1;
        }
        distances.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        for &(_, sx, sy) in distances.iter().take(4) {
            draw_line(
                x0,
                y0,
                sx,
                sy,
                sim_w as usize,
                sim_h as usize,
                0.8,
                &mut out,
            );
        }
        i += 1;
    }
    let mut with_bg = out;
    for value in with_bg.iter_mut() {
        *value = (*value * (0.75 + rng.next_f32() * 0.25)).clamp(0.0, 1.0);
    }
    let noise = noise_field(sim_w, sim_h, rng, 2);
    for idx in 0..with_bg.len() {
        with_bg[idx] = clamp01(with_bg[idx] + (noise[idx] - 0.5) * 0.03);
    }
    normalize(&mut with_bg);
    resize_nearest(&with_bg, sim_w, sim_h, width, height)
}

fn render_iterated_fractal(width: u32, height: u32, rng: &mut XorShift32, fast: bool) -> Vec<f32> {
    let sim_w = (width / 2).max(128);
    let sim_h = (height / 2).max(128);
    let mut density = vec![0.0f32; (sim_w * sim_h) as usize];
    let points = if fast { 45_000 } else { 95_000 };
    let mut x = 0.0f32;
    let mut y = 0.0f32;
    let a = rng.next_f32() * 1.2 - 0.6;
    let b = rng.next_f32() * 1.2 - 0.6;
    let c = rng.next_f32() * 1.2 - 0.6;
    let d = rng.next_f32() * 1.2 - 0.6;

    for i in 0..points {
        let r = rng.next_f32();
        let (tx, ty) = if r < 0.34 {
            (x, y)
        } else if r < 0.67 {
            (1.2 * x - 0.5 * y, 0.5 * x + 1.1 * y)
        } else {
            (0.9 * x.sin() + 0.35, 0.9 * y.cos() + 0.35)
        };
        x = a * tx + b * ty;
        y = c * tx * tx + d * ty;
        if i > 30 {
            let xr = (x + 2.0) / 4.0;
            let yr = (y + 2.0) / 4.0;
            if (0.0..=1.0).contains(&xr) && (0.0..=1.0).contains(&yr) {
                let ix = (xr * (sim_w as f32 - 1.0))
                    .round()
                    .clamp(0.0, sim_w as f32 - 1.0) as usize;
                let iy = (yr * (sim_h as f32 - 1.0))
                    .round()
                    .clamp(0.0, sim_h as f32 - 1.0) as usize;
                density[iy * sim_w as usize + ix] += 1.0;
            }
        }
        if !x.is_finite() || !y.is_finite() {
            x = 0.0;
            y = 0.0;
        }
    }

    normalize(&mut density);
    resize_nearest(&density, sim_w, sim_h, width, height)
}

fn render_strange_attractor(width: u32, height: u32, rng: &mut XorShift32, fast: bool) -> Vec<f32> {
    let sim_w = (width / 2).max(128);
    let sim_h = (height / 2).max(128);
    let points = if fast { 140_000 } else { 260_000 };
    let mut density = vec![0.0f32; (sim_w * sim_h) as usize];
    let mut x = rng.next_f32() * 0.2;
    let mut y = rng.next_f32() * 0.2;
    let variant = rng.next_u32() % 3;
    let mut min_x = f32::INFINITY;
    let mut max_x = -f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = -f32::INFINITY;

    for _ in 0..points {
        let (nx, ny) = if variant == 0 {
            let a = -1.4 + rng.next_f32() * 0.6;
            let b = 1.6 + rng.next_f32() * 0.6;
            let c = 0.7 + rng.next_f32() * 0.4;
            let d = 0.5 + rng.next_f32() * 0.5;
            (
                a * y + c * x.cos() * (a * x).sin(),
                b * x + d * (b * y).cos() + c,
            )
        } else if variant == 1 {
            let a = 1.7 + rng.next_f32() * 0.8;
            let b = 1.3 + rng.next_f32() * 0.6;
            let c = -0.8 + rng.next_f32() * 1.4;
            let d = -1.0 + rng.next_f32() * 1.4;
            (a - y * y + b * x, x + d * (c - y))
        } else {
            let a = 1.0 + rng.next_f32() * 2.0;
            let b = 2.0 + rng.next_f32() * 2.0;
            let c = -1.0 + rng.next_f32() * 2.0;
            (y - (x.signum() * (x.abs() + c).sqrt()), b * x - a * y - c)
        };

        x = nx;
        y = ny;
        if !x.is_finite() || !y.is_finite() {
            x = 0.0;
            y = 0.0;
        }
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
        let xr = (x - min_x) / (max_x - min_x).max(1e-6);
        let yr = (y - min_y) / (max_y - min_y).max(1e-6);
        let ix = (xr * (sim_w as f32 - 1.0))
            .round()
            .clamp(0.0, sim_w as f32 - 1.0) as usize;
        let iy = (yr * (sim_h as f32 - 1.0))
            .round()
            .clamp(0.0, sim_h as f32 - 1.0) as usize;
        density[iy * sim_w as usize + ix] += 1.0;
    }

    normalize(&mut density);
    let mut out = resize_bilinear(&density, sim_w, sim_h, width, height);
    if fast {
        // soften extreme spikes introduced by the attractor accumulation
        for value in out.iter_mut() {
            *value = value.powf(0.78);
        }
        normalize(&mut out);
    }
    out
}

fn render_attractor_hybrid(width: u32, height: u32, rng: &mut XorShift32, fast: bool) -> Vec<f32> {
    let sim_w = (width / 2).max(128);
    let sim_h = (height / 2).max(128);
    let points = if fast { 150_000 } else { 280_000 };
    let mut density = vec![0.0f32; (sim_w * sim_h) as usize];
    let mut x = rng.next_f32() * 0.4 - 0.2;
    let mut y = rng.next_f32() * 0.4 - 0.2;
    let a = -1.5 + rng.next_f32() * 1.6;
    let b = 1.0 + rng.next_f32() * 1.2;
    let c = -0.8 + rng.next_f32() * 1.6;
    let d = 0.5 + rng.next_f32() * 1.2;
    let phase = rng.next_f32() * TAU;
    let blend = 0.35 + rng.next_f32() * 0.45;
    let mut min_x = f32::INFINITY;
    let mut max_x = -f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = -f32::INFINITY;
    let seed = rng.next_u32();

    for i in 0..points {
        let step = i as f32 / points as f32;
        let m = ((step * 11.0 + phase).sin() + 1.0) * 0.5;
        let map_a = if m > blend {
            (
                1.2 * y + c * x.cos() + a * (b * x).sin(),
                b * x - d * (b * y).sin() + c * m,
            )
        } else {
            (a - y * y + b * x, x + d * (c - y))
        };
        let map_b = if m > 0.5 {
            (y - (x.signum() * (x.abs() + c).sqrt()), b * x - a * y - c)
        } else {
            (d - y * y - c * x.sin(), a * x + b * y)
        };
        let nx = map_a.0 * (1.0 - m) + map_b.0 * m;
        let ny = map_a.1 * (1.0 - m) + map_b.1 * m;
        x = clamp01(nx * 0.98 + 0.01 * (step * TAU).cos());
        y = clamp01(ny * 0.98 + 0.01 * (step * TAU).sin());

        if !x.is_finite() || !y.is_finite() {
            x = rng.next_f32() * 0.4 - 0.2;
            y = rng.next_f32() * 0.4 - 0.2;
        }
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);

        let xr = (x - min_x) / (max_x - min_x).max(1e-6);
        let yr = (y - min_y) / (max_y - min_y).max(1e-6);
        let ix = (xr * (sim_w as f32 - 1.0))
            .round()
            .clamp(0.0, sim_w as f32 - 1.0) as usize;
        let iy = (yr * (sim_h as f32 - 1.0))
            .round()
            .clamp(0.0, sim_h as f32 - 1.0) as usize;
        density[iy * sim_w as usize + ix] += 1.0;
        if i % 3 == 0 {
            let seed_f = (seed % 4_096) as f32;
            let noise = value_noise(x * 3.0 + seed_f * 0.25, y * 3.0 - seed_f * 0.25, seed ^ i);
            let jx = ((ix as f32 + (noise - 0.5) * 2.0).clamp(0.0, (sim_w - 1) as f32)) as usize;
            let jy = ((iy as f32 + (noise - 0.5) * 2.0).clamp(0.0, (sim_h - 1) as f32)) as usize;
            density[jy * sim_w as usize + jx] += 0.35;
        }
    }

    normalize(&mut density);
    let mut out = resize_bilinear(&density, sim_w, sim_h, width, height);
    for value in out.iter_mut() {
        let curve = if fast { 0.86 } else { 0.92 };
        *value = clamp01(value.powf(curve));
    }
    normalize(&mut out);
    out
}

#[allow(dead_code)]
fn delaunay_length(a: (i32, i32), b: (i32, i32)) -> f32 {
    let dx = (a.0 - b.0) as f32;
    let dy = (a.1 - b.1) as f32;
    (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_render_each_cpu_strategy() {
        let mut all = vec![
            CpuStrategy::EdgeSobel,
            CpuStrategy::EdgeLaplacian,
            CpuStrategy::Maze,
            CpuStrategy::ReactionDiffusion,
            CpuStrategy::LSystem,
            CpuStrategy::ProceduralNoise,
            CpuStrategy::CellularAutomata,
            CpuStrategy::ParticleFlow,
            CpuStrategy::Voronoi,
            CpuStrategy::Delaunay,
            CpuStrategy::IteratedFractal,
            CpuStrategy::StrangeAttractor,
            CpuStrategy::RadialWave,
            CpuStrategy::RecursiveFold,
            CpuStrategy::AttractorHybrid,
        ];
        for strategy in all.drain(..) {
            let img = render_cpu_strategy(strategy, 256, 256, 42, false);
            assert_eq!(img.len(), 256 * 256);
            assert!(img.iter().all(|value| value.is_finite()));
            let min = img.iter().cloned().fold(1.0f32, f32::min);
            let max = img.iter().cloned().fold(0.0f32, f32::max);
            assert!(max >= min);
        }
    }
}
