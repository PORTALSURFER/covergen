struct Params {
    width: u32,
    height: u32,
    symmetry: u32,
    symmetry_style: u32,
    iterations: u32,
    seed: u32,
    fill_scale: f32,
    fractal_zoom: f32,
    art_style: u32,
    art_style_secondary: u32,
    art_style_mix: f32,
    bend_strength: f32,
    warp_strength: f32,
    warp_frequency: f32,
    tile_scale: f32,
    tile_phase: f32,
    center_x: f32,
    center_y: f32,
    layer_count: u32,
}

@group(0) @binding(0)
var<storage, read_write> out_pixels: array<u32>;

@group(0) @binding(1)
var<uniform> params: Params;

fn hash01(x: f32, y: f32, seed: u32) -> f32 {
    let s = f32(seed) * 0.00000011920928955;
    return fract(sin(dot(vec2<f32>(x, y), vec2<f32>(12.9898, 78.233)) + s) * 43758.5453123);
}

fn apply_domain_warp(px: f32, py: f32) -> vec2<f32> {
    var p = vec2<f32>(px, py);
    let seed = f32(params.seed) * 0.00000011920928955;
    let bend = clamp(params.bend_strength, 0.0, 1.8);
    let warp = clamp(params.warp_strength, 0.0, 1.8);
    let freq = clamp(params.warp_frequency, 0.25, 6.0);

    let radius = max(length(p), 0.0001);
    let angle = atan2(p.y, p.x);

    let radial = 1.0
        + (bend * (0.16 + 0.05 * freq) * cos(radius * (2.8 + freq) + seed + angle * 0.75))
        + (warp * (0.10 + 0.03 * freq) * sin(radius * 4.0 + angle + seed));
    let angular = bend * (0.32 + 0.07 * freq) * sin(angle * (2.2 + freq) + seed * 2.0);
    let warped_angle = angle + angular;
    let warped_radius = radius * radial;

    var wx = warped_radius * cos(warped_angle);
    var wy = warped_radius * sin(warped_angle);
    let wave = (sin((wx * (4.0 + 0.9 * freq)) + (seed * 7.0)) + 1.0) * 0.5;
    let wave2 = (cos((wy * (3.8 + 0.6 * freq)) + (seed * 5.0)) + 1.0) * 0.5;
    wx = wx + (wave - 0.5) * (0.5 * warp);
    wy = wy + (wave2 - 0.5) * (0.5 * warp);

    p = vec2<f32>(wx, wy);

    let swirl = sin((wx + wy + seed * 13.0) * (1.8 + 0.6 * freq)) * (bend * 0.08);
    return vec2<f32>(
        p.x * cos(swirl) - p.y * sin(swirl),
        p.x * sin(swirl) + p.y * cos(swirl),
    );
}

fn fold_for_symmetry(px: f32, py: f32, symmetry: u32, style: u32) -> vec2<f32> {
    var repeats = f32(symmetry) * 0.55 * clamp(params.tile_scale, 0.25, 1.8);
    repeats = repeats + sin((params.tile_phase * 6.283185307179586) * 0.22);
    repeats = clamp(repeats, 1.2, 3.5);
    if (style == 0u) {
        return vec2<f32>(px, py);
    }

    if (style == 2u) {
        return fold_symmetry_mirror(px, py, symmetry);
    }

    if (style == 3u) {
        return fold_symmetry_mirror_x(px, py, symmetry);
    }

    if (style == 4u) {
        return fold_symmetry_mirror_y(px, py, symmetry);
    }

    if (style == 5u) {
        return fold_symmetry_mirror_diagonal(px, py, symmetry);
    }

    if (style == 6u) {
        return fold_symmetry_mirror_cross(px, py, symmetry);
    }

    if (style == 7u) {
        return fold_symmetry_grid(px, py, symmetry, repeats, params.tile_phase);
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

fn fold_symmetry_mirror_x(px: f32, py: f32, symmetry: u32) -> vec2<f32> {
    if (symmetry <= 1u) {
        return vec2<f32>(px, py);
    }
    if (symmetry > 3u) {
        return vec2<f32>(abs(px), abs(py));
    }

    return vec2<f32>(abs(px), py);
}

fn fold_symmetry_mirror_y(px: f32, py: f32, symmetry: u32) -> vec2<f32> {
    if (symmetry <= 1u) {
        return vec2<f32>(px, py);
    }
    if (symmetry > 3u) {
        return vec2<f32>(abs(px), abs(py));
    }

    return vec2<f32>(px, abs(py));
}

fn fold_symmetry_mirror_diagonal(px: f32, py: f32, symmetry: u32) -> vec2<f32> {
    let c = 0.70710678;
    let s = 0.70710678;
    let rotated_x = (px * c) + (py * s);
    let rotated_y = (-px * s) + (py * c);
    let folded = fold_symmetry_mirror(rotated_x, rotated_y, symmetry);
    let unrotated_x = (folded.x * c) - (folded.y * s);
    let unrotated_y = (folded.x * s) + (folded.y * c);
    return vec2<f32>(unrotated_x, unrotated_y);
}

fn fold_symmetry_mirror_cross(px: f32, py: f32, symmetry: u32) -> vec2<f32> {
    let first = fold_symmetry_mirror(abs(px), abs(py), max(symmetry, 2u));
    let second = fold_symmetry_mirror(first.x, first.y, max(symmetry, 2u));
    return vec2<f32>(abs(second.x), abs(second.y));
}

fn fold_symmetry_grid(
    px: f32,
    py: f32,
    symmetry: u32,
    repeats: f32,
    phase: f32,
) -> vec2<f32> {
    let skew = (phase * 6.283185307179586) + f32(symmetry) * 0.1;
    let rot = sin(skew) * 0.4;
    let c = cos(skew);
    let s = sin(skew);
    let rpx = px * c - py * s + (phase * 0.3);
    let rpy = px * s + py * c;

    let x_grid = fract((rpx * repeats) + (phase * 2.0) + sin(rpy * 2.0) * 0.25) - 0.5;
    let y_scale = 1.0 + sin(phase * 4.0) * 0.25;
    let y_grid = fract((rpy * repeats * y_scale) + (phase * 3.0) + cos(rpx * 3.0) * 0.2) - 0.5;

    let sx = abs(x_grid) * 2.0 - 0.5;
    let sy = abs(y_grid) * 2.0 - 0.5;
    let pulse = 0.03 * sin((sx * 12.0) + (sy * 9.0) + skew);
    return vec2<f32>(sx * (2.0 / repeats) + sy * 0.08 + rot * pulse, (sy * (2.0 / repeats)) + pulse * 1.8);
}

fn pack_gray(level: f32) -> u32 {
    let c = u32(clamp(level, 0.0, 1.0) * 255.0 + 0.5);
    return (255u << 24u) | (c << 16u) | (c << 8u) | c;
}

fn hybrid_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011920928955;
    let jitter = hash01(x + base, y - base, params.seed + layer);
    let layer_shift = f32(layer) * 0.24 + (jitter - 0.5) * 0.35;
    let angle = layer_shift;
    let phase = hash01(x * 3.1, y * 4.7, params.seed + layer + 1u);
    let radius = 0.42 + 0.06 * f32(layer) + (jitter - 0.5) * 0.08;
    let cx = (radius * x * (3.2 + 0.2 * jitter)) + 0.16 * (sin(angle) + cos(angle * 0.7 + base));
    let cy = (radius * y * (3.0 + 0.2 * (1.0 - jitter))) + 0.16 * (cos(angle) + sin(angle * 0.9 - base));

    let rot_x = x * cos(angle) - y * sin(angle);
    let rot_y = x * sin(angle) + y * cos(angle);
    let orbit_scale = 1.62 + (f32(layer) * 0.02) + (phase - 0.5) * 0.15;
    var zx = rot_x * orbit_scale;
    var zy = rot_y * orbit_scale;
    var i: u32 = 0u;
    var mag2 = 0.0;
    var escaped = false;
    var orbit_depth = 0.0;
    var z_angle = 0.0;
    var c_angle = 0.0;

    loop {
        if (i >= params.iterations) {
            break;
        }
        if (mag2 > 4.0) {
            escaped = true;
            break;
        }

        let radius2 = max(mag2, 0.0001);
        let z_radius = sqrt(radius2);
        z_angle = atan2(zy, zx);
        let mode = hash01(f32(i) * 0.17 + base, f32(layer) * 0.11 + base, params.seed ^ (i + layer));
        let power = 2.0 + (phase * 0.6) + (mode * 0.7);
        let twist = 0.12 * sin(z_angle * 5.0 + phase * 6.28318530718 + f32(i) * 0.03);
        let scaled = pow(z_radius, power) * 0.55;
        c_angle = z_angle * power + twist;
        let polar_x = scaled * cos(c_angle);
        let polar_y = scaled * sin(c_angle);
        let folded_x = abs(polar_x) + 0.18 * (0.5 - mode);
        let folded_y = abs(polar_y) + 0.12 * (0.5 - phase);
        let p1 = folded_x - folded_y * 0.35;
        let p2 = folded_y + folded_x * 0.28;

        var x2 = p1 + cx;
        var y2 = 1.5 * p2 + cy + cos(twist * 2.0);
        if (mode >= 0.33 && mode < 0.66) {
            x2 = (p1 * p1 - p2 * p2) * 0.5 + cx;
            y2 = (2.0 * p1 * p2) + cy;
        } else if (mode >= 0.66) {
            x2 = polar_x + p2 * 0.24 + cx + sin(twist * 3.0);
            y2 = polar_y + cx * 0.18 + cos(polar_x * 1.4);
        }

        zx = x2 + (jitter - 0.5) * 0.09;
        zy = y2 + (1.0 - (f32(layer) + 1.0) * 0.06) * (mode - 0.5) * 0.22;
        mag2 = zx * zx + zy * zy;
        orbit_depth = orbit_depth + abs(z_radius - 1.0);
        i = i + 1u;
    }

    let iter_ratio = f32(i) / max(f32(params.iterations), 1.0);
    let escape_term = 1.0 - iter_ratio;
    let trap_term = 1.0 - clamp(orbit_depth / (f32(params.iterations) * 0.65 + 1.0), 0.0, 1.0);
    let angle_term = 0.5 + 0.5 * sin(z_angle + c_angle + base);
    let detail_term = hash01(zx, zy, params.seed + layer + i);
    let warp_term = 0.5 + 0.5 * sin(zx * 2.3 + zy * 1.9 + base);
    let value = (0.20 * angle_term) + (0.20 * trap_term) + (0.20 * escape_term) + (0.20 * detail_term) + (0.20 * warp_term);

    return select(0.0, clamp(value, 0.0, 1.0), escaped);
}

fn julia_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011928955;
    let cx = (hash01(x * 1.7, y * 1.3, params.seed + layer + 11u) - 0.5) * 1.5;
    let cy = (hash01(x * 2.1, y * 1.9, params.seed + layer + 17u) - 0.5) * 1.5;
    let phase = hash01(x * 5.1, y * 4.7, params.seed + layer) * 6.28318530718;
    var zx = x * 0.95 + cos(phase) * 0.23;
    var zy = y * 0.95 + sin(phase) * 0.23;
    var i: u32 = 0u;
    var orbit = 0.0;
    var mag2 = 0.0;
    var escaped = false;

    loop {
        if (i >= params.iterations) {
            break;
        }
        if (mag2 > 16.0) {
            escaped = true;
            break;
        }

        let x2 = zx * zx - zy * zy + cx;
        let y2 = 2.0 * zx * zy + cy;
        zx = x2;
        zy = y2;
        mag2 = zx * zx + zy * zy;
        orbit = orbit + (1.0 - exp(-mag2 * 0.2));
        i = i + 1u;
    }

    let iter_term = 1.0 - f32(i) / max(f32(params.iterations), 1.0);
    let orbit_term = clamp(orbit / max(f32(params.iterations), 1.0), 0.0, 1.0);
    let swirl = 0.5 + 0.5 * sin(orbit_term * 6.28318530718 + base);
    return select(0.0, clamp((0.60 * iter_term) + (0.30 * orbit_term) + (0.10 * swirl), 0.0, 1.0), escaped);
}

fn burning_ship_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011928955;
    let cx = (hash01(x * 2.2, y * 1.4, params.seed + layer + 23u) - 0.5) * 1.35;
    let cy = (hash01(x * 1.6, y * 2.4, params.seed + layer + 29u) - 0.5) * 1.35;
    var zx = x + sin(base) * 0.2;
    var zy = y + cos(base) * 0.2;
    var i: u32 = 0u;
    var mag2 = 0.0;
    var escaped = false;
    var distance = 0.0;

    loop {
        if (i >= params.iterations) {
            break;
        }
        if (mag2 > 16.0) {
            escaped = true;
            break;
        }

        let zx_abs = abs(zx);
        let zy_abs = abs(zy);
        let x2 = zx_abs * zx_abs - zy_abs * zy_abs + cx;
        let y2 = 2.0 * zx_abs * zy_abs + cy;
        zx = x2;
        zy = y2;
        mag2 = zx * zx + zy * zy;
        distance = distance + (1.0 - exp(-mag2 * 0.18));
        i = i + 1u;
    }

    let iter_term = 1.0 - f32(i) / max(f32(params.iterations), 1.0);
    let shape = clamp(distance / max(f32(params.iterations), 1.0), 0.0, 1.0);
    let warp = 0.5 + 0.5 * cos(iter_term * 4.0 + shape * 3.0);
    return select(0.0, clamp((0.65 * iter_term) + (0.25 * shape) + (0.10 * warp), 0.0, 1.0), escaped);
}

fn tricorn_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011928955;
    let cx = (hash01(x * 1.9, y * 2.3, params.seed + layer + 31u) - 0.5) * 1.2;
    let cy = (hash01(x * 2.7, y * 1.8, params.seed + layer + 37u) - 0.5) * 1.2;
    var zx = x;
    var zy = y;
    var i: u32 = 0u;
    var mag2 = 0.0;
    var escaped = false;
    var trail = 0.0;

    loop {
        if (i >= params.iterations) {
            break;
        }
        if (mag2 > 12.0) {
            escaped = true;
            break;
        }

        let x2 = zx * zx - zy * zy + cx;
        let y2 = -2.0 * zx * zy + cy;
        zx = x2;
        zy = y2;
        mag2 = x2 * x2 + y2 * y2;
        trail = trail + (0.5 * hash01(x2, y2, params.seed + layer + i));
        i = i + 1u;
    }

    let iter_term = 1.0 - f32(i) / max(f32(params.iterations), 1.0);
    let trail_term = clamp(trail / max(f32(params.iterations), 1.0), 0.0, 1.0);
    let pulse = 0.5 + 0.5 * sin(base * 7.0 + trail_term * 9.0);
    return select(0.0, clamp((0.66 * iter_term) + (0.24 * trail_term) + (0.10 * pulse), 0.0, 1.0), escaped);
}

fn phoenix_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011928955;
    let cx = (hash01(x * 1.7, y * 2.1, params.seed + layer + 41u) - 0.5) * 1.1;
    let cy = (hash01(x * 2.9, y * 1.1, params.seed + layer + 47u) - 0.5) * 1.1;
    let p = (hash01(x * 0.8, y * 0.6, params.seed + layer + 53u) - 0.5) * 0.6;
    var zx = x;
    var zy = y;
    var px = 0.0;
    var py = 0.0;
    var i: u32 = 0u;
    var mag = 0.0;
    var escaped = false;
    var phase = 0.0;

    loop {
        if (i >= params.iterations) {
            break;
        }
        if (mag > 10.0) {
            escaped = true;
            break;
        }

        let x2 = zx * zx - zy * zy + cx + p * px;
        let y2 = 2.0 * zx * zy + cy + p * py;
        px = zx;
        py = zy;
        zx = x2;
        zy = y2;
        mag = zx * zx + zy * zy;
        phase = phase + (mag * 0.05);
        i = i + 1u;
    }

    let iter_term = 1.0 - f32(i) / max(f32(params.iterations), 1.0);
    let phase_term = fract(phase * 0.1);
    let wave = 0.5 + 0.5 * sin(phase_term * 12.0 + base);
    return select(0.0, clamp((0.70 * iter_term) + (0.30 * wave), 0.0, 1.0), escaped);
}

fn orbit_trap_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011928955;
    let cx = (hash01(x * 3.3, y * 1.8, params.seed + layer + 59u) - 0.5) * 0.9;
    let cy = (hash01(x * 1.8, y * 3.1, params.seed + layer + 61u) - 0.5) * 0.9;
    var zx = x * 0.98;
    var zy = y * 0.98;
    let ring = 0.25 + 0.35 * (hash01(x, y, params.seed + layer) * 0.5);
    var i: u32 = 0u;
    var mag2 = 0.0;
    var escaped = false;
    var trap = 1e9;

    loop {
        if (i >= params.iterations) {
            break;
        }
        if (mag2 > 20.0) {
            escaped = true;
            break;
        }

        let n = zx * zx - zy * zy + cx;
        let m = 2.0 * zx * zy + cy;
        zx = n;
        zy = m;
        mag2 = zx * zx + zy * zy;
        let radius = sqrt(mag2);
        trap = min(trap, abs(radius - ring));
        i = i + 1u;
    }

    let iter_term = 1.0 - f32(i) / max(f32(params.iterations), 1.0);
    let trap_term = 1.0 - clamp(trap * 1.2, 0.0, 1.0);
    let fold = 0.5 + 0.5 * cos(iter_term * 7.2 + base);
    return select(0.0, clamp((0.55 * iter_term) + (0.35 * trap_term) + (0.10 * fold), 0.0, 1.0), escaped);
}

fn field_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    var octave = 0u;
    var frequency = 1.5;
    var amplitude = 1.0;
    var total = 0.0;
    var denom = 0.0;
    let base = f32(params.seed) * 0.00000011928955;
    let drift = f32(layer) * 0.19 + base;
    let cx = hash01(x, y, params.seed + layer + 71u);
    let cy = hash01(y, x, params.seed + layer + 79u);

    loop {
        if (octave >= 5u) {
            break;
        }

        let sx = (x * frequency) + (cx * 3.0);
        let sy = (y * frequency) + (cy * 3.0);
        let wave = 0.5 + 0.5 * sin((sx + drift) * 1.5 + cos(sy * 1.3));
        let l1 = 0.5 + 0.5 * cos((sx * 1.9 - sy * 2.1) + drift);
        let ridge = hash01(sx, sy, params.seed + octave + layer) * 0.6;
        total = total + (wave * l1 * ridge) * amplitude;
        denom = denom + amplitude;
        frequency = frequency * 2.0;
        amplitude = amplitude * 0.58;
        octave = octave + 1u;
    }

    return clamp(total / max(denom, 1.0), 0.0, 1.0);
}

fn mandelbrot_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011928955;
    let cx = x * 1.25 + (hash01(x * 1.1, y * 1.2, params.seed + layer + 101u) - 0.5) * 0.14;
    let cy = y * 1.25 + (hash01(x * 1.3, y * 0.9, params.seed + layer + 103u) - 0.5) * 0.14;
    var zx = 0.0;
    var zy = 0.0;
    var i: u32 = 0u;
    var mag2 = 0.0;
    var escaped = false;
    var phase = 0.0;
    let jitter = hash01(x + base, y - base, params.seed + layer + 109u);

    loop {
        if (i >= params.iterations) {
            break;
        }
        if (mag2 > 24.0) {
            escaped = true;
            break;
        }

        let x2 = zx * zx - zy * zy + cx + (jitter - 0.5) * 0.06;
        let y2 = 2.0 * zx * zy + cy + (jitter - 0.5) * 0.05;
        zx = x2;
        zy = y2;
        mag2 = x2 * x2 + y2 * y2;
        phase = phase + sin(mag2 + f32(i) * 0.03 + base);
        i = i + 1u;
    }

    escaped = escaped || (i >= params.iterations);
    let iter_term = 1.0 - f32(i) / max(f32(params.iterations), 1.0);
    let orbit_term = (sin(phase + base + f32(layer) * 0.11) + 1.0) * 0.5;
    let radial_term = 0.5 + 0.5 * cos(sqrt(max(mag2, 0.0001)) * 0.85 + base);
    return select(
        0.0,
        clamp((0.5 * iter_term) + (0.30 * orbit_term) + (0.20 * radial_term), 0.0, 1.0),
        escaped,
    );
}

fn nova_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011928955;
    let rx = hash01(x * 1.4, y * 1.4, params.seed + layer + 131u) * 0.22;
    let ry = hash01(y * 1.8, x * 1.6, params.seed + layer + 137u) * 0.22;
    var zx = x * 0.96 + (rx - 0.11);
    var zy = y * 0.96 + (ry - 0.11);
    var angle = atan2(zy, zx);
    var i: u32 = 0u;
    var mag2 = 0.0;
    var escaped = false;
    var glow = 0.0;

    loop {
        if (i >= params.iterations) {
            break;
        }
        if (mag2 > 80.0) {
            escaped = true;
            break;
        }

        let r = sqrt(max(mag2, 1e-6));
        let spin = sin(r * 2.7 + angle * 1.9 + base) * 0.55;
        let scale = 0.62 + 0.33 * sin((angle + base) * 2.4 + f32(i) * 0.03);
        let nx = (r * scale * cos(angle + spin * 0.24) * 1.1) + x * 0.22;
        let ny = (r * scale * sin(angle + spin * 0.24) * 1.1) + y * 0.22;
        let warp = 0.05 * (hash01(nx, ny, params.seed + layer + i) - 0.5);
        zx = nx + warp;
        zy = ny - warp;
        mag2 = zx * zx + zy * zy;
        angle = atan2(zy, zx);
        glow = glow + (0.5 + 0.5 * sin(nx * 1.6 + ny * 2.1 + f32(i) * 0.08));
        i = i + 1u;
    }

    escaped = escaped || (i >= params.iterations);
    let iter_term = 1.0 - f32(i) / max(f32(params.iterations), 1.0);
    let iters = max(f32(params.iterations), 1.0);
    let glow_term = glow / iters;
    let edge_term = 1.0 - clamp(sqrt(max(mag2, 0.0001)) * 0.06, 0.0, 1.0);
    return select(
        0.0,
        clamp((0.42 * iter_term) + (0.38 * glow_term) + (0.20 * edge_term), 0.0, 1.0),
        escaped,
    );
}

fn vortex_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011928955;
    let r_offset = hash01(x * 1.1, y * 0.9, params.seed + layer + 149u);
    let mut_x = hash01(x * 1.7, y * 0.7, params.seed + layer + 151u) - 0.5;
    let mut_y = hash01(y * 1.2, x * 1.4, params.seed + layer + 157u) - 0.5;
    var zx = x + (mut_x * 0.18);
    var zy = y + (mut_y * 0.18);
    var i: u32 = 0u;
    var mag2 = 0.0;
    var escaped = false;
    var orbit = 0.0;
    let twist = 0.15 + r_offset * 0.5;

    loop {
        if (i >= params.iterations) {
            break;
        }
        if (mag2 > 50.0) {
            escaped = true;
            break;
        }

        let r = sqrt(max(mag2, 1e-6));
        let theta = atan2(zy, zx) + twist + (1.0 / (r + 0.2)) * (0.35 + 0.15 * sin(f32(i) * 0.05 + base));
        let gain = (0.8 + 0.25 * sin(theta * 1.6 + base + f32(layer)));
        let nx = (gain * r + 0.03 * r * r) * cos(theta);
        let ny = (gain * r + 0.03 * r * r) * sin(theta);
        let spiral = 0.22 * sin((r + base) * 3.0 + f32(i) * 0.07);
        zx = nx + x * 0.26 + (spiral * (mut_x + mut_y * 0.5));
        zy = ny + y * 0.26 + (spiral * (mut_y - mut_x * 0.5));
        mag2 = zx * zx + zy * zy;
        orbit = orbit + (0.5 + 0.5 * cos(theta * 2.1 + f32(i) * 0.12));
        i = i + 1u;
    }

    escaped = escaped || (i >= params.iterations);
    let iter_term = 1.0 - f32(i) / max(f32(params.iterations), 1.0);
    let orbit_term = 0.5 + (0.5 * sin(orbit * 0.02 + base + f32(layer) * 0.1));
    let edge_term = 1.0 - clamp(sqrt(max(mag2, 0.0001)) * 0.07, 0.0, 1.0);
    return select(
        0.0,
        clamp((0.46 * iter_term) + (0.34 * orbit_term) + (0.20 * edge_term), 0.0, 1.0),
        escaped,
    );
}

fn dragon_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011928955;
    let jitter_x = hash01(x * 1.8, y * 1.3, params.seed + layer + 181u);
    let jitter_y = hash01(y * 1.4, x * 1.9, params.seed + layer + 191u);
    var zx = x * (1.15 + (jitter_x - 0.5) * 0.35);
    var zy = y * (1.15 + (jitter_y - 0.5) * 0.35);
    var i: u32 = 0u;
    var mag2 = 0.0;
    var escaped = false;
    var fold = 0.0;

    loop {
        if (i >= params.iterations) {
            break;
        }
        if (mag2 > 30.0) {
            escaped = true;
            break;
        }

        let r = sqrt(max(mag2, 1e-6));
        let angle = atan2(zy, zx);
        let t = 0.5 + 0.5 * sin(base * 8.0 + f32(i) * 0.11 + f32(layer) * 0.09);
        let fold_term = 0.85 * t + 0.15 * t * sin(angle * 2.8);
        let abs_x = abs(zx);
        let abs_y = abs(zy);
        let nx = fold_term * (abs_x * abs_x - abs_y * abs_y)
            + 0.22 * r * cos(r * 1.1 + f32(layer) * 0.3);
        let ny = fold_term * (2.0 * abs_x * abs_y)
            + 0.22 * r * sin(r * 1.1 + f32(i) * 0.13);
        zx = nx + x * 0.26;
        zy = ny + y * 0.26;
        mag2 = zx * zx + zy * zy;
        fold = fold + (0.5 + 0.5 * sin(nx * 1.8 + ny * 2.6 + base));
        i = i + 1u;
    }

    escaped = escaped || (i >= params.iterations);
    let iter_term = 1.0 - f32(i) / max(f32(params.iterations), 1.0);
    let fold_term = 0.5 + (0.5 * sin(fold * 0.03 + f32(layer) * 0.17));
    let edge_term = 1.0 - clamp(sqrt(max(mag2, 0.0001)) * 0.08, 0.0, 1.0);
    return select(
        0.0,
        clamp((0.48 * iter_term) + (0.32 * fold_term) + (0.20 * edge_term), 0.0, 1.0),
        escaped,
    );
}

fn ifs_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011928955;
    let j1 = hash01(x, y, params.seed + layer + 211u) - 0.5;
    let j2 = hash01(y, x, params.seed + layer + 223u) - 0.5;
    var zx = x + (j1 * 0.2);
    var zy = y + (j2 * 0.2);
    var i: u32 = 0u;
    var mag2 = 0.0;
    var escaped = false;
    var attract = 0.0;
    var closest = 1e9;

    loop {
        if (i >= min(params.iterations, 200u)) {
            break;
        }
        if (mag2 > 60.0) {
            escaped = true;
            break;
        }

        let selector = hash01(zx + base, zy - base, params.seed + layer + i);
        if (selector < 0.33) {
            let nx = 0.86 * zx - 0.2 * zy + 0.02 + j1 * 0.1;
            let ny = 0.2 * zx + 0.86 * zy - 0.03 + j2 * 0.1;
            zx = nx;
            zy = ny;
        } else if (selector < 0.66) {
            let nx = -0.3 * zx + 0.9 * zy + 0.31;
            let ny = -0.9 * zx - 0.3 * zy - 0.09;
            zx = nx;
            zy = ny;
        } else {
            let nx = 0.4 * zx - 0.9 * zy + 0.24;
            let ny = 0.9 * zx + 0.4 * zy + 0.07;
            zx = nx;
            zy = ny;
        }

        let distance = abs(sqrt(max(mag2, 0.0001)) - (0.55 + 0.2 * j1));
        closest = min(closest, distance);
        attract = attract + (0.5 + 0.5 * cos(f32(i) * 0.16 + distance * 12.0 + base));
        mag2 = zx * zx + zy * zy;
        i = i + 1u;
    }

    escaped = escaped || (i >= min(params.iterations, 200u));
    let iters = max(f32(min(params.iterations, 200u)), 1.0);
    let iter_term = 1.0 - f32(i) / iters;
    let attract_term = 1.0 - (closest * 0.8);
    let orbit_term = 0.5 + 0.5 * sin(attract + base + f32(layer) * 0.2);
    return select(
        0.0,
        clamp((0.40 * iter_term) + (0.35 * attract_term) + (0.25 * orbit_term), 0.0, 1.0),
        escaped,
    );
}

fn moire_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011920928955;
    let a_seed = hash01(x * 2.1, y * 1.6, params.seed + layer + 261u);
    let b_seed = hash01(y * 2.3, x * 1.8, params.seed + layer + 263u);
    var phase = (a_seed - 0.5) * 3.2;
    var px = x * (1.4 + a_seed * 0.25);
    var py = y * (1.35 + b_seed * 0.25);
    var mag2 = 0.0;
    var escaped = false;
    var energy = 0.0;
    var i: u32 = 0u;

    loop {
        if (i >= 12u) {
            break;
        }
        if (mag2 > 24.0) {
            escaped = true;
            break;
        }

        let freq = 2.4 + a_seed * 3.7 + f32(i) * 0.11;
        let pulse = 0.5 + 0.5 * sin((px * freq + phase) * 6.28318530718 + b_seed * 5.0);
        let wave = 0.5 + 0.5 * cos((py * (freq + 0.8) - phase) * 6.28318530718 - base);
        let stripe = 0.5 + 0.5 * sin((pulse + wave + sin((px + py + phase) * 2.2)) * 7.2);
        let jitter = (hash01(px, py, params.seed + layer + i + 271u) - 0.5) * 0.8;
        phase = phase + stripe + jitter;
        energy = energy + stripe;

        let c = cos(phase);
        let s = sin(phase);
        let nx = (px * c - py * s) + (pulse - 0.5) * 0.16;
        let ny = (px * s + py * c) + (wave - 0.5) * 0.16;
        px = nx * 0.86;
        py = ny * 0.86;
        mag2 = px * px + py * py;
        i = i + 1u;
    }

    let iter_term = energy / 12.0;
    let wave_term = 0.5 + 0.5 * sin(energy * 2.4 + phase + base);
    let radial_term = 1.0 - clamp(sqrt(max(mag2, 0.0001)) * 0.24, 0.0, 1.0);
    return select(
        0.0,
        clamp((0.34 * iter_term) + (0.36 * wave_term) + (0.30 * radial_term), 0.0, 1.0),
        escaped,
    );
}

fn knot_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011920928955;
    let knot_seed = hash01(x * 1.4, y * 1.9, params.seed + layer + 293u);
    let twist_seed = hash01(y * 1.1, x * 2.2, params.seed + layer + 307u);
    var px = x * (1.08 + knot_seed * 0.35);
    var py = y * (1.08 + twist_seed * 0.35);
    var i: u32 = 0u;
    var mag2 = 0.0;
    var escaped = false;
    var knot = 0.0;
    var ring = 0.0;
    let max_iter = min(params.iterations, 220u);

    loop {
        if (i >= max_iter) {
            break;
        }
        if (mag2 > 40.0) {
            escaped = true;
            break;
        }

        let radius = max(sqrt(max(mag2, 1e-6)), 0.08);
        let angle = atan2(py, px) + (base * 12.0);
        let spin = (1.0 / (radius + 0.2)) + (twist_seed * 0.45);
        let cx = cos(angle + spin + f32(i) * 0.06);
        let sx = sin(angle + spin + base);
        let fold = hash01(cx, sx, params.seed + layer + i + 311u);
        let rx = 0.9 * px + (fold - 0.5) * 0.24 + cos(radius * 3.4 + base + f32(i) * 0.13) * 0.04;
        let ry = 0.9 * py + (fold - 0.5) * 0.24 + sin(radius * 3.9 + f32(i) * 0.11 + base) * 0.04;
        let knot_line = abs(sin(radius * 2.2 + angle * 1.6 + fold * 2.4));
        knot = knot + knot_line;
        ring = ring + (1.0 - clamp(abs(radius - (0.28 + 0.15 * fold)), 0.0, 1.0));
        px = rx + cx * 0.18 * knot_line;
        py = ry + sx * 0.18 * knot_line;
        mag2 = rx * rx + ry * ry;
        i = i + 1u;
    }

    let i_term = f32(i) / max(f32(max_iter), 1.0);
    let knot_term = clamp(knot / max(f32(max_iter), 1.0), 0.0, 1.0);
    let ring_term = clamp(ring / max(f32(max_iter), 1.0), 0.0, 1.0);
    let escape_term = 1.0 - i_term;
    return select(
        0.0,
        clamp((0.36 * knot_term) + (0.30 * ring_term) + (0.34 * escape_term), 0.0, 1.0),
        escaped,
    );
}

fn radial_wave_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011920928955;
    let ring_seed = hash01(x * 2.1, y * 1.7, params.seed + layer + 317u);
    let drift_seed = hash01(x * 1.4, y * 2.2, params.seed + layer + 331u);
    var px = x * (1.10 + (ring_seed * 0.25));
    var py = y * (1.10 + ((1.0 - ring_seed) * 0.25));
    let max_iter = min(params.iterations, 180u);
    var i: u32 = 0u;
    var mag2 = 0.0;
    var escaped = false;
    var envelope = 0.0;
    var phase = base * 2.5 + drift_seed * 2.0;
    var radius = max(length(vec2<f32>(px, py)), 0.0001);
    let base_angle = atan2(py, px);

    loop {
        if (i >= max_iter) {
            break;
        }
        if (mag2 > 40.0) {
            escaped = true;
            break;
        }

        let ring = (f32(i) * 0.12) + phase;
        let harmonic = (f32(((i + layer) % 4u) + 1u) * 0.75) + (drift_seed * 1.2);
        let pulse = 0.25 + 0.15 * sin(radius * harmonic + base_angle * 2.2 + ring);
        let twist = 0.15 + (0.18 * sin(base + ring * 0.4));
        let nr = radius + (0.22 * pulse) + (0.08 * cos(ring + phase));
        let local_angle = (base_angle * twist) + (ring * 0.24) + (pulse * 0.9);

        var wx = nr * cos(local_angle);
        var wy = nr * sin(local_angle);
        let wave = sin(wx * 3.1 + wy * 1.7 + phase);
        let blend = 0.55 + (0.45 * (wave * 0.5 + 0.5));
        wx = mix(wx, px, 0.38);
        wy = mix(wy, py, 0.38);

        px = (wx * 0.68) + (0.16 * (wave + 1.0) * cos(local_angle + base_angle));
        py = (wy * 0.68) + (0.16 * (wave + 1.0) * sin(local_angle - base_angle));
        px = px * (1.0 - (0.06 * blend));
        py = py * (1.0 + (0.06 * (1.0 - blend)));

        radius = max(length(vec2<f32>(px, py)), 0.0001);
        mag2 = radius * radius;
        envelope = envelope + (1.0 - clamp(abs(radius - (0.18 + pulse * 0.28)), 0.0, 1.0));
        phase = phase + (wave * 0.07) + (f32(i) * 0.002);
        i = i + 1u;
    }

    let iters = max(f32(max_iter), 1.0);
    let iter_term = 1.0 - (f32(i) / iters);
    let ring_term = clamp(envelope / iters, 0.0, 1.0);
    let orbit_term = 1.0 - clamp(radius * 0.4, 0.0, 1.0);
    return select(
        0.0,
        clamp((0.38 * iter_term) + (0.38 * ring_term) + (0.24 * orbit_term), 0.0, 1.0),
        escaped,
    );
}

fn recursive_fold_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011920928955;
    let k1 = hash01(x * 1.3, y * 1.8, params.seed + layer + 347u);
    let k2 = hash01(x * 1.9, y * 1.6, params.seed + layer + 359u);
    let k3 = hash01(x * 2.4, y * 1.1, params.seed + layer + 367u);
    var px = x * (1.0 + (k1 - 0.5) * 0.45);
    var py = y * (1.0 + (k2 - 0.5) * 0.45);
    let max_iter = min(params.iterations, 240u);
    var i: u32 = 0u;
    var mag2 = 0.0;
    var escaped = false;
    var fold_score = 0.0;
    let seed_angle = base * 4.0;

    loop {
        if (i >= max_iter) {
            break;
        }
        if (mag2 > 70.0) {
            escaped = true;
            break;
        }

        let a = sin(seed_angle + f32(i) * 0.13);
        let b = cos(seed_angle * 1.2 + f32(i) * 0.17);
        let fold_angle = atan2(py, px) + a * 1.6;
        let c = cos(fold_angle);
        let s = sin(fold_angle);
        let rx = px * c - py * s;
        let ry = px * s + py * c;
        let fold_x = abs(rx) - (0.26 + k1 * 0.24);
        let fold_y = abs(ry * 0.95) - (0.18 + k2 * 0.28);
        let twist = 0.34 + (0.26 * k3);
        let nx = (fold_x * (1.0 + a * 0.35)) * twist;
        let ny = (fold_y * (1.0 - b * 0.35)) * twist;
        px = nx + b * 0.09;
        py = ny - a * 0.09;
        px = px + (0.15 * sin(f32(i) * 0.21 + base + k1));
        py = py + (0.15 * cos(f32(i) * 0.23 + base + k2));
        mag2 = px * px + py * py;
        let fold_density = 1.0 - clamp(abs(fold_x * fold_y + 0.1), 0.0, 1.0);
        fold_score = fold_score + fold_density * (1.0 - clamp(mag2 * 0.02, 0.0, 1.0));
        i = i + 1u;
    }

    let iters = max(f32(max_iter), 1.0);
    let iter_term = 1.0 - f32(i) / iters;
    let fold_term = clamp(fold_score / iters, 0.0, 1.0);
    let orbit_term = 1.0 - clamp((sqrt(max(mag2, 0.0001)) * 0.28), 0.0, 1.0);
    return select(
        0.0,
        clamp((0.34 * iter_term) + (0.44 * fold_term) + (0.22 * orbit_term), 0.0, 1.0),
        escaped,
    );
}

fn attractor_hybrid_style_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011920928955;
    let k1 = hash01(x * 1.9, y * 1.1, params.seed + layer + 379u);
    let k2 = hash01(x * 2.5, y * 2.2, params.seed + layer + 389u);
    let k3 = hash01(x * 1.4, y * 2.7, params.seed + layer + 397u);
    let a = 1.2 + k1 * 1.1;
    let b = -0.4 + k2 * 1.6;
    let c = -1.6 + k3 * 1.2;
    let d = 0.7 + hash01(x * 3.1, y * 1.9, params.seed + layer + 401u) * 0.9;
    var zx = x * (1.3 + (k1 - 0.5) * 0.2);
    var zy = y * (1.3 + (k2 - 0.5) * 0.2);
    var i: u32 = 0u;
    var mag2 = 0.0;
    var escaped = false;
    var attract_term = 0.0;
    var orbit_term = 0.0;
    let max_iter = min(params.iterations, 240u);

    loop {
        if (i >= max_iter) {
            break;
        }
        if (mag2 > 120.0) {
            escaped = true;
            break;
        }

        let attract_mix = fract(hash01(zx + base, zy - base, params.seed + layer + i) + (0.15 * k3));
        let attractor_x = sin(a * zy) + c * cos(b * zx);
        let attractor_y = sin(d * zx) + b * cos(c * zy);
        let fallback_x = 1.4 - (1.2 * zx * zx) + (0.3 * zy);
        let fallback_y = -0.6 * zy + 0.8 * sin(zx + base);
        let nx = mix(attractor_x, fallback_x, attract_mix);
        let ny = mix(attractor_y, fallback_y, attract_mix);
        zx = nx * 0.48 + (0.52 * zx);
        zy = ny * 0.48 + (0.52 * zy);
        mag2 = zx * zx + zy * zy;
        let trap = 1.0 - clamp(abs(sqrt(max(mag2, 0.000001)) - (0.30 + 0.20 * k2)), 0.0, 1.0);
        attract_term = attract_term + (0.5 + 0.5 * sin(trap * 9.0 + base));
        orbit_term = orbit_term + (1.0 - clamp(mag2 * 0.02, 0.0, 1.0));
        i = i + 1u;
    }

    let iters = max(f32(max_iter), 1.0);
    let iter_term = 1.0 - f32(i) / iters;
    let attract_norm = clamp(attract_term / iters, 0.0, 1.0);
    let orbit_norm = clamp(orbit_term / iters, 0.0, 1.0);
    let radius_term = 1.0 - clamp(sqrt(max(mag2, 0.0001)) * 0.09, 0.0, 1.0);
    return select(
        0.0,
        clamp((0.30 * iter_term) + (0.40 * attract_norm) + (0.20 * orbit_norm) + (0.10 * radius_term), 0.0, 1.0),
        escaped,
    );
}

fn style_value(x: f32, y: f32, params: Params, layer: u32, style_selector: u32) -> f32 {
    let style = style_selector % 17u;
    switch (style) {
        case 0u: {
            return hybrid_style_value(x, y, params, layer);
        }
        case 1u: {
            return julia_style_value(x, y, params, layer);
        }
        case 2u: {
            return burning_ship_style_value(x, y, params, layer);
        }
        case 3u: {
            return tricorn_style_value(x, y, params, layer);
        }
        case 4u: {
            return phoenix_style_value(x, y, params, layer);
        }
        case 5u: {
            return orbit_trap_style_value(x, y, params, layer);
        }
        case 6u: {
            return field_style_value(x, y, params, layer);
        }
        case 7u: {
            return mandelbrot_style_value(x, y, params, layer);
        }
        case 8u: {
            return nova_style_value(x, y, params, layer);
        }
        case 9u: {
            return vortex_style_value(x, y, params, layer);
        }
        case 10u: {
            return dragon_style_value(x, y, params, layer);
        }
        case 11u: {
            return ifs_style_value(x, y, params, layer);
        }
        case 12u: {
            return moire_style_value(x, y, params, layer);
        }
        case 13u: {
            return knot_style_value(x, y, params, layer);
        }
        case 14u: {
            return radial_wave_style_value(x, y, params, layer);
        }
        case 15u: {
            return recursive_fold_style_value(x, y, params, layer);
        }
        case 16u: {
            return attractor_hybrid_style_value(x, y, params, layer);
        }
        default: {
            return field_style_value(x, y, params, layer);
        }
    }
}

fn layer_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let primary_style = style_value(x, y, params, layer, params.art_style);
    let secondary_style = style_value(x, y, params, layer, params.art_style_secondary);
    let blend = clamp(params.art_style_mix, 0.0, 1.0);
    return (1.0 - blend) * primary_style + blend * secondary_style;
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.width || id.y >= params.height) {
        return;
    }

    var px = ((f32(id.x) + 0.5) / f32(params.width)) * 2.0 - 1.0;
    var py = ((f32(id.y) + 0.5) / f32(params.height)) * 2.0 - 1.0;
    let zoom = max(0.2, params.fractal_zoom);
    px = px * params.fill_scale * zoom;
    py = py * params.fill_scale * zoom;
    let warped = apply_domain_warp(px, py);
    px = warped.x;
    py = warped.y;
    px = px + params.center_x;
    py = py + params.center_y;
    var value = 0.0;
    let layer_count = max(1u, min(14u, params.layer_count));

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
        let layer_factor = f32(layer) / f32(layer_count);
        let weight = pow(1.0 - layer_factor, 1.2) * 1.15;
        value = value + (layer_brightness * layer_brightness * weight);
        layer = layer + 1u;
    }

    out_pixels[id.x + id.y * params.width] = pack_gray(value);
}
