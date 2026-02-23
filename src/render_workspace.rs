//! Workspace buffers reused across the whole generation run.
//!
//! Keeping the large per-stage buffers in one place avoids scattered local
//! allocations in the render loop and reduces allocator churn for large image
//! batches.

/// Reusable image buffers allocated once per process execution.
#[derive(Debug)]
pub(crate) struct RenderWorkspace {
    /// Temporary per-layer filter and blur scratch.
    pub(crate) filtered: Vec<f32>,
    /// Secondary scratch for post-process combinations.
    pub(crate) detail: Vec<f32>,
    /// Secondary strategy render output before masking.
    pub(crate) blend_secondary: Vec<f32>,
    /// Layer-mask buffer used when mixing strategies.
    pub(crate) mix_mask: Vec<f32>,
    /// Temporary scratch for mask filtering to avoid per-layer allocations.
    pub(crate) mask_workspace: Vec<f32>,
    /// Accumulated layered image for the final output size.
    pub(crate) layered: Vec<f32>,
    /// Current primary rendered layer.
    pub(crate) luma: Vec<f32>,
    /// Soft background seed.
    pub(crate) background: Vec<f32>,
    /// Scratch array for percentile sorting/sample collection.
    pub(crate) percentile: Vec<f32>,
    /// Final (possibly resized) luminance buffer before encoding.
    pub(crate) final_luma: Vec<f32>,
    /// Reusable byte scratch used while downsampling `f32` luma via GrayImage.
    pub(crate) downsample_source_u8: Vec<u8>,
    /// Grayscale output bytes.
    pub(crate) final_pixels: Vec<u8>,
}

impl RenderWorkspace {
    /// Allocate all scratch buffers once.
    pub(crate) fn new(render_pixel_count: usize, final_pixel_count: usize) -> Self {
        Self {
            filtered: vec![0.0f32; render_pixel_count],
            detail: vec![0.0f32; render_pixel_count],
            blend_secondary: vec![0.0f32; render_pixel_count],
            mix_mask: vec![0.0f32; render_pixel_count],
            mask_workspace: vec![0.0f32; render_pixel_count],
            layered: vec![0.0f32; render_pixel_count],
            luma: vec![0.0f32; render_pixel_count],
            background: vec![0.0f32; render_pixel_count],
            percentile: vec![0.0f32; render_pixel_count],
            final_luma: vec![0.0f32; final_pixel_count],
            downsample_source_u8: Vec::new(),
            final_pixels: vec![0u8; final_pixel_count],
        }
    }

    /// Reset the layer accumulation buffer for the next image.
    pub(crate) fn reset_layered(&mut self) {
        self.layered.fill(0.0);
    }
}
