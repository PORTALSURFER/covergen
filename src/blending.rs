//! Strategy-mixing helpers used by `main` when combining layer renderers.

use crate::strategies::{
    RenderStrategy, normalize, pick_render_strategy_near_family_with_preferences,
    pick_render_strategy_with_preferences, value_noise,
};
use crate::{
    ArtStyle, XorShift32, apply_detail_waves, apply_dynamic_filter, clamp01, pick_filter_from_rng,
    sample_luma, tune_filter_for_speed,
};

/// Different edge, noise, and procedural mask profiles used for blending.
#[derive(Clone, Copy)]
pub enum LayerMaskKind {
    MathNoise,
    RadialBands,
    Spiral,
    CheckerFlow,
    EdgeSource,
}

impl LayerMaskKind {
    /// Picks a deterministic mask family from an input integer.
    fn from_u32(value: u32) -> Self {
        match value % 5 {
            0 => Self::MathNoise,
            1 => Self::RadialBands,
            2 => Self::Spiral,
            3 => Self::CheckerFlow,
            _ => Self::EdgeSource,
        }
    }

    /// Human-readable label used in debug output.
    pub fn label(self) -> &'static str {
        match self {
            Self::MathNoise => "noise",
            Self::RadialBands => "radial",
            Self::Spiral => "spiral",
            Self::CheckerFlow => "checker",
            Self::EdgeSource => "edge",
        }
    }
}

/// Render-strategy label used in layer debug output.
pub fn strategy_name(strategy: RenderStrategy) -> String {
    match strategy {
        RenderStrategy::Gpu(style) => format!("gpu:{}", ArtStyle::from_u32(style).label()),
        RenderStrategy::Cpu(cpu) => format!("cpu:{}", cpu.label()),
    }
}

/// Returns true when both strategies are the same backend and style family.
pub fn strategy_equivalent(a: RenderStrategy, b: RenderStrategy) -> bool {
    match (a, b) {
        (RenderStrategy::Gpu(a_style), RenderStrategy::Gpu(b_style)) => a_style == b_style,
        (RenderStrategy::Cpu(a_cpu), RenderStrategy::Cpu(b_cpu)) => a_cpu == b_cpu,
        _ => false,
    }
}

/// Pick a secondary strategy for layer mixing. Usually keeps family continuity,
/// but can occasionally jump to a distant strategy.
pub fn pick_blended_strategy(
    base: RenderStrategy,
    rng: &mut XorShift32,
    fast: bool,
    prefer_gpu: bool,
) -> RenderStrategy {
    let bias = if rng.next_f32() < 0.72 { 0.74 } else { 0.0 };
    let mut candidate = if bias > 0.0 {
        pick_render_strategy_near_family_with_preferences(rng, fast, base, bias, prefer_gpu)
    } else {
        pick_render_strategy_with_preferences(rng, fast, prefer_gpu)
    };

    if strategy_equivalent(candidate, base) {
        let mut retries = 0u32;
        while strategy_equivalent(candidate, base) && retries < 6 {
            candidate = if bias > 0.0 && rng.next_f32() < 0.80 {
                pick_render_strategy_near_family_with_preferences(rng, fast, base, bias, prefer_gpu)
            } else {
                pick_render_strategy_with_preferences(rng, fast, prefer_gpu)
            };
            retries += 1;
        }
    }

    candidate
}

/// Return true when a layer should perform a secondary strategy blend.
pub fn should_mix_strategies(
    layer_index: u32,
    rng: &mut XorShift32,
    fast: bool,
    structural: bool,
    bias: f32,
) -> bool {
    let base = if fast { 0.12 } else { 0.22 };
    let layer_bias = if layer_index == 0 { -0.04 } else { 0.10 };
    let weighted = (base + layer_bias) * bias.clamp(0.1, 1.6);
    let adjusted = if structural {
        weighted * 0.35
    } else {
        weighted
    };
    rng.next_f32() < adjusted.clamp(0.0, 0.52)
}

pub fn pick_layer_mask_kind(rng: &mut XorShift32, structural: bool) -> LayerMaskKind {
    if structural {
        LayerMaskKind::from_u32(rng.next_u32() | 4)
    } else {
        LayerMaskKind::from_u32(rng.next_u32())
    }
}

/// Parameters and scratch buffers used to generate a reusable layer mask.
pub(crate) struct LayerMaskBuildRequest<'a> {
    /// Primary rendered luma input.
    pub(crate) primary: &'a [f32],
    /// Render width for mask sampling.
    pub(crate) width: u32,
    /// Render height for mask sampling.
    pub(crate) height: u32,
    /// Seed used for randomized math masks.
    pub(crate) source_seed: u32,
    /// Which pattern family to generate.
    pub(crate) kind: LayerMaskKind,
    /// Destination map for the generated mask.
    pub(crate) out: &'a mut [f32],
    /// Reusable blur scratch for mask generation.
    pub(crate) blur_work: &'a mut [f32],
    /// Enable faster blur profile for performance mode.
    pub(crate) fast: bool,
}

fn generate_edge_mask(source: &[f32], width: u32, height: u32, out: &mut [f32]) {
    out.fill(0.0);
    let width_i32 = width as i32;
    let height_i32 = height as i32;
    let mut max_edge = 0.0f32;

    for y in 0..height as i32 {
        for x in 0..width as i32 {
            let center = source[width_i32.checked_mul(y).unwrap_or(0) as usize + x as usize];
            let right = sample_luma(source, width_i32, height_i32, x + 1, y);
            let down = sample_luma(source, width_i32, height_i32, x, y + 1);
            let left = sample_luma(source, width_i32, height_i32, x - 1, y);
            let up = sample_luma(source, width_i32, height_i32, x, y - 1);
            let edge = ((right - center).abs()
                + (down - center).abs()
                + (left - center).abs()
                + (up - center).abs())
                * 0.25;
            let idx = y as usize * width as usize + x as usize;
            out[idx] = edge;
            max_edge = max_edge.max(edge);
        }
    }

    if max_edge <= f32::EPSILON {
        return;
    }

    for value in out.iter_mut() {
        *value = (*value / max_edge).clamp(0.0, 1.0);
    }
}

fn generate_math_mask(
    width: u32,
    height: u32,
    seed: u32,
    kind: LayerMaskKind,
    rng: &mut XorShift32,
    out: &mut [f32],
) {
    out.fill(0.0);
    let freq = 1.4 + (rng.next_f32() * 3.1);
    let freq_y = 1.8 + (rng.next_f32() * 2.9);
    let phase = rng.next_f32() * std::f32::consts::TAU;
    let phase_b = rng.next_f32() * std::f32::consts::TAU;
    let freq_t = 2.0 + rng.next_f32() * 5.0;

    for y in 0..height as i32 {
        for x in 0..width as i32 {
            let nx = (x as f32) / (width.max(1) as f32);
            let ny = (y as f32) / (height.max(1) as f32);
            let u = nx * 2.0 - 1.0;
            let v = ny * 2.0 - 1.0;
            let value = match kind {
                LayerMaskKind::MathNoise => {
                    value_noise(nx * freq * 6.0 + phase, ny * freq_y * 6.0 + phase_b, seed)
                }
                LayerMaskKind::RadialBands => {
                    let radius = (u * u + v * v).sqrt().clamp(0.0, 1.0);
                    let angle = v.atan2(u) + phase;
                    (radius * freq_t + angle * 0.75 + phase_b).sin() * 0.5 + 0.5
                }
                LayerMaskKind::Spiral => {
                    let radius = (u * u + v * v).sqrt().max(0.000_1);
                    (freq_t * angle_component(u, v) + (radius * freq).cos() + phase).sin() * 0.5
                        + 0.5
                }
                LayerMaskKind::CheckerFlow => {
                    let checker = ((u * freq).floor() + (v * freq_y).floor()).sin() * 0.5 + 0.5;
                    checker * (0.35 + 0.25 * (u * phase.sin() + v * phase_b.cos()).sin().abs())
                }
                LayerMaskKind::EdgeSource => unreachable!("edge source must be handled separately"),
            };
            let idx = y as usize * width as usize + x as usize;
            out[idx] = value.clamp(0.0, 1.0);
        }
    }

    normalize(out);
}

fn angle_component(x: f32, y: f32) -> f32 {
    y.atan2(x)
}

/// Construct a blend mask into `out` with reusable temporary storage in `blur_work`.
pub fn build_layer_mask(request: &mut LayerMaskBuildRequest<'_>, rng: &mut XorShift32) {
    debug_assert_eq!(request.primary.len(), request.out.len());
    debug_assert_eq!(request.out.len(), request.blur_work.len());

    match request.kind {
        LayerMaskKind::EdgeSource => {
            generate_edge_mask(request.primary, request.width, request.height, request.out)
        }
        _ => generate_math_mask(
            request.width,
            request.height,
            request.source_seed,
            request.kind,
            rng,
            request.out,
        ),
    }

    let blur_cfg = tune_filter_for_speed(pick_filter_from_rng(rng), request.fast);
    apply_dynamic_filter(
        request.width,
        request.height,
        request.out,
        request.blur_work,
        &blur_cfg,
    );
    request.out.copy_from_slice(request.blur_work);

    let gamma = 0.35 + (rng.next_f32() * 1.45);
    for value in request.out.iter_mut() {
        *value = value.powf(gamma);
    }

    let add_detail = if rng.next_f32() < 0.4 { 0.82 } else { 0.0 };
    if add_detail > 0.0 {
        apply_detail_waves(
            request.out,
            request.width,
            request.height,
            request.source_seed ^ 0x4d5a_2f1f,
            add_detail,
        );
    }

    normalize(request.out);
}

/// Blend an alternate layer into `base` using `mask` as the per-pixel interpolation weight.
pub fn blend_with_mask(base: &mut [f32], alt: &[f32], mask: &[f32], invert_mask: bool) {
    debug_assert_eq!(base.len(), alt.len());
    debug_assert_eq!(base.len(), mask.len());

    for ((base_value, second), blend_value) in base.iter_mut().zip(alt.iter()).zip(mask.iter()) {
        let mut blend = *blend_value;
        if invert_mask {
            blend = 1.0 - blend;
        }
        *base_value = clamp01((*base_value * (1.0 - blend)) + (*second * blend));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layer_mask_kinds_cover_all_sources() {
        let mut seen = [false; 5];
        for i in 0..5u32 {
            let kind = LayerMaskKind::from_u32(i);
            let idx = match kind {
                LayerMaskKind::MathNoise => 0,
                LayerMaskKind::RadialBands => 1,
                LayerMaskKind::Spiral => 2,
                LayerMaskKind::CheckerFlow => 3,
                LayerMaskKind::EdgeSource => 4,
            };
            seen[idx] = true;
        }

        assert!(seen.iter().all(|v| *v));
    }

    #[test]
    fn strategy_mix_probability_never_panics() {
        let mut rng = XorShift32::new(123_456_789);
        let base = RenderStrategy::Gpu(0);
        let _mixed = should_mix_strategies(1, &mut rng, true, false, 1.0);
        let name = strategy_name(base);
        assert!(!name.is_empty());
    }

    #[test]
    fn build_layer_mask_reuses_provided_scratch() {
        let mut rng = XorShift32::new(7_654_321);
        let primary = vec![0.1f32, 0.2, 0.3, 0.4];
        let mut out = vec![0.0f32; primary.len()];
        let mut blur = vec![0.0f32; primary.len()];
        let mut request = LayerMaskBuildRequest {
            primary: &primary,
            width: 2,
            height: 2,
            source_seed: 123,
            kind: LayerMaskKind::MathNoise,
            out: &mut out,
            blur_work: &mut blur,
            fast: true,
        };

        build_layer_mask(&mut request, &mut rng);

        assert_eq!(out.len(), primary.len());
        assert!(!out.iter().all(|value| *value == 0.0));
        assert!(blur.iter().all(|value| value.is_finite()));
    }
}
