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
    /// Canny-like edge map from layered noise gradients.
    CannyEdge,
    /// Ridged, multi-octave noise composition.
    PerlinRidge,
    /// Plasma field generated from phase-shifted sine and cosine waves.
    PlasmaField,
    /// Sierpinski-style recursive point distribution.
    SierpinskiCarpet,
    /// Affine fern-like attractor growth.
    BarnsleyFern,
    /// Particle advection in a turbulent vector field.
    TurbulentFlow,
    /// Deep mandelbrot-style orbit trap render.
    MandelbrotField,
    /// Recursive tile subdivision with edge-dominant structure.
    RecursiveTiling,
    /// Multi-stage reaction-diffusion cascade with injected turbulence.
    TuringCascade,
    /// Dense flow field filaments with high curvature paths.
    FlowFilaments,
    /// Multi-orbit trajectory atlas with layered chaotic maps.
    OrbitalAtlas,
    /// Diffusion-limited crystal growth grown by random walkers.
    CrystalGrowth,
    /// Vortex-driven particle convection with noise-warped fields.
    VortexConvection,
    /// Erosion-like channel carving on a synthetic terrain field.
    ErosionChannels,
    /// Stochastic affine-IFS style orbit map with mixed nonlinear transforms.
    StochasticIFS,
    /// De Jong-style chaotic attractor traced through nonlinear maps.
    DeJongAttractor,
    /// Recursive ribbon traces with branching and angular modulation.
    RecursiveRibbon,
    /// Magnetic dipole-like field-line tracing with pseudo-physical dynamics.
    MagneticFieldlines,
    /// Lorenz-style chaotic attractor projected into 2D.
    LorenzAttractor,
    /// Recursive starburst growth with branching trajectories.
    RecursiveStarburst,
    /// Chaotic logistic-map evolution in 2D projected to the canvas.
    LogisticChaos,
    /// Interference-like wave superposition on a noisy phase field.
    InterferenceWaves,
    /// Clifford attractor family projected in 2D with folding nonlinearities.
    CliffordAttractor,
    /// Julia set escape-time rendering with randomized complex parameter.
    JuliaSet,
    /// Recursive Koch-like curve network with bifurcating segments.
    KochSnowflake,
    /// Branching transport process that recursively splits path segments.
    BifurcationTree,
    /// Multi-scale elevation-like relief with ridge-derived structure.
    DepthRelief,
    /// Chaotic attractor trajectory projected through a 3D tunnel warp.
    AttractorTunnel,
    /// Nested orbital rings with branching filament paths.
    OrbitalLabyrinth,
}

impl CpuStrategy {
    /// Total number of CPU strategies available.
    fn count() -> u32 {
        44
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
            14 => Self::AttractorHybrid,
            15 => Self::CannyEdge,
            16 => Self::PerlinRidge,
            17 => Self::PlasmaField,
            18 => Self::SierpinskiCarpet,
            19 => Self::BarnsleyFern,
            20 => Self::TurbulentFlow,
            21 => Self::MandelbrotField,
            22 => Self::RecursiveTiling,
            23 => Self::TuringCascade,
            24 => Self::FlowFilaments,
            25 => Self::OrbitalAtlas,
            26 => Self::CrystalGrowth,
            27 => Self::VortexConvection,
            28 => Self::ErosionChannels,
            29 => Self::StochasticIFS,
            30 => Self::DeJongAttractor,
            31 => Self::RecursiveRibbon,
            32 => Self::MagneticFieldlines,
            33 => Self::LorenzAttractor,
            34 => Self::RecursiveStarburst,
            35 => Self::LogisticChaos,
            36 => Self::InterferenceWaves,
            37 => Self::CliffordAttractor,
            38 => Self::JuliaSet,
            39 => Self::KochSnowflake,
            40 => Self::BifurcationTree,
            41 => Self::DepthRelief,
            42 => Self::AttractorTunnel,
            _ => Self::OrbitalLabyrinth,
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
            Self::DeJongAttractor => "de-jong-attractor",
            Self::RecursiveRibbon => "recursive-ribbon",
            Self::MagneticFieldlines => "magnetic-fieldlines",
            Self::RadialWave => "radial-wave",
            Self::RecursiveFold => "recursive-fold",
            Self::AttractorHybrid => "attractor-hybrid",
            Self::CannyEdge => "canny-edge",
            Self::PerlinRidge => "perlin-ridge",
            Self::PlasmaField => "plasma-field",
            Self::SierpinskiCarpet => "sierpinski-carpet",
            Self::BarnsleyFern => "barnsley-fern",
            Self::TurbulentFlow => "turbulent-flow",
            Self::MandelbrotField => "mandelbrot-field",
            Self::RecursiveTiling => "recursive-tiling",
            Self::TuringCascade => "turing-cascade",
            Self::FlowFilaments => "flow-filaments",
            Self::OrbitalAtlas => "orbital-atlas",
            Self::CrystalGrowth => "crystal-growth",
            Self::VortexConvection => "vortex-convection",
            Self::ErosionChannels => "erosion-channels",
            Self::StochasticIFS => "stochastic-ifs",
            Self::LorenzAttractor => "lorenz-attractor",
            Self::RecursiveStarburst => "recursive-starburst",
            Self::LogisticChaos => "logistic-chaos",
            Self::InterferenceWaves => "interference-waves",
            Self::CliffordAttractor => "clifford-attractor",
            Self::JuliaSet => "julia-set",
            Self::KochSnowflake => "koch-snowflake",
            Self::BifurcationTree => "bifurcation-tree",
            Self::DepthRelief => "depth-relief",
            Self::AttractorTunnel => "attractor-tunnel",
            Self::OrbitalLabyrinth => "orbital-labyrinth",
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
    let gpu_chance = if fast { 0.33 } else { 0.35 };

    if strategy_roll < gpu_chance {
        return RenderStrategy::Gpu(crate::ArtStyle::from_u32(rng.next_u32()).as_u32());
    }

    RenderStrategy::Cpu(CpuStrategy::from_u32(rng.next_u32()))
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
            CpuStrategy::CellularAutomata
            | CpuStrategy::ProceduralNoise
            | CpuStrategy::PerlinRidge => StrategyProfile {
                filter_bias: 0.44,
                gradient_bias: 0.38,
                force_detail: false,
            },
            CpuStrategy::AttractorHybrid
            | CpuStrategy::PlasmaField
            | CpuStrategy::SierpinskiCarpet
            | CpuStrategy::BarnsleyFern
            | CpuStrategy::TurbulentFlow
            | CpuStrategy::MandelbrotField
            | CpuStrategy::TuringCascade
            | CpuStrategy::FlowFilaments => StrategyProfile {
                filter_bias: 0.18,
                gradient_bias: 0.24,
                force_detail: true,
            },
            CpuStrategy::OrbitalAtlas
            | CpuStrategy::CrystalGrowth
            | CpuStrategy::RecursiveTiling => StrategyProfile {
                filter_bias: 0.24,
                gradient_bias: 0.18,
                force_detail: true,
            },
            CpuStrategy::VortexConvection
            | CpuStrategy::ErosionChannels
            | CpuStrategy::StochasticIFS
            | CpuStrategy::DeJongAttractor
            | CpuStrategy::RecursiveRibbon
            | CpuStrategy::MagneticFieldlines
            | CpuStrategy::LorenzAttractor
            | CpuStrategy::RecursiveStarburst
            | CpuStrategy::InterferenceWaves
            | CpuStrategy::CliffordAttractor
            | CpuStrategy::KochSnowflake
            | CpuStrategy::BifurcationTree
            | CpuStrategy::DepthRelief
            | CpuStrategy::AttractorTunnel
            | CpuStrategy::OrbitalLabyrinth
            | CpuStrategy::JuliaSet => StrategyProfile {
                filter_bias: 0.13,
                gradient_bias: 0.12,
                force_detail: true,
            },
            CpuStrategy::LogisticChaos => StrategyProfile {
                filter_bias: 0.18,
                gradient_bias: 0.20,
                force_detail: true,
            },
            CpuStrategy::CannyEdge => StrategyProfile {
                filter_bias: 0.42,
                gradient_bias: 0.26,
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
        CpuStrategy::CannyEdge => render_canny_edge(width, height, &mut rng, fast),
        CpuStrategy::PerlinRidge => render_perlin_ridge(width, height, &mut rng),
        CpuStrategy::PlasmaField => render_plasma_field(width, height, &mut rng),
        CpuStrategy::SierpinskiCarpet => render_sierpinski_carpet(width, height, &mut rng),
        CpuStrategy::BarnsleyFern => render_barnsley_fern(width, height, &mut rng),
        CpuStrategy::TurbulentFlow => render_turbulent_flow(width, height, &mut rng, fast),
        CpuStrategy::MandelbrotField => render_mandelbrot_field(width, height, &mut rng),
        CpuStrategy::RecursiveTiling => render_recursive_tiling(width, height, &mut rng),
        CpuStrategy::TuringCascade => render_turing_cascade(width, height, &mut rng),
        CpuStrategy::FlowFilaments => render_flow_filaments(width, height, &mut rng),
        CpuStrategy::OrbitalAtlas => render_orbital_atlas(width, height, &mut rng),
        CpuStrategy::CrystalGrowth => render_crystal_growth(width, height, &mut rng),
        CpuStrategy::VortexConvection => render_vortex_convection(width, height, &mut rng),
        CpuStrategy::ErosionChannels => render_erosion_channels(width, height, &mut rng),
        CpuStrategy::StochasticIFS => render_stochastic_ifs(width, height, &mut rng),
        CpuStrategy::LorenzAttractor => render_lorenz_attractor(width, height, &mut rng),
        CpuStrategy::RecursiveStarburst => render_recursive_starburst(width, height, &mut rng),
        CpuStrategy::LogisticChaos => render_logistic_chaos(width, height, &mut rng),
        CpuStrategy::DeJongAttractor => render_de_jong_attractor(width, height, &mut rng),
        CpuStrategy::RecursiveRibbon => render_recursive_ribbon(width, height, &mut rng),
        CpuStrategy::MagneticFieldlines => render_magnetic_fieldlines(width, height, &mut rng),
        CpuStrategy::InterferenceWaves => render_interference_waves(width, height, &mut rng),
        CpuStrategy::CliffordAttractor => render_clifford_attractor(width, height, &mut rng),
        CpuStrategy::JuliaSet => render_julia_set(width, height, &mut rng),
        CpuStrategy::KochSnowflake => render_koch_snowflake(width, height, &mut rng),
        CpuStrategy::BifurcationTree => render_bifurcation_tree(width, height, &mut rng),
        CpuStrategy::DepthRelief => render_depth_relief(width, height, &mut rng),
        CpuStrategy::AttractorTunnel => render_attractor_tunnel(width, height, &mut rng),
        CpuStrategy::OrbitalLabyrinth => render_orbital_labyrinth(width, height, &mut rng),
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

fn render_canny_edge(width: u32, height: u32, rng: &mut XorShift32, fast: bool) -> Vec<f32> {
    let base = noise_field(width, height, rng, if fast { 4 } else { 6 });
    let mut edge = vec![0.0f32; base.len()];
    let low = 0.22 + rng.next_f32() * 0.18;
    let high = (low + 0.24 + rng.next_f32() * 0.25).min(0.98);

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

            let gx = -ne - 2.0 * e - se + nw + 2.0 * wv + sw;
            let gy = -nw - 2.0 * n - ne + sw + 2.0 * s + se;
            let mag = (gx.abs() + gy.abs()) * 0.35;

            let is_edge = if mag >= high {
                1.0
            } else if mag <= low {
                0.0
            } else {
                let mut has_strong = false;
                let checks = [
                    sample_nearest(&base, width, height, x - 1, y - 1),
                    sample_nearest(&base, width, height, x, y - 1),
                    sample_nearest(&base, width, height, x + 1, y - 1),
                    sample_nearest(&base, width, height, x - 1, y),
                    sample_nearest(&base, width, height, x + 1, y),
                    sample_nearest(&base, width, height, x - 1, y + 1),
                    sample_nearest(&base, width, height, x, y + 1),
                    sample_nearest(&base, width, height, x + 1, y + 1),
                ];
                for sample in checks {
                    if (sample - c).abs() > high {
                        has_strong = true;
                        break;
                    }
                }
                if has_strong { 0.72 } else { 0.0 }
            };
            edge[(y as usize * width as usize) + x as usize] = is_edge;
        }
    }

    normalize(&mut edge);
    edge
}

fn render_perlin_ridge(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let width_f = width.max(1) as f32;
    let height_f = height.max(1) as f32;
    let seed = rng.next_u32();
    let octaves = 4 + (rng.next_u32() % 3) as u32;
    let mut out = vec![0.0f32; (width * height) as usize];
    let mut level = 0u32;
    while level < octaves {
        let freq = (1.1 + rng.next_f32()) * (1.85f32).powi(level as i32);
        let amp = 0.58f32.powi(level as i32);
        for y in 0..height {
            for x in 0..width {
                let u = x as f32 / width_f;
                let v = y as f32 / height_f;
                let n = value_noise(
                    u * 6.2 * freq + (seed as f32 * 0.0001),
                    v * 6.2 * freq + (seed as f32 * 0.0002),
                    seed + level,
                );
                let ridge = 1.0 - ((n * 2.0 - 1.0).abs());
                let idx = (y * width + x) as usize;
                out[idx] += amp * ridge.max(0.0);
            }
        }
        level += 1;
    }
    normalize(&mut out);
    out
}

fn render_plasma_field(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let width_f = width.max(1) as f32;
    let height_f = height.max(1) as f32;
    let bands = 3 + (rng.next_u32() % 4) as i32;
    let base_angle = rng.next_f32() * TAU;
    let mut out = vec![0.0f32; (width * height) as usize];

    for y in 0..height {
        let v = y as f32 / height_f - 0.5;
        for x in 0..width {
            let u = x as f32 / width_f - 0.5;
            let mut value = 0.0f32;
            let mut level = 0;
            let mut amp = 1.0f32;
            let mut norm = 0.0f32;
            while level < bands {
                let freq = 1.3 + level as f32 * 1.7;
                let phase = base_angle + 0.27 * level as f32;
                let s1 = (u * freq * 9.0 + phase).sin();
                let s2 = (v * freq * 7.0 + phase * 1.2).cos();
                let noise = value_noise(
                    u * freq * 4.2 + phase,
                    v * freq * 3.8 + phase * 0.8,
                    rng.next_u32(),
                );
                value += (s1 + s2 * 0.65 + noise) * amp;
                norm += amp;
                amp *= 0.46;
                level += 1;
            }
            out[(y * width + x) as usize] = (value / norm.max(1e-6) + 1.2) * 0.5;
        }
    }
    normalize(&mut out);
    for value in out.iter_mut() {
        *value = (*value * (0.95 + rng.next_f32() * 0.12)).clamp(0.0, 1.0);
    }
    normalize(&mut out);
    out
}

fn render_sierpinski_carpet(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim = (width.min(height)).max(1);
    let mut density = vec![0.0f32; (sim * sim) as usize];
    let iterations = if width * height > 600_000 {
        260_000
    } else {
        170_000
    };
    let skew = 0.5 + (rng.next_f32() * 0.1);
    let mut x = 0.5f32;
    let mut y = 0.5f32;

    let mut i = 0u32;
    while i < iterations {
        let noise_x = (value_noise(x * 0.5, y * 0.5, i) - 0.5) * 0.01;
        let noise_y = (value_noise(x * 0.61, y * 0.43, i ^ 0x9f1f_1234) - 0.5) * 0.01;

        match rng.next_u32() % 3 {
            0 => {
                x = (x + noise_x) * 0.5;
                y = (y + noise_y) * 0.5;
            }
            1 => {
                x = 0.5 + (x + noise_x) * 0.5;
                y = (y + noise_y) * 0.5;
            }
            _ => {
                x = 0.5 + (x + noise_x) * 0.5 - skew * 0.02;
                y = 0.5 + (y + noise_y) * 0.5 - skew * 0.02;
            }
        }

        if i > 120 {
            let ix = (x * (sim as f32 - 1.0))
                .round()
                .clamp(0.0, sim as f32 - 1.0) as usize;
            let iy = (y * (sim as f32 - 1.0))
                .round()
                .clamp(0.0, sim as f32 - 1.0) as usize;
            draw_point(
                ix as i32,
                iy as i32,
                sim as usize,
                sim as usize,
                0,
                0.85,
                &mut density,
            );
            if i % 4 == 0 {
                let ox = (ix as i32 + if rng.next_f32() < 0.5 { -1 } else { 1 })
                    .clamp(0, sim as i32 - 1);
                let oy = (iy as i32 + if rng.next_f32() < 0.5 { -1 } else { 1 })
                    .clamp(0, sim as i32 - 1);
                draw_point(ox, oy, sim as usize, sim as usize, 0, 0.34, &mut density);
            }
        }
        i += 1;
    }

    let mut out = resize_bilinear(&density, sim, sim, width, height);
    for value in out.iter_mut() {
        *value = (*value * (1.1 + rng.next_f32() * 0.2)).clamp(0.0, 1.0);
    }
    normalize(&mut out);
    out
}

fn render_barnsley_fern(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim = (width.min(height)).max(1);
    let mut density = vec![0.0f32; (sim * sim) as usize];
    let points = if width * height > 600_000 {
        300_000
    } else {
        180_000
    };
    let mut x = 0.0f32;
    let mut y = 0.0f32;

    let mut i = 0u32;
    while i < points {
        let roll = rng.next_f32();
        let (nx, ny) = if roll < 0.01 {
            (0.0, 0.16 * y)
        } else if roll < 0.08 {
            (0.20 * x - 0.05, 0.26 * y + 0.23)
        } else if roll < 0.15 {
            (-0.15 * x + 0.28 * y + 0.42, 0.26 * x + 0.24 * y + 0.44)
        } else {
            (0.85 * x + 0.04 * y, -0.04 * x + 0.85 * y + 1.6)
        };
        x = nx;
        y = ny;
        if i > 80 {
            let xr = (x + 2.5) / 6.0;
            let yr = (8.0 - y) / 11.0;
            let ix = (xr * (sim as f32 - 1.0))
                .round()
                .clamp(0.0, sim as f32 - 1.0) as usize;
            let iy = (yr * (sim as f32 - 1.0))
                .round()
                .clamp(0.0, sim as f32 - 1.0) as usize;
            draw_point(
                ix as i32,
                iy as i32,
                sim as usize,
                sim as usize,
                1,
                0.95,
                &mut density,
            );
        }
        i += 1;
    }

    let mut out = resize_bilinear(&density, sim, sim, width, height);
    normalize(&mut out);
    out
}

fn render_turbulent_flow(width: u32, height: u32, rng: &mut XorShift32, fast: bool) -> Vec<f32> {
    let sim = (width / 2).max(160);
    let mut density = vec![0.0f32; (sim * sim) as usize];
    let particles = if fast { 1_800 } else { 3_200 };
    let steps = if fast { 140 } else { 260 };
    let scale = 0.003 + rng.next_f32() * 0.004;
    let speed = 0.35 + rng.next_f32() * 1.05;
    let seed = rng.next_u32();

    for p in 0..particles {
        let mut x = (rng.next_u32() as f32 / (u32::MAX as f32)) * (sim as f32 - 1.0);
        let mut y = (rng.next_u32() as f32 / (u32::MAX as f32)) * (sim as f32 - 1.0);
        let mut step = 0u32;
        while step < steps {
            let fx = value_noise(x * scale, y * scale, seed + p) * 2.0 - 1.0;
            let fy =
                value_noise(y * scale * 1.2 + 11.0, x * scale * 1.3, seed + p + 21) * 2.0 - 1.0;
            let swirl =
                (value_noise(x * 0.1 + step as f32 * 0.01, y * 0.1, seed + step) - 0.5) * 0.4;
            let vx = fx * speed * (1.0 + swirl);
            let vy = fy * speed * (1.0 - swirl);
            x = (x + vx - vy * 0.08).rem_euclid(sim as f32 - 1.0);
            y = (y + vy + vx * 0.08).rem_euclid(sim as f32 - 1.0);
            if step > 10 {
                draw_point(
                    x as i32,
                    y as i32,
                    sim as usize,
                    sim as usize,
                    1,
                    0.86,
                    &mut density,
                );
            }
            step += 1;
        }
    }

    let mut out = resize_nearest(&density, sim, sim, width, height);
    for value in out.iter_mut() {
        *value = value.powf(0.97);
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

fn render_mandelbrot_field(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim_w = (width / 2).max(144);
    let sim_h = (height / 2).max(144);
    let mut out = vec![0.0f32; (sim_w * sim_h) as usize];
    let cx = (rng.next_f32() - 0.5) * 0.8;
    let cy = (rng.next_f32() - 0.5) * 0.8;
    let zoom = 0.65 + rng.next_f32() * 0.9;
    let max_iter = 180 + (rng.next_u32() % 260);
    let mut y = 0u32;
    while y < sim_h {
        let mut x = 0u32;
        while x < sim_w {
            let cr = ((x as f32 / sim_w as f32) - 0.5) * 2.9 * zoom + cx;
            let ci = ((y as f32 / sim_h as f32) - 0.5) * 2.9 * zoom + cy;
            let mut zr = 0.0f32;
            let mut zi = 0.0f32;
            let mut i = 0u32;
            let mut orbit = 0.0f32;
            let mut mag2 = 0.0f32;

            while i < max_iter {
                let zr2 = zr * zr - zi * zi + cr;
                let zi2 = 2.0 * zr * zi + ci;
                zr = zr2;
                zi = zi2;
                mag2 = zr2 * zr2 + zi2 * zi2;
                orbit += (mag2.sqrt() * 0.22) * (1.0 - (i as f32 / max_iter as f32));
                if mag2 > 256.0 {
                    break;
                }
                i += 1;
            }

            let idx = (y * sim_w + x) as usize;
            if mag2 <= 256.0 {
                out[idx] = 0.02;
            } else {
                let smooth = (i as f32 + 1.0 - mag2.max(1.0).log2()).max(1.0);
                let value = 1.0 - (smooth / max_iter as f32);
                out[idx] =
                    (value * 0.78 + (orbit / (max_iter as f32 + 1.0) * 0.22)).clamp(0.0, 1.0);
            }
            x += 1;
        }
        y += 1;
    }

    normalize(&mut out);
    for value in out.iter_mut() {
        *value = clamp01(value.powf(0.81 + rng.next_f32() * 0.22));
    }
    normalize(&mut out);
    resize_bilinear(&out, sim_w, sim_h, width, height)
}

fn render_recursive_tiling(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim = width.min(height).max(176);
    let mut canvas = vec![0.0f32; (sim * sim) as usize];
    let max_depth = 3 + (rng.next_u32() % 3);
    let mut queue = vec![(0i32, 0i32, sim as i32, sim as i32, 0u32)];
    let mut regions = Vec::with_capacity(72);

    while let Some((x, y, w, h, depth)) = queue.pop() {
        let can_split = w > 40 && h > 40 && depth < max_depth && rng.next_f32() < 0.84;
        if !can_split {
            regions.push((x, y, w, h, depth));
            continue;
        }

        if rng.next_f32() < 0.54 {
            let min = (w as f32 * 0.32) as i32;
            let max = (w as f32 * 0.68) as i32;
            let cut = min.max(2) + (rng.next_u32() as i32 % (max.max(min + 3) - min).max(1));
            queue.push((x + cut, y, w - cut, h, depth + 1));
            queue.push((x, y, cut, h, depth + 1));
        } else {
            let min = (h as f32 * 0.33) as i32;
            let max = (h as f32 * 0.67) as i32;
            let cut = min.max(2) + (rng.next_u32() as i32 % (max.max(min + 3) - min).max(1));
            queue.push((x, y + cut, w, h - cut, depth + 1));
            queue.push((x, y, w, cut, depth + 1));
        }
    }

    for &(x, y, w, h, depth) in regions.iter() {
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + w).min(sim as i32);
        let y1 = (y + h).min(sim as i32);
        if x1 <= x0 || y1 <= y0 {
            continue;
        }
        let edge = 0.36 + (depth as f32 * 0.09) + (rng.next_f32() * 0.2);
        let x0u = x0;
        let y0u = y0;
        let x1u = x1 - 1;
        let y1u = y1 - 1;

        draw_line(
            x0u,
            y0u,
            x1u,
            y0u,
            sim as usize,
            sim as usize,
            edge * 0.85,
            &mut canvas,
        );
        draw_line(
            x1u,
            y0u,
            x1u,
            y1u,
            sim as usize,
            sim as usize,
            edge * 0.85,
            &mut canvas,
        );
        draw_line(
            x1u,
            y1u,
            x0u,
            y1u,
            sim as usize,
            sim as usize,
            edge,
            &mut canvas,
        );
        draw_line(
            x0u,
            y1u,
            x0u,
            y0u,
            sim as usize,
            sim as usize,
            edge,
            &mut canvas,
        );

        if rng.next_f32() < 0.66 {
            let mx = ((x0 + x1) / 2).clamp(0, sim as i32 - 1);
            let my = ((y0 + y1) / 2).clamp(0, sim as i32 - 1);
            draw_line(
                mx,
                y0,
                mx,
                y1 - 1,
                sim as usize,
                sim as usize,
                edge * 0.55,
                &mut canvas,
            );
            if rng.next_f32() < 0.5 {
                draw_line(
                    x0,
                    my,
                    x1 - 1,
                    my,
                    sim as usize,
                    sim as usize,
                    edge * 0.45,
                    &mut canvas,
                );
            }
        }
    }

    normalize(&mut canvas);
    let mut out = resize_bilinear(&canvas, sim, sim, width, height);
    let grain = noise_field(sim, sim, rng, 2);
    let mut i = 0usize;
    while i < out.len() {
        out[i] = clamp01(0.83 * out[i] + 0.17 * grain[i]);
        i += 1;
    }
    normalize(&mut out);
    out
}

fn render_turing_cascade(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim_w = (width / 3).max(112);
    let sim_h = (height / 3).max(112);
    let size = (sim_w * sim_h) as usize;
    let mut u = vec![1.0f32; size];
    let mut v = vec![0.0f32; size];
    let mut u_next = vec![0.0f32; size];
    let mut v_next = vec![0.0f32; size];
    let mut seed_rng = XorShift32::new(rng.next_u32());
    for value in v.iter_mut() {
        *value = if seed_rng.next_f32() < 0.05 { 1.0 } else { 0.0 };
    }

    let phases = 2 + (rng.next_u32() % 2);
    let mut phase = 0u32;
    while phase < phases {
        let iterations = 78 + (phase * 14);
        let du = 0.14 + rng.next_f32() * 0.07;
        let dv = 0.06 + rng.next_f32() * 0.06;
        let f = 0.024 + rng.next_f32() * 0.028;
        let k = 0.045 + rng.next_f32() * 0.028;
        let field_scale = 0.0018 + rng.next_f32() * 0.0025;
        let phase_seed = seed_rng.next_u32();
        let mut step = 0u32;
        while step < iterations {
            let mut y = 0u32;
            while y < sim_h {
                let y_prev = if y == 0 { sim_h - 1 } else { y - 1 };
                let y_next = if y + 1 >= sim_h { 0 } else { y + 1 };
                let mut x = 0u32;
                while x < sim_w {
                    let x_prev = if x == 0 { sim_w - 1 } else { x - 1 };
                    let x_next = if x + 1 >= sim_w { 0 } else { x + 1 };
                    let idx = (y * sim_w + x) as usize;
                    let lap_u = u[(y * sim_w + x_prev) as usize]
                        + u[(y * sim_w + x_next) as usize]
                        + u[(y_prev * sim_w + x) as usize]
                        + u[(y_next * sim_w + x) as usize]
                        - 4.0 * u[idx];
                    let lap_v = v[(y * sim_w + x_prev) as usize]
                        + v[(y * sim_w + x_next) as usize]
                        + v[(y_prev * sim_w + x) as usize]
                        + v[(y_next * sim_w + x) as usize]
                        - 4.0 * v[idx];
                    let uvv = u[idx] * v[idx] * v[idx];
                    u_next[idx] = clamp01(u[idx] + du * lap_u - uvv + f * (1.0 - u[idx]));
                    v_next[idx] = clamp01(v[idx] + dv * lap_v + uvv - (f + k) * v[idx]);
                    x += 1;
                }
                y += 1;
            }
            std::mem::swap(&mut u, &mut u_next);
            std::mem::swap(&mut v, &mut v_next);
            if step % 3 == 0 {
                let mut i = 0usize;
                while i < size {
                    let fx = i as f32 / sim_w as f32;
                    let fy = (i / sim_w as usize) as f32;
                    let wave = value_noise(
                        fx * field_scale * 600.0,
                        fy * field_scale * 600.0,
                        phase_seed ^ step,
                    );
                    v[i] = clamp01(v[i] + (wave - 0.5) * 0.02);
                    u[i] = clamp01(u[i] - (wave - 0.5) * 0.01);
                    i += 1;
                }
            }
            step += 1;
        }
        phase += 1;
    }

    let mut out = resize_bilinear(&v, sim_w, sim_h, width, height);
    for value in out.iter_mut() {
        *value = clamp01(value.powf(0.78));
    }
    normalize(&mut out);
    out
}

fn render_flow_filaments(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim_w = (width / 2).max(180);
    let sim_h = (height / 2).max(180);
    let mut density = vec![0.0f32; (sim_w * sim_h) as usize];
    let strands = ((sim_w * sim_h) / 190).max(850);
    let steps = 180 + (rng.next_u32() % 180);
    let base_scale = 0.0017 + rng.next_f32() * 0.0022;
    let speed = 0.42 + rng.next_f32() * 1.2;
    let swirl = 1.2 + rng.next_f32() * 2.4;
    let mut i = 0u32;
    while i < strands {
        let mut x = rng.next_f32() * (sim_w as f32 - 1.0);
        let mut y = rng.next_f32() * (sim_h as f32 - 1.0);
        let mut angle = rng.next_f32() * TAU;
        let seed = rng.next_u32();
        let mut step = 0u32;
        while step < steps {
            let fx = value_noise(x * base_scale * swirl, y * base_scale, seed ^ step);
            let fy = value_noise(
                y * base_scale * 0.9,
                x * base_scale * 1.1,
                seed ^ (step + 23),
            );
            let target = (fx * 2.0 - 1.0).atan2(fy * 2.0 - 1.0);
            let mut delta = (target - angle + TAU * 0.5).rem_euclid(TAU) - TAU * 0.5;
            delta *= 0.22 + 0.32 * (rng.next_f32() - 0.5).abs();
            angle += delta;
            let nx = (x + angle.cos() * speed + (fx - 0.5) * 2.2).rem_euclid(sim_w as f32 - 1.0);
            let ny = (y + angle.sin() * speed + (fy - 0.5) * 2.2).rem_euclid(sim_h as f32 - 1.0);
            draw_line(
                x.round() as i32,
                y.round() as i32,
                nx.round() as i32,
                ny.round() as i32,
                sim_w as usize,
                sim_h as usize,
                0.9 + if step % 3 == 0 { 0.2 } else { 0.0 },
                &mut density,
            );
            if step % 10 == 0 {
                let glow_x = ((x + (fx - 0.5) * 7.0).round() as i32).clamp(0, sim_w as i32 - 1);
                let glow_y = ((y + (fy - 0.5) * 7.0).round() as i32).clamp(0, sim_h as i32 - 1);
                draw_point(
                    glow_x,
                    glow_y,
                    sim_w as usize,
                    sim_h as usize,
                    2,
                    0.4,
                    &mut density,
                );
            }
            x = nx;
            y = ny;
            step += 1;
        }
        i += 1;
    }

    normalize(&mut density);
    let mut out = resize_bilinear(&density, sim_w, sim_h, width, height);
    for value in out.iter_mut() {
        *value = clamp01(value.powf(0.86) * 1.06);
    }
    normalize(&mut out);
    out
}

fn render_orbital_atlas(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim_w = (width / 2).max(168);
    let sim_h = (height / 2).max(168);
    let mut density = vec![0.0f32; (sim_w * sim_h) as usize];
    let layers = 2 + (rng.next_u32() % 3);
    let mut phase = 0u32;
    let mut base = 0.75f32 + rng.next_f32() * 0.7;
    let jitter = 0.34f32 + rng.next_f32() * 0.42;
    let scale = 0.0019 + rng.next_f32() * 0.0046;

    while phase < layers {
        let steps = 60_000 + (sim_w * sim_h / 8) + (phase * 8_000);
        let mut x = 0.23 + (rng.next_f32() * 0.54);
        let mut y = 0.23 + (rng.next_f32() * 0.54);
        let k1 = 0.9 + rng.next_f32() * 1.1;
        let k2 = -1.6 + rng.next_f32() * 2.9;
        let k3 = 0.15 + rng.next_f32() * 0.9;
        let k4 = 1.1 + rng.next_f32() * 1.2;
        let k5 = rng.next_f32();
        let seed = rng.next_u32();

        let mut i = 0u32;
        while i < steps {
            let n1 = value_noise(x * 3.3 + phase as f32 * 0.11, y * 2.8, seed + i) - 0.5;
            let n2 = value_noise(
                y * 2.7 - phase as f32 * 0.13,
                x * 3.1 + i as f32,
                seed + i + 23,
            );
            let n2 = n2 - 0.5;
            let theta = (n2 * 2.2 + y.atan2(x) + (phase as f32 + 1.0) * 0.18).sin();
            let r = (x * x + y * y).sqrt() + 0.0003;
            let mix = (n1 * 2.0 + jitter * 0.2).sin();
            let nx = (1.2 * x - k2 * y * y + k3 * mix * theta).tanh();
            let ny = (k4 * y + k1 * x * x.sin() + k5 * mix - k2 * x * n1).tanh();

            let vx = k1 * theta + n1 * base;
            let vy = k2 * (theta * 1.2) + n2 * base;
            x = (nx + vx * 0.07 + k3 * r * 0.5 * base).clamp(-1.0, 1.0);
            y = (ny + vy * 0.07 + k4 * r * 0.5 * base).clamp(-1.0, 1.0);

            let sx = ((x + 1.0) * 0.5 * (sim_w as f32 - 1.0)).round() as i32;
            let sy = ((y + 1.0) * 0.5 * (sim_h as f32 - 1.0)).round() as i32;
            let s = (sim_w as i32 - 1).max(1);
            let t = (sim_h as i32 - 1).max(1);
            let ix = sx.rem_euclid(s) as usize;
            let iy = sy.rem_euclid(t) as usize;
            let idx = iy * sim_w as usize + ix;
            density[idx] += 0.85 / (1.0 + phase as f32);

            let pulse = value_noise(x * 12.0 + i as f32 * 0.003, y * 11.0, seed ^ i);
            if pulse > 0.74 {
                let ox =
                    (ix as i32 + ((pulse - 0.5) * 9.0).round() as i32).clamp(0, sim_w as i32 - 1);
                let oy = (iy as i32 + (((1.0 - pulse) - 0.5) * 9.0).round() as i32)
                    .clamp(0, sim_h as i32 - 1);
                draw_point(
                    ox,
                    oy,
                    sim_w as usize,
                    sim_h as usize,
                    1,
                    0.55,
                    &mut density,
                );
            }

            if i % 4 == 0 {
                draw_point(
                    ix as i32,
                    iy as i32,
                    sim_w as usize,
                    sim_h as usize,
                    0,
                    1.0,
                    &mut density,
                );
            }
            i += 1;
        }

        if phase > 0 {
            let detail = resize_nearest(&density, sim_w, sim_h, sim_w / 2, sim_h / 2);
            let blurred = resize_nearest(&detail, sim_w / 2, sim_h / 2, sim_w, sim_h);
            let mut j = 0usize;
            while j < density.len() {
                density[j] = clamp01(density[j] * 0.82 + blurred[j] * 0.18);
                j += 1;
            }
        }
        base *= 0.91 + rng.next_f32() * 0.05;
        phase += 1;
    }

    let mut out = resize_bilinear(&density, sim_w, sim_h, width, height);
    for value in out.iter_mut() {
        *value = clamp01(value.powf(0.76 + (scale % 0.3)));
    }
    normalize(&mut out);
    out
}

fn render_crystal_growth(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim_w = (width / 2).max(190);
    let sim_h = (height / 2).max(190);
    let size = (sim_w * sim_h) as usize;
    let mut density = vec![0.0f32; size];
    let mut occupied = vec![false; size];

    let center_x = (sim_w / 2) as i32;
    let center_y = (sim_h / 2) as i32;
    let center_idx = center_y as usize * sim_w as usize + center_x as usize;
    occupied[center_idx] = true;
    density[center_idx] = 1.0;

    let mut frontier = vec![(center_x, center_y)];
    let walkers = ((size as u32 / 60) + (rng.next_u32() % 900)) as usize;
    let max_steps = 260 + (rng.next_u32() % 260) as i32;
    let mut placed = 1usize;
    let mut attempts = 0u32;
    let mut i = 0u32;

    while placed < walkers && i < walkers as u32 * 5 {
        if frontier.is_empty() {
            break;
        }

        let pivot = frontier[(rng.next_u32() as usize) % frontier.len()];
        let mut wx = pivot.0 + 2 - (rng.next_u32() % 5) as i32;
        let mut wy = pivot.1 + 2 - (rng.next_u32() % 5) as i32;
        wx = wx.clamp(1, sim_w as i32 - 2);
        wy = wy.clamp(1, sim_h as i32 - 2);

        let mut step = 0i32;
        while step < max_steps {
            let choice = rng.next_u32() % 8;
            wx += match choice {
                0 => 1,
                1 => -1,
                2 | 3 => 0,
                _ => 0,
            };
            wy += match choice {
                2 => 1,
                3 => -1,
                4 | 5 => 0,
                _ => 0,
            };
            wx = wx.clamp(1, sim_w as i32 - 2);
            wy = wy.clamp(1, sim_h as i32 - 2);

            let idx = wy as usize * sim_w as usize + wx as usize;
            if occupied[idx] {
                wx = (rng.next_u32() % (sim_w - 2)) as i32 + 1;
                wy = (rng.next_u32() % (sim_h - 2)) as i32 + 1;
                step += 1;
                continue;
            }

            let has_neighbor = {
                let left = density[(wy as usize) * sim_w as usize + (wx as usize - 1)] > 0.0;
                let right = density[(wy as usize) * sim_w as usize + (wx as usize + 1)] > 0.0;
                let up = density[(wy as usize - 1) * sim_w as usize + (wx as usize)] > 0.0;
                let down = density[(wy as usize + 1) * sim_w as usize + (wx as usize)] > 0.0;
                left || right || up || down
            };

            if has_neighbor {
                let noise = value_noise(wx as f32 * 0.018, wy as f32 * 0.018, rng.next_u32());
                let glow = 0.8 + noise * 0.2;
                occupied[idx] = true;
                draw_point(
                    wx,
                    wy,
                    sim_w as usize,
                    sim_h as usize,
                    1,
                    glow,
                    &mut density,
                );
                frontier.push((wx, wy));
                placed += 1;
                break;
            }

            if rng.next_f32() < 0.03 {
                let drift_x = (noise_value(wx as f32, wy as f32, i) - 0.5) * 0.85;
                wx = (wx + drift_x as i32).clamp(1, sim_w as i32 - 2);
            }
            step += 1;
            attempts += 1;
        }

        if i % 2 == 0 {
            let sx = 1.0 - (attempts as f32 / ((walkers * 3) as f32)).min(0.8);
            let noise = noise_field(sim_w, sim_h, rng, 2);
            let mut p = 0usize;
            while p < density.len() {
                density[p] =
                    clamp01((density[p] * (0.9 + 0.1 * sx)) + (noise[p] * (1.0 - sx) * 0.3));
                p += 1;
            }
        }
        i += 1;
    }

    let mut out = resize_bilinear(&density, sim_w, sim_h, width, height);
    for value in out.iter_mut() {
        *value = clamp01(value.powf(0.82));
    }
    normalize(&mut out);
    out
}

fn render_vortex_convection(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim_w = (width / 2).max(176);
    let sim_h = (height / 2).max(176);
    let mut density = vec![0.0f32; (sim_w * sim_h) as usize];
    let vortex_count = 3 + (rng.next_u32() % 4);
    let mut vortices = Vec::with_capacity(vortex_count as usize);
    let mut i = 0u32;

    let diag = sim_w.max(sim_h) as f32;
    while i < vortex_count {
        let cx = rng.next_f32() * (sim_w as f32 - 1.0);
        let cy = rng.next_f32() * (sim_h as f32 - 1.0);
        let strength = 0.7 + rng.next_f32() * 2.0;
        let radius = diag * (0.18 + rng.next_f32() * 0.34);
        let rotation = (if rng.next_f32() < 0.5 { 1.0 } else { -1.0 }) * (0.75 + rng.next_f32());
        vortices.push((cx, cy, strength, radius * radius, rotation));
        i += 1;
    }

    let strands = (((sim_w * sim_h) as u32) / 56).max(1_600);
    let steps = 220 + (rng.next_u32() % 220);
    let base_seed = rng.next_u32();
    let noise_seed = rng.next_u32();

    let mut strand = 0u32;
    while strand < strands {
        let mut x = rng.next_f32() * (sim_w as f32 - 1.0);
        let mut y = rng.next_f32() * (sim_h as f32 - 1.0);
        let mut phase = rng.next_f32() * TAU;
        let mut step = 0u32;

        while step < steps {
            let mut vx = 0.0f32;
            let mut vy = 0.0f32;
            for &(cx, cy, strength, radius2, rotation) in vortices.iter() {
                let dx = x - cx;
                let dy = y - cy;
                let dist2 = dx * dx + dy * dy + 0.001;
                let falloff = (-dist2 / radius2.max(1.0)).exp();
                let tangent = dy.atan2(dx) + TAU * 0.25;
                vx += rotation * strength * tangent.cos() * falloff;
                vy += rotation * strength * tangent.sin() * falloff;
            }

            let n1 = value_noise(
                x * 0.006 + phase,
                y * 0.006 + (step as f32 * 0.003),
                base_seed ^ step,
            );
            let n2 = value_noise(y * 0.005 + 7.0, x * 0.005 - 11.0, noise_seed + strand);
            let n3 = value_noise(
                (x + y) * 0.004 + (strand as f32),
                (x - y) * 0.004 - (strand as f32),
                base_seed ^ (noise_seed + step),
            );

            vx = (vx + (n1 - 0.5) * 2.4 + (n3 - 0.5) * 0.9) * 1.3;
            vy = (vy + (n2 - 0.5) * 2.4 - (n3 - 0.5) * 0.9) * 1.3;
            let nx = (x + vx).rem_euclid(sim_w as f32 - 1.0);
            let ny = (y + vy).rem_euclid(sim_h as f32 - 1.0);

            draw_line(
                x.round() as i32,
                y.round() as i32,
                nx.round() as i32,
                ny.round() as i32,
                sim_w as usize,
                sim_h as usize,
                0.82 + ((n1 + n2) * 0.06),
                &mut density,
            );

            if step % 14 == 0 {
                let px = (nx + ((n3 - 0.5) * 2.6)).clamp(0.0, sim_w as f32 - 1.0) as i32;
                let py = (ny - ((n3 - 0.5) * 2.6)).clamp(0.0, sim_h as f32 - 1.0) as i32;
                draw_point(
                    px,
                    py,
                    sim_w as usize,
                    sim_h as usize,
                    1,
                    0.36,
                    &mut density,
                );
            }

            x = nx;
            y = ny;
            phase += 0.055 + (n1 - 0.5) * 0.06;
            step += 1;
        }
        strand += 1;
    }

    let warp = noise_field(sim_w, sim_h, rng, 2);
    let mut i = 0usize;
    while i < density.len() {
        density[i] = clamp01(density[i] * 0.94 + warp[i] * 0.06);
        i += 1;
    }

    normalize(&mut density);
    let mut out = resize_bilinear(&density, sim_w, sim_h, width, height);
    for value in out.iter_mut() {
        *value = clamp01(value.powf(0.87));
    }
    normalize(&mut out);
    out
}

fn render_erosion_channels(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim_w = (width / 3).max(224);
    let sim_h = (height / 3).max(224);
    let size = (sim_w * sim_h) as usize;
    let mut terrain = noise_field(sim_w, sim_h, rng, 6);
    let mut channels = vec![0.0f32; size];

    let drops = ((size as u32) / 35).max(1_600);
    let steps = 130 + (rng.next_u32() % 160);
    let mut drop = 0u32;

    while drop < drops {
        let mut x = if rng.next_f32() < 0.5 {
            if rng.next_u32() & 1 == 0 {
                0.0
            } else {
                sim_w as f32 - 1.0
            }
        } else {
            rng.next_f32() * (sim_w as f32 - 1.0)
        };
        let mut y = if rng.next_f32() < 0.5 {
            if rng.next_u32() & 1 == 0 {
                0.0
            } else {
                sim_h as f32 - 1.0
            }
        } else {
            rng.next_f32() * (sim_h as f32 - 1.0)
        };
        let mut speed = 1.0 + (rng.next_f32() * 0.9);
        let mut sediment = 0.0f32;
        let mut step = 0u32;

        while step < steps {
            let cx = x.round() as i32;
            let cy = y.round() as i32;
            let ix = cx.clamp(0, sim_w as i32 - 1) as usize;
            let iy = cy.clamp(0, sim_h as i32 - 1) as usize;
            let idx = iy * sim_w as usize + ix;
            let mut best_drop = 0.0f32;
            let mut nx = cx;
            let mut ny = cy;

            let mut oy = -1i32;
            while oy <= 1 {
                let mut ox = -1i32;
                while ox <= 1 {
                    if ox != 0 || oy != 0 {
                        let sx = (ix as i32 + ox).clamp(0, sim_w as i32 - 1);
                        let sy = (iy as i32 + oy).clamp(0, sim_h as i32 - 1);
                        let nidx = sy as usize * sim_w as usize + sx as usize;
                        let drop_delta = terrain[idx] - terrain[nidx];
                        let orth = if ox == 0 || oy == 0 { 1.0 } else { 0.74 };
                        let score = drop_delta * orth;
                        if score > best_drop {
                            best_drop = score;
                            nx = sx;
                            ny = sy;
                        }
                    }
                    ox += 1;
                }
                oy += 1;
            }

            if best_drop <= 0.0008 {
                terrain[idx] = clamp01(terrain[idx] + sediment * 0.16);
                channels[idx] = clamp01(channels[idx] + sediment * 0.12);
                break;
            }

            let transfer = ((speed * 0.06) + (best_drop * 2.2))
                .min(terrain[idx])
                .min(0.028);
            terrain[idx] = clamp01(terrain[idx] - transfer);
            let next_idx = ny as usize * sim_w as usize + nx as usize;
            terrain[next_idx] = clamp01(terrain[next_idx] + transfer * 0.38);
            channels[idx] = clamp01(channels[idx] + transfer * 1.9);
            channels[next_idx] = clamp01(channels[next_idx] + transfer * 0.8);

            sediment = (sediment + transfer * 2.1) * 0.96;
            let edge = 0.74
                + (transfer + (sediment * 0.4)).clamp(0.0, 0.2)
                + value_noise(
                    nx as f32 * 0.2 + steps as f32 * 0.001,
                    ny as f32 * 0.2 + drop as f32 * 0.001,
                    drop ^ step,
                ) * 0.07;
            draw_line(
                ix as i32,
                iy as i32,
                nx,
                ny,
                sim_w as usize,
                sim_h as usize,
                edge,
                &mut channels,
            );

            let drift = (value_noise(
                x * 0.008 + step as f32 * 0.004,
                y * 0.008 - step as f32 * 0.002,
                drop ^ 0x55AA_55AA,
            ) - 0.5)
                * 0.7;
            x = (nx as f32 + drift).clamp(0.0, sim_w as f32 - 1.0);
            y = (ny as f32 - drift).clamp(0.0, sim_h as f32 - 1.0);
            speed = (speed * 1.08 + transfer * 14.0).clamp(0.4, 2.5);
            step += 1;
        }

        drop += 1;
    }

    let noise = noise_field(sim_w, sim_h, rng, 2);
    let mut out = vec![0.0f32; size];
    let mut i = 0usize;
    while i < size {
        out[i] = clamp01(channels[i] * 1.02 + (1.0 - terrain[i]) * 0.37 + noise[i] * 0.05);
        i += 1;
    }

    normalize(&mut out);
    let mut resized = resize_bilinear(&out, sim_w, sim_h, width, height);
    for value in resized.iter_mut() {
        *value = clamp01(value.powf(0.84));
    }
    normalize(&mut resized);
    resized
}

#[derive(Clone, Copy)]
struct StochasticTransform {
    mode: u32,
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    e: f32,
    f: f32,
    scale: f32,
    twist: f32,
    weight: f32,
}

fn render_stochastic_ifs(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim_w = (width / 2).max(148);
    let sim_h = (height / 2).max(148);
    let size = (sim_w * sim_h) as usize;
    let mut density = vec![0.0f32; size];
    let transform_count = 5 + (rng.next_u32() % 5) as usize;
    let mut transforms = Vec::with_capacity(transform_count);
    let mut total_weight = 0.0f32;
    let mut i = 0usize;

    while i < transform_count {
        let mode = rng.next_u32() % 4;
        let a = 1.3 + rng.next_f32() * 1.2;
        let b = 0.8 + rng.next_f32() * 0.9;
        let c = 0.8 + rng.next_f32() * 0.9;
        let d = 1.3 + rng.next_f32() * 1.2;
        let e = (rng.next_f32() * 2.0) - 1.0;
        let f = (rng.next_f32() * 2.0) - 1.0;
        let scale = 0.55 + rng.next_f32() * 1.15;
        let twist = (rng.next_f32() * TAU) - (TAU * 0.5);
        let weight = 0.35 + rng.next_f32() * 0.8;
        transforms.push(StochasticTransform {
            mode,
            a,
            b,
            c,
            d,
            e,
            f,
            scale,
            twist,
            weight,
        });
        total_weight += weight;
        i += 1;
    }

    let points = 120_000 + ((sim_w * sim_h) / 2);
    let warmup = 240u32;
    let seed = rng.next_u32();
    let mut xs = Vec::with_capacity(points as usize);
    let mut ys = Vec::with_capacity(points as usize);
    let mut x = rng.next_f32() * 0.2 + -0.1;
    let mut y = rng.next_f32() * 0.2 + -0.1;
    let mut min_x = f32::INFINITY;
    let mut max_x = -f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = -f32::INFINITY;
    let mut i = 0u32;

    while i < points + warmup {
        let mut pick = rng.next_f32() * total_weight;
        let mut idx = 0usize;
        let mut selected = 0usize;
        while idx < transforms.len() {
            pick -= transforms[idx].weight;
            if pick <= 0.0 {
                selected = idx;
                break;
            }
            idx += 1;
        }
        let tr = transforms[selected];
        let (nx_raw, ny_raw) = match tr.mode {
            0 => {
                let nx = tr.a * x + tr.b * y + tr.e;
                let ny = tr.c * x + tr.d * y + tr.f;
                (nx * 0.45, ny * 0.45)
            }
            1 => {
                let r2 = (x * x + y * y).max(0.000_01);
                let nx = (tr.a * x - tr.b * y) / r2 + tr.e;
                let ny = (tr.c * y + tr.d * x) / (r2 * 1.1) + tr.f;
                (nx * 0.52, ny * 0.52)
            }
            2 => {
                let r = (x * x + y * y).max(0.000_01).sqrt();
                let ang = y.atan2(x) * tr.scale + tr.twist;
                let nx = (ang.cos() * r) * 0.56 + tr.e;
                let ny = (ang.sin() * r) * 0.56 + tr.f;
                (nx, ny)
            }
            _ => {
                let swirl = (tr.scale * (x * x + y * y).sqrt()).sin() * tr.twist.sin();
                let nx = tr.a * x + tr.b * swirl + tr.e;
                let ny = tr.c * y + tr.d * swirl + tr.f;
                (nx, ny)
            }
        };

        let n = value_noise(nx_raw * 0.7, ny_raw * 0.7, seed ^ i);
        x = nx_raw + (n - 0.5) * 0.16;
        y = ny_raw - (n - 0.5) * 0.16;
        let scale = tr.scale * 0.92;
        x = x.clamp(-scale, scale);
        y = y.clamp(-scale, scale);

        if i >= warmup {
            xs.push(x);
            ys.push(y);
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
        i += 1;
    }

    let span_x = (max_x - min_x).max(0.000_01);
    let span_y = (max_y - min_y).max(0.000_01);
    let mut i = 0usize;
    let mut px = 0.0f32;
    let mut py = 0.0f32;
    while i < xs.len() {
        let ux = (xs[i] - min_x) / span_x;
        let uy = (ys[i] - min_y) / span_y;
        let px_i = (ux * (sim_w as f32 - 1.0)).clamp(0.0, sim_w as f32 - 1.0);
        let py_i = (uy * (sim_h as f32 - 1.0)).clamp(0.0, sim_h as f32 - 1.0);
        let x_i = px_i.round() as i32;
        let y_i = py_i.round() as i32;
        let value = 0.7 + (ux * 0.3);
        density[sim_w as usize * y_i.max(0) as usize + x_i.max(0) as usize] = clamp01(
            density[sim_w as usize * y_i.max(0) as usize + x_i.max(0) as usize] + value * 1.05,
        );

        if i > 0 {
            let nx = x_i;
            let ny = y_i;
            draw_line(
                px as i32,
                py as i32,
                nx,
                ny,
                sim_w as usize,
                sim_h as usize,
                0.84,
                &mut density,
            );
        }
        px = px_i;
        py = py_i;
        i += 1;
    }

    let mut out = resize_bilinear(&density, sim_w, sim_h, width, height);
    let grain = noise_field(width, height, rng, 2);
    let mut i = 0usize;
    while i < out.len() {
        out[i] = clamp01((out[i] * 0.9) + (grain[i] * 0.03));
        i += 1;
    }
    for value in out.iter_mut() {
        *value = clamp01(value.powf(0.84));
    }
    normalize(&mut out);
    out
}

#[derive(Clone, Copy)]
struct MagneticPole {
    x: f32,
    y: f32,
    polarity: f32,
}

fn render_de_jong_attractor(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim_w = (width / 2).max(180);
    let sim_h = (height / 2).max(180);
    let mut density = vec![0.0f32; (sim_w * sim_h) as usize];
    let points = 300_000 + ((sim_w * sim_h) / 2);
    let warmup = 180u32;

    let mut x = rng.next_f32() * 0.8 - 0.4;
    let mut y = rng.next_f32() * 0.8 - 0.4;
    let a = 1.5 + rng.next_f32() * 2.2;
    let b = 0.9 + rng.next_f32() * 2.2;
    let c = 0.7 + rng.next_f32() * 1.8;
    let d = 1.4 + rng.next_f32() * 2.4;
    let mix = 0.12 + rng.next_f32() * 0.55;
    let seed = rng.next_u32();

    let mut previous_x = x;
    let mut previous_y = y;
    let mut i = 0u32;
    while i < points + warmup {
        let nx = (a * y.sin() + b * y - c * x * x).sin() + (mix * (x * y)).sin();
        let ny = (c * x.cos() + d * x - a * y * y).cos() + (mix * (x * x)).cos();
        x = nx.rem_euclid(2.0) - 1.0;
        y = ny.rem_euclid(2.0) - 1.0;

        if !x.is_finite() || !y.is_finite() {
            x = 0.1 + (value_noise(i as f32, y, seed) - 0.5) * 0.4;
            y = 0.1 + (value_noise(x, i as f32, seed ^ 0x9e37_79b9) - 0.5) * 0.4;
            i += 1;
            continue;
        }

        if i >= warmup {
            let ux = ((x + 1.0) * 0.5 * (sim_w as f32 - 1.0)).clamp(0.0, sim_w as f32 - 1.0);
            let uy = ((y + 1.0) * 0.5 * (sim_h as f32 - 1.0)).clamp(0.0, sim_h as f32 - 1.0);
            let px = ux.round() as i32;
            let py = uy.round() as i32;

            let jitter = value_noise(ux * 0.11, uy * 0.15, seed ^ (i as u32)) * 0.36;
            draw_line(
                previous_x.round() as i32,
                previous_y.round() as i32,
                px,
                py,
                sim_w as usize,
                sim_h as usize,
                0.5 + jitter,
                &mut density,
            );
            if i % 3 == 0 {
                draw_point(
                    px,
                    py,
                    sim_w as usize,
                    sim_h as usize,
                    1,
                    0.8 + jitter,
                    &mut density,
                );
            }
            previous_x = ux;
            previous_y = uy;
        }

        i += 1;
    }

    let mut out = resize_bilinear(&density, sim_w, sim_h, width, height);
    for value in out.iter_mut() {
        *value = clamp01(*value * 1.08 + 0.03 * value.powf(1.25));
    }
    normalize(&mut out);
    out
}

#[derive(Clone, Copy)]
struct RibbonThread {
    x: f32,
    y: f32,
    angle: f32,
    length: f32,
    depth: u32,
    density: f32,
}

fn render_recursive_ribbon(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim_w = (width / 2).max(196);
    let sim_h = (height / 2).max(196);
    let mut out = vec![0.0f32; (sim_w * sim_h) as usize];
    let mut stack: Vec<RibbonThread> = Vec::with_capacity(768);
    let start_count = 3 + (rng.next_u32() % 8);
    let base_length = (sim_w.min(sim_h) as f32 * (0.22 + rng.next_f32() * 0.10)).max(20.0);
    let base_seed = rng.next_u32();

    let mut i = 0u32;
    while i < start_count {
        let radius = (sim_w.min(sim_h) as f32 * (0.33 + rng.next_f32() * 0.20)) * 0.5;
        let base_angle = rng.next_f32() * TAU + value_noise(i as f32, radius, base_seed) * 0.35;
        let sx = ((sim_w as f32 * 0.5) + base_angle.cos() * radius).clamp(0.0, sim_w as f32 - 1.0);
        let sy = ((sim_h as f32 * 0.5) + base_angle.sin() * radius).clamp(0.0, sim_h as f32 - 1.0);
        stack.push(RibbonThread {
            x: sx,
            y: sy,
            angle: base_angle + 0.5 * rng.next_f32(),
            length: base_length * (0.7 + rng.next_f32() * 0.45),
            depth: 5 + (rng.next_u32() % 4),
            density: 0.62 + rng.next_f32() * 0.35,
        });
        i += 1;
    }

    while let Some(thread) = stack.pop() {
        if thread.length < 2.5 || thread.depth == 0 {
            continue;
        }

        let mut x = thread.x;
        let mut y = thread.y;
        let mut angle = thread.angle;
        let steps = 6 + (rng.next_u32() % 7);
        let mut step = 0u32;
        while step < steps {
            let seg = thread.length / (steps as f32);
            let wave = value_noise(x * 0.008, y * 0.009, base_seed + step) - 0.5;
            let curvature = wave * 0.92;
            let modulation = (value_noise(x * 0.013, y * 0.011, base_seed ^ step) - 0.5) * 0.55;
            angle += curvature + modulation;
            let nx = x + angle.cos() * seg * (0.95 + 0.18 * wave);
            let ny = y + angle.sin() * seg * (1.02 - 0.12 * wave);
            let px = nx.clamp(0.0, sim_w as f32 - 1.0);
            let py = ny.clamp(0.0, sim_h as f32 - 1.0);
            let strength = thread.density * (0.56 + 0.2 * wave);
            draw_line(
                x.round() as i32,
                y.round() as i32,
                px.round() as i32,
                py.round() as i32,
                sim_w as usize,
                sim_h as usize,
                strength,
                &mut out,
            );

            if step % 2 == 0 {
                draw_point(
                    px.round() as i32,
                    py.round() as i32,
                    sim_w as usize,
                    sim_h as usize,
                    1,
                    strength * 1.05,
                    &mut out,
                );
            }

            if step == 2 && thread.depth > 1 && rng.next_f32() < 0.42 {
                stack.push(RibbonThread {
                    x: px,
                    y: py,
                    angle: angle + (rng.next_f32() * 1.2 - 0.6),
                    length: thread.length * (0.42 + rng.next_f32() * 0.21),
                    depth: thread.depth - 1,
                    density: (thread.density * 0.86).max(0.18),
                });
            } else if step == 4 && thread.depth > 2 && rng.next_f32() < 0.33 {
                stack.push(RibbonThread {
                    x: px,
                    y: py,
                    angle: angle + (rng.next_f32() * 1.2 - 0.6) - std::f32::consts::PI * 0.25,
                    length: thread.length * (0.35 + rng.next_f32() * 0.20),
                    depth: thread.depth - 1,
                    density: (thread.density * 0.74).max(0.18),
                });
            }

            x = px;
            y = py;
            step += 1;
        }
    }

    for value in out.iter_mut() {
        *value = (*value + (value.powf(1.05) * 0.16)).clamp(0.0, 1.0);
    }
    normalize(&mut out);
    resize_bilinear(&out, sim_w, sim_h, width, height)
}

fn render_magnetic_fieldlines(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim_w = (width / 2).max(180);
    let sim_h = (height / 2).max(180);
    let mut density = vec![0.0f32; (sim_w * sim_h) as usize];
    let pole_count = 4 + (rng.next_u32() % 6) as usize;
    let mut poles = Vec::with_capacity(pole_count);
    let mut i = 0usize;
    while i < pole_count {
        poles.push(MagneticPole {
            x: 0.2 + rng.next_f32() * 0.6,
            y: 0.2 + rng.next_f32() * 0.6,
            polarity: (if rng.next_f32() < 0.55 { 1.0 } else { -1.0 })
                * (0.7 + rng.next_f32() * 0.9),
        });
        i += 1;
    }

    let tracks = 4 + (sim_w * sim_h / 420);
    let track_steps = 320 + (rng.next_u32() % 260);
    let base_speed = 0.72 + rng.next_f32() * 0.65;
    let seed = rng.next_u32();

    let mut track = 0u32;
    while track < tracks {
        let mut x = rng.next_f32();
        let mut y = rng.next_f32();
        if rng.next_f32() < 0.35 {
            x = if rng.next_f32() < 0.5 { 0.0 } else { 1.0 };
            y = rng.next_f32();
        } else if rng.next_f32() < 0.5 {
            x = rng.next_f32();
            y = if rng.next_f32() < 0.5 { 0.0 } else { 1.0 };
        }

        let mut step = 0u32;
        while step < track_steps {
            let mut vx = 0.0f32;
            let mut vy = 0.0f32;
            let nx = x * (sim_w as f32 - 1.0);
            let ny = y * (sim_h as f32 - 1.0);
            let sx = nx / sim_w.max(1) as f32;
            let sy = ny / sim_h.max(1) as f32;

            let mut p = 0usize;
            while p < poles.len() {
                let pole = poles[p];
                let dx = sx - pole.x;
                let dy = sy - pole.y;
                let dist2 = dx * dx + dy * dy + 0.000_3;
                let inv = 1.0 / (dist2.sqrt() * dist2);
                let swirl = pole.polarity * inv;
                vx += -dy * swirl;
                vy += dx * swirl;
                p += 1;
            }

            let noise = value_noise(sx * 4.6 + seed as f32 * 0.000_4, sy * 4.2, seed ^ track) - 0.5;
            let angle = noise * TAU;
            vx += angle.cos() * 0.04;
            vy += angle.sin() * 0.04;
            let speed = base_speed + ((noise + 0.5) * 0.5);
            let mag = (vx * vx + vy * vy).sqrt().max(1e-6);
            let dx = (vx / mag) * speed;
            let dy = (vy / mag) * speed;

            let px = nx;
            let py = ny;
            let nx = (nx + dx).clamp(0.0, sim_w as f32 - 1.0);
            let ny = (ny + dy).clamp(0.0, sim_h as f32 - 1.0);
            draw_line(
                px as i32,
                py as i32,
                nx as i32,
                ny as i32,
                sim_w as usize,
                sim_h as usize,
                0.48 + (noise.abs() * 0.45),
                &mut density,
            );
            if step % 4 == 0 {
                draw_point(
                    nx as i32,
                    ny as i32,
                    sim_w as usize,
                    sim_h as usize,
                    1,
                    0.55 + noise.abs() * 0.28,
                    &mut density,
                );
            }
            x = nx / sim_w.max(1) as f32;
            y = ny / sim_h.max(1) as f32;
            step += 1;
        }
        track += 1;
    }

    for value in density.iter_mut() {
        *value = (*value * 0.95).powf(0.84);
    }
    normalize(&mut density);
    let mut out = resize_bilinear(&density, sim_w, sim_h, width, height);
    let grain = noise_field(width, height, rng, 1);
    let mut i = 0usize;
    while i < out.len() {
        out[i] = clamp01(out[i] + grain[i] * 0.04);
        i += 1;
    }
    normalize(&mut out);
    out
}

fn render_lorenz_attractor(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim_w = (width / 2).max(180);
    let sim_h = (height / 2).max(180);
    let size = (sim_w * sim_h) as usize;
    let mut density = vec![0.0f32; size];
    let mut trajectory_x = Vec::with_capacity(180_000);
    let mut trajectory_y = Vec::with_capacity(180_000);

    let points = 180_000 + ((sim_w * sim_h) / 2);
    let warmup = 220u32;

    let mut x = rng.next_f32() * 0.8 - 0.4;
    let mut y = rng.next_f32() * 0.8 - 0.4;
    let mut z = rng.next_f32() * 0.8 - 0.4;
    let sigma = 8.5 + rng.next_f32() * 4.7;
    let rho = 20.0 + rng.next_f32() * 45.0;
    let beta = 1.8 + rng.next_f32() * 0.9;
    let dt = 0.0025 + rng.next_f32() * 0.0035;
    let seed = rng.next_u32();

    let mut min_x = f32::INFINITY;
    let mut max_x = -f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = -f32::INFINITY;

    let mut i = 0u32;
    while i < points + warmup {
        let dx = sigma * (y - x);
        let dy = x * (rho - z) - y;
        let dz = x * y - beta * z;
        x = x + dx * dt;
        y = y + dy * dt;
        z = z + dz * dt;

        if !x.is_finite() || !y.is_finite() || !z.is_finite() {
            x = 0.1 + (value_noise(i as f32, z, seed) - 0.5) * 0.3;
            y = 0.1 + (value_noise(y, i as f32, seed ^ 0x1a2b_3c4d) - 0.5) * 0.3;
            z = 0.1 + (value_noise(z, y, seed ^ 0x2b3c_4d5e) - 0.5) * 0.3;
            i = i.saturating_add(1);
            continue;
        }

        if i >= warmup {
            trajectory_x.push(x);
            trajectory_y.push(y);
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }

        i += 1;
    }

    let span_x = (max_x - min_x).max(0.000_05);
    let span_y = (max_y - min_y).max(0.000_05);
    let mut i = 0usize;
    let mut prev_x = if !trajectory_x.is_empty() {
        ((trajectory_x[0] - min_x) / span_x * (sim_w as f32 - 1.0)).clamp(0.0, sim_w as f32 - 1.0)
    } else {
        0.0
    };
    let mut prev_y = if !trajectory_y.is_empty() {
        ((trajectory_y[0] - min_y) / span_y * (sim_h as f32 - 1.0)).clamp(0.0, sim_h as f32 - 1.0)
    } else {
        0.0
    };

    while i < trajectory_x.len() {
        let tx = (trajectory_x[i] - min_x) / span_x * (sim_w as f32 - 1.0);
        let ty = (trajectory_y[i] - min_y) / span_y * (sim_h as f32 - 1.0);
        let ux = tx.clamp(0.0, sim_w as f32 - 1.0);
        let uy = ty.clamp(0.0, sim_h as f32 - 1.0);
        let px = ux.round() as i32;
        let py = uy.round() as i32;
        let wave = value_noise(ux * 0.13, uy * 0.21, seed ^ (i as u32)) * 0.5;
        let strength = 0.45 + wave * 0.35;
        draw_line(
            prev_x.round() as i32,
            prev_y.round() as i32,
            px,
            py,
            sim_w as usize,
            sim_h as usize,
            strength,
            &mut density,
        );
        draw_point(
            px,
            py,
            sim_w as usize,
            sim_h as usize,
            1,
            strength * 0.9,
            &mut density,
        );
        prev_x = ux;
        prev_y = uy;
        i += 1;
    }

    normalize(&mut density);
    for value in density.iter_mut() {
        *value = (*value * 1.03).clamp(0.0, 1.0);
    }
    normalize(&mut density);
    resize_bilinear(&density, sim_w, sim_h, width, height)
}

#[derive(Clone, Copy)]
struct StarburstBranch {
    x: f32,
    y: f32,
    angle: f32,
    length: f32,
    depth: u32,
}

fn render_recursive_starburst(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim_w = (width / 2).max(200);
    let sim_h = (height / 2).max(200);
    let mut density = vec![0.0f32; (sim_w * sim_h) as usize];
    let root_count = 2 + (rng.next_u32() % 6);
    let max_depth = 5 + (rng.next_u32() % 4);
    let base_len = (sim_w.min(sim_h) as f32 * (0.10 + rng.next_f32() * 0.16)).max(8.0);
    let mut stack: Vec<StarburstBranch> = Vec::with_capacity(768);
    let mut i = 0u32;

    while i < root_count {
        let side = rng.next_u32() % 4;
        let (sx, sy, dir) = match side {
            0 => (
                (rng.next_f32() * sim_w as f32),
                0.0,
                rng.next_f32() * std::f32::consts::PI + std::f32::consts::FRAC_PI_2,
            ),
            1 => (
                sim_w as f32 - 1.0,
                rng.next_f32() * sim_h as f32,
                rng.next_f32() * std::f32::consts::PI + std::f32::consts::PI,
            ),
            2 => (
                rng.next_f32() * sim_w as f32,
                sim_h as f32 - 1.0,
                rng.next_f32() * std::f32::consts::PI - std::f32::consts::FRAC_PI_2,
            ),
            _ => (
                0.0,
                rng.next_f32() * sim_h as f32,
                rng.next_f32() * std::f32::consts::PI,
            ),
        };

        let jitter = (value_noise(sx, sy, rng.next_u32()) - 0.5) * 0.9;
        stack.push(StarburstBranch {
            x: sx,
            y: sy,
            angle: dir + jitter,
            length: base_len,
            depth: max_depth,
        });
        i += 1;
    }

    while let Some(branch) = stack.pop() {
        if branch.length < 3.0 || branch.depth == 0 {
            continue;
        }
        let mut x = branch.x;
        let mut y = branch.y;
        let segments = 4 + (rng.next_u32() % 6);
        let mut segment = 0u32;
        while segment < segments {
            let seg_len = branch.length * (0.45 + 0.25 * rng.next_f32()) / (segments as f32 * 0.85);
            let noise = value_noise(
                x * 0.007 + branch.angle,
                y * 0.009 - branch.angle,
                rng.next_u32(),
            ) - 0.5;
            let angle = branch.angle + (noise * 0.85);
            let nx = x + angle.cos() * seg_len;
            let ny = y + angle.sin() * seg_len;
            let strength = 0.45 + noise * 0.35;
            draw_line(
                x.round() as i32,
                y.round() as i32,
                nx.round() as i32,
                ny.round() as i32,
                sim_w as usize,
                sim_h as usize,
                strength * 0.82,
                &mut density,
            );
            if segment % 2 == 0 {
                draw_point(
                    nx.round() as i32,
                    ny.round() as i32,
                    sim_w as usize,
                    sim_h as usize,
                    1 + (segment % 3) as i32,
                    strength,
                    &mut density,
                );
            }

            let branch_remaining = branch.depth.saturating_sub(1);
            if segment >= 1 && rng.next_f32() < 0.54 && branch_remaining > 0 {
                let child_count = 1 + (rng.next_u32() % 2);
                let mut c = 0u32;
                while c < child_count {
                    let child_angle = angle + (rng.next_f32() * 1.9 - 0.95);
                    let child_len = branch.length * (0.44 + rng.next_f32() * 0.32);
                    let child_depth = branch_remaining;
                    stack.push(StarburstBranch {
                        x: nx,
                        y: ny,
                        angle: child_angle,
                        length: child_len,
                        depth: child_depth,
                    });
                    c += 1;
                }
            } else if segment % 4 == 0 && branch_remaining > 0 && rng.next_f32() < 0.35 {
                let child_angle = angle + (rng.next_f32() * 0.6 - 0.3) + std::f32::consts::PI * 0.5;
                stack.push(StarburstBranch {
                    x: nx,
                    y: ny,
                    angle: child_angle,
                    length: branch.length * (0.32 + rng.next_f32() * 0.25),
                    depth: branch_remaining,
                });
            }

            x = nx;
            y = ny;
            segment += 1;
        }
    }

    for value in density.iter_mut() {
        *value = (*value + (value.powf(1.07) * 0.15)).clamp(0.0, 1.0);
    }
    normalize(&mut density);
    resize_bilinear(&density, sim_w, sim_h, width, height)
}

fn render_logistic_chaos(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim_w = (width / 2).max(176);
    let sim_h = (height / 2).max(176);
    let mut density = vec![0.0f32; (sim_w * sim_h) as usize];
    let points = 320_000 + (sim_w * sim_h);
    let warmup = 500u32;
    let mut x = rng.next_f32() * 0.8 + 0.1;
    let mut y = rng.next_f32() * 0.8 + 0.1;
    let a = 3.56 + rng.next_f32() * 0.3;
    let b = 0.18 + rng.next_f32() * 0.45;
    let c = 0.22 + rng.next_f32() * 0.48;
    let seed = rng.next_u32();
    let mut prev_x = x;
    let mut prev_y = y;
    let mut i = 0u32;
    while i < points + warmup {
        let nx = (a * x * (1.0 - x) + b * (y - 0.5) * (y - 0.5)).rem_euclid(1.0);
        let ny = (c * y * (1.0 - y) + (a - b) * (x - 0.5).abs() * (y - 0.5)).rem_euclid(1.0);
        x = nx;
        y = ny;
        if i >= warmup {
            let ux = (x * (sim_w as f32 - 1.0)).clamp(0.0, sim_w as f32 - 1.0);
            let uy = (y * (sim_h as f32 - 1.0)).clamp(0.0, sim_h as f32 - 1.0);
            let px = ux.round() as i32;
            let py = uy.round() as i32;
            let jitter = value_noise(ux * 0.23, uy * 0.19, seed + i) * 0.5;
            draw_line(
                (prev_x * (sim_w as f32 - 1.0)).round() as i32,
                (prev_y * (sim_h as f32 - 1.0)).round() as i32,
                px,
                py,
                sim_w as usize,
                sim_h as usize,
                0.62 + jitter,
                &mut density,
            );
            draw_point(
                px,
                py,
                sim_w as usize,
                sim_h as usize,
                0,
                0.84,
                &mut density,
            );
            prev_x = x;
            prev_y = y;
        }
        i += 1;
    }
    normalize(&mut density);
    let mut out = resize_nearest(&density, sim_w, sim_h, width, height);
    for value in out.iter_mut() {
        *value = (*value * 0.88 + 0.12 * value.powf(1.18)).clamp(0.0, 1.0);
    }
    normalize(&mut out);
    out
}

fn render_interference_waves(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim_w = (width / 2).max(192);
    let sim_h = (height / 2).max(192);
    let mut field = vec![0.0f32; (sim_w * sim_h) as usize];
    let wave_count = 16 + (rng.next_u32() % 18);
    let mut i = 0u32;

    while i < wave_count {
        let cx = rng.next_f32() * sim_w as f32;
        let cy = rng.next_f32() * sim_h as f32;
        let freq = 0.7 + rng.next_f32() * 3.6;
        let speed = 0.6 + rng.next_f32() * 1.4;
        let rot = rng.next_f32() * TAU;
        let phase = rng.next_f32() * TAU;
        let amp = 0.08 + rng.next_f32() * 0.15;
        let mut y = 0u32;
        while y < sim_h {
            let mut x = 0u32;
            while x < sim_w {
                let nx = (x as f32 - cx) / sim_w.max(1) as f32;
                let ny = (y as f32 - cy) / sim_h.max(1) as f32;
                let r = nx * nx + ny * ny;
                let wave = (nx * freq * rot.cos()
                    + ny * freq * rot.sin()
                    + (nx * speed).sin() * (ny * speed).cos()
                    + phase)
                    .sin()
                    * amp;
                let phase_term = r.sqrt().sin() * 1.7;
                let noise = value_noise(nx * 5.3 + phase, ny * 5.7 + phase, i ^ (x * 13 + y * 17));
                let idx = (y * sim_w + x) as usize;
                field[idx] += wave * (0.8 + noise * 0.2) + phase_term;
                x += 1;
            }
            y += 1;
        }
        i += 1;
    }

    normalize(&mut field);
    let mut edges = vec![0.0f32; field.len()];
    let mut y = 1u32;
    while y + 1 < sim_h {
        let mut x = 1u32;
        while x + 1 < sim_w {
            let idx = y as usize * sim_w as usize + x as usize;
            let c = sample_nearest(&field, sim_w, sim_h, x as i32, y as i32);
            let gx = (sample_nearest(&field, sim_w, sim_h, x as i32 + 1, y as i32)
                - sample_nearest(&field, sim_w, sim_h, x as i32 - 1, y as i32))
            .abs();
            let gy = (sample_nearest(&field, sim_w, sim_h, x as i32, y as i32 + 1)
                - sample_nearest(&field, sim_w, sim_h, x as i32, y as i32 - 1))
            .abs();
            let mut value = (gx + gy) * 0.75 + (1.0 - c) * 0.35;
            if (c > 0.52 && (gx + gy) > 0.08) || rng.next_f32() < 0.0008 {
                value += 0.25;
            }
            edges[idx] = clamp01(value);
            x += 1;
        }
        y += 1;
    }

    let mut out = resize_bilinear(&edges, sim_w, sim_h, width, height);
    normalize(&mut out);
    for value in out.iter_mut() {
        let jitter = value_noise(*value * 9.0, *value * 7.0, 0xA5A5_77F7) * 0.14;
        *value = clamp01(*value + (jitter - 0.07));
    }
    normalize(&mut out);
    out
}

fn render_clifford_attractor(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim_w = (width / 2).max(176);
    let sim_h = (height / 2).max(176);
    let mut density = vec![0.0f32; (sim_w * sim_h) as usize];
    let points = 320_000 + ((sim_w * sim_h) / 2);
    let warmup = 320u32;
    let a = 1.1 + rng.next_f32() * 2.4;
    let b = 0.7 + rng.next_f32() * 1.8;
    let c = 0.2 + rng.next_f32() * 1.6;
    let d = 0.9 + rng.next_f32() * 1.8;
    let mut x = (rng.next_f32() * 2.0) - 1.0;
    let mut y = (rng.next_f32() * 2.0) - 1.0;
    let mut px = x;
    let mut py = y;
    let seed = rng.next_u32();

    let mut i = 0u32;
    while i < points + warmup {
        let nx = (a * y).sin() + c * (x * y).cos();
        let ny = (b * x).sin() + d * (x - y).cos();
        x = (nx * 0.98).rem_euclid(2.0) - 1.0;
        y = (ny * 0.98).rem_euclid(2.0) - 1.0;

        if !x.is_finite() || !y.is_finite() {
            let jitter = (value_noise(i as f32, y, seed) - 0.5) * 0.2;
            x = (x + jitter).clamp(-1.0, 1.0);
            y = (y - jitter).clamp(-1.0, 1.0);
        }

        if i >= warmup {
            let sx = ((x + 1.0) * 0.5 * (sim_w as f32 - 1.0))
                .round()
                .clamp(0.0, sim_w as f32 - 1.0);
            let sy = ((y + 1.0) * 0.5 * (sim_h as f32 - 1.0))
                .round()
                .clamp(0.0, sim_h as f32 - 1.0);
            let ox = sx as i32;
            let oy = sy as i32;
            let strength = 0.46 + (value_noise(sx * 0.2, sy * 0.2, seed ^ i) * 0.34);
            draw_line(
                (px * 0.5 * (sim_w as f32 - 1.0)).round() as i32,
                (py * 0.5 * (sim_h as f32 - 1.0)).round() as i32,
                ox,
                oy,
                sim_w as usize,
                sim_h as usize,
                strength,
                &mut density,
            );
            if i % 2 == 0 {
                draw_point(
                    ox,
                    oy,
                    sim_w as usize,
                    sim_h as usize,
                    1,
                    strength + 0.2,
                    &mut density,
                );
            }
            px = sx / (sim_w as f32 - 1.0) * 2.0 - 1.0;
            py = sy / (sim_h as f32 - 1.0) * 2.0 - 1.0;
        }
        i += 1;
    }

    for value in density.iter_mut() {
        *value = clamp01(*value * 0.9 + (*value).powf(1.12) * 0.15);
    }
    normalize(&mut density);
    resize_bilinear(&density, sim_w, sim_h, width, height)
}

fn render_julia_set(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim_w = (width / 2).max(160);
    let sim_h = (height / 2).max(160);
    let mut out = vec![0.0f32; (sim_w * sim_h) as usize];
    let cx = (rng.next_f32() - 0.5) * 1.6;
    let cy = (rng.next_f32() - 0.5) * 1.6;
    let zoom = 0.28 + rng.next_f32() * 0.65;
    let max_iter = 140 + (rng.next_u32() % 220);
    let seed = rng.next_u32();

    let mut y = 0u32;
    while y < sim_h {
        let mut x = 0u32;
        while x < sim_w {
            let mut zx = (x as f32 / sim_w as f32 - 0.5) * 2.6 * zoom
                + (value_noise(seed as f32, y as f32, seed ^ x) - 0.5) * 0.06;
            let mut zy = (y as f32 / sim_h as f32 - 0.5) * 2.6 * zoom
                + (value_noise(x as f32, seed as f32, seed ^ (y << 1)) - 0.5) * 0.06;
            let mut i = 0u32;
            let mut escaped = false;
            let mut smooth = 0.0f32;
            while i < max_iter {
                let x2 = zx * zx - zy * zy + cx;
                let y2 = 2.0 * zx * zy + cy;
                zx = x2;
                zy = y2;
                let mag2 = x2 * x2 + y2 * y2;
                if mag2 > 12.0 {
                    smooth = (i as f32 - mag2.log(2.0)) / max_iter as f32;
                    escaped = true;
                    break;
                }
                i += 1;
            }

            let idx = (y * sim_w + x) as usize;
            out[idx] = if !escaped {
                0.06
            } else {
                (1.0 - smooth).clamp(0.0, 1.0)
            };
            x += 1;
        }
        y += 1;
    }

    for value in out.iter_mut() {
        let noise = value_noise(*value * 8.0, 0.33, seed) * 0.11;
        *value = clamp01(*value + noise * 0.25 + (1.0 - *value) * 0.08);
    }
    normalize(&mut out);
    let mut resized = resize_bilinear(&out, sim_w, sim_h, width, height);
    for value in resized.iter_mut() {
        *value = clamp01(value.powf(0.79));
    }
    normalize(&mut resized);
    resized
}

fn render_koch_snowflake(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim_w = (width / 2).max(212);
    let sim_h = (height / 2).max(212);
    let mut out = vec![0.0f32; (sim_w * sim_h) as usize];
    let ring_count = 2 + (rng.next_u32() % 4);
    let depth = 3 + (rng.next_u32() % 2);
    let seed = rng.next_u32();

    let mut ring = 0u32;
    while ring < ring_count {
        let cx = (sim_w as f32 * 0.5) + ((rng.next_f32() - 0.5) * sim_w as f32 * 0.22);
        let cy = (sim_h as f32 * 0.5) + ((rng.next_f32() - 0.5) * sim_h as f32 * 0.22);
        let radius = sim_w.min(sim_h) as f32 * (0.12 + rng.next_f32() * 0.28);
        let sides = 3 + (rng.next_u32() % 3);

        let mut p = 0u32;
        while p < sides {
            let a0 = TAU * (p as f32 / sides as f32) + (seed as f32 * 0.000_001);
            let a1 = TAU * ((p as f32 + 1.0) / sides as f32) + (seed as f32 * 0.000_001);
            let x0 = cx + radius * a0.cos();
            let y0 = cy + radius * a0.sin();
            let x1 = cx + radius * a1.cos();
            let y1 = cy + radius * a1.sin();
            render_koch_curve(
                x0,
                y0,
                x1,
                y1,
                depth,
                1.0,
                sim_w as usize,
                sim_h as usize,
                (1.0 - ring as f32 / ring_count as f32).clamp(0.2, 1.0),
                &mut out,
                rng,
                seed,
            );
            p += 1;
        }
        ring += 1;
    }

    normalize(&mut out);
    let mut resized = resize_bilinear(&out, sim_w, sim_h, width, height);
    for value in resized.iter_mut() {
        *value = clamp01(*value * 1.1 - 0.1 * (value.sqrt()));
    }
    normalize(&mut resized);
    resized
}

#[allow(clippy::too_many_arguments)]
fn render_koch_curve(
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    depth: u32,
    side_sign: f32,
    width: usize,
    height: usize,
    value: f32,
    out: &mut [f32],
    rng: &mut XorShift32,
    seed: u32,
) {
    if depth == 0 {
        draw_line(
            x0.round() as i32,
            y0.round() as i32,
            x1.round() as i32,
            y1.round() as i32,
            width,
            height,
            value.clamp(0.1, 1.0),
            out,
        );
        return;
    }

    let dx = x1 - x0;
    let dy = y1 - y0;
    let x3 = x0 + dx / 3.0;
    let y3 = y0 + dy / 3.0;
    let x4 = x0 + 2.0 * dx / 3.0;
    let y4 = y0 + 2.0 * dy / 3.0;
    let ux = x4 - x3;
    let uy = y4 - y3;
    let len = (ux * ux + uy * uy).sqrt().max(1e-6);
    let offset = len * (0.86 * 0.5);
    let nx = -uy / len;
    let ny = ux / len;
    let peak_x = (x3 + x4) * 0.5 + nx * offset * side_sign;
    let peak_y = (y3 + y4) * 0.5 + ny * offset * side_sign;
    let jitter = 1.0 + (value_noise(seed as f32, value * 10.0, seed ^ (depth << 8)) - 0.5) * 0.2;
    let next_depth = depth - 1;
    let next = value * jitter.clamp(0.75, 1.25);
    let next_side = if rng.next_f32() < 0.5 {
        side_sign
    } else {
        -side_sign
    };
    render_koch_curve(
        x0,
        y0,
        x3,
        y3,
        next_depth,
        next_side,
        width,
        height,
        next,
        out,
        rng,
        seed ^ 0x1,
    );
    render_koch_curve(
        x3,
        y3,
        peak_x,
        peak_y,
        next_depth,
        next_side,
        width,
        height,
        next * 1.03,
        out,
        rng,
        seed ^ 0x2,
    );
    render_koch_curve(
        peak_x,
        peak_y,
        x4,
        y4,
        next_depth,
        -next_side,
        width,
        height,
        next * 1.03,
        out,
        rng,
        seed ^ 0x3,
    );
    render_koch_curve(
        x4,
        y4,
        x1,
        y1,
        next_depth,
        -next_side,
        width,
        height,
        next,
        out,
        rng,
        seed ^ 0x4,
    );
}

#[derive(Clone, Copy)]
struct BifurcationBranch {
    x: f32,
    y: f32,
    angle: f32,
    length: f32,
    depth: u32,
    density: f32,
}

fn render_bifurcation_tree(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim_w = (width / 2).max(200);
    let sim_h = (height / 2).max(200);
    let mut out = vec![0.0f32; (sim_w * sim_h) as usize];
    let roots = 2 + (rng.next_u32() % 4);
    let base_len = sim_w.min(sim_h) as f32 * (0.24 + rng.next_f32() * 0.12);
    let mut stack: Vec<BifurcationBranch> = Vec::with_capacity(128);
    let mut i = 0u32;

    while i < roots {
        let angle = rng.next_f32() * TAU + (if rng.next_f32() < 0.5 { 0.0 } else { TAU * 0.5 });
        let sx = (sim_w as f32 * 0.15) + rng.next_f32() * (sim_w as f32 * 0.7);
        let sy = (sim_h as f32 * 0.15) + rng.next_f32() * (sim_h as f32 * 0.7);
        stack.push(BifurcationBranch {
            x: sx,
            y: sy,
            angle,
            length: base_len * (0.35 + rng.next_f32() * 0.65),
            depth: 6 + (rng.next_u32() % 3),
            density: 1.0,
        });
        i += 1;
    }

    while let Some(branch) = stack.pop() {
        if branch.depth == 0 || branch.length < 1.5 {
            continue;
        }
        let mut x = branch.x;
        let mut y = branch.y;
        let mut angle = branch.angle;
        let mut step = 0u32;
        let segments = 10 + (rng.next_u32() % 14);

        while step < segments {
            let n = value_noise(x * 0.013, y * 0.011, rng.next_u32()) - 0.5;
            let drift = (n * 2.0).clamp(-1.0, 1.0);
            angle += drift * 0.35 + (segment_wave(step) - 0.5) * 0.08;
            let seg = branch.length / segments as f32;
            let nx = x + angle.cos() * seg * (0.5 + n.abs());
            let ny = y + angle.sin() * seg * (0.9 - n.abs() * 0.2);
            let sx = x.clamp(0.0, sim_w as f32 - 1.0).round() as i32;
            let sy = y.clamp(0.0, sim_h as f32 - 1.0).round() as i32;
            let ex = nx.clamp(0.0, sim_w as f32 - 1.0).round() as i32;
            let ey = ny.clamp(0.0, sim_h as f32 - 1.0).round() as i32;
            let strength =
                branch.density * (0.42 + 0.14 * (1.0 - (step as f32 / segments as f32)).powf(1.0));
            draw_line(
                sx,
                sy,
                ex,
                ey,
                sim_w as usize,
                sim_h as usize,
                strength,
                &mut out,
            );
            if step % 3 == 0 {
                draw_point(
                    ex,
                    ey,
                    sim_w as usize,
                    sim_h as usize,
                    1,
                    strength * 1.02,
                    &mut out,
                );
            }

            if step > 1 && branch.depth > 1 && step % 4 == 0 {
                let child_angle = angle + (rng.next_f32() * 1.5 - 0.75);
                stack.push(BifurcationBranch {
                    x: nx,
                    y: ny,
                    angle: child_angle,
                    length: branch.length * (0.32 + rng.next_f32() * 0.24),
                    depth: branch.depth - 1,
                    density: (branch.density * 0.84).max(0.2),
                });
            } else if branch.depth > 2 && rng.next_f32() < 0.24 {
                let child_angle = angle + (rng.next_f32() * 0.52 - 0.26) + TAU * 0.5;
                stack.push(BifurcationBranch {
                    x: nx,
                    y: ny,
                    angle: child_angle,
                    length: branch.length * (0.2 + rng.next_f32() * 0.22),
                    depth: branch.depth - 1,
                    density: (branch.density * 0.77).max(0.16),
                });
            }

            x = nx;
            y = ny;
            step += 1;
        }
    }

    normalize(&mut out);
    let mut resized = resize_bilinear(&out, sim_w, sim_h, width, height);
    for value in resized.iter_mut() {
        *value = clamp01(value.powf(0.82) + *value * 0.08);
    }
    normalize(&mut resized);
    resized
}

fn render_depth_relief(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim_w = (width / 2).max(192);
    let sim_h = (height / 2).max(192);
    let size = (sim_w * sim_h) as usize;
    let mut field = noise_field(sim_w, sim_h, rng, 4);
    let mut next = vec![0.0f32; size];
    let mut seed = XorShift32::new(rng.next_u32());
    let passes = 4 + (seed.next_u32() % 4) as usize;
    let layer_count = 2 + (seed.next_u32() % 3) as usize;
    let mut stage = 0usize;

    while stage < layer_count {
        let mut pass = 0usize;
        while pass < passes {
            for y in 0..sim_h as i32 {
                for x in 0..sim_w as i32 {
                    let idx = (y as usize * sim_w as usize) + x as usize;
                    if x == 0 || y == 0 || x + 1 >= sim_w as i32 || y + 1 >= sim_h as i32 {
                        next[idx] = field[idx];
                        continue;
                    }

                    let i = idx;
                    let left = field[i - 1];
                    let right = field[i + 1];
                    let up = field[i - sim_w as usize];
                    let down = field[i + sim_w as usize];
                    let center = field[i];
                    let laplacian = (left + right + up + down) * 0.25 - center;
                    let warp = value_noise(
                        x as f32 * (0.002 + stage as f32 * 0.0008) + y as f32 * 0.0001,
                        y as f32 * (0.002 + pass as f32 * 0.0006) + 13.0,
                        seed.next_u32() + (pass as u32 * 3),
                    );
                    let ridge = warp * 0.16;
                    let blend = if stage == 0 { 0.82 } else { 0.63 };
                    let value = center + laplacian * blend + (ridge - 0.5) * 0.12;
                    next[idx] = clamp01(value.clamp(0.0, 1.0));
                }
            }
            std::mem::swap(&mut field, &mut next);
            pass += 1;
        }
        stage += 1;
    }

    let mut out = vec![0.0f32; size];
    for y in 1..(sim_h as i32 - 1) {
        for x in 1..(sim_w as i32 - 1) {
            let idx = (y as usize * sim_w as usize) + x as usize;
            let left = field[idx - 1];
            let right = field[idx + 1];
            let up = field[idx - sim_w as usize];
            let down = field[idx + sim_w as usize];
            let gradient = ((right - left).abs() + (down - up).abs()) * 0.5;
            let ridge = (field[idx] - (left + right + up + down) * 0.25).abs();
            let mut value = field[idx] * 0.72 + (1.0 - gradient * 1.6).clamp(0.0, 1.0) * 0.22;
            value = value + ridge * 0.35;
            out[idx] = clamp01(value);
        }
    }

    let mut i = 0usize;
    while i < size {
        if out[i] == 0.0 {
            out[i] = field[i];
        }
        out[i] = clamp01(out[i]);
        i += 1;
    }

    normalize(&mut out);
    for value in out.iter_mut() {
        let tone = 0.75 + 0.18 * value_noise(*value * 8.0, 0.21, seed.next_u32());
        *value = clamp01(value.powf(0.9) * tone);
    }
    normalize(&mut out);
    let mut resized = resize_bilinear(&out, sim_w, sim_h, width, height);
    for value in resized.iter_mut() {
        *value = clamp01(*value * 1.05 + 0.02);
    }
    normalize(&mut resized);
    resized
}

fn render_attractor_tunnel(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim_w = (width / 2).max(192);
    let sim_h = (height / 2).max(192);
    let mut density = vec![0.0f32; (sim_w * sim_h) as usize];
    let points = 220_000 + (sim_w * sim_h);
    let warmup = 420u32;
    let seed = rng.next_u32();
    let a = 1.3 + rng.next_f32() * 2.0;
    let b = 1.9 + rng.next_f32() * 1.8;
    let c = 1.1 + rng.next_f32() * 2.1;
    let d = 0.7 + rng.next_f32() * 1.6;
    let e = 0.4 + rng.next_f32() * 1.2;
    let mut x = rng.next_f32() * 2.0 - 1.0;
    let mut y = rng.next_f32() * 2.0 - 1.0;
    let mut z = rng.next_f32() * 2.0 - 1.0;
    let mut prev_set = false;
    let mut prev_x = 0.0f32;
    let mut prev_y = 0.0f32;
    let mut i = 0u32;

    while i < points + warmup {
        let n1 = value_noise(x * 0.7 + i as f32 * 0.000_4, y * 0.7, seed ^ i) - 0.5;
        let n2 = value_noise(
            y * 0.5 + z * 0.4,
            z * 0.8 + i as f32 * 0.000_3,
            seed ^ (i + 17),
        ) - 0.5;
        let tunnel = (n1 + n2) * 0.5;
        let _blend = (i as f32 / points.max(1) as f32) * TAU;
        let nx = (a * (y + tunnel)).sin() - (d * x * z).cos() + tunnel * 0.2;
        let ny = (b * (z + tunnel)).cos() + (c * x).sin() + tunnel * 0.18;
        let nz = (d * (x - y)).sin() - (e * z * tunnel).cos();
        x = (nx * 0.97).tanh();
        y = (ny * 0.97).tanh();
        z = (nz * 0.97).tanh();

        if i >= warmup {
            let px = ((x * 0.53 + 0.5) * (sim_w as f32 - 1.0)).clamp(0.0, sim_w as f32 - 1.0);
            let py = (((y + 0.35 * z) * 0.53 + 0.5) * (sim_h as f32 - 1.0))
                .clamp(0.0, sim_h as f32 - 1.0);
            let strength = 0.38 + (0.62 * (1.0 - z.abs())) * (0.75 + tunnel.abs());
            if prev_set {
                draw_line(
                    prev_x.round() as i32,
                    prev_y.round() as i32,
                    px.round() as i32,
                    py.round() as i32,
                    sim_w as usize,
                    sim_h as usize,
                    strength,
                    &mut density,
                );
                if i % 4 == 0 {
                    draw_point(
                        px.round() as i32,
                        py.round() as i32,
                        sim_w as usize,
                        sim_h as usize,
                        1,
                        strength * 0.8,
                        &mut density,
                    );
                }
            }
            prev_x = px;
            prev_y = py;
            prev_set = true;
        }

        if !x.is_finite() || !y.is_finite() || !z.is_finite() {
            x = (value_noise(i as f32, z, seed) - 0.5) * 1.4;
            y = (value_noise(x, i as f32, seed ^ 0x1234) - 0.5) * 1.4;
            z = (value_noise(y, x, seed ^ 0x5678) - 0.5) * 1.4;
        }

        i += 1;
    }

    for value in density.iter_mut() {
        *value = (*value * 0.92 + value.powf(1.02) * 0.16).clamp(0.0, 1.0);
    }
    normalize(&mut density);
    let mut out = resize_bilinear(&density, sim_w, sim_h, width, height);
    for value in out.iter_mut() {
        *value = clamp01(value.powf(0.88));
    }
    normalize(&mut out);
    out
}

#[derive(Clone, Copy)]
struct OrbitalLabyrinthArc {
    cx: f32,
    cy: f32,
    radius: f32,
    phase: f32,
    spins: f32,
    depth: u32,
}

fn render_orbital_labyrinth(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim_w = (width / 2).max(192);
    let sim_h = (height / 2).max(192);
    let mut out = vec![0.0f32; (sim_w * sim_h) as usize];
    let mut arcs = Vec::with_capacity(64);
    let root_count = 3 + (rng.next_u32() % 5);
    let mut root = 0u32;
    let field_seed = rng.next_u32();

    while root < root_count {
        let cx = sim_w as f32 * (0.2 + rng.next_f32() * 0.6);
        let cy = sim_h as f32 * (0.2 + rng.next_f32() * 0.6);
        let radius = (sim_w.min(sim_h) as f32) * (0.16 + rng.next_f32() * 0.18);
        arcs.push(OrbitalLabyrinthArc {
            cx,
            cy,
            radius,
            phase: rng.next_f32() * TAU,
            spins: 1.0 + rng.next_f32() * 0.9,
            depth: 4 + (rng.next_u32() % 3),
        });
        root += 1;
    }

    while let Some(arc) = arcs.pop() {
        if arc.radius < 2.5 || arc.depth == 0 {
            continue;
        }

        let segments = 24 + (rng.next_u32() % 32) as i32;
        let mut prev_x = arc.cx + arc.radius * arc.phase.cos();
        let mut prev_y = arc.cy + arc.radius * arc.phase.sin();
        for step in 0..=segments {
            let t = (step as f32) / (segments as f32 + 1e-6);
            let angle = arc.phase
                + t * TAU * arc.spins
                + (value_noise(t * 5.0, field_seed as f32, rng.next_u32()) - 0.5) * 0.65;
            let wave = value_noise(
                arc.cx * 0.001 + t * 4.2,
                arc.cy * 0.001 + t * 3.3,
                field_seed ^ step as u32,
            );
            let radius = arc.radius * (0.65 + (wave - 0.5) * 0.18 + t * 0.08);
            let x = (arc.cx + radius * angle.cos()).clamp(0.0, sim_w as f32 - 1.0);
            let y = (arc.cy + radius * angle.sin() * (1.0 + 0.35 * wave))
                .clamp(0.0, sim_h as f32 - 1.0);
            let depth = arc.depth as f32 / 8.0;
            let strength = 0.38 + 0.2 * wave + (0.16 * depth);
            draw_line(
                prev_x.round() as i32,
                prev_y.round() as i32,
                x.round() as i32,
                y.round() as i32,
                sim_w as usize,
                sim_h as usize,
                strength,
                &mut out,
            );
            if step % 4 == 0 {
                draw_point(
                    x.round() as i32,
                    y.round() as i32,
                    sim_w as usize,
                    sim_h as usize,
                    1,
                    strength * 0.76,
                    &mut out,
                );
            }
            if step == segments / 2 && arc.depth > 1 && rng.next_f32() < 0.74 {
                arcs.push(OrbitalLabyrinthArc {
                    cx: x,
                    cy: y,
                    radius: arc.radius * (0.48 + rng.next_f32() * 0.18),
                    phase: angle + (rng.next_f32() * 1.1 - 0.55),
                    spins: arc.spins * (1.1 + rng.next_f32() * 0.8),
                    depth: arc.depth - 1,
                });
            } else if step == segments / 3 && arc.depth > 1 && rng.next_f32() < 0.55 {
                arcs.push(OrbitalLabyrinthArc {
                    cx: x,
                    cy: y,
                    radius: arc.radius * (0.54 + rng.next_f32() * 0.22),
                    phase: angle + TAU * 0.5 + (rng.next_f32() * 0.7 - 0.35),
                    spins: arc.spins * (0.7 + rng.next_f32() * 0.6),
                    depth: arc.depth - 1,
                });
            }
            prev_x = x;
            prev_y = y;
        }
    }

    normalize(&mut out);
    let grain = noise_field(sim_w, sim_h, rng, 2);
    let mut i = 0usize;
    while i < out.len() {
        out[i] = clamp01(out[i] * 0.88 + grain[i] * 0.12);
        i += 1;
    }
    normalize(&mut out);
    let mut resized = resize_nearest(&out, sim_w, sim_h, width, height);
    for value in resized.iter_mut() {
        let v = *value;
        *value = clamp01(v.powf(0.82) + v * 0.12);
    }
    normalize(&mut resized);
    resized
}

fn segment_wave(step: u32) -> f32 {
    let phase = step as f32 * 0.4;
    let s = phase.sin() * 0.5 + 0.5;
    s * 0.5 + 0.25
}

fn noise_value(x: f32, y: f32, seed: u32) -> f32 {
    let base = 0.003_2 * ((seed % 129) as f32 + 11.0);
    value_noise(x * 1.9 + base, y * 1.7 + base, seed).clamp(0.0, 1.0)
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
            CpuStrategy::CannyEdge,
            CpuStrategy::PerlinRidge,
            CpuStrategy::PlasmaField,
            CpuStrategy::SierpinskiCarpet,
            CpuStrategy::BarnsleyFern,
            CpuStrategy::TurbulentFlow,
            CpuStrategy::MandelbrotField,
            CpuStrategy::RecursiveTiling,
            CpuStrategy::TuringCascade,
            CpuStrategy::FlowFilaments,
            CpuStrategy::OrbitalAtlas,
            CpuStrategy::CrystalGrowth,
            CpuStrategy::VortexConvection,
            CpuStrategy::ErosionChannels,
            CpuStrategy::StochasticIFS,
            CpuStrategy::DeJongAttractor,
            CpuStrategy::RecursiveRibbon,
            CpuStrategy::MagneticFieldlines,
            CpuStrategy::LorenzAttractor,
            CpuStrategy::RecursiveStarburst,
            CpuStrategy::LogisticChaos,
            CpuStrategy::InterferenceWaves,
            CpuStrategy::CliffordAttractor,
            CpuStrategy::JuliaSet,
            CpuStrategy::KochSnowflake,
            CpuStrategy::BifurcationTree,
            CpuStrategy::DepthRelief,
            CpuStrategy::AttractorTunnel,
            CpuStrategy::OrbitalLabyrinth,
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
