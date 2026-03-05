use std::error::Error;
use std::sync::mpsc::Receiver;

use bytemuck::{Pod, Zeroable};

use crate::gpu_timestamp::OptionalGpuTimestampQueries;

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

/// User-controlled settings for final retained-output readback.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FinalReadbackSettings {
    /// Output contrast scaling after histogram stretch.
    pub(crate) contrast: f32,
    /// Lower percentile clamp for histogram stretch.
    pub(crate) low_pct: f32,
    /// Upper percentile clamp for histogram stretch.
    pub(crate) high_pct: f32,
    /// Enable fast finalize path.
    pub(crate) fast_mode: bool,
}

/// Build finalized readback uniforms with validated user settings.
fn finalize_params_for_readback(
    src_width: u32,
    src_height: u32,
    dst_width: u32,
    dst_height: u32,
    settings: FinalReadbackSettings,
) -> RetainedFinalizeParams {
    RetainedFinalizeParams {
        src_width,
        src_height,
        dst_width,
        dst_height,
        contrast: settings.contrast.clamp(1.0, 3.0),
        low_pct: settings.low_pct.clamp(0.0, 1.0),
        high_pct: settings.high_pct.clamp(0.0, 1.0),
        fast_mode: u32::from(settings.fast_mode),
    }
}

/// GPU resources used to retain and finalize layer output without intermediate readbacks.
#[derive(Debug)]
pub(crate) struct RetainedGpuPost {
    clear_pipeline: wgpu::ComputePipeline,
    #[cfg(test)]
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
    width: u32,
    height: u32,
    output_width: u32,
    output_height: u32,
    timestamps: OptionalGpuTimestampQueries,
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
            #[cfg(test)]
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
            width,
            height,
            output_width,
            output_height,
            timestamps: OptionalGpuTimestampQueries::new(device, "retained-compute-pass", 32),
        })
    }

    /// Dispatch clear pass for a new image.
    pub(crate) fn begin_image(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
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
        self.timestamps.begin_frame();
        let timestamp_parts = self.timestamps.next_compute_pass_parts();
        let timestamp_writes = timestamp_parts.as_ref().map(|(query_set, begin, end)| {
            wgpu::ComputePassTimestampWrites {
                query_set: query_set.as_ref(),
                beginning_of_pass_write_index: Some(*begin),
                end_of_pass_write_index: Some(*end),
            }
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("retained clear pass"),
                timestamp_writes,
            });
            pass.set_pipeline(&self.clear_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(self.width.div_ceil(16), self.height.div_ceil(16), 1);
        }
        self.timestamps.resolve_and_reset(&mut encoder);
        queue.submit(Some(encoder.finish()));
    }

    /// Encode one retained blend pass after the main fractal pass has populated source pixels.
    #[cfg(test)]
    pub(crate) fn encode_blend_pass(
        &mut self,
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

        self.timestamps.begin_frame();
        let timestamp_parts = self.timestamps.next_compute_pass_parts();
        let timestamp_writes = timestamp_parts.as_ref().map(|(query_set, begin, end)| {
            wgpu::ComputePassTimestampWrites {
                query_set: query_set.as_ref(),
                beginning_of_pass_write_index: Some(*begin),
                end_of_pass_write_index: Some(*end),
            }
        });
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("retained blend pass"),
            timestamp_writes,
        });
        pass.set_pipeline(&self.blend_pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.dispatch_workgroups(self.width.div_ceil(16), self.height.div_ceil(16), 1);
    }

    /// Resolve retained timestamp queries into the per-node resolve buffer.
    #[cfg(test)]
    pub(crate) fn resolve_timestamps(&mut self, encoder: &mut wgpu::CommandEncoder) {
        self.timestamps.resolve_and_reset(encoder);
    }

    /// Run final GPU passes and map readback data into one caller-provided staging buffer.
    pub(crate) fn begin_final_readback_into(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        staging_buffer: &wgpu::Buffer,
        settings: FinalReadbackSettings,
    ) -> Receiver<Result<(), wgpu::BufferAsyncError>> {
        let finalize = finalize_params_for_readback(
            self.width,
            self.height,
            self.output_width,
            self.output_height,
            settings,
        );
        queue.write_buffer(&self.finalize_uniform, 0, bytemuck::bytes_of(&finalize));

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("retained final readback encoder"),
        });

        self.timestamps.begin_frame();
        let clear_hist_timestamp_parts = self.timestamps.next_compute_pass_parts();
        let clear_hist_timestamp =
            clear_hist_timestamp_parts
                .as_ref()
                .map(|(query_set, begin, end)| wgpu::ComputePassTimestampWrites {
                    query_set: query_set.as_ref(),
                    beginning_of_pass_write_index: Some(*begin),
                    end_of_pass_write_index: Some(*end),
                });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("retained histogram clear pass"),
                timestamp_writes: clear_hist_timestamp,
            });
            pass.set_pipeline(&self.clear_hist_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(4, 1, 1);
        }
        let histogram_timestamp_parts = self.timestamps.next_compute_pass_parts();
        let histogram_timestamp =
            histogram_timestamp_parts
                .as_ref()
                .map(|(query_set, begin, end)| wgpu::ComputePassTimestampWrites {
                    query_set: query_set.as_ref(),
                    beginning_of_pass_write_index: Some(*begin),
                    end_of_pass_write_index: Some(*end),
                });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("retained histogram pass"),
                timestamp_writes: histogram_timestamp,
            });
            pass.set_pipeline(&self.histogram_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(self.width.div_ceil(16), self.height.div_ceil(16), 1);
        }
        let threshold_timestamp_parts = self.timestamps.next_compute_pass_parts();
        let threshold_timestamp =
            threshold_timestamp_parts
                .as_ref()
                .map(|(query_set, begin, end)| wgpu::ComputePassTimestampWrites {
                    query_set: query_set.as_ref(),
                    beginning_of_pass_write_index: Some(*begin),
                    end_of_pass_write_index: Some(*end),
                });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("retained threshold pass"),
                timestamp_writes: threshold_timestamp,
            });
            pass.set_pipeline(&self.thresholds_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        let finalize_timestamp_parts = self.timestamps.next_compute_pass_parts();
        let finalize_timestamp =
            finalize_timestamp_parts
                .as_ref()
                .map(|(query_set, begin, end)| wgpu::ComputePassTimestampWrites {
                    query_set: query_set.as_ref(),
                    beginning_of_pass_write_index: Some(*begin),
                    end_of_pass_write_index: Some(*end),
                });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("retained finalize pass"),
                timestamp_writes: finalize_timestamp,
            });
            pass.set_pipeline(&self.finalize_pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(
                self.output_width.div_ceil(16),
                self.output_height.div_ceil(16),
                1,
            );
        }
        self.timestamps.resolve_and_reset(&mut encoder);

        encoder.copy_buffer_to_buffer(
            &self.final_output_buffer,
            0,
            staging_buffer,
            0,
            (self.expected_output_pixels() * std::mem::size_of::<u32>()) as u64,
        );
        queue.submit(Some(encoder.finish()));

        map_buffer_async(staging_buffer)
    }

    /// Copy mapped grayscale output bytes from one caller-provided final staging buffer.
    pub(crate) fn finish_final_readback_gray_from(
        &self,
        staging_buffer: &wgpu::Buffer,
        out_gray: &mut [u8],
    ) -> Result<(), Box<dyn Error>> {
        if out_gray.len() != self.expected_output_pixels() {
            return Err(
                "output gray buffer length does not match configured output dimensions".into(),
            );
        }
        let slice = staging_buffer.slice(..);
        {
            let raw = slice.get_mapped_range();
            let mapped: &[u32] = bytemuck::cast_slice(&raw);
            for (dst, src) in out_gray.iter_mut().zip(mapped.iter()) {
                *dst = (*src & 255u32) as u8;
            }
        }
        staging_buffer.unmap();
        Ok(())
    }

    /// Copy mapped BGRA output bytes from one caller-provided final staging buffer.
    pub(crate) fn finish_final_readback_bgra_from(
        &self,
        staging_buffer: &wgpu::Buffer,
        out_bgra: &mut [u8],
    ) -> Result<(), Box<dyn Error>> {
        let expected_bytes = self.expected_output_pixels().saturating_mul(4);
        if out_bgra.len() != expected_bytes {
            return Err(
                "output BGRA buffer length does not match configured output dimensions".into(),
            );
        }
        let slice = staging_buffer.slice(..);
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
        staging_buffer.unmap();
        Ok(())
    }

    /// Return source accumulation pixel count for current retained dimensions.
    pub(crate) fn expected_pixels(&self) -> usize {
        (self.width as usize) * (self.height as usize)
    }

    /// Return final output pixel count for current configured output dimensions.
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

#[cfg(test)]
mod tests {
    use super::{finalize_params_for_readback, FinalReadbackSettings};

    #[test]
    fn finalize_params_clamp_settings_to_supported_ranges() {
        let params = finalize_params_for_readback(
            1024,
            768,
            640,
            360,
            FinalReadbackSettings {
                contrast: 9.0,
                low_pct: -0.25,
                high_pct: 1.5,
                fast_mode: false,
            },
        );
        assert_eq!(params.contrast, 3.0);
        assert_eq!(params.low_pct, 0.0);
        assert_eq!(params.high_pct, 1.0);
        assert_eq!(params.fast_mode, 0);
    }

    #[test]
    fn finalize_params_preserve_dimensions_and_fast_mode_flag() {
        let params = finalize_params_for_readback(
            3840,
            2160,
            1920,
            1080,
            FinalReadbackSettings {
                contrast: 2.2,
                low_pct: 0.03,
                high_pct: 0.94,
                fast_mode: true,
            },
        );
        assert_eq!(params.src_width, 3840);
        assert_eq!(params.src_height, 2160);
        assert_eq!(params.dst_width, 1920);
        assert_eq!(params.dst_height, 1080);
        assert_eq!(params.contrast, 2.2);
        assert_eq!(params.low_pct, 0.03);
        assert_eq!(params.high_pct, 0.94);
        assert_eq!(params.fast_mode, 1);
    }
}
