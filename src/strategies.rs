//! Generative strategy module used for CPU-side procedural rendering paths.
//!
//! The main application primarily renders through GPU compute shaders. This module
//! adds a secondary family of CPU generators to diversify outputs with non-fractal
//! algorithms and to reduce over-convergence toward one visual style.

use crate::model::ArtStyle;
use crate::model::XorShift32;
use rayon::prelude::*;
use std::f32::consts::TAU;

/// CPU generator strategies that can replace GPU-based render layers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    /// Multi-frequency Lissajous trajectories with branching harmonic drift.
    LissajousOrbits,
    /// Chaotic logistic-map evolution in 2D projected to the canvas.
    LogisticChaos,
    /// Interference-like wave superposition on a noisy phase field.
    InterferenceWaves,
    /// Gravitational mesh field with interacting particles and drifting trajectories.
    GraviticWeb,
    /// Poisson-sampled nodes joined with adaptive near-neighbor edge drawing.
    PoissonMesh,
    /// Blended meta-ball field driven by moving attractor seeds.
    MetaballField,
    /// Recursive braided flow with noise-modulated curvature.
    BraidFlow,
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
    /// Smooth phase-domain morphologies from low-frequency harmonic noise.
    PhaseField,
    /// Lenia-like growing cellular structures at low resolution.
    Lenia,
    /// Particle traces from curl-noise vector field integration.
    CurlNoiseFlow,
    /// Reaction lattice simulation with altered feed/kill behavior.
    ReactionLattice,
    /// Harmonic interference over warped coordinate noise.
    HarmonicInterference,
    /// Attractor path traces modulated by Voronoi style ridges.
    AttractorVoronoiHybrid,
    /// Recursive noise terrain with warped ridges.
    RecursiveNoiseTerrain,
    /// Bifurcation map sampled across 2D phase space.
    BifurcationGrid,
}

/// Coarse grouping used to keep layer families coherent across an image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrategyFamily {
    /// Fractal or orbit-driven iterative maps.
    Fractal,
    /// Particle flow and field advection systems.
    Flow,
    /// Diffusion and lattice reaction systems.
    Diffusion,
    /// Cellular and rule-evolving simulations.
    Cellular,
    /// Geometric/recursive combinatorial drawing systems.
    Geometry,
    /// Noise and phase based harmonic textures.
    Harmonic,
    /// Tiling-like recursive or periodic generators.
    Tiling,
}

impl CpuStrategy {
    /// Total number of CPU strategies available.
    fn count() -> u32 {
        57
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
            37 => Self::LissajousOrbits,
            38 => Self::GraviticWeb,
            39 => Self::PoissonMesh,
            40 => Self::MetaballField,
            41 => Self::BraidFlow,
            42 => Self::CliffordAttractor,
            43 => Self::JuliaSet,
            44 => Self::KochSnowflake,
            45 => Self::BifurcationTree,
            46 => Self::DepthRelief,
            47 => Self::AttractorTunnel,
            48 => Self::OrbitalLabyrinth,
            49 => Self::PhaseField,
            50 => Self::Lenia,
            51 => Self::CurlNoiseFlow,
            52 => Self::ReactionLattice,
            53 => Self::HarmonicInterference,
            54 => Self::AttractorVoronoiHybrid,
            55 => Self::RecursiveNoiseTerrain,
            _ => Self::BifurcationGrid,
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
            Self::LissajousOrbits => "lissajous-orbits",
            Self::LogisticChaos => "logistic-chaos",
            Self::InterferenceWaves => "interference-waves",
            Self::GraviticWeb => "gravitic-web",
            Self::PoissonMesh => "poisson-mesh",
            Self::MetaballField => "metaball-field",
            Self::BraidFlow => "braid-flow",
            Self::CliffordAttractor => "clifford-attractor",
            Self::JuliaSet => "julia-set",
            Self::KochSnowflake => "koch-snowflake",
            Self::BifurcationTree => "bifurcation-tree",
            Self::DepthRelief => "depth-relief",
            Self::AttractorTunnel => "attractor-tunnel",
            Self::OrbitalLabyrinth => "orbital-labyrinth",
            Self::PhaseField => "phase-field",
            Self::Lenia => "lenia",
            Self::CurlNoiseFlow => "curl-noise-flow",
            Self::ReactionLattice => "reaction-lattice",
            Self::HarmonicInterference => "harmonic-interference",
            Self::AttractorVoronoiHybrid => "attractor-voronoi-hybrid",
            Self::RecursiveNoiseTerrain => "recursive-noise-terrain",
            Self::BifurcationGrid => "bifurcation-grid",
        }
    }

    /// Returns a broad family used to bias strategy continuity per image.
    pub fn family(self) -> StrategyFamily {
        match self {
            Self::ReactionDiffusion
            | Self::TuringCascade
            | Self::CrystalGrowth
            | Self::ReactionLattice => StrategyFamily::Diffusion,
            Self::LSystem
            | Self::CannyEdge
            | Self::EdgeSobel
            | Self::EdgeLaplacian
            | Self::Voronoi
            | Self::Delaunay
            | Self::Maze
            | Self::PoissonMesh
            | Self::BarnsleyFern
            | Self::SierpinskiCarpet => StrategyFamily::Geometry,
            Self::CellularAutomata | Self::Lenia => StrategyFamily::Cellular,
            Self::ParticleFlow
            | Self::FlowFilaments
            | Self::OrbitalAtlas
            | Self::VortexConvection
            | Self::ErosionChannels
            | Self::StochasticIFS
            | Self::MagneticFieldlines
            | Self::GraviticWeb
            | Self::BraidFlow
            | Self::RecursiveFold
            | Self::AttractorHybrid
            | Self::TurbulentFlow
            | Self::CurlNoiseFlow
            | Self::LissajousOrbits => StrategyFamily::Flow,
            Self::MandelbrotField
            | Self::DeJongAttractor
            | Self::LogisticChaos
            | Self::LorenzAttractor
            | Self::JuliaSet
            | Self::DepthRelief
            | Self::AttractorTunnel
            | Self::OrbitalLabyrinth
            | Self::BifurcationTree
            | Self::InterferenceWaves
            | Self::BifurcationGrid
            | Self::RecursiveRibbon
            | Self::RecursiveStarburst
            | Self::AttractorVoronoiHybrid
            | Self::CliffordAttractor
            | Self::IteratedFractal
            | Self::StrangeAttractor => StrategyFamily::Fractal,
            Self::RecursiveTiling | Self::KochSnowflake => StrategyFamily::Tiling,
            Self::RadialWave
            | Self::PlasmaField
            | Self::PerlinRidge
            | Self::PhaseField
            | Self::RecursiveNoiseTerrain
            | Self::HarmonicInterference
            | Self::MetaballField => StrategyFamily::Harmonic,
            _ => StrategyFamily::Geometry,
        }
    }

    fn is_tiling(self) -> bool {
        matches!(
            self,
            Self::Maze | Self::RecursiveTiling | Self::SierpinskiCarpet | Self::KochSnowflake
        )
    }

    fn from_non_tiling_u32(value: u32) -> Self {
        let non_tiling = [
            Self::EdgeSobel,
            Self::EdgeLaplacian,
            Self::ReactionDiffusion,
            Self::LSystem,
            Self::ProceduralNoise,
            Self::CellularAutomata,
            Self::ParticleFlow,
            Self::Voronoi,
            Self::Delaunay,
            Self::IteratedFractal,
            Self::StrangeAttractor,
            Self::RadialWave,
            Self::RecursiveFold,
            Self::AttractorHybrid,
            Self::CannyEdge,
            Self::PerlinRidge,
            Self::PlasmaField,
            Self::BarnsleyFern,
            Self::TurbulentFlow,
            Self::MandelbrotField,
            Self::TuringCascade,
            Self::FlowFilaments,
            Self::OrbitalAtlas,
            Self::CrystalGrowth,
            Self::VortexConvection,
            Self::ErosionChannels,
            Self::StochasticIFS,
            Self::DeJongAttractor,
            Self::RecursiveRibbon,
            Self::MagneticFieldlines,
            Self::LorenzAttractor,
            Self::RecursiveStarburst,
            Self::LogisticChaos,
            Self::InterferenceWaves,
            Self::LissajousOrbits,
            Self::GraviticWeb,
            Self::PoissonMesh,
            Self::MetaballField,
            Self::BraidFlow,
            Self::CliffordAttractor,
            Self::JuliaSet,
            Self::BifurcationTree,
            Self::DepthRelief,
            Self::AttractorTunnel,
            Self::OrbitalLabyrinth,
            Self::PhaseField,
            Self::Lenia,
            Self::CurlNoiseFlow,
            Self::ReactionLattice,
            Self::HarmonicInterference,
            Self::AttractorVoronoiHybrid,
            Self::RecursiveNoiseTerrain,
            Self::BifurcationGrid,
        ];
        non_tiling[(value as usize) % non_tiling.len()]
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
            Self::Gpu(style) => ArtStyle::from_u32(style).label(),
            Self::Cpu(kind) => kind.label(),
        }
    }

    /// Approximate family used for same-style continuity.
    pub fn family(self) -> StrategyFamily {
        match self {
            Self::Gpu(style) => gpu_style_family(style),
            Self::Cpu(kind) => kind.family(),
        }
    }
}

fn gpu_style_family(style: u32) -> StrategyFamily {
    match style % 17 {
        0 | 1 | 2 | 3 | 4 | 5 | 7 | 8 | 10 | 16 => StrategyFamily::Fractal,
        11 => StrategyFamily::Flow,
        12..=15 => StrategyFamily::Harmonic,
        _ => StrategyFamily::Geometry,
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

/// Reusable scratch buffers for CPU strategy rendering.
///
/// The render loop reuses these vectors across layers so hot strategies can
/// update workspace output buffers in place without allocating a new output
/// `Vec<f32>` each call.
#[derive(Debug, Default)]
pub struct StrategyScratch {
    reaction_u: Vec<f32>,
    reaction_v: Vec<f32>,
    reaction_next_u: Vec<f32>,
    reaction_next_v: Vec<f32>,
    reaction_noise: Vec<f32>,
}

fn ensure_len_with_fill(buffer: &mut Vec<f32>, len: usize, fill: f32) {
    if buffer.len() != len {
        buffer.resize(len, fill);
    }
    buffer.fill(fill);
}

/// Returns a render strategy for the next layer.
#[allow(dead_code)]
pub fn pick_render_strategy(rng: &mut XorShift32, fast: bool) -> RenderStrategy {
    pick_render_strategy_with_preferences(rng, fast, true)
}

/// Returns a render strategy for the next layer with an explicit preference
/// for GPU or CPU compute.
pub fn pick_render_strategy_with_preferences(
    rng: &mut XorShift32,
    fast: bool,
    prefer_gpu: bool,
) -> RenderStrategy {
    let strategy_roll = rng.next_f32();
    let gpu_chance = if fast { 0.9 } else { 0.35 };

    if prefer_gpu && strategy_roll < gpu_chance {
        let mut style = ArtStyle::from_u32(rng.next_u32());
        let extra_diversify = if fast { 0.65 } else { 0.55 };
        if style.is_tiling_like() || rng.next_f32() < extra_diversify {
            style = ArtStyle::next_non_tiling_from(rng);
        }
        return RenderStrategy::Gpu(style.as_u32());
    }

    let mut strategy = CpuStrategy::from_u32(rng.next_u32());
    let keep_tiling = if fast { 0.06 } else { 0.03 };
    if strategy.is_tiling() && rng.next_f32() > keep_tiling {
        strategy = CpuStrategy::from_non_tiling_u32(rng.next_u32());
    }

    RenderStrategy::Cpu(strategy)
}

/// Returns a coarse runtime cost estimate for a CPU strategy.
pub fn cpu_strategy_cost(strategy: CpuStrategy) -> u32 {
    let family_cost = match strategy.family() {
        StrategyFamily::Fractal => 180,
        StrategyFamily::Flow => 220,
        StrategyFamily::Diffusion => 260,
        StrategyFamily::Cellular => 170,
        StrategyFamily::Geometry => 150,
        StrategyFamily::Harmonic => 140,
        StrategyFamily::Tiling => 165,
    };
    let extra = match strategy {
        CpuStrategy::ReactionDiffusion
        | CpuStrategy::ReactionLattice
        | CpuStrategy::TuringCascade
        | CpuStrategy::CrystalGrowth => 120,
        CpuStrategy::StrangeAttractor
        | CpuStrategy::AttractorHybrid
        | CpuStrategy::LorenzAttractor
        | CpuStrategy::AttractorTunnel
        | CpuStrategy::GraviticWeb
        | CpuStrategy::OrbitalAtlas => 90,
        CpuStrategy::PerlinRidge
        | CpuStrategy::ProceduralNoise
        | CpuStrategy::InterferenceWaves
        | CpuStrategy::HarmonicInterference
        | CpuStrategy::PlasmaField => 0,
        _ => 35,
    };
    family_cost + extra
}

/// Returns a coarse runtime cost estimate for any render strategy.
pub fn render_strategy_cost(strategy: RenderStrategy) -> u32 {
    match strategy {
        RenderStrategy::Gpu(_) => 40,
        RenderStrategy::Cpu(strategy) => cpu_strategy_cost(strategy),
    }
}

fn pick_cpu_strategy_for_budget(rng: &mut XorShift32, budget: u32) -> CpuStrategy {
    const CANDIDATES: [CpuStrategy; 12] = [
        CpuStrategy::PerlinRidge,
        CpuStrategy::ProceduralNoise,
        CpuStrategy::InterferenceWaves,
        CpuStrategy::HarmonicInterference,
        CpuStrategy::PlasmaField,
        CpuStrategy::RadialWave,
        CpuStrategy::Maze,
        CpuStrategy::BifurcationGrid,
        CpuStrategy::Voronoi,
        CpuStrategy::LSystem,
        CpuStrategy::KochSnowflake,
        CpuStrategy::RecursiveTiling,
    ];
    let threshold = budget.saturating_add(30);
    let mut seen = 0u32;
    let mut chosen = CpuStrategy::PerlinRidge;
    for candidate in CANDIDATES {
        if cpu_strategy_cost(candidate) <= threshold {
            seen += 1;
            if (rng.next_u32() % seen) == 0 {
                chosen = candidate;
            }
        }
    }
    if seen == 0 {
        CpuStrategy::PerlinRidge
    } else {
        chosen
    }
}

/// Constrain a selected strategy to the remaining compute budget.
///
/// This keeps per-image tail latency bounded on CPU-heavy runs while preserving
/// family continuity when possible.
pub fn fit_strategy_to_budget(
    rng: &mut XorShift32,
    strategy: RenderStrategy,
    budget_left: u32,
    fast: bool,
    prefer_gpu: bool,
) -> RenderStrategy {
    if render_strategy_cost(strategy) <= budget_left {
        return strategy;
    }

    if prefer_gpu && rng.next_f32() < 0.35 {
        let candidate = pick_render_strategy_with_preferences(rng, fast, prefer_gpu);
        if matches!(candidate, RenderStrategy::Gpu(_)) {
            return candidate;
        }
    }

    let attempts = if fast { 12 } else { 22 };
    let mut i = 0u32;
    while i < attempts {
        let candidate = pick_render_strategy_near_family_with_preferences(
            rng, fast, strategy, 0.92, prefer_gpu,
        );
        if render_strategy_cost(candidate) <= budget_left {
            return candidate;
        }
        i += 1;
    }

    RenderStrategy::Cpu(pick_cpu_strategy_for_budget(rng, budget_left))
}

/// Pick a render strategy with a bias toward the same family as `base`.
#[allow(dead_code)]
pub fn pick_render_strategy_near_family(
    rng: &mut XorShift32,
    fast: bool,
    base: RenderStrategy,
    family_bias: f32,
) -> RenderStrategy {
    pick_render_strategy_near_family_with_preferences(rng, fast, base, family_bias, true)
}

/// Pick a render strategy with a bias toward the same family as `base` and an
/// explicit preference for GPU or CPU strategy selection.
pub fn pick_render_strategy_near_family_with_preferences(
    rng: &mut XorShift32,
    fast: bool,
    base: RenderStrategy,
    family_bias: f32,
    prefer_gpu: bool,
) -> RenderStrategy {
    if family_bias <= 0.0 {
        return pick_render_strategy_with_preferences(rng, fast, prefer_gpu);
    }

    if rng.next_f32() < family_bias {
        let family = base.family();
        let attempts = if fast { 12 } else { 22 };
        let mut i = 0;
        while i < attempts {
            let candidate = pick_render_strategy_with_preferences(rng, fast, prefer_gpu);
            if candidate.family() == family {
                return candidate;
            }
            i += 1;
        }
    }

    pick_render_strategy_with_preferences(rng, fast, prefer_gpu)
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
            CpuStrategy::LissajousOrbits
            | CpuStrategy::GraviticWeb
            | CpuStrategy::PoissonMesh
            | CpuStrategy::MetaballField => StrategyProfile {
                filter_bias: 0.17,
                gradient_bias: 0.19,
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
            | CpuStrategy::FlowFilaments
            | CpuStrategy::BraidFlow => StrategyProfile {
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
            CpuStrategy::PhaseField
            | CpuStrategy::Lenia
            | CpuStrategy::ReactionLattice
            | CpuStrategy::RecursiveNoiseTerrain
            | CpuStrategy::BifurcationGrid
            | CpuStrategy::HarmonicInterference => StrategyProfile {
                filter_bias: 0.34,
                gradient_bias: 0.26,
                force_detail: true,
            },
            CpuStrategy::CurlNoiseFlow | CpuStrategy::AttractorVoronoiHybrid => StrategyProfile {
                filter_bias: 0.28,
                gradient_bias: 0.16,
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

/// Render a CPU strategy for a layer into a caller-provided output slice.
///
/// This API is intended for the main render loop and reuses `scratch` across
/// layers to minimize allocation churn on hot strategies.
pub fn render_cpu_strategy_into(
    strategy: CpuStrategy,
    width: u32,
    height: u32,
    seed: u32,
    fast: bool,
    complexity_budget: u32,
    out: &mut [f32],
    scratch: &mut StrategyScratch,
) {
    debug_assert_eq!(out.len(), (width as usize) * (height as usize));
    let mut rng = XorShift32::new(seed ^ 0x9e37_79b9);
    let complexity_budget = complexity_budget.max(64);
    let budget_fast = fast || complexity_budget <= 220;
    let strategy = if cpu_strategy_cost(strategy) > complexity_budget {
        pick_cpu_strategy_for_budget(&mut rng, complexity_budget)
    } else {
        strategy
    };
    match strategy {
        CpuStrategy::ReactionDiffusion => {
            render_reaction_diffusion_into(width, height, &mut rng, budget_fast, out, scratch)
        }
        CpuStrategy::ReactionLattice => {
            render_reaction_lattice_into(width, height, &mut rng, budget_fast, out, scratch)
        }
        _ => {
            let generated =
                render_cpu_strategy_alloc(strategy, width, height, &mut rng, budget_fast);
            out.copy_from_slice(&generated);
        }
    }
}

fn render_cpu_strategy_alloc(
    strategy: CpuStrategy,
    width: u32,
    height: u32,
    rng: &mut XorShift32,
    fast: bool,
) -> Vec<f32> {
    match strategy {
        CpuStrategy::EdgeSobel => render_edge_field(width, height, rng, true),
        CpuStrategy::EdgeLaplacian => render_edge_field(width, height, rng, false),
        CpuStrategy::Maze => render_maze_field(width, height, rng),
        CpuStrategy::ReactionDiffusion => render_reaction_diffusion(width, height, rng, fast),
        CpuStrategy::LSystem => render_lsystem(width, height, rng),
        CpuStrategy::ProceduralNoise => render_noise_field(width, height, rng),
        CpuStrategy::CellularAutomata => render_cellular_automata(width, height, rng, fast),
        CpuStrategy::ParticleFlow => render_particle_flow(width, height, rng, fast),
        CpuStrategy::Voronoi => render_voronoi(width, height, rng),
        CpuStrategy::Delaunay => render_delaunay(width, height, rng),
        CpuStrategy::IteratedFractal => render_iterated_fractal(width, height, rng, fast),
        CpuStrategy::StrangeAttractor => render_strange_attractor(width, height, rng, fast),
        CpuStrategy::RadialWave => render_radial_wave(width, height, rng),
        CpuStrategy::RecursiveFold => render_recursive_fold(width, height, rng, fast),
        CpuStrategy::AttractorHybrid => render_attractor_hybrid(width, height, rng, fast),
        CpuStrategy::CannyEdge => render_canny_edge(width, height, rng, fast),
        CpuStrategy::PerlinRidge => render_perlin_ridge(width, height, rng),
        CpuStrategy::PlasmaField => render_plasma_field(width, height, rng),
        CpuStrategy::SierpinskiCarpet => render_sierpinski_carpet(width, height, rng),
        CpuStrategy::BarnsleyFern => render_barnsley_fern(width, height, rng),
        CpuStrategy::TurbulentFlow => render_turbulent_flow(width, height, rng, fast),
        CpuStrategy::MandelbrotField => render_mandelbrot_field(width, height, rng),
        CpuStrategy::RecursiveTiling => render_recursive_tiling(width, height, rng),
        CpuStrategy::TuringCascade => render_turing_cascade(width, height, rng),
        CpuStrategy::FlowFilaments => render_flow_filaments(width, height, rng),
        CpuStrategy::OrbitalAtlas => render_orbital_atlas(width, height, rng),
        CpuStrategy::CrystalGrowth => render_crystal_growth(width, height, rng),
        CpuStrategy::VortexConvection => render_vortex_convection(width, height, rng),
        CpuStrategy::ErosionChannels => render_erosion_channels(width, height, rng),
        CpuStrategy::StochasticIFS => render_stochastic_ifs(width, height, rng),
        CpuStrategy::LorenzAttractor => render_lorenz_attractor(width, height, rng),
        CpuStrategy::RecursiveStarburst => render_recursive_starburst(width, height, rng),
        CpuStrategy::LissajousOrbits => render_lissajous_orbits(width, height, rng),
        CpuStrategy::GraviticWeb => render_gravitic_web(width, height, rng),
        CpuStrategy::PoissonMesh => render_poisson_mesh(width, height, rng),
        CpuStrategy::MetaballField => render_metaball_field(width, height, rng),
        CpuStrategy::BraidFlow => render_braid_flow(width, height, rng),
        CpuStrategy::LogisticChaos => render_logistic_chaos(width, height, rng),
        CpuStrategy::DeJongAttractor => render_de_jong_attractor(width, height, rng),
        CpuStrategy::RecursiveRibbon => render_recursive_ribbon(width, height, rng),
        CpuStrategy::MagneticFieldlines => render_magnetic_fieldlines(width, height, rng),
        CpuStrategy::InterferenceWaves => render_interference_waves(width, height, rng),
        CpuStrategy::CliffordAttractor => render_clifford_attractor(width, height, rng),
        CpuStrategy::JuliaSet => render_julia_set(width, height, rng),
        CpuStrategy::KochSnowflake => render_koch_snowflake(width, height, rng),
        CpuStrategy::BifurcationTree => render_bifurcation_tree(width, height, rng),
        CpuStrategy::DepthRelief => render_depth_relief(width, height, rng),
        CpuStrategy::AttractorTunnel => render_attractor_tunnel(width, height, rng),
        CpuStrategy::OrbitalLabyrinth => render_orbital_labyrinth(width, height, rng),
        CpuStrategy::PhaseField => render_phase_field(width, height, rng, fast),
        CpuStrategy::Lenia => render_lenia(width, height, rng, fast),
        CpuStrategy::CurlNoiseFlow => render_curl_noise_flow(width, height, rng, fast),
        CpuStrategy::ReactionLattice => render_reaction_lattice(width, height, rng, fast),
        CpuStrategy::HarmonicInterference => render_harmonic_interference(width, height, rng),
        CpuStrategy::AttractorVoronoiHybrid => {
            render_attractor_voronoi_hybrid(width, height, rng, fast)
        }
        CpuStrategy::RecursiveNoiseTerrain => {
            render_recursive_noise_terrain(width, height, rng, fast)
        }
        CpuStrategy::BifurcationGrid => render_bifurcation_grid(width, height, rng),
    }
}

/// Render a CPU strategy for a layer into a caller-provided output slice.
///
/// This is the allocation-free API for callers that already own per-layer
/// buffers. The `scratch` workspace should be reused across calls.
#[allow(dead_code)]
pub fn render_cpu_strategy(
    strategy: CpuStrategy,
    width: u32,
    height: u32,
    seed: u32,
    fast: bool,
    complexity_budget: u32,
    out: &mut [f32],
    scratch: &mut StrategyScratch,
) {
    render_cpu_strategy_into(
        strategy,
        width,
        height,
        seed,
        fast,
        complexity_budget,
        out,
        scratch,
    );
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

pub fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
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

pub fn normalize(src: &mut [f32]) {
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
    resize_nearest_into(src, src_w, src_h, dst_w, dst_h, &mut out);
    out
}

fn resize_bilinear(src: &[f32], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<f32> {
    let mut out = vec![0.0f32; (dst_w * dst_h) as usize];
    resize_bilinear_into(src, src_w, src_h, dst_w, dst_h, &mut out);
    out
}

fn resize_nearest_into(
    src: &[f32],
    src_w: u32,
    src_h: u32,
    dst_w: u32,
    dst_h: u32,
    out: &mut [f32],
) {
    debug_assert_eq!(out.len(), (dst_w * dst_h) as usize);
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
}

fn resize_bilinear_into(
    src: &[f32],
    src_w: u32,
    src_h: u32,
    dst_w: u32,
    dst_h: u32,
    out: &mut [f32],
) {
    debug_assert_eq!(out.len(), (dst_w * dst_h) as usize);
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

#[allow(clippy::too_many_arguments)]
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

/// Uniform spatial bins for local-neighborhood point queries in mesh-like generators.
#[derive(Debug)]
struct MeshSpatialBins {
    bins: Vec<Vec<usize>>,
    cols: usize,
    rows: usize,
    cell_size: f32,
}

/// Build spatial bins for points over a bounded simulation domain.
fn build_mesh_spatial_bins(
    points: &[(f32, f32)],
    width: u32,
    height: u32,
    cell_size: f32,
) -> MeshSpatialBins {
    let cell_size = cell_size.max(1.0);
    let cols = ((width as f32 / cell_size).ceil() as usize).max(1);
    let rows = ((height as f32 / cell_size).ceil() as usize).max(1);
    let mut bins = vec![Vec::<usize>::new(); cols * rows];

    for (idx, &(x, y)) in points.iter().enumerate() {
        let cx = mesh_bin_coord(x, cell_size, cols);
        let cy = mesh_bin_coord(y, cell_size, rows);
        bins[cy * cols + cx].push(idx);
    }

    MeshSpatialBins {
        bins,
        cols,
        rows,
        cell_size,
    }
}

/// Convert a point coordinate to a clamped bin index.
fn mesh_bin_coord(value: f32, cell_size: f32, max_bins: usize) -> usize {
    let max_index = max_bins.saturating_sub(1) as i32;
    ((value / cell_size).floor() as i32).clamp(0, max_index) as usize
}

/// Insert one candidate into a sorted bounded nearest-neighbor set.
fn push_nearest_candidate(
    best: &mut Vec<(f32, usize)>,
    max_neighbors: usize,
    dist2: f32,
    idx: usize,
) {
    if best.len() < max_neighbors {
        best.push((dist2, idx));
        let mut pos = best.len() - 1;
        while pos > 0 && best[pos].0 < best[pos - 1].0 {
            best.swap(pos, pos - 1);
            pos -= 1;
        }
        return;
    }

    if dist2 >= best[max_neighbors - 1].0 {
        return;
    }

    best[max_neighbors - 1] = (dist2, idx);
    let mut pos = max_neighbors - 1;
    while pos > 0 && best[pos].0 < best[pos - 1].0 {
        best.swap(pos, pos - 1);
        pos -= 1;
    }
}

/// Collect nearest neighbors using expanding bin rings and bounded k-selection.
fn collect_mesh_neighbors(
    points: &[(f32, f32)],
    bins: &MeshSpatialBins,
    point_idx: usize,
    max_neighbors: usize,
    best: &mut Vec<(f32, usize)>,
) {
    best.clear();
    if points.len() <= 1 || max_neighbors == 0 {
        return;
    }

    let (sx, sy) = points[point_idx];
    let center_x = mesh_bin_coord(sx, bins.cell_size, bins.cols) as i32;
    let center_y = mesh_bin_coord(sy, bins.cell_size, bins.rows) as i32;
    let max_radius = bins.cols.max(bins.rows) as i32;
    let target_neighbors = max_neighbors.min(points.len().saturating_sub(1));
    let mut radius = 0i32;

    while radius <= max_radius {
        let min_x = (center_x - radius).max(0);
        let max_x = (center_x + radius).min(bins.cols as i32 - 1);
        let min_y = (center_y - radius).max(0);
        let max_y = (center_y + radius).min(bins.rows as i32 - 1);

        let mut by = min_y;
        while by <= max_y {
            let mut bx = min_x;
            while bx <= max_x {
                if radius == 0 || bx == min_x || bx == max_x || by == min_y || by == max_y {
                    let cell = (by as usize) * bins.cols + (bx as usize);
                    for &other_idx in bins.bins[cell].iter() {
                        if other_idx == point_idx {
                            continue;
                        }
                        let dx = sx - points[other_idx].0;
                        let dy = sy - points[other_idx].1;
                        push_nearest_candidate(
                            best,
                            target_neighbors,
                            dx * dx + dy * dy,
                            other_idx,
                        );
                    }
                }
                bx += 1;
            }
            by += 1;
        }

        if best.len() >= target_neighbors {
            // Points outside this ring are at least roughly one ring-width away.
            let outside_min = (radius as f32 - 1.0).max(0.0) * bins.cell_size;
            if best[target_neighbors - 1].0 <= outside_min * outside_min {
                break;
            }
        }
        radius += 1;
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
    let octaves = 4 + (rng.next_u32() % 3);
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
            if i.is_multiple_of(4) {
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
    let maze_w = (width / 6).clamp(56, 168);
    let maze_h = (height / 6).clamp(56, 168);
    let maze_grid_w = (maze_w | 1) as usize;
    let maze_grid_h = (maze_h | 1) as usize;
    let cell_w = maze_grid_w / 2;
    let cell_h = maze_grid_h / 2;

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
    let mut out = vec![0.0f32; (width * height) as usize];
    let mut scratch = StrategyScratch::default();
    render_reaction_diffusion_into(width, height, rng, fast, &mut out, &mut scratch);
    out
}

fn render_reaction_diffusion_into(
    width: u32,
    height: u32,
    rng: &mut XorShift32,
    fast: bool,
    out: &mut [f32],
    scratch: &mut StrategyScratch,
) {
    debug_assert_eq!(out.len(), (width * height) as usize);
    let sim_w = (width / 2).max(128);
    let sim_h = (height / 2).max(128);
    let iterations: usize = if fast { 420 } else { 900 };
    let sim_len = (sim_w * sim_h) as usize;
    ensure_len_with_fill(&mut scratch.reaction_u, sim_len, 1.0);
    ensure_len_with_fill(&mut scratch.reaction_v, sim_len, 0.0);
    ensure_len_with_fill(&mut scratch.reaction_next_u, sim_len, 0.0);
    ensure_len_with_fill(&mut scratch.reaction_next_v, sim_len, 0.0);
    let (mut u, mut v, mut un, mut vn) = (
        &mut scratch.reaction_u,
        &mut scratch.reaction_v,
        &mut scratch.reaction_next_u,
        &mut scratch.reaction_next_v,
    );

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
        un.par_chunks_mut(w)
            .zip(vn.par_chunks_mut(w))
            .enumerate()
            .for_each(|(y, (row_u, row_v))| {
                if y == 0 || y + 1 >= h {
                    return;
                }
                let row = y * w;
                for x in 1..w - 1 {
                    let idx = row + x;
                    let u0 = u[idx];
                    let v0 = v[idx];
                    let lap_u = u[idx - 1] + u[idx + 1] + u[idx - w] + u[idx + w] - 4.0 * u0;
                    let lap_v = v[idx - 1] + v[idx + 1] + v[idx - w] + v[idx + w] - 4.0 * v0;
                    let uvv = u0 * v0 * v0;
                    row_u[x] = (u0 + (du * lap_u - uvv + f * (1.0 - u0)) * dt).clamp(0.0, 1.0);
                    row_v[x] = (v0 + (dv * lap_v + uvv - (f + k) * v0) * dt).clamp(0.0, 1.0);
                }
            });
        std::mem::swap(&mut u, &mut un);
        std::mem::swap(&mut v, &mut vn);
        step += 1;
    }

    normalize(v);
    resize_nearest_into(v, sim_w, sim_h, width, height, out);
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
    let bands = 2 + (rng.next_u32() % 4);
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
            *value /= max_diff;
        }
    }
    normalize(&mut out);
    resize_bilinear(&out, sim_w, sim_h, width, height)
}

fn render_delaunay(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim_w = (width / 2).max(128);
    let sim_h = (height / 2).max(128);
    let point_count = 10 + (rng.next_u32() % 20) as usize;
    let mut points: Vec<(f32, f32)> = Vec::with_capacity(point_count);
    for _ in 0..point_count {
        points.push((
            (rng.next_u32() % sim_w) as f32,
            (rng.next_u32() % sim_h) as f32,
        ));
    }
    let area = (sim_w as f32) * (sim_h as f32);
    let average_spacing = (area / points.len().max(1) as f32).sqrt();
    let bins = build_mesh_spatial_bins(&points, sim_w, sim_h, average_spacing.max(8.0));
    let mut nearest = Vec::<(f32, usize)>::with_capacity(4);
    let mut out = vec![0.05f32; (sim_w * sim_h) as usize];
    let mut i = 0usize;
    while i < points.len() {
        let (x0, y0) = points[i];
        collect_mesh_neighbors(&points, &bins, i, 4, &mut nearest);
        for &(_, neighbor_idx) in nearest.iter() {
            let (sx, sy) = points[neighbor_idx];
            draw_line(
                x0.round() as i32,
                y0.round() as i32,
                sx.round() as i32,
                sy.round() as i32,
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
            if step.is_multiple_of(3) {
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
                0.9 + if step.is_multiple_of(3) { 0.2 } else { 0.0 },
                &mut density,
            );
            if step.is_multiple_of(10) {
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

            if i.is_multiple_of(4) {
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

        if i.is_multiple_of(2) {
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

    let strands = ((sim_w * sim_h) / 56).max(1_600);
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

            if step.is_multiple_of(14) {
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

            let jitter = value_noise(ux * 0.11, uy * 0.15, seed ^ i) * 0.36;
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
            if i.is_multiple_of(3) {
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

            if step.is_multiple_of(2) {
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
            if step.is_multiple_of(4) {
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
        x += dx * dt;
        y += dy * dt;
        z += dz * dt;

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
            if segment.is_multiple_of(2) {
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
            } else if segment.is_multiple_of(4) && branch_remaining > 0 && rng.next_f32() < 0.35 {
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
            if i.is_multiple_of(2) {
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
            if step.is_multiple_of(3) {
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

            if step > 1 && branch.depth > 1 && step.is_multiple_of(4) {
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
            value += ridge * 0.35;
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
                if i.is_multiple_of(4) {
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

fn render_lissajous_orbits(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim_w = (width / 2).max(200);
    let sim_h = (height / 2).max(200);
    let mut density = vec![0.0f32; (sim_w * sim_h) as usize];
    let orbits = 4 + (rng.next_u32() % 8);
    let mut orbit = 0u32;
    let seed = rng.next_u32();

    while orbit < orbits {
        let center_x = sim_w as f32 * (0.18 + rng.next_f32() * 0.64);
        let center_y = sim_h as f32 * (0.18 + rng.next_f32() * 0.64);
        let scale_x = sim_w as f32 * (0.10 + rng.next_f32() * 0.40);
        let scale_y = sim_h as f32 * (0.10 + rng.next_f32() * 0.40);
        let freq_x = 2.0 + rng.next_f32() * 11.0;
        let freq_y = 2.0 + rng.next_f32() * 11.0;
        let phase = rng.next_f32() * TAU;
        let modulation = (rng.next_f32() - 0.5) * 0.45;
        let turns = 900 + (rng.next_u32() % 1_400);
        let strength = 0.22 + rng.next_f32() * 0.46;
        let mut prev_x = center_x;
        let mut prev_y = center_y;
        let mut t = 0u32;

        while t < turns {
            let tt = t as f32 / turns as f32;
            let ring = (tt * TAU * (1.8 + rng.next_f32() * 1.4)).fract() * 0.5 + 0.5;
            let amp = 0.28 + 1.1 * tt;
            let cx = value_noise(phase, tt * 4.3, seed ^ t) - 0.5;
            let cy = value_noise(tt * 3.7, phase, seed ^ (t + 11)) - 0.5;
            let nx = center_x
                + (freq_x * tt * TAU + phase + modulation * ring).cos()
                    * scale_x
                    * amp
                    * (0.55 + cx * 0.3)
                + cx * scale_x * 0.08;
            let ny = center_y
                + (freq_y * tt * TAU + phase * 0.77 + cx).sin()
                    * scale_y
                    * amp
                    * (0.55 + cy * 0.28)
                + cy * scale_y * 0.09;
            let x = nx.clamp(0.0, sim_w as f32 - 1.0);
            let y = ny.clamp(0.0, sim_h as f32 - 1.0);
            let wave = value_noise(nx * 0.01, ny * 0.01, seed ^ (t + orbit)) * 1.8 - 0.9;
            let px = (x + wave * 0.9 * amp).clamp(0.0, sim_w as f32 - 1.0);
            let py = (y - wave * 0.9 * amp).clamp(0.0, sim_h as f32 - 1.0);
            let local_strength = strength * (0.5 + tt * 0.5 + wave.abs() * 0.2);
            draw_line(
                prev_x.round() as i32,
                prev_y.round() as i32,
                px.round() as i32,
                py.round() as i32,
                sim_w as usize,
                sim_h as usize,
                local_strength,
                &mut density,
            );
            if t.is_multiple_of(8) {
                draw_point(
                    px.round() as i32,
                    py.round() as i32,
                    sim_w as usize,
                    sim_h as usize,
                    1,
                    local_strength * 0.75,
                    &mut density,
                );
            }
            if t.is_multiple_of(120) && rng.next_f32() < 0.7 {
                let branch_freq_x = 2.0 + rng.next_f32() * 8.0;
                let branch_freq_y = 2.0 + rng.next_f32() * 8.0;
                let branch_steps = 90 + (rng.next_u32() % 180);
                let mut inner_t = 0u32;
                let base_x = px;
                let base_y = py;
                while inner_t < branch_steps {
                    let tt = inner_t as f32 / branch_steps as f32;
                    let bx = base_x
                        + (branch_freq_x * tt * TAU).sin() * amp * scale_x * 0.08
                        + value_noise(branch_freq_x, tt * 3.0, seed ^ t ^ (inner_t << 2))
                            * scale_x
                            * 0.04;
                    let by = base_y
                        + (branch_freq_y * tt * TAU).cos() * amp * scale_y * 0.08
                        + value_noise(tt * 2.3, branch_freq_y, seed ^ (t + 1 + inner_t))
                            * scale_y
                            * 0.04;
                    draw_point(
                        bx.round() as i32,
                        by.round() as i32,
                        sim_w as usize,
                        sim_h as usize,
                        0,
                        local_strength * 0.42,
                        &mut density,
                    );
                    inner_t += 1;
                }
            }
            prev_x = px;
            prev_y = py;
            t += 1;
        }

        orbit += 1;
    }

    normalize(&mut density);
    let mut out = resize_bilinear(&density, sim_w, sim_h, width, height);
    for value in out.iter_mut() {
        *value = clamp01(value.powf(0.82));
    }
    normalize(&mut out);
    out
}

fn render_gravitic_web(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim_w = (width / 2).max(180);
    let sim_h = (height / 2).max(180);
    let count = 48 + (rng.next_u32() % 36) as usize;
    let steps = 220 + (rng.next_u32() % 320);
    let mut x = vec![0.0f32; count];
    let mut y = vec![0.0f32; count];
    let mut vx = vec![0.0f32; count];
    let mut vy = vec![0.0f32; count];
    let mut mass = vec![0.0f32; count];
    let mut density = vec![0.0f32; (sim_w * sim_h) as usize];
    let mut i = 0usize;

    while i < count {
        x[i] = rng.next_f32() * (sim_w as f32 - 1.0);
        y[i] = rng.next_f32() * (sim_h as f32 - 1.0);
        vx[i] = (rng.next_f32() - 0.5) * 0.3;
        vy[i] = (rng.next_f32() - 0.5) * 0.3;
        mass[i] = 0.55 + rng.next_f32() * 1.5;
        i += 1;
    }

    let center_x = sim_w as f32 * 0.5;
    let center_y = sim_h as f32 * 0.5;
    let world_scale = sim_w.max(sim_h) as f32;
    let softness = 0.000_8 * world_scale * world_scale;
    let gravity = 0.014 + rng.next_f32() * 0.018;
    let damping = 0.93 + rng.next_f32() * 0.03;
    let mut step = 0u32;

    while step < steps {
        let mut j = 0usize;
        while j < count {
            let mut fx = 0.0f32;
            let mut fy = 0.0f32;
            let mut k = 0usize;
            while k < count {
                if j != k {
                    let dx = x[k] - x[j];
                    let dy = y[k] - y[j];
                    let dist2 = dx * dx + dy * dy + softness;
                    let inv = mass[k] / (dist2.sqrt() * dist2).max(1e-6);
                    fx += dx * inv;
                    fy += dy * inv;
                }
                k += 1;
            }

            let cx = (center_x - x[j]) * 0.000_05;
            let cy = (center_y - y[j]) * 0.000_05;
            let fx = (fx + cx) * gravity * mass[j];
            let fy = (fy + cy) * gravity * mass[j];
            vx[j] = (vx[j] + fx).clamp(-2.0, 2.0) * damping;
            vy[j] = (vy[j] + fy).clamp(-2.0, 2.0) * damping;

            let nx = (x[j] + vx[j]).rem_euclid(sim_w as f32 - 1.0);
            let ny = (y[j] + vy[j]).rem_euclid(sim_h as f32 - 1.0);
            let strength = 0.3
                + (mass[j] * 0.11)
                + (0.25
                    * value_noise(x[j] * 0.01, y[j] * 0.01, rng.next_u32() ^ (step + j as u32)));
            draw_line(
                x[j].round() as i32,
                y[j].round() as i32,
                nx.round() as i32,
                ny.round() as i32,
                sim_w as usize,
                sim_h as usize,
                clamp01(strength),
                &mut density,
            );
            if step.is_multiple_of(4) {
                draw_point(
                    nx.round() as i32,
                    ny.round() as i32,
                    sim_w as usize,
                    sim_h as usize,
                    1,
                    strength * 0.55,
                    &mut density,
                );
            }
            if step.is_multiple_of(18) && step.is_multiple_of(2) {
                let nx2 = (nx + value_noise(x[j], y[j], rng.next_u32()) - 0.5) % (sim_w as f32);
                let ny2 = (ny + value_noise(y[j], x[j], rng.next_u32()) - 0.5) % (sim_h as f32);
                let link = ((j + 7) % count) as f32 / (count as f32).max(1.0);
                let blend = 1.0 - (link + (step as f32 / steps as f32)).abs();
                draw_line(
                    nx.round() as i32,
                    ny.round() as i32,
                    nx2 as i32,
                    ny2 as i32,
                    sim_w as usize,
                    sim_h as usize,
                    clamp01(blend * 0.34),
                    &mut density,
                );
            }

            x[j] = nx;
            y[j] = ny;
            j += 1;
        }
        step += 1;
    }

    let mut out = resize_bilinear(&density, sim_w, sim_h, width, height);
    for value in out.iter_mut() {
        *value = clamp01(value.powf(0.9));
    }
    normalize(&mut out);
    out
}

fn render_poisson_mesh(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim_w = (width / 2).max(180);
    let sim_h = (height / 2).max(180);
    let mut density = vec![0.0f32; (sim_w * sim_h) as usize];
    let mut points = Vec::<(f32, f32)>::new();
    let target = 220 + (rng.next_u32() % 260) as usize;
    let min_dist = (sim_w.min(sim_h) as f32) * (0.02 + rng.next_f32() * 0.018);
    let min_dist2 = min_dist * min_dist;
    let mut attempts = 0usize;

    while points.len() < target && attempts < target * 90 {
        let px = rng.next_f32() * (sim_w as f32 - 1.0);
        let py = rng.next_f32() * (sim_h as f32 - 1.0);
        let mut accepted = true;
        for &(qx, qy) in points.iter() {
            let dx = px - qx;
            let dy = py - qy;
            if dx * dx + dy * dy < min_dist2 {
                accepted = false;
                break;
            }
        }
        if accepted {
            points.push((px, py));
        }
        attempts += 1;
    }

    if points.len() < 16 {
        let fallback = 16usize;
        points.extend((0..fallback).map(|_| {
            let px = rng.next_f32() * (sim_w as f32 - 1.0);
            let py = rng.next_f32() * (sim_h as f32 - 1.0);
            (px, py)
        }));
    }

    let bins = build_mesh_spatial_bins(&points, sim_w, sim_h, min_dist.max(4.0));
    let mut nearest = Vec::<(f32, usize)>::with_capacity(4);
    let mut idx = 0usize;
    while idx < points.len() {
        let (sx, sy) = points[idx];
        draw_point(
            sx.round() as i32,
            sy.round() as i32,
            sim_w as usize,
            sim_h as usize,
            0,
            0.95,
            &mut density,
        );
        collect_mesh_neighbors(&points, &bins, idx, 4, &mut nearest);
        let max_links = 4.min(nearest.len());
        let mut l = 0usize;
        while l < max_links {
            let (_, ni) = nearest[l];
            let (tx, ty) = points[ni];
            let wave = value_noise(sx * 0.012, sy * 0.012, ni as u32 ^ (idx as u32)) * 0.5 + 0.5;
            draw_line(
                sx.round() as i32,
                sy.round() as i32,
                tx.round() as i32,
                ty.round() as i32,
                sim_w as usize,
                sim_h as usize,
                0.36 + wave * 0.34,
                &mut density,
            );
            if l == 0 || (l + 1 == max_links && rng.next_f32() < 0.35) {
                let mid_x = ((sx + tx) * 0.5).round() as i32;
                let mid_y = ((sy + ty) * 0.5).round() as i32;
                draw_point(
                    mid_x,
                    mid_y,
                    sim_w as usize,
                    sim_h as usize,
                    1,
                    0.55 + wave * 0.25,
                    &mut density,
                );
            }
            l += 1;
        }
        idx += 1;
    }

    let grain = noise_field(sim_w, sim_h, rng, 2);
    let mut i = 0usize;
    while i < density.len() {
        density[i] = clamp01(
            density[i] * 0.94
                + grain[i] * 0.06
                + value_noise(
                    (i % sim_w as usize) as f32 * 0.01,
                    (i / sim_w as usize) as f32 * 0.01,
                    0xDEAD,
                ) * 0.05,
        );
        i += 1;
    }

    let mut out = resize_bilinear(&density, sim_w, sim_h, width, height);
    for value in out.iter_mut() {
        let wave = value_noise(*value * 10.0, *value * 12.0, 0xBEEF) * 0.17;
        *value = clamp01(value.powf(0.84) + wave - 0.06);
    }
    normalize(&mut out);
    out
}

#[derive(Clone, Copy)]
struct Metaball {
    x: f32,
    y: f32,
    radius: f32,
    strength: f32,
}

fn render_metaball_field(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim_w = (width / 3).clamp(180, 512);
    let sim_h = (height / 3).clamp(180, 512);
    let seed = rng.next_u32();
    let count = 22 + (seed % 34) as usize;
    let base_scale = sim_w.min(sim_h) as f32;
    let mut balls = Vec::with_capacity(count);

    let mut i = 0usize;
    while i < count {
        let radius = base_scale * (0.008 + rng.next_f32() * 0.022);
        balls.push(Metaball {
            x: rng.next_f32() * (sim_w as f32 - 1.0),
            y: rng.next_f32() * (sim_h as f32 - 1.0),
            radius,
            strength: 0.45 + rng.next_f32() * 1.25,
        });
        i += 1;
    }

    let mut density = vec![0.0f32; (sim_w * sim_h) as usize];
    let mut y = 0u32;
    while y < sim_h {
        let mut x = 0u32;
        while x < sim_w {
            let wx = x as f32 + (value_noise(x as f32 * 0.012, y as f32 * 0.011, seed) - 0.5) * 6.0;
            let wy = y as f32
                + (value_noise(y as f32 * 0.009, x as f32 * 0.010, seed ^ 0xAA) - 0.5) * 6.0;
            let mut field = 0.0f32;
            for ball in balls.iter() {
                let dx = wx - ball.x;
                let dy = wy - ball.y;
                let dist2 = dx * dx + dy * dy;
                let denom = dist2 + ball.radius * ball.radius;
                field += (ball.strength * ball.radius * ball.radius) / denom;
            }
            let motion = value_noise(wx * 0.004, wy * 0.005, seed ^ (x * 17)) - 0.5;
            let idx = (y * sim_w + x) as usize;
            density[idx] = (field * (0.7 + motion)).clamp(0.0, 1.0);
            x += 1;
        }
        y += 1;
    }

    let mut out = resize_bilinear(&density, sim_w, sim_h, width, height);
    for value in out.iter_mut() {
        let v = *value * 0.95;
        *value = clamp01(v.powf(0.8) + value_noise(*value * 12.0, *value * 10.0, seed) * 0.18);
    }
    normalize(&mut out);
    out
}

#[derive(Clone, Copy)]
struct BraidSeed {
    x: f32,
    y: f32,
    angle: f32,
    velocity: f32,
    remaining: u32,
    depth: u32,
    seed: u32,
}

fn render_braid_flow(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim_w = (width / 3).clamp(180, 520);
    let sim_h = (height / 3).clamp(180, 520);
    let mut out = vec![0.0f32; (sim_w * sim_h) as usize];
    let mut active = Vec::with_capacity((8 + (rng.next_u32() % 12)) as usize);
    let count = 10 + (rng.next_u32() % 14);

    let mut i = 0u32;
    while i < count {
        active.push(BraidSeed {
            x: rng.next_f32() * (sim_w as f32 - 1.0),
            y: rng.next_f32() * (sim_h as f32 - 1.0),
            angle: rng.next_f32() * TAU,
            velocity: 0.55 + rng.next_f32() * 1.45,
            remaining: 170 + (rng.next_u32() % 250),
            depth: 2 + (rng.next_u32() % 3),
            seed: rng.next_u32(),
        });
        i += 1;
    }

    while let Some(thread) = active.pop() {
        if thread.remaining < 2 || thread.depth == 0 {
            continue;
        }

        let mut x = thread.x;
        let mut y = thread.y;
        let mut angle = thread.angle;
        let mut step = 0u32;
        let width_scale = sim_w as f32;
        let height_scale = sim_h as f32;
        while step < thread.remaining {
            let noise = value_noise(
                x * 0.009 + thread.seed as f32 * 0.001,
                y * 0.011 + (thread.seed >> 3) as f32 * 0.001,
                thread.seed ^ step,
            );
            let curl = noise - 0.5;
            angle += curl * 1.85
                - (value_noise(thread.seed as f32 * 0.01, x * 0.007, thread.seed ^ 0x1234) - 0.5)
                    * 0.95;

            let mut speed =
                thread.velocity * (0.72 + 0.8 * (1.0 - (step as f32 / thread.remaining as f32)));
            speed = speed.clamp(0.18, 2.8);

            let nx = (x + angle.cos() * speed).rem_euclid(width_scale - 1.0);
            let ny = (y + angle.sin() * speed).rem_euclid(height_scale - 1.0);

            let t = (1.0 - (step as f32 / thread.remaining as f32)).powf(0.7);
            let strength = (0.38 + 0.12 * thread.depth as f32 + 0.28 * t).clamp(0.3, 1.0);
            draw_line(
                x.round() as i32,
                y.round() as i32,
                nx.round() as i32,
                ny.round() as i32,
                sim_w as usize,
                sim_h as usize,
                strength,
                &mut out,
            );
            if step.is_multiple_of(9) {
                draw_point(
                    nx.round() as i32,
                    ny.round() as i32,
                    sim_w as usize,
                    sim_h as usize,
                    1,
                    strength * 0.75,
                    &mut out,
                );
            }

            if step > 22
                && thread.depth > 1
                && (step.is_multiple_of(17)
                    || value_noise(nx * 0.02, ny * 0.018, thread.seed ^ step) > 0.87)
            {
                let branch_angle = angle
                    + (value_noise(nx * 0.009, ny * 0.008, thread.seed ^ step ^ 0xBEEF) - 0.5)
                        * 1.9;
                let branch_velocity =
                    speed * (0.45 + 0.35 * value_noise(nx * 0.013, ny * 0.012, step));
                active.push(BraidSeed {
                    x: nx,
                    y: ny,
                    angle: branch_angle,
                    velocity: branch_velocity,
                    remaining: (thread.remaining - step) / 2 + 40,
                    depth: thread.depth - 1,
                    seed: thread.seed ^ (step << 3) ^ 0x4A55,
                });
            }

            x = nx;
            y = ny;
            step += 1;
        }
    }

    let grain = noise_field(sim_w, sim_h, rng, 2);
    let mut i = 0usize;
    while i < out.len() {
        out[i] = clamp01(out[i] * 0.92 + grain[i] * 0.08);
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

fn render_phase_field(width: u32, height: u32, rng: &mut XorShift32, fast: bool) -> Vec<f32> {
    let sim_w = (width / 2).clamp(150, 520);
    let sim_h = (height / 2).clamp(150, 520);
    let seed = rng.next_u32();
    let mut field = noise_field(sim_w, sim_h, rng, if fast { 2 } else { 3 });
    let mut next = vec![0.0f32; (sim_w * sim_h) as usize];
    let iterations = if fast { 6 } else { 12 };
    let dt = 0.28 + (seed % 11) as f32 * 0.02;
    let w = sim_w as i32;
    let h = sim_h as i32;

    let mut step = 0u32;
    while step < iterations {
        let mut y = 1i32;
        while y < h - 1 {
            let mut x = 1i32;
            while x < w - 1 {
                let idx = (y as usize) * w as usize + x as usize;
                let c = field[idx];
                let phase = c * TAU;
                let n0 = field[(y as usize - 1) * w as usize + x as usize];
                let n1 = field[(y as usize + 1) * w as usize + x as usize];
                let n2 = field[y as usize * w as usize + (x as usize - 1)];
                let n3 = field[y as usize * w as usize + (x as usize + 1)];
                let laplace = (n0 + n1 + n2 + n3) * 0.25 - c;
                let push = value_noise(
                    x as f32 * 0.013,
                    y as f32 * 0.011,
                    seed.wrapping_add(step.wrapping_mul(0x9e37_79b9)),
                );
                let wave = (phase.sin() + (phase * 1.7).cos()) * 0.15;
                let coupler = (push - 0.5) * 0.3;
                next[idx] = clamp01(c + (laplace * 0.18 + wave * 0.25 + coupler) * dt);
                x += 1;
            }
            y += 1;
        }
        std::mem::swap(&mut field, &mut next);
        step += 1;
    }

    let mut out = resize_bilinear(&field, sim_w, sim_h, width, height);
    for value in out.iter_mut() {
        let warp = value_noise(*value * 10.0, *value * 7.0, seed ^ 0xA1CE) - 0.5;
        *value = clamp01(*value * 0.8 + warp * 0.08);
    }
    normalize(&mut out);
    out
}

fn render_lenia(width: u32, height: u32, rng: &mut XorShift32, fast: bool) -> Vec<f32> {
    let sim_w = (width / 3).clamp(140, 520);
    let sim_h = (height / 3).clamp(140, 520);
    let mut current = vec![0.0f32; (sim_w * sim_h) as usize];
    let mut next = vec![0.0f32; (sim_w * sim_h) as usize];
    let mut seed_rng = XorShift32::new(rng.next_u32() ^ 0x2b2b_0001);
    let mut i = 0usize;
    while i < current.len() {
        current[i] = if seed_rng.next_f32() < 0.12 { 1.0 } else { 0.0 };
        i += 1;
    }

    let kernel = [0.05f32, 0.2f32, 0.4f32, 0.2f32, 0.05f32];
    let iterations = if fast { 14 } else { 22 };
    let radius = 2i32;
    let growth_center = 0.35 + (seed_rng.next_f32() * 0.12);
    let width_i = sim_w as i32;
    let height_i = sim_h as i32;

    let mut step = 0u32;
    while step < iterations {
        let mut y = radius;
        while y < height_i - radius {
            let mut x = radius;
            while x < width_i - radius {
                let mut n = 0.0f32;
                let mut ky = 0i32;
                while ky <= radius * 2 {
                    let mut kx = 0i32;
                    while kx <= radius * 2 {
                        let idx = ((y + ky - radius) as usize) * sim_w as usize
                            + (x + kx - radius) as usize;
                        let weight = kernel[(kx as usize).min(4)] * kernel[(ky as usize).min(4)];
                        n += current[idx] * weight;
                        kx += 1;
                    }
                    ky += 1;
                }

                let diff = n - growth_center;
                let sigma = 0.06 + (seed_rng.next_f32() * 0.04);
                let growth = ((-(diff * diff) / (2.0 * sigma * sigma)).exp() - 0.5) * 0.55;
                let idx = y as usize * sim_w as usize + x as usize;
                next[idx] = clamp01(
                    current[idx]
                        + growth
                            * (0.45 + value_noise(x as f32, y as f32, seed_rng.next_u32()) * 0.28),
                );
                x += 1;
            }
            y += 1;
        }
        std::mem::swap(&mut current, &mut next);
        step += 1;
    }

    let mut out = resize_bilinear(&current, sim_w, sim_h, width, height);
    for value in out.iter_mut() {
        *value = clamp01(value.powf(0.9) + value_noise(*value * 5.0, *value * 8.0, step) * 0.1);
    }
    normalize(&mut out);
    out
}

fn render_curl_noise_flow(width: u32, height: u32, rng: &mut XorShift32, fast: bool) -> Vec<f32> {
    let sim_w = (width / 2).clamp(140, 520);
    let sim_h = (height / 2).clamp(140, 520);
    let mut density = vec![0.0f32; (sim_w * sim_h) as usize];
    let particles = if fast { 900 } else { 1500 };
    let steps = if fast { 66 } else { 130 };
    let scale = 0.0042 + rng.next_f32() * 0.006;
    let seed = rng.next_u32();

    let mut p = 0u32;
    while p < particles {
        let mut x = rng.next_f32() * (sim_w as f32 - 1.0);
        let mut y = rng.next_f32() * (sim_h as f32 - 1.0);
        let mut i = 0u32;
        while i < steps {
            let base = value_noise(x * scale, y * scale, seed ^ p);
            let _nx = value_noise(x * scale + 0.17, y * scale, seed ^ (p + 0xF3F3)) - 0.5;
            let _ny = value_noise(x * scale, y * scale + 0.17, seed ^ (p + 0xA1A1)) - 0.5;
            let dxy = 0.08;
            let vx = value_noise((x + dxy) * scale, y * scale, seed ^ p)
                - value_noise((x - dxy) * scale, y * scale, seed ^ p);
            let vy = value_noise(x * scale, (y + dxy) * scale, seed ^ (p + 1))
                - value_noise(x * scale, (y - dxy) * scale, seed ^ (p + 1));
            let u = -vy * (1.2 + (base * 2.0));
            let v = vx * (1.2 + (base * 2.0));
            let len = (u * u + v * v).sqrt().max(1e-3);
            let speed = 1.1 + (0.7 * rng.next_f32());
            x = (x + u / len * speed + sim_w as f32).rem_euclid(sim_w as f32 - 1.0);
            y = (y + v / len * speed + sim_h as f32).rem_euclid(sim_h as f32 - 1.0);

            let ix = x.round() as usize;
            let iy = y.round() as usize;
            draw_point(
                ix as i32,
                iy as i32,
                sim_w as usize,
                sim_h as usize,
                1,
                0.86,
                &mut density,
            );
            if i.is_multiple_of(3) {
                draw_line(
                    (ix as f32 - u) as i32,
                    (iy as f32 - v) as i32,
                    ix as i32,
                    iy as i32,
                    sim_w as usize,
                    sim_h as usize,
                    0.35,
                    &mut density,
                );
            }
            i += 1;
        }
        p += 1;
    }

    let mut out = resize_nearest(&density, sim_w, sim_h, width, height);
    for value in out.iter_mut() {
        *value = clamp01((*value).powf(0.82));
    }
    normalize(&mut out);
    out
}

fn render_reaction_lattice(width: u32, height: u32, rng: &mut XorShift32, fast: bool) -> Vec<f32> {
    let mut out = vec![0.0f32; (width * height) as usize];
    let mut scratch = StrategyScratch::default();
    render_reaction_lattice_into(width, height, rng, fast, &mut out, &mut scratch);
    out
}

fn render_reaction_lattice_into(
    width: u32,
    height: u32,
    rng: &mut XorShift32,
    fast: bool,
    out: &mut [f32],
    scratch: &mut StrategyScratch,
) {
    debug_assert_eq!(out.len(), (width * height) as usize);
    let sim_w = (width / 2).clamp(130, 500);
    let sim_h = (height / 2).clamp(130, 500);
    let sim_len = (sim_w * sim_h) as usize;
    ensure_len_with_fill(&mut scratch.reaction_u, sim_len, 1.0);
    ensure_len_with_fill(&mut scratch.reaction_v, sim_len, 0.0);
    ensure_len_with_fill(&mut scratch.reaction_next_u, sim_len, 0.0);
    ensure_len_with_fill(&mut scratch.reaction_next_v, sim_len, 0.0);
    let (mut u, mut v, mut next_u, mut next_v) = (
        &mut scratch.reaction_u,
        &mut scratch.reaction_v,
        &mut scratch.reaction_next_u,
        &mut scratch.reaction_next_v,
    );
    let mut seed_rng = XorShift32::new(rng.next_u32() ^ 0xC0DE_F00D);

    let mut i = 0usize;
    while i < v.len() {
        if seed_rng.next_f32() < 0.03 {
            v[i] = 0.9;
            u[i] = 0.1;
        }
        i += 1;
    }

    let iterations = if fast { 420 } else { 760 };
    let du = 0.14;
    let dv = 0.08;
    let dt = 1.0f32;
    let feed = 0.018 + seed_rng.next_f32() * 0.038;
    let kill = 0.046 + seed_rng.next_f32() * 0.038;
    let w = sim_w as usize;
    let h = sim_h as usize;
    let lattice_seed = seed_rng.next_u32();
    ensure_len_with_fill(&mut scratch.reaction_noise, sim_len, 0.0);
    let lattice_noise = &mut scratch.reaction_noise;
    lattice_noise
        .par_chunks_mut(w)
        .enumerate()
        .for_each(|(y, row)| {
            let y_f = y as f32;
            for (x, value) in row.iter_mut().enumerate() {
                *value = 0.5 + 0.4 * value_noise(x as f32 * 0.73, y_f * 0.67, lattice_seed);
            }
        });

    let mut step = 0usize;
    while step < iterations {
        next_u
            .par_chunks_mut(w)
            .zip(next_v.par_chunks_mut(w))
            .enumerate()
            .for_each(|(y, (row_u, row_v))| {
                if y == 0 || y + 1 >= h {
                    return;
                }
                let row = y * w;
                for x in 1..w - 1 {
                    let idx = row + x;
                    let u0 = u[idx];
                    let v0 = v[idx];
                    let lap_u = u[idx - 1] + u[idx + 1] + u[idx - w] + u[idx + w] - 4.0 * u0;
                    let lap_v = v[idx - 1] + v[idx + 1] + v[idx - w] + v[idx + w] - 4.0 * v0;
                    let uvv = u0 * v0 * v0;
                    let uvv2 = lattice_noise[idx];
                    row_u[x] =
                        (u0 + (du * lap_u - uvv + (feed * uvv2) * (1.0 - u0)) * dt).clamp(0.0, 1.0);
                    row_v[x] =
                        (v0 + (dv * lap_v + uvv - (kill + feed) * v0) * dt * uvv2).clamp(0.0, 1.0);
                }
            });
        std::mem::swap(&mut u, &mut next_u);
        std::mem::swap(&mut v, &mut next_v);
        step += 1;
    }

    let tone_seed = seed_rng.next_u32();
    v.par_iter_mut().enumerate().for_each(|(idx, value)| {
        let idx_f = idx as f32;
        let noise = value_noise(*value * 5.0 + idx_f * 0.0007, *value * 7.0, tone_seed);
        *value = clamp01(*value * (1.0 + noise * 0.3));
    });
    normalize(v);
    resize_bilinear_into(v, sim_w, sim_h, width, height, out);
}

fn render_harmonic_interference(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let mut out = vec![0.0f32; (width * height) as usize];
    let width_f = width.max(1) as f32;
    let height_f = height.max(1) as f32;
    let seed = rng.next_u32();
    let layers = 4 + (seed % 5) as i32;
    let mut y = 0u32;
    while y < height {
        let mut x = 0u32;
        while x < width {
            let u = x as f32 / width_f;
            let v = y as f32 / height_f;
            let mut value = 0.0f32;
            let mut i = 0i32;
            while i < layers {
                let angle = (seed % 37) as f32 * 0.13 + i as f32 * 0.9;
                let freq = 1.3 + i as f32 * 0.66;
                let drift = value_noise(
                    (u * freq * 1.5) + angle,
                    v * freq * 1.2,
                    seed ^ (i as u32 * 11),
                );
                let wave = ((u * freq * 6.2 + angle).sin() * (v * freq * 4.1 + angle * 0.7).cos()
                    + drift * 0.8)
                    * 0.5;
                value += wave / (1.0 + i as f32 * 0.35);
                i += 1;
            }
            out[(y * width + x) as usize] = (value * 0.22 + 0.5) % 1.0;
            x += 1;
        }
        y += 1;
    }
    let mut warped = vec![0.0f32; (width * height) as usize];
    let width_half = width as f32 / 2.0;
    let height_half = height as f32 / 2.0;
    let mut y2 = 0u32;
    while y2 < height {
        let mut x2 = 0u32;
        while x2 < width {
            let u = (x2 as f32 - width_half) / width_f.max(1.0);
            let v = (y2 as f32 - height_half) / height_f.max(1.0);
            let w_scale = 0.02 + value_noise(u * 7.0, v * 6.0, seed ^ 0xF00D) * 0.02;
            let xw = (x2 as f32 + u * w_scale * width_f)
                .clamp(0.0, (width - 1) as f32)
                .round() as usize;
            let yw = (y2 as f32 + v * w_scale * height_f)
                .clamp(0.0, (height - 1) as f32)
                .round() as usize;
            warped[(y2 * width + x2) as usize] = out[yw * width as usize + xw];
            x2 += 1;
        }
        y2 += 1;
    }
    for v in warped.iter_mut() {
        *v = (*v * 1.08).fract();
    }
    normalize(&mut warped);
    warped
}

fn render_attractor_voronoi_hybrid(
    width: u32,
    height: u32,
    rng: &mut XorShift32,
    fast: bool,
) -> Vec<f32> {
    let sim_w = (width / 3).clamp(160, 540);
    let sim_h = (height / 3).clamp(160, 540);
    let point_count = 24 + if fast { 0 } else { 10 };
    let mut points: Vec<(f32, f32)> = Vec::with_capacity(point_count);
    let mut seed_rng = XorShift32::new(rng.next_u32() ^ 0x6d65_7461);
    let mut x = 0.5f32 + (seed_rng.next_f32() - 0.5) * 0.25;
    let mut y = 0.5f32 + (seed_rng.next_f32() - 0.5) * 0.25;
    let params = [
        seed_rng.next_f32() * 2.0,
        seed_rng.next_f32() * 2.0,
        seed_rng.next_f32() * 2.0 + 1.0,
        seed_rng.next_f32() * 2.0 + 0.5,
    ];

    let mut i = 0u32;
    while i < point_count as u32 {
        let idx = (i as usize) % params.len();
        let nx = params[idx] * 0.02 * (i as f32 * 0.001);
        let ny = params[(idx + 1) % params.len()] * 0.02 * (i as f32 * 0.001);
        let dx = (params[idx] - 1.0) * (x + nx);
        let dy = (params[(idx + 1) % params.len()] - 1.0) * (y + ny);
        x = dx.cos().fract().abs() * 0.7 + x * 0.15 + 0.15;
        y = dy.sin().fract().abs() * 0.7 + y * 0.15 + 0.15;
        points.push((x.clamp(0.02, 0.98), y.clamp(0.02, 0.98)));
        if seed_rng.next_f32() < 0.15 {
            points.push((rng.next_f32(), rng.next_f32()));
        }
        i += 1;
    }

    let mut out = vec![0.0f32; (sim_w * sim_h) as usize];
    let mut y_i = 0u32;
    while y_i < sim_h {
        let mut x_i = 0u32;
        while x_i < sim_w {
            let u = x_i as f32 / sim_w.max(1) as f32;
            let v = y_i as f32 / sim_h.max(1) as f32;
            let mut first = f32::INFINITY;
            let mut second = f32::INFINITY;
            let mut j = 0usize;
            while j < points.len() {
                let dx = u - points[j].0;
                let dy = v - points[j].1;
                let d = (dx * dx + dy * dy).sqrt();
                if d < first {
                    second = first;
                    first = d;
                } else if d < second {
                    second = d;
                }
                j += 1;
            }
            let gap = (second - first).abs();
            let orbit = value_noise(
                u * 6.1 + (first * 20.0),
                v * 5.9 + (second * 17.0),
                seed_rng.next_u32(),
            );
            let w = if fast { 0.88 } else { 1.0 };
            out[(y_i * sim_w + x_i) as usize] = clamp01((gap * w + orbit * 0.18).sin().abs());
            x_i += 1;
        }
        y_i += 1;
    }
    normalize(&mut out);
    resize_bilinear(&out, sim_w, sim_h, width, height)
}

fn render_recursive_noise_terrain(
    width: u32,
    height: u32,
    rng: &mut XorShift32,
    fast: bool,
) -> Vec<f32> {
    let mut sim_w = (width / 4).clamp(128, 500);
    let mut sim_h = (height / 4).clamp(128, 500);
    let seed = rng.next_u32();
    let mut field = noise_field(sim_w, sim_h, rng, if fast { 2 } else { 3 });
    let levels = if fast { 3 } else { 5 };
    let mut level = 0u32;
    while level < levels {
        let mut next = vec![0.0f32; (sim_w * sim_h) as usize];
        let freq = 1.7 + level as f32 * 1.4;
        let amp = 1.0 / (level as f32 + 1.0);
        let mut y = 0u32;
        while y < sim_h {
            let mut x = 0u32;
            while x < sim_w {
                let u = x as f32 / sim_w.max(1) as f32;
                let v = y as f32 / sim_h.max(1) as f32;
                let warp_x = (value_noise(u * freq, v * freq, seed ^ level) - 0.5) * 0.35;
                let warp_y =
                    (value_noise(u * freq + 1.2, v * freq + 2.3, seed ^ (level + 1)) - 0.5) * 0.35;
                let sample_u =
                    ((u + warp_x).clamp(0.0, 1.0) * (sim_w.max(1) as f32 - 1.0)) as usize;
                let sample_v =
                    ((v + warp_y).clamp(0.0, 1.0) * (sim_h.max(1) as f32 - 1.0)) as usize;
                let idx = y as usize * sim_w as usize + x as usize;
                let sample_idx = sample_v * sim_w as usize + sample_u;
                let edge = ((field[sample_idx] - 0.5) * 2.0).abs();
                next[idx] = clamp01(
                    field[idx] * 0.55
                        + edge * amp
                        + value_noise(u * freq, v * freq, seed ^ (level << 7)) * 0.15,
                );
                x += 1;
            }
            y += 1;
        }
        std::mem::swap(&mut field, &mut next);
        level += 1;
        if sim_w > 340 && sim_h > 340 {
            sim_w -= 34;
            sim_h -= 34;
        }
    }
    normalize(&mut field);
    resize_bilinear(&field, sim_w, sim_h, width, height)
}

fn render_bifurcation_grid(width: u32, height: u32, rng: &mut XorShift32) -> Vec<f32> {
    let sim_w = (width / 3).clamp(160, 540);
    let sim_h = (height / 3).clamp(160, 540);
    let seed = rng.next_u32();
    let mut out = vec![0.0f32; (sim_w * sim_h) as usize];

    let a = 3.6 + (rng.next_f32() * 0.45);
    let b = 2.8 + (rng.next_f32() * 0.5);
    let c = 0.15 + rng.next_f32() * 0.35;
    let d = 0.85 + rng.next_f32() * 0.25;
    let iters = 120u32;
    for gy in 0..sim_h {
        let row = sim_w as usize * gy as usize;
        for gx in 0..sim_w {
            let rxa = (gx as f32) / sim_w.max(1) as f32;
            let rya = (gy as f32) / sim_h.max(1) as f32;
            let mut i = 0u32;
            let mut acc = 0.0f32;
            let mut sx = rxa * 0.82 + 0.09 + (seed as f32 * 1e-6);
            let mut sy = rya * 0.79 + 0.11 + (seed as f32 * 2e-6);
            while i < iters {
                let nx = a * sx * (1.0 - sx) + c * sy * sy.sin();
                let ny = b * sy * (1.0 - sy) + d * sx * sx.cos();
                sx = nx.fract();
                sy = ny.fract();
                let distance = (sx - rxa).abs() + (sy - rya).abs();
                acc += distance.exp();
                i += 1;
            }
            out[row + gx as usize] = (acc / iters as f32).clamp(0.0, 1.0);
        }
    }

    for value in out.iter_mut() {
        let m = value_noise(*value * 14.0, *value * 11.0, seed);
        *value = clamp01(*value * 0.6 + m * 0.25);
    }
    let mut smoothed = out;
    let mut k = 0u32;
    while k < 2 {
        let mut y = 1u32;
        let mut next = smoothed.clone();
        while y + 1 < sim_h {
            let mut x = 1u32;
            while x + 1 < sim_w {
                let idx = y as usize * sim_w as usize + x as usize;
                let mut sum = 0.0f32;
                let mut dy = -1i32;
                while dy <= 1 {
                    let mut dx = -1i32;
                    while dx <= 1 {
                        let sx = (x as i32 + dx) as usize;
                        let sy = (y as i32 + dy) as usize;
                        sum += smoothed[sy * sim_w as usize + sx];
                        dx += 1;
                    }
                    dy += 1;
                }
                next[idx] = sum / 9.0;
                x += 1;
            }
            y += 1;
        }
        smoothed = next;
        k += 1;
    }
    normalize(&mut smoothed);
    resize_nearest(&smoothed, sim_w, sim_h, width, height)
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
            CpuStrategy::LissajousOrbits,
            CpuStrategy::GraviticWeb,
            CpuStrategy::PoissonMesh,
            CpuStrategy::MetaballField,
            CpuStrategy::BraidFlow,
            CpuStrategy::InterferenceWaves,
            CpuStrategy::CliffordAttractor,
            CpuStrategy::JuliaSet,
            CpuStrategy::KochSnowflake,
            CpuStrategy::BifurcationTree,
            CpuStrategy::DepthRelief,
            CpuStrategy::AttractorTunnel,
            CpuStrategy::OrbitalLabyrinth,
            CpuStrategy::PhaseField,
            CpuStrategy::Lenia,
            CpuStrategy::CurlNoiseFlow,
            CpuStrategy::ReactionLattice,
            CpuStrategy::HarmonicInterference,
            CpuStrategy::AttractorVoronoiHybrid,
            CpuStrategy::RecursiveNoiseTerrain,
            CpuStrategy::BifurcationGrid,
        ];
        let mut img = vec![0.0f32; 256 * 256];
        let mut scratch = StrategyScratch::default();
        for strategy in all.drain(..) {
            render_cpu_strategy(strategy, 256, 256, 42, false, 512, &mut img, &mut scratch);
            assert_eq!(img.len(), 256 * 256);
            assert!(img.iter().all(|value| value.is_finite()));
            let min = img.iter().cloned().fold(1.0f32, f32::min);
            let max = img.iter().cloned().fold(0.0f32, f32::max);
            assert!(max >= min);
        }
    }

    #[test]
    fn strategy_cost_reflects_heavy_kernels() {
        let cheap = cpu_strategy_cost(CpuStrategy::PerlinRidge);
        let heavy = cpu_strategy_cost(CpuStrategy::ReactionLattice);
        assert!(heavy > cheap);
    }

    #[test]
    fn budget_fit_returns_affordable_cpu_strategy() {
        let mut rng = XorShift32::new(0xA1B2_C3D4);
        let original = RenderStrategy::Cpu(CpuStrategy::ReactionLattice);
        let fitted = fit_strategy_to_budget(&mut rng, original, 110, true, false);
        match fitted {
            RenderStrategy::Cpu(kind) => {
                assert!(cpu_strategy_cost(kind) <= 140);
            }
            RenderStrategy::Gpu(_) => panic!("expected cpu strategy when gpu is not preferred"),
        }
    }

    #[test]
    fn reaction_diffusion_is_deterministic_for_fixed_seed() {
        let mut rng_a = XorShift32::new(1_337_331);
        let mut rng_b = XorShift32::new(1_337_331);
        let a = render_reaction_diffusion(192, 192, &mut rng_a, true);
        let b = render_reaction_diffusion(192, 192, &mut rng_b, true);
        assert_eq!(a, b);
    }

    #[test]
    fn reaction_lattice_is_deterministic_for_fixed_seed() {
        let mut rng_a = XorShift32::new(9_812_443);
        let mut rng_b = XorShift32::new(9_812_443);
        let a = render_reaction_lattice(192, 192, &mut rng_a, true);
        let b = render_reaction_lattice(192, 192, &mut rng_b, true);
        assert_eq!(a, b);
    }

    /// Manual benchmark for validating reaction-kernel latency and determinism.
    #[test]
    #[ignore = "manual perf probe; run with cargo test bench_reaction_kernels_fixed_seed -- --ignored --nocapture"]
    fn bench_reaction_kernels_fixed_seed() {
        use std::time::Instant;

        fn mean_abs_diff(a: &[f32], b: &[f32]) -> f32 {
            let len = a.len().min(b.len()).max(1);
            let sum: f32 = a
                .iter()
                .zip(b.iter())
                .map(|(left, right)| (left - right).abs())
                .sum();
            sum / len as f32
        }

        for size in [512u32, 1024u32] {
            let mut rng_a = XorShift32::new(0xD1FF_1000 ^ size);
            let start = Instant::now();
            let diffusion_a = render_reaction_diffusion(size, size, &mut rng_a, true);
            let diffusion_ms = start.elapsed().as_secs_f64() * 1000.0;
            let mut rng_b = XorShift32::new(0xD1FF_1000 ^ size);
            let diffusion_b = render_reaction_diffusion(size, size, &mut rng_b, true);
            let diffusion_diff = mean_abs_diff(&diffusion_a, &diffusion_b);

            let mut rng_c = XorShift32::new(0x1A77_1000 ^ size);
            let start = Instant::now();
            let lattice_a = render_reaction_lattice(size, size, &mut rng_c, true);
            let lattice_ms = start.elapsed().as_secs_f64() * 1000.0;
            let mut rng_d = XorShift32::new(0x1A77_1000 ^ size);
            let lattice_b = render_reaction_lattice(size, size, &mut rng_d, true);
            let lattice_diff = mean_abs_diff(&lattice_a, &lattice_b);

            println!(
                "size={} diffusion_ms={:.1} lattice_ms={:.1} diffusion_mad={:.8} lattice_mad={:.8}",
                size, diffusion_ms, lattice_ms, diffusion_diff, lattice_diff
            );
            assert!(diffusion_diff <= 1.0e-7);
            assert!(lattice_diff <= 1.0e-7);
        }
    }
}
