use std::error::Error;
use std::sync::mpsc::Receiver;

use bytemuck::{Pod, Zeroable};

mod pipeline;
use pipeline::{build_setup, map_buffer_async};

/// Uniforms for one retained blend dispatch.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(super) struct RetainedPostParams {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) blend_mode: u32,
    pub(super) _pad0: u32,
    pub(super) opacity: f32,
    pub(super) contrast: f32,
    pub(super) _pad1: f32,
    pub(super) _pad2: f32,
}

/// Uniforms for final GPU output passes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(super) struct RetainedFinalizeParams {
    pub(super) src_width: u32,
    pub(super) src_height: u32,
    pub(super) dst_width: u32,
    pub(super) dst_height: u32,
    pub(super) contrast: f32,
    pub(super) low_pct: f32,
    pub(super) high_pct: f32,
    pub(super) fast_mode: u32,
}

/// GPU resources used to retain and finalize layer output without intermediate readbacks.
#[derive(Debug)]
pub(crate) struct RetainedGpuPost {
    clear_pipeline: wgpu::ComputePipeline,
    #[cfg_attr(not(test), allow(dead_code))]
    blend_pipeline: wgpu::ComputePipeline,
    clear_hist_pipeline: wgpu::ComputePipeline,
    histogram_pipeline: wgpu::ComputePipeline,
    thresholds_pipeline: wgpu::ComputePipeline,
    finalize_pipeline: wgpu::ComputePipeline,
    bind_group: wgpu::BindGroup,
    accum_buffer: wgpu::Buffer,
    post_uniform: wgpu::Buffer,
    finalize_uniform: wgpu::Buffer,
    final_output_buffer: wgpu::Buffer,
    #[allow(dead_code)]
    staging_buffer: wgpu::Buffer,
    final_staging_buffer: wgpu::Buffer,
    width: u32,
    height: u32,
    output_width: u32,
    output_height: u32,
}

impl RetainedGpuPost {
    /// Build retained post resources for a render size and downsampled output size.
    pub(crate) fn new_with_output(
        device: &wgpu::Device,
        out_buffer: &wgpu::Buffer,
        width: u32,
        height: u32,
        output_width: u32,
        output_height: u32,
    ) -> Result<Self, Box<dyn Error>> {
        let post_init = RetainedPostParams {
            width,
            height,
            blend_mode: 0,
            _pad0: 0,
            opacity: 1.0,
            contrast: 1.0,
            _pad1: 0.0,
            _pad2: 0.0,
        };
        let finalize_init = RetainedFinalizeParams {
            src_width: width,
            src_height: height,
            dst_width: output_width,
            dst_height: output_height,
            contrast: 1.0,
            low_pct: 0.01,
            high_pct: 0.99,
            fast_mode: 0,
        };

        let setup = build_setup(
            device,
            out_buffer,
            width,
            height,
            output_width,
            output_height,
            post_init,
            finalize_init,
        )?;

        Ok(Self {
            clear_pipeline: setup.clear_pipeline,
            blend_pipeline: setup.blend_pipeline,
            clear_hist_pipeline: setup.clear_hist_pipeline,
            histogram_pipeline: setup.histogram_pipeline,
            thresholds_pipeline: setup.thresholds_pipeline,
            finalize_pipeline: setup.finalize_pipeline,
            bind_group: setup.bind_group,
            accum_buffer: setup.accum_buffer,
            post_uniform: setup.post_uniform,
            finalize_uniform: setup.finalize_uniform,
            final_output_buffer: setup.final_output_buffer,
            staging_buffer: setup.staging_buffer,
            final_staging_buffer: setup.final_staging_buffer,
            width,
            height,
            output_width,
            output_height,
        })
    }

    /// Dispatch clear pass for a new image.
    pub(crate) fn begin_image(&self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let clear = RetainedPostParams {
            width: self.width,
            height: self.height,
            blend_mode: 0,
            _pad0: 0,
            opacity: 1.0,
            contrast: 1.0,
            _pad1: 0.0,
            _pad2: 0.0,
        };
        queue.write_buffer(&self.post_uniform, 0, bytemuck::bytes_of(&clear));

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("retained clear encoder"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("retained clear pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.clear_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(self.width.div_ceil(16), self.height.div_ceil(16), 1);
        }
        queue.submit(Some(encoder.finish()));
    }

    /// Encode one retained blend pass after the main fractal pass has populated source pixels.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn encode_blend_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        queue: &wgpu::Queue,
        opacity: f32,
        blend_mode: u32,
        contrast: f32,
    ) {
        let post = RetainedPostParams {
            width: self.width,
            height: self.height,
            blend_mode: blend_mode % 10,
            _pad0: 0,
            opacity: opacity.clamp(0.0, 1.0),
            contrast: contrast.clamp(1.0, 3.0),
            _pad1: 0.0,
            _pad2: 0.0,
        };
        queue.write_buffer(&self.post_uniform, 0, bytemuck::bytes_of(&post));

        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("retained blend pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.blend_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(self.width.div_ceil(16), self.height.div_ceil(16), 1);
    }

    /// Map retained accumulation for legacy host-side processing.
    #[allow(dead_code)]
    pub(crate) fn begin_readback(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Receiver<Result<(), wgpu::BufferAsyncError>> {
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("retained readback encoder"),
        });
        encoder.copy_buffer_to_buffer(
            &self.accum_buffer,
            0,
            &self.staging_buffer,
            0,
            (self.expected_pixels() * std::mem::size_of::<f32>()) as u64,
        );
        queue.submit(Some(encoder.finish()));
        map_buffer_async(&self.staging_buffer)
    }

    /// Copy mapped retained accumulation into `out`.
    #[allow(dead_code)]
    pub(crate) fn finish_readback(&self, out: &mut [f32]) -> Result<(), Box<dyn Error>> {
        if out.len() != self.expected_pixels() {
            return Err("output buffer length does not match render dimensions".into());
        }
        let slice = self.staging_buffer.slice(..);
        {
            let raw = slice.get_mapped_range();
            let mapped: &[f32] = bytemuck::cast_slice(&raw);
            out.copy_from_slice(&mapped[..self.expected_pixels()]);
        }
        self.staging_buffer.unmap();
        Ok(())
    }

    /// Run final contrast/stretch/downsample on GPU and map final grayscale output.
    pub(crate) fn begin_final_readback(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        contrast: f32,
        low_pct: f32,
        high_pct: f32,
        fast_mode: bool,
    ) -> Receiver<Result<(), wgpu::BufferAsyncError>> {
        let finalize = RetainedFinalizeParams {
            src_width: self.width,
            src_height: self.height,
            dst_width: self.output_width,
            dst_height: self.output_height,
            contrast: contrast.clamp(1.0, 3.0),
            low_pct: low_pct.clamp(0.0, 1.0),
            high_pct: high_pct.clamp(0.0, 1.0),
            fast_mode: u32::from(fast_mode),
        };
        queue.write_buffer(&self.finalize_uniform, 0, bytemuck::bytes_of(&finalize));

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("retained final readback encoder"),
        });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("retained histogram clear pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.clear_hist_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(4, 1, 1);
        }
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("retained histogram pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.histogram_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(self.width.div_ceil(16), self.height.div_ceil(16), 1);
        }
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("retained threshold pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.thresholds_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("retained finalize pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.finalize_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(
                self.output_width.div_ceil(16),
                self.output_height.div_ceil(16),
                1,
            );
        }

        encoder.copy_buffer_to_buffer(
            &self.final_output_buffer,
            0,
            &self.final_staging_buffer,
            0,
            (self.expected_output_pixels() * std::mem::size_of::<u32>()) as u64,
        );
        queue.submit(Some(encoder.finish()));

        map_buffer_async(&self.final_staging_buffer)
    }

    /// Copy mapped GPU-finalized grayscale output bytes into `out_gray`.
    pub(crate) fn finish_final_readback_gray(
        &self,
        out_gray: &mut [u8],
    ) -> Result<(), Box<dyn Error>> {
        if out_gray.len() != self.expected_output_pixels() {
            return Err(
                "output gray buffer length does not match configured output dimensions".into(),
            );
        }
        let slice = self.final_staging_buffer.slice(..);
        {
            let raw = slice.get_mapped_range();
            let mapped: &[u32] = bytemuck::cast_slice(&raw);
            for (dst, src) in out_gray.iter_mut().zip(mapped.iter()) {
                *dst = (*src & 255u32) as u8;
            }
        }
        self.final_staging_buffer.unmap();
        Ok(())
    }

    /// Copy mapped GPU-finalized output into BGRA bytes.
    pub(crate) fn finish_final_readback_bgra(
        &self,
        out_bgra: &mut [u8],
    ) -> Result<(), Box<dyn Error>> {
        let expected_bytes = self.expected_output_pixels().saturating_mul(4);
        if out_bgra.len() != expected_bytes {
            return Err(
                "output BGRA buffer length does not match configured output dimensions".into(),
            );
        }
        let slice = self.final_staging_buffer.slice(..);
        {
            let raw = slice.get_mapped_range();
            let mapped: &[u32] = bytemuck::cast_slice(&raw);
            for (src, dst) in mapped.iter().zip(out_bgra.chunks_exact_mut(4)) {
                let gray = (*src & 255u32) as u8;
                dst[0] = gray;
                dst[1] = gray;
                dst[2] = gray;
                dst[3] = 255;
            }
        }
        self.final_staging_buffer.unmap();
        Ok(())
    }

    pub(crate) fn expected_pixels(&self) -> usize {
        (self.width as usize) * (self.height as usize)
    }

    pub(crate) fn expected_output_pixels(&self) -> usize {
        (self.output_width as usize) * (self.output_height as usize)
    }

    /// Copy an external luma buffer into retained accumulation state.
    pub(crate) fn encode_copy_from_luma(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::Buffer,
    ) {
        encoder.copy_buffer_to_buffer(
            source,
            0,
            &self.accum_buffer,
            0,
            (self.expected_pixels() * std::mem::size_of::<f32>()) as u64,
        );
    }
}
