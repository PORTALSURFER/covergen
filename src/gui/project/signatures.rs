use super::*;

impl GuiProject {
    pub(super) fn bump_render_epoch(&mut self) {
        self.render_epoch = self.render_epoch.wrapping_add(1);
        self.bump_nodes_epoch();
        self.bump_tex_eval_epoch();
        self.render_signature_cache = self.compute_render_signature();
        self.graph_signature_cache =
            compose_graph_signature(self.render_signature_cache, self.ui_signature_cache);
    }

    pub(super) fn bump_ui_epoch(&mut self) {
        self.ui_epoch = self.ui_epoch.wrapping_add(1);
        self.ui_signature_cache = signature_from_ui_epoch(self.ui_epoch);
        self.graph_signature_cache =
            compose_graph_signature(self.render_signature_cache, self.ui_signature_cache);
    }

    pub(super) fn bump_nodes_epoch(&mut self) {
        self.nodes_epoch = self.nodes_epoch.wrapping_add(1);
    }

    pub(super) fn bump_wires_epoch(&mut self) {
        self.wires_epoch = self.wires_epoch.wrapping_add(1);
    }

    pub(super) fn bump_tex_eval_epoch(&mut self) {
        self.tex_eval_epoch = self.tex_eval_epoch.wrapping_add(1);
    }

    pub(super) fn compute_render_signature(&self) -> u64 {
        let mut hash = 0xcbf29ce484222325_u64;
        for node in &self.nodes {
            hash = fnv1a_u64(hash, node.id as u64);
            for byte in node.kind.stable_id().as_bytes() {
                hash = fnv1a_u64(hash, *byte as u64);
            }
            if let Some(texture_input) = node.texture_input {
                hash = fnv1a_u64(hash, texture_input as u64);
            }
            hash = fnv1a_u64(hash, 0xff);
            for slot in &node.params {
                for byte in slot.key.as_bytes() {
                    hash = fnv1a_u64(hash, *byte as u64);
                }
                hash = fnv1a_u64(hash, slot.value.to_bits() as u64);
                if let Some(source) = slot.signal_source {
                    hash = fnv1a_u64(hash, source as u64);
                }
                if let Some(source) = slot.texture_source {
                    hash = fnv1a_u64(hash, source as u64);
                }
            }
            hash = fnv1a_u64(hash, 0xfe);
        }
        hash
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn render_signature(&self) -> u64 {
        self.render_signature_cache
    }

    /// Return scoped project invalidation epochs for retained GUI subtrees.
    pub(crate) fn invalidation(&self) -> GuiProjectInvalidation {
        GuiProjectInvalidation {
            nodes: self.nodes_epoch,
            wires: self.wires_epoch,
            tex_eval: self.tex_eval_epoch,
        }
    }

    /// Return stable signature for UI-only node-editor state.
    ///
    /// This cached signature tracks UI epoch updates for node-card expansion,
    /// row selection, and node positioning without forcing render invalidation.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn ui_signature(&self) -> u64 {
        self.ui_signature_cache
    }

    /// Return stable signature for both render and UI graph state.
    ///
    /// Prefer [`Self::render_signature`] for tex/render invalidation.
    #[allow(dead_code)]
    pub(crate) fn graph_signature(&self) -> u64 {
        self.graph_signature_cache
    }

    /// Return true when at least one parameter has a live signal binding.
    pub(crate) fn has_signal_bindings(&self) -> bool {
        self.nodes
            .iter()
            .any(|node| node.params.iter().any(|slot| slot.signal_source.is_some()))
    }

    /// Return true when the graph contains time-driven nodes.
    ///
    /// This includes nodes that change output over time without explicit
    /// signal bindings, such as feedback and buffer noise deformation.
    pub(crate) fn has_temporal_nodes(&self) -> bool {
        self.nodes.iter().any(|node| {
            matches!(
                node.kind,
                ProjectNodeKind::TexFeedback
                    | ProjectNodeKind::TexReactionDiffusion
                    | ProjectNodeKind::BufNoise
            )
        })
    }
}

fn fnv1a_u64(hash: u64, data: u64) -> u64 {
    (hash ^ data).wrapping_mul(0x100000001b3)
}

pub(super) fn signature_from_ui_epoch(epoch: u64) -> u64 {
    let hash = fnv1a_u64(0xcbf29ce484222325_u64, SIGNATURE_DOMAIN_UI);
    fnv1a_u64(hash, epoch)
}

pub(super) fn compose_graph_signature(render_signature: u64, ui_signature: u64) -> u64 {
    fnv1a_u64(render_signature, ui_signature)
}
