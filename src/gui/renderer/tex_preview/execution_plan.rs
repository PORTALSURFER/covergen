//! Operation planning helpers for tex preview execution.
//!
//! This planner performs two low-risk optimizations before GPU submission:
//! - collapse adjacent `Transform` operations into one fused render step
//! - keep `StoreTexture` as explicit steps for downstream blend dependencies

use crate::gui::tex_view::TexViewerOp;

/// One transform payload used by fused transform execution.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TransformParams {
    pub(super) brightness: f32,
    pub(super) gain_r: f32,
    pub(super) gain_g: f32,
    pub(super) gain_b: f32,
    pub(super) alpha_mul: f32,
}

impl TransformParams {
    /// Extract transform parameters when `op` is a `Transform`.
    pub(super) fn from_transform_op(op: TexViewerOp) -> Option<Self> {
        let TexViewerOp::Transform {
            brightness,
            gain_r,
            gain_g,
            gain_b,
            alpha_mul,
        } = op
        else {
            return None;
        };
        Some(Self {
            brightness,
            gain_r,
            gain_g,
            gain_b,
            alpha_mul,
        })
    }
}

/// One render operation in a compiled tex execution plan.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum PlannedRenderOp {
    /// Render one runtime op directly.
    Runtime(TexViewerOp),
    /// Render two adjacent transform operations in one fused pass.
    TransformPair {
        first: TransformParams,
        second: TransformParams,
    },
}

/// One execution step in submission order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PlannedStep {
    /// Render operation index into the `planned_render_ops` array.
    Render { render_index: usize },
    /// Snapshot current output for one texture-node id.
    StoreTexture { texture_node_id: u32 },
}

/// Build a fused execution plan from runtime operations.
pub(super) fn build_execution_plan(
    ops: &[TexViewerOp],
    planned_steps: &mut Vec<PlannedStep>,
    planned_render_ops: &mut Vec<PlannedRenderOp>,
) {
    planned_steps.clear();
    planned_render_ops.clear();
    let mut index = 0usize;
    while let Some(op) = ops.get(index).copied() {
        match op {
            TexViewerOp::StoreTexture { texture_node_id } => {
                planned_steps.push(PlannedStep::StoreTexture { texture_node_id });
                index += 1;
            }
            TexViewerOp::Transform { .. } => {
                let first = TransformParams::from_transform_op(op).expect("checked match");
                if let Some(second) = ops
                    .get(index + 1)
                    .copied()
                    .and_then(TransformParams::from_transform_op)
                {
                    let render_index = planned_render_ops.len();
                    planned_render_ops.push(PlannedRenderOp::TransformPair { first, second });
                    planned_steps.push(PlannedStep::Render { render_index });
                    index += 2;
                    continue;
                }
                let render_index = planned_render_ops.len();
                planned_render_ops.push(PlannedRenderOp::Runtime(op));
                planned_steps.push(PlannedStep::Render { render_index });
                index += 1;
            }
            _ => {
                let render_index = planned_render_ops.len();
                planned_render_ops.push(PlannedRenderOp::Runtime(op));
                planned_steps.push(PlannedStep::Render { render_index });
                index += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{build_execution_plan, PlannedRenderOp, PlannedStep};
    use crate::gui::tex_view::TexViewerOp;

    #[test]
    fn planner_fuses_adjacent_transform_steps() {
        let ops = vec![
            TexViewerOp::Solid {
                color_r: 0.2,
                color_g: 0.3,
                color_b: 0.4,
                alpha: 1.0,
            },
            TexViewerOp::Transform {
                brightness: 1.1,
                gain_r: 1.0,
                gain_g: 1.0,
                gain_b: 1.0,
                alpha_mul: 0.8,
            },
            TexViewerOp::Transform {
                brightness: 0.9,
                gain_r: 0.8,
                gain_g: 0.7,
                gain_b: 0.6,
                alpha_mul: 0.5,
            },
        ];
        let mut planned_steps = Vec::new();
        let mut planned_render_ops = Vec::new();
        build_execution_plan(&ops, &mut planned_steps, &mut planned_render_ops);

        assert_eq!(planned_steps.len(), 2);
        assert_eq!(planned_render_ops.len(), 2);
        assert!(matches!(
            planned_render_ops[1],
            PlannedRenderOp::TransformPair { .. }
        ));
    }

    #[test]
    fn planner_keeps_store_texture_barrier_between_transforms() {
        let ops = vec![
            TexViewerOp::Transform {
                brightness: 1.0,
                gain_r: 1.0,
                gain_g: 1.0,
                gain_b: 1.0,
                alpha_mul: 1.0,
            },
            TexViewerOp::StoreTexture { texture_node_id: 7 },
            TexViewerOp::Transform {
                brightness: 0.8,
                gain_r: 0.9,
                gain_g: 1.0,
                gain_b: 1.0,
                alpha_mul: 0.7,
            },
        ];
        let mut planned_steps = Vec::new();
        let mut planned_render_ops = Vec::new();
        build_execution_plan(&ops, &mut planned_steps, &mut planned_render_ops);

        assert_eq!(planned_steps.len(), 3);
        assert_eq!(planned_render_ops.len(), 2);
        assert!(matches!(
            planned_steps[1],
            PlannedStep::StoreTexture { texture_node_id: 7 }
        ));
        assert!(matches!(
            planned_render_ops[0],
            PlannedRenderOp::Runtime(TexViewerOp::Transform { .. })
        ));
        assert!(matches!(
            planned_render_ops[1],
            PlannedRenderOp::Runtime(TexViewerOp::Transform { .. })
        ));
    }
}
