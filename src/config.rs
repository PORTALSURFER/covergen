//! Configuration and execution-configuration helpers for image generation.
//!
//! This module centralizes CLI parsing and top-level knobs that affect
//! render-scale, quality/performance profiles, and deterministic seeding.

/// Maximum amount of pixels the supersampled resolution can use before clamping.
pub(crate) const MAX_RENDER_PIXELS: u64 = 16_777_216;

/// Absolute output size cap for generated PNG files.
pub(crate) const MAX_OUTPUT_BYTES: usize = 10 * 1024 * 1024;

/// Minimum allowed output dimension used when shrinking oversized outputs.
pub(crate) const MIN_IMAGE_DIMENSION: u32 = 64;

/// Parsed command-line arguments and derived generation parameters.
#[derive(Debug)]
pub(crate) struct Config {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) symmetry: u32,
    pub(crate) iterations: u32,
    pub(crate) seed: u32,
    pub(crate) fill_scale: f32,
    pub(crate) fractal_zoom: f32,
    pub(crate) fast: bool,
    pub(crate) layers: Option<u32>,
    pub(crate) count: u32,
    pub(crate) output: String,
    pub(crate) antialias: u32,
}

/// Performance profile settings derived from resolution and image count.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FastProfile {
    pub(crate) iteration_cap: u32,
    pub(crate) layer_cap: u32,
    pub(crate) render_side_cap: u32,
}

impl FastProfile {
    /// Build a profile with no extra caps.
    pub(crate) const fn unlimited() -> Self {
        Self {
            iteration_cap: u32::MAX,
            layer_cap: u32::MAX,
            render_side_cap: u32::MAX,
        }
    }
}

/// Pick a coarse speed profile for high-resolution batch runs.
pub(crate) fn resolve_fast_profile(
    render_width: u32,
    image_count: u32,
    fast_enabled: bool,
) -> FastProfile {
    if !fast_enabled {
        return FastProfile::unlimited();
    }

    if render_width >= 2048 {
        if image_count >= 40 {
            return FastProfile {
                iteration_cap: 56,
                layer_cap: 1,
                render_side_cap: 1400,
            };
        }

        if image_count >= 20 {
            return FastProfile {
                iteration_cap: 180,
                layer_cap: 2,
                render_side_cap: 1400,
            };
        }
        if image_count >= 10 {
            return FastProfile {
                iteration_cap: 120,
                layer_cap: 2,
                render_side_cap: 1500,
            };
        }
        return FastProfile {
            iteration_cap: 280,
            layer_cap: 3,
            render_side_cap: 1800,
        };
    }

    if render_width >= 1536 {
        if image_count >= 10 {
            return FastProfile {
                iteration_cap: 140,
                layer_cap: 3,
                render_side_cap: 1700,
            };
        }
        return FastProfile {
            iteration_cap: 340,
            layer_cap: 4,
            render_side_cap: 1900,
        };
    }

    FastProfile::unlimited()
}

/// Tighten fast-profile caps when running on a CPU fallback adapter.
///
/// CPU fallback runs can exhibit very high tail latency when expensive CPU
/// strategies are picked repeatedly. This helper applies stricter iteration and
/// layer limits to keep latency predictable without changing the visual pipeline.
pub(crate) fn apply_cpu_fallback_profile(
    base: FastProfile,
    render_width: u32,
    image_count: u32,
    enabled: bool,
) -> FastProfile {
    if !enabled {
        return base;
    }

    let cpu_iteration_cap = if render_width >= 2048 {
        140
    } else if render_width >= 1536 {
        170
    } else {
        230
    };
    let cpu_layer_cap = if image_count >= 12 { 3 } else { 4 };
    let cpu_side_cap = if render_width >= 2048 {
        1600
    } else if render_width >= 1536 {
        1700
    } else {
        u32::MAX
    };

    FastProfile {
        iteration_cap: base.iteration_cap.min(cpu_iteration_cap),
        layer_cap: base.layer_cap.min(cpu_layer_cap),
        render_side_cap: base.render_side_cap.min(cpu_side_cap),
    }
}

/// Estimate a per-image CPU strategy budget used to avoid expensive tail-latency picks.
pub(crate) fn resolve_strategy_budget(
    render_width: u32,
    render_height: u32,
    layer_count: u32,
    iterations: u32,
    fast: bool,
    cpu_fallback_safe: bool,
) -> u32 {
    let pixel_scale = ((u64::from(render_width) * u64::from(render_height)) / 1_048_576) as u32;
    let pixel_scale = pixel_scale.max(1);
    let per_layer = if cpu_fallback_safe {
        24
    } else if fast {
        90
    } else {
        140
    };
    let iter_factor = if cpu_fallback_safe {
        14
    } else if fast {
        10
    } else {
        8
    };
    let iter_budget = (iterations / iter_factor).clamp(12, 120);
    let layer_budget = layer_count.saturating_mul(per_layer);
    layer_budget
        .saturating_add(iter_budget)
        .saturating_mul(pixel_scale.clamp(1, 4))
}

/// Resolve whether GPU/CPU buffers should be temporarily rendered at capped side length.
pub(crate) fn resolve_fast_resolution(
    width: u32,
    height: u32,
    requested_antialias: u32,
    profile: FastProfile,
) -> (u32, u32, u32, bool) {
    if profile.render_side_cap == u32::MAX {
        return (width, height, requested_antialias, false);
    }

    let requested_side_cap = profile.render_side_cap.max(1);
    let mut render_width = width;
    let mut render_height = height;

    if render_width > requested_side_cap {
        render_width = requested_side_cap;
    }
    if render_height > requested_side_cap {
        render_height = requested_side_cap;
    }

    if render_width == width && render_height == height {
        return (width, height, requested_antialias, false);
    }

    (render_width, render_height, 1, true)
}

/// Resolve initial supersampled render resolution.
pub(crate) fn resolve_render_resolution(
    width: u32,
    height: u32,
    requested_supersample: u32,
) -> (u32, u32, u32) {
    let mut supersample = requested_supersample.max(1);
    while supersample > 1 {
        let Some(render_width) = width.checked_mul(supersample) else {
            supersample -= 1;
            continue;
        };
        let Some(render_height) = height.checked_mul(supersample) else {
            supersample -= 1;
            continue;
        };

        let render_pixels = u64::from(render_width) * u64::from(render_height);
        if render_pixels <= MAX_RENDER_PIXELS {
            return (render_width, render_height, supersample);
        }

        supersample -= 1;
    }

    (width, height, 1)
}

/// Clamp iteration count to a hard upper bound.
pub(crate) fn clamp_iteration_count(iterations: u32, cap: u32) -> u32 {
    iterations.min(cap)
}

/// Clamp layer count to a hard upper bound.
pub(crate) fn clamp_layer_count(layer_count: u32, cap: u32) -> u32 {
    layer_count.min(cap)
}

#[cfg(test)]
mod tests {
    use super::{
        apply_cpu_fallback_profile, clamp_iteration_count, clamp_layer_count, resolve_fast_profile,
        resolve_fast_resolution, resolve_render_resolution, resolve_strategy_budget,
    };

    #[test]
    fn profile_disables_limits_when_disabled() {
        let profile = resolve_fast_profile(4096, 1, false);
        assert_eq!(profile.iteration_cap, u32::MAX);
        assert_eq!(profile.layer_cap, u32::MAX);
        assert_eq!(profile.render_side_cap, u32::MAX);
    }

    #[test]
    fn resolution_scales_down_to_render_budget() {
        let (w, h, aa) = resolve_render_resolution(8192, 512, 1);
        let max_pixels = super::MAX_RENDER_PIXELS as f64;
        assert_eq!(aa, 1);
        assert!(w as f64 * h as f64 <= max_pixels);
        assert!(w <= 8192);
        assert!(h <= 512);
    }

    #[test]
    fn fast_resolution_caps_side_lengths() {
        let profile = resolve_fast_profile(2048, 25, true);
        let (w, h, aa, rendered) = resolve_fast_resolution(3000, 3000, 2, profile);
        assert_eq!(aa, 1);
        assert!(rendered);
        assert!(w <= 1400);
        assert!(h <= 1400);
    }

    #[test]
    fn clamp_helpers_work_with_large_inputs() {
        assert_eq!(clamp_iteration_count(400, 128), 128);
        assert_eq!(clamp_layer_count(3, 1), 1);
    }

    #[test]
    fn cpu_profile_tightens_limits_when_enabled() {
        let base = resolve_fast_profile(2048, 1, false);
        let cpu = apply_cpu_fallback_profile(base, 2048, 20, true);
        assert!(cpu.iteration_cap < u32::MAX);
        assert!(cpu.layer_cap < u32::MAX);
        assert!(cpu.render_side_cap < u32::MAX);
    }

    #[test]
    fn strategy_budget_scales_with_pixels_and_layers() {
        let small = resolve_strategy_budget(512, 512, 2, 180, true, true);
        let large = resolve_strategy_budget(2048, 2048, 6, 280, true, true);
        assert!(large > small);
    }
}
