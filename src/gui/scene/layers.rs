//! Retained-layer selection helpers for scene assembly.

use super::{SceneFrame, SceneLayer};

/// Active retained target layer while assembling scene primitives.
#[derive(Clone, Copy, Debug, Default)]
pub(super) enum ActiveLayer {
    StaticPanel,
    Edges,
    #[default]
    Nodes,
    SignalScopes,
    ParamWires,
    Overlays,
    Timeline,
}

/// Return mutable access to one retained scene layer.
pub(super) fn active_scene_layer_mut(
    frame: &mut SceneFrame,
    layer: ActiveLayer,
) -> &mut SceneLayer {
    match layer {
        ActiveLayer::StaticPanel => &mut frame.static_panel,
        ActiveLayer::Edges => &mut frame.edges,
        ActiveLayer::Nodes => &mut frame.nodes,
        ActiveLayer::SignalScopes => &mut frame.signal_scopes,
        ActiveLayer::ParamWires => &mut frame.param_wires,
        ActiveLayer::Overlays => &mut frame.overlays,
        ActiveLayer::Timeline => &mut frame.timeline,
    }
}
