//! Parameter randomization and strategy-selection heuristics.

use crate::model::{
    ArtStyle, BlurConfig, FilterMode, GradientConfig, GradientMode, LayerBlendMode, SymmetryStyle,
    XorShift32,
};
use crate::strategies::{RenderStrategy, pick_render_strategy_near_family_with_preferences};

pub(crate) fn randomize_symmetry(base: u32, rng: &mut XorShift32) -> u32 {
    if rng.next_f32() < 0.34 {
        return 1;
    }

    if rng.next_f32() < 0.56 {
        return 2 + (rng.next_u32() % 8);
    }

    if base <= 1 {
        return 2 + (rng.next_u32() % 5);
    }

    let full_range: u32 = 12;
    if rng.next_f32() < 0.45 {
        return 2 + (rng.next_u32() % 7);
    }

    let spread = (base as f32 * 0.65).round() as u32;
    let low = base.saturating_sub(spread).max(1);
    let high = (base.saturating_add(spread)).max(low + 1).min(full_range);
    low + (rng.next_u32() % (high - low + 1))
}

pub(crate) fn randomize_iterations(base: u32, rng: &mut XorShift32) -> u32 {
    let low = (base as f32 * 0.28).floor().max(96.0) as u32;
    let high = (base as f32 * 3.2).ceil().max(300.0) as u32;
    low + (rng.next_u32() % (high - low + 1))
}

pub(crate) fn randomize_fill_scale(base: f32, rng: &mut XorShift32) -> f32 {
    let jitter = 0.65 + (rng.next_f32() * 1.2);
    (base * jitter).clamp(0.6, 2.4)
}

pub(crate) fn randomize_zoom(base: f32, rng: &mut XorShift32) -> f32 {
    let jitter = 0.42 + (rng.next_f32() * 1.18);
    (base * jitter).clamp(0.35, 1.6)
}

pub(crate) fn randomize_center_offset(rng: &mut XorShift32, fast: bool) -> (f32, f32) {
    let center_lock = if fast { 0.12 } else { 0.18 };
    if rng.next_f32() < center_lock {
        return (0.0, 0.0);
    }

    let max_shift = if fast { 0.24 } else { 0.44 };
    let radius = max_shift * rng.next_f32().sqrt();
    let angle = rng.next_f32() * std::f32::consts::TAU;
    (radius * angle.cos(), radius * angle.sin())
}

pub(crate) fn modulate_center_offset(base: f32, rng: &mut XorShift32, fast: bool) -> f32 {
    let jitter = (rng.next_f32() * 2.0 - 1.0) * if fast { 0.12 } else { 0.20 };
    (base + jitter).clamp(-0.5, 0.5)
}

pub(crate) fn pick_bend_strength(rng: &mut XorShift32) -> f32 {
    1.5 * rng.next_f32()
}

pub(crate) fn pick_warp_strength(rng: &mut XorShift32) -> f32 {
    1.5 * rng.next_f32()
}

pub(crate) fn pick_warp_frequency(rng: &mut XorShift32) -> f32 {
    0.6 + (rng.next_f32() * 5.2)
}

pub(crate) fn pick_tile_scale(rng: &mut XorShift32) -> f32 {
    0.45 + (rng.next_f32() * 1.0)
}

pub(crate) fn pick_tile_phase(rng: &mut XorShift32) -> f32 {
    rng.next_f32()
}

pub(crate) fn pick_art_style(rng: &mut XorShift32) -> ArtStyle {
    ArtStyle::from_u32(rng.next_u32())
}

pub(crate) fn modulate_art_style(base: ArtStyle, rng: &mut XorShift32, fast: bool) -> ArtStyle {
    let roll = rng.next_f32();
    let stride = 1 + (rng.next_u32() % (ArtStyle::total() - 1));
    if fast && base.is_tiling_like() && rng.next_f32() < 0.80 {
        return ArtStyle::next_non_tiling_from(rng);
    }

    if fast {
        if roll < 0.22 {
            return base;
        }
        if roll < 0.44 {
            return ArtStyle::from_u32(base.as_u32() + stride);
        }
        if roll < 0.58 {
            return ArtStyle::from_u32(base.as_u32() + ArtStyle::total() - 1);
        }
        return pick_art_style(rng);
    }

    if roll < 0.20 {
        return base;
    }
    if roll < 0.50 {
        return ArtStyle::from_u32(base.as_u32() + stride);
    }
    if roll < 0.74 {
        return ArtStyle::from_u32(base.as_u32() + ArtStyle::total() - 1);
    }
    pick_art_style(rng)
}

pub(crate) fn pick_art_style_secondary(base: ArtStyle, rng: &mut XorShift32) -> ArtStyle {
    let secondary = pick_art_style(rng);
    if secondary.as_u32() == base.as_u32() {
        ArtStyle::from_u32(base.as_u32() + 1)
    } else {
        secondary
    }
}

pub(crate) fn modulate_style_mix(base: f32, rng: &mut XorShift32, fast: bool) -> f32 {
    let spread = if fast { 0.15 } else { 0.28 };
    let jitter = (rng.next_f32() * 2.0 - 1.0) * spread;
    (base + jitter).clamp(0.0, 1.0)
}

pub(crate) fn pick_layer_count(rng: &mut XorShift32, user_count: Option<u32>, fast: bool) -> u32 {
    if let Some(fixed) = user_count {
        return fixed;
    }

    if fast {
        2 + (rng.next_u32() % 5)
    } else {
        2 + (rng.next_u32() % 7)
    }
}

pub(crate) fn pick_shader_layer_count(
    base_layer_count: u32,
    rng: &mut XorShift32,
    fast: bool,
) -> u32 {
    let base = (base_layer_count.clamp(1, 14)) as f32;
    let spread = if fast { 0.25 } else { 0.45 };
    let factor = (1.0 - spread) + (rng.next_f32() * (2.0 * spread));
    (base * factor).clamp(1.0, 14.0).round() as u32
}

pub(crate) fn modulate_shader_layer_count(
    base_layer_count: u32,
    rng: &mut XorShift32,
    fast: bool,
    structural: bool,
) -> u32 {
    let mut base = base_layer_count.clamp(1, 14) as f32;
    let spread = if structural {
        0.12
    } else if fast {
        0.20
    } else {
        0.34
    };
    let drift = (rng.next_f32() * 2.0 - 1.0) * spread;
    base *= 1.0 + drift;
    if structural {
        base = base.max(2.0);
    }
    base.clamp(1.0, 14.0).round() as u32
}

pub(crate) fn modulate_symmetry(base: u32, rng: &mut XorShift32, fast: bool) -> u32 {
    if rng.next_f32() < if fast { 0.18 } else { 0.10 } {
        return 1;
    }

    if base <= 1 {
        return 2;
    }

    let jitter = if fast { 4 } else { 6 };
    if rng.next_f32() < 0.30 {
        return 2 + (rng.next_u32() % 15);
    }

    let jitter_range = jitter.min(base - 1);
    let shift = (rng.next_u32() % (jitter_range * 2 + 1)) as i32 - jitter_range as i32;
    ((base as i32 + shift).clamp(1, 16)) as u32
}

pub(crate) fn modulate_symmetry_style(
    base: u32,
    rng: &mut XorShift32,
    fast: bool,
    allow_grid: bool,
) -> u32 {
    let keep_base = if fast { 0.24 } else { 0.30 };
    let roll = rng.next_f32();
    let mut style = if roll < keep_base {
        SymmetryStyle::from_u32(base)
    } else {
        let sampled = SymmetryStyle::from_u32(base + rng.next_u32());
        if sampled.as_u32() == base {
            SymmetryStyle::from_u32(pick_symmetry_style(rng))
        } else {
            sampled
        }
    };

    if allow_grid {
        if style == SymmetryStyle::Grid && rng.next_f32() > 0.01 {
            return pick_non_grid_symmetry_style(rng).as_u32();
        }
    } else if style == SymmetryStyle::Grid {
        style = pick_non_grid_symmetry_style(rng);
    }

    style.as_u32()
}

pub(crate) fn should_apply_grid_across_layers(
    base_style: SymmetryStyle,
    layer_count: u32,
    rng: &mut XorShift32,
    fast: bool,
) -> bool {
    if base_style != SymmetryStyle::Grid || layer_count <= 1 {
        return false;
    }

    let base_chance = if fast { 0.001 } else { 0.004 };
    let layer_scale = ((layer_count as f32) / 8.0).clamp(0.45, 1.0);
    rng.next_f32() < (base_chance * layer_scale)
}

pub(crate) fn resolve_symmetry_style(
    base_style: SymmetryStyle,
    apply_to_all_layers: bool,
    rng: &mut XorShift32,
) -> SymmetryStyle {
    if base_style == SymmetryStyle::Grid && !apply_to_all_layers {
        return pick_non_grid_symmetry_style(rng);
    }

    base_style
}

pub(crate) fn modulate_iterations(base: u32, rng: &mut XorShift32, fast: bool) -> u32 {
    let spread = if fast { 0.18 } else { 0.42 };
    let factor = (1.0 - spread) + (rng.next_f32() * (2.0 * spread));
    let value = (base as f32 * factor).max(64.0).round() as u32;
    value.max(1)
}

pub(crate) fn modulate_fill_scale(base: f32, rng: &mut XorShift32, fast: bool) -> f32 {
    let spread = if fast { 0.12 } else { 0.24 };
    let factor = (1.0 - spread) + (rng.next_f32() * (2.0 * spread));
    (base * factor).clamp(0.80, 2.4)
}

pub(crate) fn modulate_bend_strength(base: f32, rng: &mut XorShift32, fast: bool) -> f32 {
    let spread = if fast { 0.30 } else { 0.65 };
    (base + ((rng.next_f32() * 2.0 - 1.0) * spread)).clamp(0.0, 1.9)
}

pub(crate) fn modulate_warp_strength(base: f32, rng: &mut XorShift32, fast: bool) -> f32 {
    let spread = if fast { 0.30 } else { 0.65 };
    (base + ((rng.next_f32() * 2.0 - 1.0) * spread)).clamp(0.0, 1.9)
}

pub(crate) fn modulate_warp_frequency(base: f32, rng: &mut XorShift32, fast: bool) -> f32 {
    let spread = if fast { 0.15 } else { 0.35 };
    (base + ((rng.next_f32() * 2.0 - 1.0) * spread)).clamp(0.2, 6.2)
}

pub(crate) fn modulate_tile_scale(
    base: f32,
    for_grid: bool,
    rng: &mut XorShift32,
    fast: bool,
) -> f32 {
    let spread = if fast { 0.18 } else { 0.33 };
    let clamp_max = if for_grid { 1.2 } else { 1.7 };
    (base + ((rng.next_f32() * 2.0 - 1.0) * spread)).clamp(0.22, clamp_max)
}

pub(crate) fn modulate_tile_phase(base: f32, rng: &mut XorShift32, fast: bool) -> f32 {
    let spread = if fast { 0.30 } else { 0.62 };
    (base + ((rng.next_f32() * 2.0 - 1.0) * spread)).rem_euclid(1.0)
}

pub(crate) fn modulate_zoom(base: f32, rng: &mut XorShift32, fast: bool) -> f32 {
    let spread = if fast { 0.18 } else { 0.30 };
    let factor = (1.0 - spread) + (rng.next_f32() * (2.0 * spread));
    (base * factor).clamp(0.35, 1.65)
}

pub(crate) fn bias_layer_strategy(
    current: RenderStrategy,
    rng: &mut XorShift32,
    fast: bool,
    prefer_gpu: bool,
    budget_left: u32,
) -> RenderStrategy {
    let switch_prob = if fast { 0.06 } else { 0.04 };
    if rng.next_f32() < switch_prob {
        let family_bias = if fast { 0.9 } else { 0.86 };
        pick_render_strategy_near_family_with_preferences(
            rng,
            fast,
            current,
            family_bias,
            prefer_gpu,
            budget_left,
        )
    } else {
        current
    }
}

pub(crate) fn pick_layer_blend(rng: &mut XorShift32) -> LayerBlendMode {
    LayerBlendMode::from_u32(rng.next_u32())
}

pub(crate) fn pick_layer_contrast(rng: &mut XorShift32, fast: bool) -> f32 {
    let low = if fast { 1.18 } else { 1.35 };
    let high = if fast { 1.58 } else { 1.95 };
    low + (rng.next_f32() * (high - low))
}

pub(crate) fn layer_opacity(rng: &mut XorShift32) -> f32 {
    0.30 + (rng.next_f32() * 0.55)
}

pub(crate) fn pick_symmetry_style(rng: &mut XorShift32) -> u32 {
    let roll = rng.next_f32();
    if roll < 0.02 {
        SymmetryStyle::Grid.as_u32()
    } else if roll < 0.52 {
        SymmetryStyle::Radial.as_u32()
    } else if roll < 0.80 {
        SymmetryStyle::None.as_u32()
    } else if roll < 0.88 {
        SymmetryStyle::Mirror.as_u32()
    } else if roll < 0.93 {
        SymmetryStyle::MirrorX.as_u32()
    } else if roll < 0.97 {
        SymmetryStyle::MirrorY.as_u32()
    } else if roll < 0.99 {
        SymmetryStyle::MirrorDiagonal.as_u32()
    } else {
        SymmetryStyle::MirrorCross.as_u32()
    }
}

pub(crate) fn pick_non_grid_symmetry_style(rng: &mut XorShift32) -> SymmetryStyle {
    let roll = rng.next_f32();
    if roll < 0.20 {
        SymmetryStyle::None
    } else if roll < 0.52 {
        SymmetryStyle::Radial
    } else if roll < 0.72 {
        SymmetryStyle::Mirror
    } else if roll < 0.82 {
        SymmetryStyle::MirrorX
    } else if roll < 0.92 {
        SymmetryStyle::MirrorY
    } else if roll < 0.96 {
        SymmetryStyle::MirrorDiagonal
    } else {
        SymmetryStyle::MirrorCross
    }
}

pub(crate) fn pick_filter_from_rng(rng: &mut XorShift32) -> BlurConfig {
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

pub(crate) fn should_apply_dynamic_filter(
    layer_index: u32,
    rng: &mut XorShift32,
    fast: bool,
    structural_profile: bool,
    strategy_bias: f32,
) -> bool {
    let base: f32 = if fast { 0.26 } else { 0.20 };
    let layer_bias: f32 = if layer_index == 0 { -0.20 } else { 0.12 };
    let strategy_bias = strategy_bias.clamp(0.0, 1.5);
    let threshold = ((base + layer_bias) * strategy_bias).clamp(0.02, 0.95);
    let adjusted = if structural_profile {
        threshold * 0.35
    } else {
        threshold
    };
    rng.next_f32() < adjusted
}

pub(crate) fn should_apply_gradient_map(
    layer_index: u32,
    rng: &mut XorShift32,
    fast: bool,
    structural_profile: bool,
    strategy_bias: f32,
) -> bool {
    let base: f32 = if fast { 0.34 } else { 0.26 };
    let layer_bias: f32 = if layer_index == 0 { -0.25 } else { 0.00 };
    let strategy_bias = strategy_bias.clamp(0.0, 1.5);
    let threshold = ((base + layer_bias) * strategy_bias).clamp(0.05, 0.95);
    let adjusted = if structural_profile {
        threshold * 0.33
    } else {
        threshold
    };
    rng.next_f32() < adjusted
}

pub(crate) fn should_use_structural_profile(fast: bool, rng: &mut XorShift32) -> bool {
    let threshold = if fast { 0.55 } else { 0.38 };
    rng.next_f32() < threshold
}

pub(crate) fn tune_filter_for_speed(cfg: BlurConfig, fast: bool) -> BlurConfig {
    if !fast {
        return cfg;
    }

    BlurConfig {
        mode: cfg.mode,
        max_radius: (cfg.max_radius / 2).clamp(1, 4),
        axis_x: cfg.axis_x.signum(),
        axis_y: cfg.axis_y.signum(),
        softness: cfg.softness.min(2),
    }
}

pub(crate) fn pick_gradient_from_rng(rng: &mut XorShift32) -> GradientConfig {
    let mode = GradientMode::from_u32(rng.next_u32());
    let gamma = 0.45 + (rng.next_u32() % 160) as f32 * 0.01;
    let contrast = 0.6 + (rng.next_u32() % 240) as f32 * 0.01;
    let pivot = 0.25 + (rng.next_u32() % 70) as f32 * 0.01;
    let invert = rng.next_u32().is_multiple_of(2);
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
