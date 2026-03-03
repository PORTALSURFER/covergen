//! Signal-node evaluation math helpers.

/// Quantize time to a stable memo bucket for per-frame signal reuse.
pub(super) fn sample_time_bucket(time_secs: f32, buckets_per_sec: f32) -> i32 {
    let clamped = time_secs.max(0.0);
    let bucket = (clamped * buckets_per_sec).round();
    bucket.clamp(i32::MIN as f32, i32::MAX as f32) as i32
}

/// Sample one LFO waveform variant after cycle shaping.
pub(super) fn lfo_wave_sample(cycle: f32, phase_time: f32, lfo_type: usize, shape: f32) -> f32 {
    let cycle = cycle.rem_euclid(1.0);
    let shaped_cycle = apply_cycle_shape(cycle, shape);
    match lfo_type {
        1 => (2.0 * shaped_cycle) - 1.0,
        2 => 1.0 - (4.0 * (shaped_cycle - 0.5).abs()),
        3 => {
            let width = ((shape + 1.0) * 0.5).mul_add(0.8, 0.1);
            if cycle < width {
                1.0
            } else {
                -1.0
            }
        }
        4 => {
            // Drift is intentionally soft and slowly moving, using smooth 1D
            // value-noise layers over unwrapped phase time.
            let roughness = ((shape + 1.0) * 0.5).clamp(0.0, 1.0);
            let base = phase_time * (0.42 + roughness * 0.48);
            let low = smooth_value_noise(base * 0.65, 7.13);
            let mid = smooth_value_noise(base * 1.20, 19.71);
            let hi = smooth_value_noise(base * 2.30, 43.09);
            let blend = low * 0.72 + mid * 0.23 + hi * (0.05 + roughness * 0.12);
            let neighbor = smooth_value_noise((base - 0.35) * 0.65, 7.13);
            (blend * 0.78 + neighbor * 0.22).clamp(-1.0, 1.0)
        }
        _ => {
            let base = (shaped_cycle * std::f32::consts::TAU).sin();
            let harmonic = (shaped_cycle * std::f32::consts::TAU * 2.0).sin() * shape * 0.35;
            (base + harmonic).clamp(-1.0, 1.0)
        }
    }
}

fn smooth_value_noise(t: f32, offset: f32) -> f32 {
    let x = t + offset;
    let i0 = x.floor() as i32;
    let frac = x - i0 as f32;
    let v0 = hash01(i0);
    let v1 = hash01(i0 + 1);
    let smooth = frac * frac * (3.0 - 2.0 * frac);
    ((v0 + (v1 - v0) * smooth) * 2.0) - 1.0
}

fn hash01(index: i32) -> f32 {
    let value = ((index as f32 + 1.0) * 12.9898).sin() * 43_758.547;
    value - value.floor()
}

fn apply_cycle_shape(cycle: f32, shape: f32) -> f32 {
    if shape.abs() < f32::EPSILON {
        return cycle;
    }
    if shape > 0.0 {
        cycle.powf(1.0 + shape * 3.0)
    } else {
        1.0 - (1.0 - cycle).powf(1.0 + (-shape) * 3.0)
    }
}
