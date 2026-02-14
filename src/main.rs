use std::cmp::Ordering;
use std::sync::mpsc::channel;
use std::{
    env,
    error::Error,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use bytemuck::{Pod, Zeroable};
use image::{ImageBuffer, Rgba};
use wgpu::util::DeviceExt;

const SHADER: &str = r#"
struct Params {
    width: u32,
    height: u32,
    symmetry: u32,
    symmetry_style: u32,
    iterations: u32,
    seed: u32,
    fill_scale: f32,
}

@group(0) @binding(0)
var<storage, read_write> out_pixels: array<u32>;

@group(0) @binding(1)
var<uniform> params: Params;

fn hash01(x: f32, y: f32, seed: u32) -> f32 {
    let s = f32(seed) * 0.00000011920928955;
    return fract(sin(dot(vec2<f32>(x, y), vec2<f32>(12.9898, 78.233)) + s) * 43758.5453123);
}

fn fold_for_symmetry(px: f32, py: f32, symmetry: u32, style: u32) -> vec2<f32> {
    if (style == 1u) {
        return fold_symmetry_mirror(px, py, symmetry);
    }

    if (style == 2u) {
        return fold_symmetry_grid(px, py, symmetry);
    }

    return fold_symmetry_radial(px, py, symmetry);
}

fn fold_symmetry_radial(px: f32, py: f32, symmetry: u32) -> vec2<f32> {
    let radius = length(vec2<f32>(px, py));
    if (radius <= 0.0 || symmetry <= 1u) {
        return vec2<f32>(px, py);
    }

    let angle = atan2(py, px);
    let sector = 6.283185307179586 / f32(symmetry);
    let folded = fract((angle / sector) + 0.5) - 0.5;
    let folded_angle = folded * sector;

    let corner = 2.0 + (f32(symmetry) * 0.2);
    let edge = pow(abs(cos(folded_angle)), corner) + pow(abs(sin(folded_angle)), corner);
    let squircle = pow(edge, -1.0 / corner);
    let petals = abs(cos(folded_angle * f32(symmetry)));
    let folded_radius = radius * (0.75 + (petals * 0.45)) * squircle * 0.72;

    return vec2<f32>(folded_radius * cos(folded_angle), folded_radius * sin(folded_angle));
}

fn fold_symmetry_mirror(px: f32, py: f32, symmetry: u32) -> vec2<f32> {
    if (symmetry <= 1u) {
        return vec2<f32>(px, py);
    }

    var sx = px;
    var sy = py;
    sx = abs(sx);
    if (symmetry > 2u) {
        sy = abs(sy);
    }
    if (symmetry > 3u) {
        if (abs(sy) > abs(sx)) {
            let t = sx;
            sx = sy;
            sy = t;
        }
    }

    return vec2<f32>(sx * 1.0, sy * 1.0);
}

fn fold_symmetry_grid(px: f32, py: f32, symmetry: u32) -> vec2<f32> {
    let repeats = max(2.0, f32(symmetry) * 0.7);
    let gx = fract((px * repeats) + 0.5) - 0.5;
    let gy = fract((py * repeats * 1.15) + 0.5) - 0.5;
    let mx = abs(gx) * 2.0 - 0.5;
    let my = abs(gy) * 2.0 - 0.5;
    let skew = sin(px * 8.0 + py * 4.0) * 0.15;
    return vec2<f32>(mx * (2.0 / repeats) + (my * 0.08), (my * (2.0 / repeats)) + skew);
}

fn pack_gray(level: f32) -> u32 {
    let c = u32(clamp(level, 0.0, 1.0) * 255.0 + 0.5);
    return (255u << 24u) | (c << 16u) | (c << 8u) | c;
}

fn layer_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011920928955;
    let jitter = hash01(x + base, y - base, params.seed + layer);
    let layer_shift = f32(layer) * 0.24 + (jitter - 0.5) * 0.35;
    let angle = layer_shift;
    let radius = 0.48 + 0.08 * f32(layer) + (jitter - 0.5) * 0.1;
    let cx = (radius * x * (3.2 + 0.2 * jitter)) + 0.16 * (sin(angle) + cos(angle * 0.7 + base));
    let cy = (radius * y * (3.0 + 0.2 * (1.0 - jitter))) + 0.16 * (cos(angle) + sin(angle * 0.9 - base));

    let rot_x = x * cos(angle) - y * sin(angle);
    let rot_y = x * sin(angle) + y * cos(angle);
    var zx = rot_x * 2.6;
    var zy = rot_y * 2.6;
    var i: u32 = 0u;
    var mag2 = 0.0;
    var escaped = false;

    loop {
        if (i >= params.iterations) {
            break;
        }
        if (mag2 > 4.0) {
            escaped = true;
            break;
        }

        let x2 = zx * zx - zy * zy + cx;
        let y2 = 2.0 * zx * zy + cy;
        zx = x2;
        zy = y2;
        mag2 = zx * zx + zy * zy;
        i = i + 1u;
    }

    return select(
        0.0,
        1.0 - (f32(i) / f32(params.iterations)),
        escaped,
    );
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.width || id.y >= params.height) {
        return;
    }

    var px = ((f32(id.x) + 0.5) / f32(params.width)) * 2.0 - 1.0;
    var py = ((f32(id.y) + 0.5) / f32(params.height)) * 2.0 - 1.0;
    px = px * params.fill_scale;
    py = py * params.fill_scale;
    var value = 0.0;
    let layer_count = 6u;
    let layer_scale: f32 = 1.0 / f32(layer_count);

    if (params.symmetry > 1u) {
        let folded = fold_for_symmetry(px, py, params.symmetry, params.symmetry_style);
        px = folded.x;
        py = folded.y;
    }

    let swirl = hash01(px * 11.0, py * 13.0, params.seed);
    let sx = px + (swirl - 0.5) * 0.08;
    let sy = py + (swirl - 0.5) * 0.08;

    var layer: u32 = 0u;
    loop {
        if (layer >= layer_count) {
            break;
        }
        let layer_brightness = layer_value(sx, sy, params, layer);
        let weight = 1.0 - (f32(layer) * 0.11);
        value = value + (layer_brightness * layer_brightness * weight * layer_scale);
        layer = layer + 1u;
    }

    out_pixels[id.x + id.y * params.width] = pack_gray(value);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    width: u32,
    height: u32,
    symmetry: u32,
    symmetry_style: u32,
    iterations: u32,
    seed: u32,
    fill_scale: f32,
}

#[derive(Clone, Copy)]
enum FilterMode {
    Motion,
    Gaussian,
    Median,
    Bilateral,
}

impl FilterMode {
    fn from_u32(value: u32) -> Self {
        match value % 4 {
            0 => Self::Motion,
            1 => Self::Gaussian,
            2 => Self::Median,
            _ => Self::Bilateral,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Motion => "motion",
            Self::Gaussian => "gaussian",
            Self::Median => "median",
            Self::Bilateral => "bilateral",
        }
    }
}

#[derive(Clone, Copy)]
enum GradientMode {
    Linear,
    Contrast,
    Gamma,
    Sine,
    Sigmoid,
    Posterize,
}

impl GradientMode {
    fn from_u32(value: u32) -> Self {
        match value % 6 {
            0 => Self::Linear,
            1 => Self::Contrast,
            2 => Self::Gamma,
            3 => Self::Sine,
            4 => Self::Sigmoid,
            _ => Self::Posterize,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Contrast => "contrast",
            Self::Gamma => "gamma",
            Self::Sine => "sine",
            Self::Sigmoid => "sigmoid",
            Self::Posterize => "posterize",
        }
    }
}

#[derive(Clone, Copy)]
struct BlurConfig {
    mode: FilterMode,
    max_radius: u32,
    axis_x: i32,
    axis_y: i32,
    softness: u32,
}

#[derive(Clone, Copy)]
struct GradientConfig {
    mode: GradientMode,
    gamma: f32,
    contrast: f32,
    pivot: f32,
    invert: bool,
    frequency: f32,
    phase: f32,
    bands: u32,
}

#[derive(Clone, Copy)]
enum SymmetryStyle {
    Radial,
    Mirror,
    Grid,
}

impl SymmetryStyle {
    fn from_u32(value: u32) -> Self {
        match value % 3 {
            0 => Self::Radial,
            1 => Self::Mirror,
            _ => Self::Grid,
        }
    }

    fn as_u32(self) -> u32 {
        match self {
            Self::Radial => 0,
            Self::Mirror => 1,
            Self::Grid => 2,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Radial => "radial",
            Self::Mirror => "mirror",
            Self::Grid => "grid",
        }
    }
}

struct Config {
    width: u32,
    height: u32,
    symmetry: u32,
    iterations: u32,
    seed: u32,
    fill_scale: f32,
    count: u32,
    output: String,
}

struct XorShift32 {
    state: u32,
}

impl XorShift32 {
    fn new(seed: u32) -> Self {
        let state = if seed == 0 { 0x9e3779b9 } else { seed };
        Self { state }
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }

    fn next_f32(&mut self) -> f32 {
        (self.next_u32() as f32) / (u32::MAX as f32)
    }
}

impl Config {
    fn from_env() -> Result<Self, Box<dyn Error>> {
        let mut args = env::args().skip(1);
        let mut cfg = Config {
            width: 1024,
            height: 1024,
            symmetry: 4,
            iterations: 240,
            seed: random_seed(),
            fill_scale: 1.55,
            count: 1,
            output: "fractal.png".to_string(),
        };

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--width" | "-w" => {
                    let value = args
                        .next()
                        .ok_or("missing width value, pass --width <u32>")?;
                    cfg.width = value.parse()?;
                }
                "--height" | "-h" => {
                    let value = args
                        .next()
                        .ok_or("missing height value, pass --height <u32>")?;
                    cfg.height = value.parse()?;
                }
                "--size" => {
                    let value = args
                        .next()
                        .ok_or("missing size value, pass --size <width>x<height>")?;
                    let mut split = value.split('x');
                    cfg.width = split.next().ok_or("size needs WIDTHxHEIGHT")?.parse()?;
                    cfg.height = split.next().ok_or("size needs WIDTHxHEIGHT")?.parse()?;
                }
                "--symmetry" => {
                    let value = args
                        .next()
                        .ok_or("missing symmetry value, pass --symmetry <1-8>")?;
                    cfg.symmetry = value.parse()?;
                }
                "--iterations" => {
                    let value = args
                        .next()
                        .ok_or("missing iterations value, pass --iterations <u32>")?;
                    cfg.iterations = value.parse()?;
                }
                "--seed" => {
                    let value = args.next().ok_or("missing seed value, pass --seed <u32>")?;
                    cfg.seed = value.parse()?;
                }
                "--fill" => {
                    let value = args.next().ok_or("missing fill value, pass --fill <f32>")?;
                    cfg.fill_scale = value.parse()?;
                }
                "--count" | "-n" => {
                    let value = args
                        .next()
                        .ok_or("missing count value, pass --count <u32>")?;
                    cfg.count = value.parse()?;
                }
                "--output" | "-o" => {
                    cfg.output = args
                        .next()
                        .ok_or("missing output file name, pass --output <path>")?
                        .to_string();
                }
                _ => return Err(format!("unknown argument: {arg}").into()),
            }
        }

        if cfg.width == 0 || cfg.height == 0 {
            return Err("width and height must be greater than zero".into());
        }
        if cfg.symmetry == 0 {
            return Err("symmetry must be at least 1".into());
        }
        if cfg.iterations == 0 {
            return Err("iterations must be at least 1".into());
        }
        if cfg.fill_scale <= 0.0 {
            return Err("fill scale must be greater than 0".into());
        }
        if cfg.count == 0 {
            return Err("count must be at least 1".into());
        }

        Ok(cfg)
    }
}

fn random_seed() -> u32 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_nanos() as u64)
        .unwrap_or(0);
    let mut seed = now as u32 ^ (now >> 32) as u32;
    seed ^= seed << 13;
    seed ^= seed >> 17;
    seed ^= seed << 5;
    seed
}

fn randomize_symmetry(base: u32, rng: &mut XorShift32) -> u32 {
    let spread = (base as f32 * 0.2).round() as u32;
    let low = base.saturating_sub(spread).max(1);
    let high = (base.saturating_add(spread)).max(low);
    let value = low + (rng.next_u32() % (high - low + 1));
    value
}

fn randomize_iterations(base: u32, rng: &mut XorShift32) -> u32 {
    let low = (base as f32 * 0.4).floor().max(64.0) as u32;
    let high = (base as f32 * 1.8).ceil().max(80.0) as u32;
    low + (rng.next_u32() % (high - low + 1))
}

fn randomize_fill_scale(base: f32, rng: &mut XorShift32) -> f32 {
    let jitter = 0.8 + (rng.next_f32() * 0.6);
    (base * jitter).clamp(1.2, 2.4)
}

fn pick_symmetry_style(rng: &mut XorShift32) -> u32 {
    SymmetryStyle::from_u32(rng.next_u32()).as_u32()
}

fn pick_filter_from_rng(rng: &mut XorShift32) -> BlurConfig {
    let mode = FilterMode::from_u32(rng.next_u32());
    let mut axis_x = (rng.next_u32() % 5) as i32 - 2;
    let mut axis_y = (rng.next_u32() % 5) as i32 - 2;
    if axis_x == 0 && axis_y == 0 {
        axis_x = 1;
        axis_y = 0;
    }
    BlurConfig {
        mode,
        max_radius: 2 + (rng.next_u32() % 8),
        axis_x,
        axis_y,
        softness: 1 + (rng.next_u32() % 4),
    }
}

fn pick_gradient_from_rng(rng: &mut XorShift32) -> GradientConfig {
    let mode = GradientMode::from_u32(rng.next_u32());
    let gamma = 0.45 + (rng.next_u32() % 160) as f32 * 0.01;
    let contrast = 0.6 + (rng.next_u32() % 240) as f32 * 0.01;
    let pivot = 0.25 + (rng.next_u32() % 70) as f32 * 0.01;
    let invert = (rng.next_u32() % 2) == 0;
    let frequency = 0.5 + (rng.next_u32() % 250) as f32 * 0.02;
    let phase = (rng.next_u32() % 360) as f32 * 0.0174533;
    let bands = (rng.next_u32() % 6) + 1;

    GradientConfig {
        mode,
        gamma,
        contrast,
        pivot,
        invert,
        frequency,
        phase,
        bands,
    }
}

fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

fn apply_posterize(mut value: f32, bands: u32) -> f32 {
    if bands <= 1 {
        return clamp01(value);
    }
    let levels = bands as f32;
    value = clamp01(value) * levels;
    (value.floor() / levels).min(1.0)
}

fn apply_gradient_map(src: Vec<f32>, cfg: GradientConfig) -> Vec<f32> {
    src.into_iter()
        .map(|value| {
            let mut mapped = clamp01(value);
            match cfg.mode {
                GradientMode::Linear => {}
                GradientMode::Contrast => {
                    mapped = mapped.powf(1.0 + cfg.contrast * 0.05) * cfg.pivot;
                    mapped = mapped.clamp(0.0, 1.0);
                }
                GradientMode::Gamma => {
                    mapped = mapped.powf(cfg.gamma);
                }
                GradientMode::Sine => {
                    mapped = 0.5
                        + (0.5
                            * (cfg.frequency * mapped * std::f32::consts::PI * 2.0 + cfg.phase)
                                .sin());
                }
                GradientMode::Sigmoid => {
                    let x = cfg.contrast * 0.1 * (mapped - cfg.pivot);
                    mapped = 1.0 / (1.0 + (-x).exp());
                }
                GradientMode::Posterize => {}
            }

            if cfg.invert {
                mapped = 1.0 - mapped;
            }

            mapped = (mapped * cfg.contrast.recip()).clamp(0.0, 1.0);
            mapped = apply_posterize(mapped, cfg.bands);
            clamp01(mapped)
        })
        .collect()
}

fn pixel_index(x: i32, y: i32, width: i32) -> usize {
    (y * width + x) as usize
}

fn sample_luma(src: &[f32], width: i32, height: i32, x: i32, y: i32) -> f32 {
    let clamped_x = x.clamp(0, width - 1);
    let clamped_y = y.clamp(0, height - 1);
    let idx = pixel_index(clamped_x, clamped_y, width);
    src[idx]
}

fn decode_luma(rgba: &[u8]) -> Vec<f32> {
    let mut luma = Vec::with_capacity(rgba.len() / 4);
    for px in rgba.chunks_exact(4) {
        luma.push(px[0] as f32 / 255.0);
    }
    luma
}

fn encode_rgba_gray(luma: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(luma.len() * 4);
    for &v in luma {
        let c = (clamp01(v) * 255.0).round() as u8;
        out.push(c);
        out.push(c);
        out.push(c);
        out.push(255);
    }
    out
}

#[derive(Clone, Copy)]
struct LumaStats {
    min: f32,
    max: f32,
    mean: f32,
    std: f32,
}

fn luma_stats(luma: &[f32]) -> LumaStats {
    let mut min: f32 = 1.0;
    let mut max: f32 = 0.0;
    let mut sum = 0.0;
    for &value in luma {
        min = min.min(value);
        max = max.max(value);
        sum += value;
    }
    let mean = if luma.is_empty() {
        0.0
    } else {
        sum / (luma.len() as f32)
    };

    let mut variance = 0.0;
    for &value in luma {
        let delta = value - mean;
        variance += delta * delta;
    }
    let std = if luma.is_empty() {
        0.0
    } else {
        (variance / (luma.len() as f32)).sqrt()
    };

    LumaStats {
        min,
        max,
        mean,
        std,
    }
}

fn stretch_to_percentile(src: &[f32], low_pct: f32, high_pct: f32) -> Vec<f32> {
    if src.is_empty() {
        return Vec::new();
    }

    let mut sorted = src.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let len_minus_1 = sorted.len() - 1;
    let low = (len_minus_1 as f32 * low_pct.clamp(0.0, 1.0)).round() as usize;
    let high = (len_minus_1 as f32 * high_pct.clamp(0.0, 1.0)).round() as usize;
    let in_min = sorted[low];
    let in_max = sorted[high];
    let span = in_max - in_min;

    if span <= f32::EPSILON {
        return vec![0.5; src.len()];
    }

    src.iter()
        .map(|value| ((value - in_min) / span).clamp(0.0, 1.0))
        .collect()
}

fn inject_noise(mut src: Vec<f32>, seed: u32, strength: f32) -> Vec<f32> {
    let mut rng = XorShift32::new(seed);
    let gain = strength * 0.5;
    for value in &mut src {
        let noise = ((rng.next_u32() as f32) / (u32::MAX as f32) - 0.5) * 2.0;
        *value = clamp01(*value + noise * gain);
    }
    src
}

fn create_soft_background(width: u32, height: u32, seed: u32) -> Vec<f32> {
    let mut rng = XorShift32::new(seed ^ 0x9e37_79b9);
    let freq_x = 0.25 + (rng.next_f32() * 1.8);
    let freq_y = 0.25 + (rng.next_f32() * 1.8);
    let phase_a = rng.next_f32() * std::f32::consts::TAU;
    let phase_b = rng.next_f32() * std::f32::consts::TAU;
    let noise_strength = 0.08 + (rng.next_f32() * 0.1);
    let mut jitter_rng = XorShift32::new(seed ^ 0xA53F_12B1);

    let mut out = Vec::with_capacity((width as usize) * (height as usize));
    let width_f = width as f32;
    let height_f = height as f32;
    for y in 0..height {
        let v = (y as f32 / height_f) * 2.0 - 1.0;
        let v_l1 = v.abs();
        for x in 0..width {
            let u = (x as f32 / width_f) * 2.0 - 1.0;
            let u_l1 = u.abs();
            let wave_x = (u * std::f32::consts::TAU * freq_x + phase_a).sin() * 0.3;
            let wave_y = (v * std::f32::consts::TAU * freq_y + phase_b).cos() * 0.3;
            let cross = ((u - v) * 1.5).sin() * 0.2;
            let jitter = (jitter_rng.next_f32() - 0.5) * 2.0;
            let l1_falloff = 0.82 - (u_l1 + v_l1) * 0.22;
            out.push(clamp01(
                0.46 + (wave_x * 0.24)
                    + (wave_y * 0.24)
                    + (cross * 0.16)
                    + (l1_falloff * 0.24)
                    + (jitter - 0.5) * noise_strength,
            ));
        }
    }
    out
}

fn blend_background(mut src: Vec<f32>, bg: &[f32], strength: f32) -> Vec<f32> {
    for (value, bg_value) in src.iter_mut().zip(bg.iter()) {
        *value = clamp01((*value * (1.0 - strength)) + (*bg_value * strength));
    }
    src
}

fn apply_motion_blur(width: u32, height: u32, src: &[f32], cfg: &BlurConfig) -> Vec<f32> {
    let width_i32 = width as i32;
    let height_i32 = height as i32;
    let mut dst = vec![0.0f32; src.len()];

    for y in 0..height_i32 {
        for x in 0..width_i32 {
            let idx = pixel_index(x, y, width_i32);
            let center = src[idx];
            let local_blur = (1.0 - center).powi(2);
            let radius = (1.0 + (cfg.max_radius as f32 * (0.2 + 0.8 * local_blur))).round() as i32;

            let mut numerator = 0.0;
            let mut denominator = 0.0;
            let half = -radius;
            let mut step = half;
            while step <= radius {
                let t = 1.0 - (step.abs() as f32 / (radius as f32 + 1.0));
                let sx = x + step * cfg.axis_x;
                let sy = y + step * cfg.axis_y;
                let sample = sample_luma(src, width_i32, height_i32, sx, sy);
                numerator += sample * t;
                denominator += t;
                step += 1;
            }

            if denominator > 0.0 {
                dst[idx] = numerator / denominator;
            } else {
                dst[idx] = center;
            }
        }
    }

    dst
}

fn apply_gaussian_blur(width: u32, height: u32, src: &[f32], cfg: &BlurConfig) -> Vec<f32> {
    let width_i32 = width as i32;
    let height_i32 = height as i32;
    let mut dst = vec![0.0f32; src.len()];

    for y in 0..height_i32 {
        for x in 0..width_i32 {
            let idx = pixel_index(x, y, width_i32);
            let center = src[idx];
            let local_blur = (1.0 - center).powf(1.5);
            let radius = (1.0 + (cfg.max_radius as f32 * (0.2 + 0.8 * local_blur))).round() as i32;
            let sigma = (radius as f32 + 1.0) * 0.5;
            let sigma2 = sigma * sigma * 2.0;

            let mut num = 0.0;
            let mut den = 0.0;
            let mut dy = -radius;
            while dy <= radius {
                let mut dx = -radius;
                while dx <= radius {
                    let sx = x + dx;
                    let sy = y + dy;
                    let d2 = (dx * dx + dy * dy) as f32;
                    let spatial = (-d2 / sigma2).exp();
                    let weight = spatial;
                    let sample = sample_luma(src, width_i32, height_i32, sx, sy);
                    num += sample * weight;
                    den += weight;
                    dx += 1;
                }
                dy += 1;
            }

            if den > 0.0 {
                dst[idx] = num / den;
            } else {
                dst[idx] = center;
            }
        }
    }

    dst
}

fn apply_median_blur(width: u32, height: u32, src: &[f32], cfg: &BlurConfig) -> Vec<f32> {
    let width_i32 = width as i32;
    let height_i32 = height as i32;
    let mut dst = vec![0.0f32; src.len()];

    for y in 0..height_i32 {
        for x in 0..width_i32 {
            let idx = pixel_index(x, y, width_i32);
            let center = src[idx];
            let local_blur = (1.0 - center).powi(2);
            let base = 1 + ((cfg.max_radius as f32 * (0.4 + 0.6 * local_blur)).floor() as i32);
            let radius = base.clamp(1, 2);
            let mut values = [0f32; 25];
            let mut count = 0usize;

            let mut dy = -radius;
            while dy <= radius {
                let mut dx = -radius;
                while dx <= radius {
                    let sample = sample_luma(src, width_i32, height_i32, x + dx, y + dy);
                    values[count] = sample;
                    count += 1;
                    dx += 1;
                }
                dy += 1;
            }

            let slice = &mut values[..count];
            slice.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
            dst[idx] = slice[count / 2];
        }
    }

    dst
}

fn apply_bilateral_blur(width: u32, height: u32, src: &[f32], cfg: &BlurConfig) -> Vec<f32> {
    let width_i32 = width as i32;
    let height_i32 = height as i32;
    let mut dst = vec![0.0f32; src.len()];
    let sigma_r = 0.1 + (cfg.softness as f32 * 0.03);

    for y in 0..height_i32 {
        for x in 0..width_i32 {
            let idx = pixel_index(x, y, width_i32);
            let center = src[idx];
            let local_blur = (1.0 - center).powi(2);
            let radius = (1 + ((cfg.max_radius as f32 * (0.2 + 0.8 * local_blur)).round() as i32))
                .max(1)
                .min(2);

            let mut num = 0.0;
            let mut den = 0.0;
            let mut dy = -radius;
            while dy <= radius {
                let mut dx = -radius;
                while dx <= radius {
                    let sample = sample_luma(src, width_i32, height_i32, x + dx, y + dy);
                    let d = sample - center;
                    let range = (-(d * d / (2.0 * sigma_r * sigma_r))).exp();
                    let weight = (-((dx * dx + dy * dy) as f32) / 16.0).exp() * range;
                    num += sample * weight;
                    den += weight;
                    dx += 1;
                }
                dy += 1;
            }

            if den > 0.0 {
                dst[idx] = num / den;
            } else {
                dst[idx] = center;
            }
        }
    }

    dst
}

fn apply_dynamic_filter(width: u32, height: u32, luma: Vec<f32>, cfg: BlurConfig) -> Vec<f32> {
    match cfg.mode {
        FilterMode::Motion => apply_motion_blur(width, height, &luma, &cfg),
        FilterMode::Gaussian => apply_gaussian_blur(width, height, &luma, &cfg),
        FilterMode::Median => apply_median_blur(width, height, &luma, &cfg),
        FilterMode::Bilateral => apply_bilateral_blur(width, height, &luma, &cfg),
    }
}

fn resolve_output_path(output: &str) -> PathBuf {
    let base_path = Path::new(output);
    if !base_path.exists() {
        return base_path.to_path_buf();
    }

    let parent = base_path.parent().unwrap_or_else(|| Path::new(""));
    let stem = base_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("output");
    let extension = base_path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("");
    let mut index = 1u32;

    loop {
        let candidate_name = if extension.is_empty() {
            format!("{stem}_{index}")
        } else {
            format!("{stem}_{index}.{extension}")
        };

        let candidate = if parent.as_os_str().is_empty() {
            PathBuf::from(candidate_name)
        } else {
            parent.join(candidate_name)
        };

        if !candidate.exists() {
            return candidate;
        }

        index += 1;
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let config = Config::from_env()?;
    pollster::block_on(run(config))?;
    Ok(())
}

async fn run(config: Config) -> Result<(), Box<dyn Error>> {
    let shader = wgpu::ShaderModuleDescriptor {
        label: Some("fractal shader"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    };

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        })
        .await
        .ok_or("no compatible GPU adapter found")?;

    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
            },
            None,
        )
        .await?;

    let shader_module = device.create_shader_module(shader);
    let mut params = Params {
        width: config.width,
        height: config.height,
        symmetry: config.symmetry,
        symmetry_style: 0,
        iterations: config.iterations,
        seed: config.seed,
        fill_scale: config.fill_scale,
    };

    let output_size =
        (config.width as usize * config.height as usize * std::mem::size_of::<u32>()) as u64;

    let out_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("output storage"),
        size: output_size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("uniforms"),
        contents: bytemuck::bytes_of(&params),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("staging"),
        size: output_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("bind group layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("bind group"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: out_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: uniform_buffer.as_entire_binding(),
            },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("pipeline layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("compute pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader_module,
        entry_point: "main",
        compilation_options: wgpu::PipelineCompilationOptions::default(),
    });
    let mut image_rng = XorShift32::new(config.seed);

    for i in 0..config.count {
        let mut image_seed = image_rng.next_u32();
        if image_seed == 0 {
            image_seed = 0x9e3779b9;
        }
        params.seed = image_seed;
        params.symmetry = randomize_symmetry(config.symmetry, &mut image_rng);
        params.iterations = randomize_iterations(config.iterations, &mut image_rng);
        params.fill_scale = randomize_fill_scale(config.fill_scale, &mut image_rng);
        params.symmetry_style = pick_symmetry_style(&mut image_rng);
        queue.write_buffer(&uniform_buffer, 0, bytemuck::bytes_of(&params));

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("command encoder"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("compute pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            let work_x = (config.width + 15) / 16;
            let work_y = (config.height + 15) / 16;
            pass.dispatch_workgroups(work_x, work_y, 1);
        }
        encoder.copy_buffer_to_buffer(&out_buffer, 0, &staging_buffer, 0, output_size);
        queue.submit(Some(encoder.finish()));

        let slice = staging_buffer.slice(..);
        let (sender, receiver) = channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).expect("map callback dropped");
        });
        device.poll(wgpu::Maintain::Wait);
        match receiver.recv()? {
            Ok(()) => {}
            Err(err) => return Err(format!("buffer map failed: {err:?}").into()),
        }

        let filter = pick_filter_from_rng(&mut image_rng);
        let gradient = pick_gradient_from_rng(&mut image_rng);
        let background = create_soft_background(
            config.width,
            config.height,
            params.seed ^ (i + 0x0BADC0DEu32),
        );
        let background_strength = 0.2 + (image_rng.next_f32() * 0.14);
        let frame_bytes = {
            let raw = slice.get_mapped_range();
            let data = raw.to_vec();
            drop(raw);
            data
        };
        staging_buffer.unmap();

        let gray = decode_luma(&frame_bytes);
        let pre_filter_stats = luma_stats(&gray);
        let filtered = apply_dynamic_filter(config.width, config.height, gray, filter);
        let filtered = stretch_to_percentile(&filtered, 0.04, 0.96);
        let mapped = apply_gradient_map(filtered, gradient);
        let mut final_luma = blend_background(
            stretch_to_percentile(&mapped, 0.02, 0.98),
            &background,
            background_strength,
        );
        let mut final_stats = luma_stats(&final_luma);

        if final_stats.std < 0.06 || (final_stats.max - final_stats.min) < 0.18 {
            final_luma = stretch_to_percentile(
                &inject_noise(final_luma, params.seed ^ (i + 1), 0.06),
                0.01,
                0.99,
            );
            final_stats = luma_stats(&final_luma);
        }

        let final_pixels = encode_rgba_gray(&final_luma);
        let image: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_raw(config.width, config.height, final_pixels)
                .ok_or("could not create image buffer from GPU output")?;
        let final_output = resolve_output_path(&config.output);
        image.save(&final_output)?;

        println!(
            "Generated {} | index {} | seed {} | fill {} | symmetry {} [{}] | iterations {} | filter {} | radius {} | gradient {} ({:.2},{:.2}) | pre({:.2}-{:.2},{:.2}) post({:.2}-{:.2},{:.2})",
            final_output.display(),
            i,
            params.seed,
            params.fill_scale,
            params.symmetry,
            SymmetryStyle::from_u32(params.symmetry_style).label(),
            params.iterations,
            filter.mode.label(),
            filter.max_radius,
            gradient.mode.label(),
            gradient.gamma,
            gradient.contrast,
            pre_filter_stats.min,
            pre_filter_stats.max,
            pre_filter_stats.mean,
            final_stats.min,
            final_stats.max,
            final_stats.mean
        );
    }
    Ok(())
}
