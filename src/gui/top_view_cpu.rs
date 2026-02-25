//! CPU fallback evaluator for GUI TOP preview rendering.
//!
//! This path is kept as a fallback and test baseline while GPU preview
//! evaluation is the default path for supported node chains.

use super::project::{GuiProject, ProjectNodeKind};

/// Fill `out` with one evaluated preview frame using the CPU fallback path.
///
/// `scratch` must be the same length as `out` and is used for transform
/// intermediate storage.
pub(crate) fn generate_output_pixels(
    out: &mut [u8],
    scratch: &mut [u8],
    width: u32,
    height: u32,
    project: &GuiProject,
    time_secs: f32,
    eval_stack: &mut Vec<u32>,
) {
    fill_rgba(out, 8, 8, 8, 255);
    if width == 0 || height == 0 {
        return;
    }
    let Some(output_source_id) = project.window_out_input_node_id() else {
        return;
    };
    let rendered = render_node(
        project,
        output_source_id,
        width,
        height,
        out,
        scratch,
        time_secs,
        eval_stack,
    );
    if !rendered {
        fill_rgba(out, 8, 8, 8, 255);
    }
}

fn fill_rgba(out: &mut [u8], r: u8, g: u8, b: u8, a: u8) {
    for px in out.chunks_exact_mut(4) {
        px[0] = r;
        px[1] = g;
        px[2] = b;
        px[3] = a;
    }
}

fn draw_circle(
    out: &mut [u8],
    width: u32,
    height: u32,
    center_x: f32,
    center_y: f32,
    radius_norm: f32,
    feather_norm: f32,
    color_r: f32,
    color_g: f32,
    color_b: f32,
    alpha_mul: f32,
) {
    let cx = (width as f32) * center_x.clamp(0.0, 1.0);
    let cy = (height as f32) * center_y.clamp(0.0, 1.0);
    let radius = (width.min(height) as f32) * radius_norm.clamp(0.01, 1.0);
    let feather = (width.min(height) as f32) * feather_norm.clamp(0.0, 0.5);
    if radius <= 1.0 {
        return;
    }
    for y in 0..height {
        for x in 0..width {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let alpha = smoothstep(radius + feather, radius - feather, dist);
            if alpha <= 0.0 {
                continue;
            }
            let idx = ((y * width + x) * 4) as usize;
            let blend = (alpha * alpha_mul.clamp(0.0, 1.0)).clamp(0.0, 1.0);
            let tr = color_r.clamp(0.0, 1.0) * 255.0;
            let tg = color_g.clamp(0.0, 1.0) * 255.0;
            let tb = color_b.clamp(0.0, 1.0) * 255.0;
            out[idx] = ((out[idx] as f32) * (1.0 - blend) + tr * blend).clamp(0.0, 255.0) as u8;
            out[idx + 1] =
                ((out[idx + 1] as f32) * (1.0 - blend) + tg * blend).clamp(0.0, 255.0) as u8;
            out[idx + 2] =
                ((out[idx + 2] as f32) * (1.0 - blend) + tb * blend).clamp(0.0, 255.0) as u8;
            out[idx + 3] = 255;
        }
    }
}

fn render_node(
    project: &GuiProject,
    node_id: u32,
    width: u32,
    height: u32,
    out: &mut [u8],
    scratch: &mut [u8],
    time_secs: f32,
    eval_stack: &mut Vec<u32>,
) -> bool {
    if eval_stack.contains(&node_id) {
        return false;
    }
    let Some(node) = project.node(node_id) else {
        return false;
    };
    eval_stack.push(node_id);
    let rendered = match node.kind() {
        ProjectNodeKind::TexSolid => {
            fill_rgba(out, 8, 8, 8, 255);
            let center_x = project
                .node_param_value(node_id, "center_x", time_secs, eval_stack)
                .unwrap_or(0.5);
            let center_y = project
                .node_param_value(node_id, "center_y", time_secs, eval_stack)
                .unwrap_or(0.5);
            let radius = project
                .node_param_value(node_id, "radius", time_secs, eval_stack)
                .unwrap_or(0.24);
            let feather = project
                .node_param_value(node_id, "feather", time_secs, eval_stack)
                .unwrap_or(0.06);
            let color_r = project
                .node_param_value(node_id, "color_r", time_secs, eval_stack)
                .unwrap_or(0.9);
            let color_g = project
                .node_param_value(node_id, "color_g", time_secs, eval_stack)
                .unwrap_or(0.9);
            let color_b = project
                .node_param_value(node_id, "color_b", time_secs, eval_stack)
                .unwrap_or(0.9);
            let alpha = project
                .node_param_value(node_id, "alpha", time_secs, eval_stack)
                .unwrap_or(1.0);
            draw_circle(
                out, width, height, center_x, center_y, radius, feather, color_r, color_g, color_b,
                alpha,
            );
            true
        }
        ProjectNodeKind::TexTransform2D => {
            if let Some(source_id) = project.input_source_node_id(node_id) {
                if !render_node(
                    project, source_id, width, height, scratch, out, time_secs, eval_stack,
                ) {
                    false
                } else {
                    out.copy_from_slice(scratch);
                    let brightness = project
                        .node_param_value(node_id, "brightness", time_secs, eval_stack)
                        .unwrap_or(1.08);
                    let gain_r = project
                        .node_param_value(node_id, "gain_r", time_secs, eval_stack)
                        .unwrap_or(0.45);
                    let gain_g = project
                        .node_param_value(node_id, "gain_g", time_secs, eval_stack)
                        .unwrap_or(0.8);
                    let gain_b = project
                        .node_param_value(node_id, "gain_b", time_secs, eval_stack)
                        .unwrap_or(1.0);
                    let alpha_mul = project
                        .node_param_value(node_id, "alpha_mul", time_secs, eval_stack)
                        .unwrap_or(0.8);
                    apply_tex_transform(out, brightness, gain_r, gain_g, gain_b, alpha_mul);
                    true
                }
            } else {
                false
            }
        }
        ProjectNodeKind::CtlLfo => false,
        ProjectNodeKind::IoWindowOut => false,
    };
    eval_stack.pop();
    rendered
}

fn apply_tex_transform(
    out: &mut [u8],
    brightness: f32,
    gain_r: f32,
    gain_g: f32,
    gain_b: f32,
    alpha_mul: f32,
) {
    for px in out.chunks_exact_mut(4) {
        let r = ((px[0] as f32) * gain_r * brightness).clamp(0.0, 255.0);
        let g = ((px[1] as f32) * gain_g * brightness).clamp(0.0, 255.0);
        let b = ((px[2] as f32) * gain_b * brightness).clamp(0.0, 255.0);
        let a = ((px[3] as f32) * alpha_mul).clamp(0.0, 255.0);
        px[0] = r as u8;
        px[1] = g as u8;
        px[2] = b as u8;
        px[3] = a as u8;
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::generate_output_pixels;
    use crate::gui::project::{GuiProject, ProjectNodeKind};

    #[test]
    fn cpu_fallback_renders_when_graph_is_connected() {
        let mut project = GuiProject::new_empty(640, 480);
        let solid = project.add_node(ProjectNodeKind::TexSolid, 40, 80, 420, 480);
        let out = project.add_node(ProjectNodeKind::IoWindowOut, 220, 80, 420, 480);
        assert!(project.connect_image_link(solid, out));

        let width = 256_u32;
        let height = 256_u32;
        let mut pixels = vec![0_u8; (width * height * 4) as usize];
        let mut scratch = vec![0_u8; pixels.len()];
        let mut eval_stack = Vec::new();
        generate_output_pixels(
            &mut pixels,
            &mut scratch,
            width,
            height,
            &project,
            0.0,
            &mut eval_stack,
        );
        assert!(pixels.iter().any(|byte| *byte != 0));
    }
}
