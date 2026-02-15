//! Configuration and execution-configuration helpers for image generation.
//!
//! This module centralizes CLI parsing and top-level knobs that affect
//! render-scale, quality/performance profiles, and deterministic seeding.

use std::{
    env,
    error::Error,
    time::{SystemTime, UNIX_EPOCH},
};

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

impl Config {
    /// Parse CLI flags into a validated configuration.
    pub(crate) fn from_env() -> Result<Self, Box<dyn Error>> {
        let mut args = env::args().skip(1);
        let mut cfg = Config {
            width: 1024,
            height: 1024,
            symmetry: 4,
            iterations: 320,
            seed: random_seed(),
            fill_scale: 1.35,
            fractal_zoom: 0.72,
            fast: false,
            layers: None,
            count: 1,
            output: "fractal.png".to_string(),
            antialias: 1,
        };

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--size" => {
                    let value = args.next().ok_or("missing size value, pass --size <u32>")?;
                    let size = value.parse::<u32>()?;
                    cfg.width = size;
                    cfg.height = size;
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
                "--zoom" => {
                    let value = args.next().ok_or("missing zoom value, pass --zoom <f32>")?;
                    cfg.fractal_zoom = value.parse()?;
                }
                "--fast" => {
                    cfg.fast = true;
                }
                "--layers" => {
                    cfg.layers = Some(
                        args.next()
                            .ok_or("missing layers value, pass --layers <u32>")?
                            .parse()?,
                    );
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
                "--antialias" | "--aa" => {
                    cfg.antialias = args
                        .next()
                        .ok_or("missing antialias value, pass --antialias <1|2|3|4>")?
                        .parse()?;
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
        if cfg.fractal_zoom <= 0.0 {
            return Err("zoom must be greater than 0".into());
        }
        if let Some(0) = cfg.layers {
            return Err("layers must be greater than 0".into());
        }
        if cfg.count == 0 {
            return Err("count must be at least 1".into());
        }
        if cfg.antialias == 0 {
            return Err("antialias must be at least 1".into());
        }

        Ok(cfg)
    }
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

/// Build a time-based seed for initial random state.
pub(crate) fn random_seed() -> u32 {
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

#[cfg(test)]
mod tests {
    use super::{
        clamp_iteration_count, clamp_layer_count, resolve_fast_profile, resolve_fast_resolution,
        resolve_render_resolution,
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
}
