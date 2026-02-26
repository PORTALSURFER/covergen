//! Generate-score-select loop for still-image rendering.
//!
//! This module explores low-resolution candidates, scores them for composition,
//! novelty, and temporal stability, then renders the top-scoring seeds at full
//! output quality.

use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;
use std::time::Instant;

use crate::compiler::CompiledGraph;
use crate::gpu_render::GpuLayerRenderer;
use crate::runtime::{
    apply_motion_temporal_constraints, create_renderer, create_runtime_buffers,
    finalize_luma_for_output, indexed_output, render_graph_frame, RuntimeBuffers,
};
use crate::runtime_config::{SelectionConfig, V2Config};
use crate::selection::{score_candidate, top_k, CandidateScore};
use crate::telemetry;
use crate::{image_ops::resolve_output_path, image_ops::save_png_under_10mb, node::GraphTimeInput};

thread_local! {
    static LOW_RES_RESOURCE_POOL: RefCell<HashMap<LowResResourceKey, LowResSelectionResources>> =
        RefCell::new(HashMap::new());
}

/// Resource key used to reuse low-resolution selection renderers across runs.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct LowResResourceKey {
    graph_width: u32,
    graph_height: u32,
    output_width: u32,
    output_height: u32,
    alias_luma_slots: usize,
    alias_mask_slots: usize,
    feedback_slots: usize,
}

impl LowResResourceKey {
    fn for_run(config: &V2Config, compiled: &CompiledGraph) -> Self {
        Self {
            graph_width: compiled.width,
            graph_height: compiled.height,
            output_width: config.width,
            output_height: config.height,
            alias_luma_slots: compiled.resource_plan.gpu_peak_luma_slots,
            alias_mask_slots: compiled.resource_plan.gpu_peak_mask_slots,
            feedback_slots: compiled.feedback_slots.len(),
        }
    }
}

/// Pooled low-resolution renderer and scratch buffers.
struct LowResSelectionResources {
    renderer: GpuLayerRenderer,
    buffers: RuntimeBuffers,
    primary_probe: Vec<u8>,
    temporal_probe: Vec<u8>,
}

impl LowResSelectionResources {
    fn new(renderer: GpuLayerRenderer, buffers: RuntimeBuffers) -> Self {
        let probe_len = buffers.output_gray.len();
        Self {
            renderer,
            buffers,
            primary_probe: vec![0u8; probe_len],
            temporal_probe: vec![0u8; probe_len],
        }
    }

    fn ensure_probe_lengths(&mut self) {
        let required = self.buffers.output_gray.len();
        if self.primary_probe.len() != required {
            self.primary_probe.resize(required, 0);
        }
        if self.temporal_probe.len() != required {
            self.temporal_probe.resize(required, 0);
        }
    }
}

/// Lease wrapper that returns low-resolution resources back to the pool on drop.
struct LowResResourceLease {
    key: LowResResourceKey,
    resources: Option<LowResSelectionResources>,
}

impl LowResResourceLease {
    fn new(key: LowResResourceKey, resources: LowResSelectionResources) -> Self {
        Self {
            key,
            resources: Some(resources),
        }
    }

    fn resources_mut(&mut self) -> Result<&mut LowResSelectionResources, Box<dyn Error>> {
        self.resources
            .as_mut()
            .ok_or("low-res resource lease is empty".into())
    }
}

impl Drop for LowResResourceLease {
    fn drop(&mut self) {
        let Some(resources) = self.resources.take() else {
            return;
        };
        return_low_res_resources(self.key, resources);
    }
}

/// Return true when runtime should execute the selection pass.
pub(crate) fn should_use_selection(
    selection: &SelectionConfig,
    low_res_explore: Option<(&V2Config, &CompiledGraph)>,
) -> bool {
    selection.enabled() && low_res_explore.is_some()
}

/// Execute still rendering using low-res candidate exploration and score ranking.
pub(crate) async fn execute_still_with_selection(
    config: &V2Config,
    compiled: &CompiledGraph,
    renderer: &mut GpuLayerRenderer,
    buffers: &mut RuntimeBuffers,
    low_res_config: &V2Config,
    low_res_compiled: &CompiledGraph,
) -> Result<(), Box<dyn Error>> {
    let acquire_start = Instant::now();
    let mut low_res_lease = acquire_low_res_resources(low_res_config, low_res_compiled).await?;
    telemetry::record_timing(
        "v2.selection.low_res_resources.acquire",
        acquire_start.elapsed(),
    );
    let low_res_resources = low_res_lease.resources_mut()?;
    let low_res_renderer = &mut low_res_resources.renderer;
    let low_res_buffers = &mut low_res_resources.buffers;
    let primary_probe = &mut low_res_resources.primary_probe;
    let temporal_probe = &mut low_res_resources.temporal_probe;

    let candidate_count = config.selection.explore_candidates.max(config.count);
    let mut scored = Vec::with_capacity(candidate_count as usize);
    let mut prior_histograms = Vec::with_capacity(candidate_count as usize);
    let stability_time = apply_motion_temporal_constraints(
        GraphTimeInput::from_frame(1, 48).with_intensity(0.25),
        low_res_config.animation.motion,
    );
    let base_seed = config.seed.wrapping_add(compiled.seed);
    println!(
        "[v2] selecting {} outputs from {} low-res candidates ({}x{})",
        config.count, candidate_count, low_res_config.width, low_res_config.height
    );

    let explore_start = Instant::now();
    for candidate_index in 0..candidate_count {
        low_res_renderer.reset_feedback_state()?;
        let seed_offset = base_seed.wrapping_add(candidate_index.wrapping_mul(0x9E37_79B9));
        render_graph_frame(low_res_compiled, low_res_renderer, seed_offset, None)?;
        finalize_luma_for_output(low_res_config, low_res_renderer, low_res_buffers)?;
        primary_probe.copy_from_slice(&low_res_buffers.output_gray);

        render_graph_frame(
            low_res_compiled,
            low_res_renderer,
            seed_offset,
            Some(stability_time),
        )?;
        finalize_luma_for_output(low_res_config, low_res_renderer, low_res_buffers)?;
        temporal_probe.copy_from_slice(&low_res_buffers.output_gray);

        let breakdown = score_candidate(
            primary_probe,
            temporal_probe,
            low_res_config.width,
            low_res_config.height,
            &prior_histograms,
        );
        prior_histograms.push(breakdown.histogram);
        scored.push(CandidateScore {
            candidate_index,
            seed_offset,
            total: breakdown.total,
            composition: breakdown.composition,
            novelty: breakdown.novelty,
            stability: breakdown.stability,
        });
    }
    telemetry::record_timing("v2.selection.explore.total", explore_start.elapsed());

    let selected = top_k(scored, config.count as usize);
    for (output_index, winner) in selected.into_iter().enumerate() {
        let output_index = output_index as u32;
        let image_start = Instant::now();
        telemetry::snapshot_memory(format!("v2.image.{output_index}.start"));
        renderer.reset_feedback_state()?;

        let render_start = Instant::now();
        render_graph_frame(compiled, renderer, winner.seed_offset, None)?;
        telemetry::record_timing("v2.image.render", render_start.elapsed());

        let finalize_start = Instant::now();
        finalize_luma_for_output(config, renderer, buffers)?;
        telemetry::record_timing("v2.image.finalize", finalize_start.elapsed());

        let output_start = Instant::now();
        let indexed_output = indexed_output(&config.output, output_index, config.count);
        let output_path = resolve_output_path(&indexed_output.to_string_lossy());
        let (w, h, bytes) = save_png_under_10mb(
            &output_path,
            config.width,
            config.height,
            &buffers.output_gray,
        )?;
        telemetry::record_timing("v2.image.output", output_start.elapsed());
        telemetry::record_timing("v2.image.total", image_start.elapsed());
        telemetry::snapshot_memory(format!("v2.image.{output_index}.end"));

        println!(
            "[v2] generated {} | score {:.3} (comp {:.3} nov {:.3} stab {:.3}) | graph {}x{} -> output {}x{} | nodes {} | outputs {} | {:.2}MB",
            output_path.display(),
            winner.total,
            winner.composition,
            winner.novelty,
            winner.stability,
            compiled.width,
            compiled.height,
            w,
            h,
            compiled.steps.len(),
            compiled.output_bindings.len(),
            bytes as f64 / (1024.0 * 1024.0)
        );
    }

    telemetry::snapshot_memory("v2.run.end");
    Ok(())
}

async fn acquire_low_res_resources(
    low_res_config: &V2Config,
    low_res_compiled: &CompiledGraph,
) -> Result<LowResResourceLease, Box<dyn Error>> {
    let key = LowResResourceKey::for_run(low_res_config, low_res_compiled);
    if let Some(mut cached) = take_low_res_resources(key) {
        cached.ensure_probe_lengths();
        return Ok(LowResResourceLease::new(key, cached));
    }

    let mut renderer = create_renderer(low_res_config, low_res_compiled).await?;
    renderer.ensure_node_alias_buffers(
        low_res_compiled.resource_plan.gpu_peak_luma_slots,
        low_res_compiled.resource_plan.gpu_peak_mask_slots,
    )?;
    renderer.ensure_node_feedback_buffers(low_res_compiled.feedback_slots.len())?;
    let buffers = create_runtime_buffers(
        low_res_compiled.width,
        low_res_compiled.height,
        low_res_config.width,
        low_res_config.height,
    )?;
    let resources = LowResSelectionResources::new(renderer, buffers);
    Ok(LowResResourceLease::new(key, resources))
}

fn take_low_res_resources(key: LowResResourceKey) -> Option<LowResSelectionResources> {
    LOW_RES_RESOURCE_POOL.with(|pool| pool.borrow_mut().remove(&key))
}

fn return_low_res_resources(key: LowResResourceKey, resources: LowResSelectionResources) {
    LOW_RES_RESOURCE_POOL.with(|pool| {
        pool.borrow_mut().insert(key, resources);
    });
}
