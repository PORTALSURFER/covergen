//! WGPU renderer for the realtime GUI editor.

mod setup;
mod top_preview;
mod viewer;

use std::error::Error;
use std::sync::Arc;

use winit::window::Window;

use crate::runtime_config::GuiVsync;

use super::scene::{Color, SceneFrame, SceneLayer};
use super::text::GuiTextRenderer;
use super::top_view::TopViewerFrame;
use setup::{
    create_pipeline, create_uniform_bind_group, create_vertex_buffer, grow_capacity,
    preferred_surface_format, push_rect_triangles, request_hardware_adapter, select_present_mode,
    Vertex, ViewportUniform,
};
use top_preview::TopPreviewRenderer;

const HUD_MARGIN_PX: i32 = 12;
const HUD_PAD_X: i32 = 8;
const HUD_PAD_Y: i32 = 6;
const HUD_BG: Color = Color::argb(0xCC000000);
const HUD_BORDER: Color = Color::argb(0xFF3A3A3A);
const HUD_TEXT: Color = Color::argb(0xFFE8E8E8);

/// Per-frame GUI renderer counters.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct GuiRenderPerfCounters {
    pub(crate) submit_count: u32,
    pub(crate) upload_bytes: u64,
    pub(crate) alloc_bytes: u64,
}

#[derive(Clone, Copy, Debug, Default)]
struct LayerRebuildStats {
    upload_bytes: u64,
    alloc_bytes: u64,
}

/// Retained GPU buffers/vertices for one scene layer.
#[derive(Debug)]
struct LayerGpuGeometry {
    triangle_buffer: wgpu::Buffer,
    line_buffer: wgpu::Buffer,
    triangle_capacity: usize,
    line_capacity: usize,
    triangle_vertices: Vec<Vertex>,
    line_vertices: Vec<Vertex>,
    triangle_count: u32,
    line_count: u32,
}

impl LayerGpuGeometry {
    /// Create one retained layer geometry cache.
    fn new(device: &wgpu::Device, label_prefix: &str, initial_capacity: usize) -> Self {
        let tri_cap = initial_capacity.max(1);
        let line_cap = initial_capacity.max(1);
        let triangle_buffer =
            create_vertex_buffer(device, tri_cap, &format!("gui-{label_prefix}-triangle-vb"));
        let line_buffer =
            create_vertex_buffer(device, line_cap, &format!("gui-{label_prefix}-line-vb"));
        Self {
            triangle_buffer,
            line_buffer,
            triangle_capacity: tri_cap,
            line_capacity: line_cap,
            triangle_vertices: Vec::with_capacity(tri_cap),
            line_vertices: Vec::with_capacity(line_cap),
            triangle_count: 0,
            line_count: 0,
        }
    }

    /// Rebuild vertex payload and upload to GPU buffers.
    fn rebuild(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layer: &SceneLayer,
        label_prefix: &str,
    ) -> LayerRebuildStats {
        let tri_capacity_before = self.triangle_vertices.capacity();
        let line_capacity_before = self.line_vertices.capacity();
        self.triangle_vertices.clear();
        self.line_vertices.clear();

        let triangle_target = layer.rects.len().saturating_mul(6);
        if triangle_target > self.triangle_vertices.capacity() {
            self.triangle_vertices
                .reserve(triangle_target - self.triangle_vertices.capacity());
        }
        let line_target = layer.lines.len().saturating_mul(2);
        if line_target > self.line_vertices.capacity() {
            self.line_vertices
                .reserve(line_target - self.line_vertices.capacity());
        }

        for rect in &layer.rects {
            push_rect_triangles(&mut self.triangle_vertices, rect.rect, rect.color);
        }
        for line in &layer.lines {
            self.line_vertices
                .push(Vertex::new(line.x0, line.y0, line.color));
            self.line_vertices
                .push(Vertex::new(line.x1, line.y1, line.color));
        }

        self.ensure_capacity(
            device,
            self.triangle_vertices.len(),
            self.line_vertices.len(),
            label_prefix,
        );
        let mut stats = LayerRebuildStats::default();
        stats.alloc_bytes = self
            .triangle_vertices
            .capacity()
            .saturating_sub(tri_capacity_before)
            .saturating_mul(std::mem::size_of::<Vertex>())
            .saturating_add(
                self.line_vertices
                    .capacity()
                    .saturating_sub(line_capacity_before)
                    .saturating_mul(std::mem::size_of::<Vertex>()),
            ) as u64;

        if !self.triangle_vertices.is_empty() {
            stats.upload_bytes = stats.upload_bytes.saturating_add(
                self.triangle_vertices
                    .len()
                    .saturating_mul(std::mem::size_of::<Vertex>()) as u64,
            );
            queue.write_buffer(
                &self.triangle_buffer,
                0,
                bytemuck::cast_slice(&self.triangle_vertices),
            );
        }
        if !self.line_vertices.is_empty() {
            stats.upload_bytes = stats.upload_bytes.saturating_add(
                self.line_vertices
                    .len()
                    .saturating_mul(std::mem::size_of::<Vertex>()) as u64,
            );
            queue.write_buffer(
                &self.line_buffer,
                0,
                bytemuck::cast_slice(&self.line_vertices),
            );
        }

        self.triangle_count = self.triangle_vertices.len() as u32;
        self.line_count = self.line_vertices.len() as u32;
        stats
    }

    fn ensure_capacity(
        &mut self,
        device: &wgpu::Device,
        triangles: usize,
        lines: usize,
        label_prefix: &str,
    ) {
        if triangles > self.triangle_capacity {
            self.triangle_capacity = grow_capacity(triangles);
            self.triangle_buffer = create_vertex_buffer(
                device,
                self.triangle_capacity,
                &format!("gui-{label_prefix}-triangle-vb"),
            );
        }
        if lines > self.line_capacity {
            self.line_capacity = grow_capacity(lines);
            self.line_buffer = create_vertex_buffer(
                device,
                self.line_capacity,
                &format!("gui-{label_prefix}-line-vb"),
            );
        }
    }
}

/// GPU renderer state for one GUI window/surface.
pub(crate) struct GuiRenderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    triangles_pipeline: wgpu::RenderPipeline,
    lines_pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    top_preview: TopPreviewRenderer,
    static_panel_geometry: LayerGpuGeometry,
    edges_geometry: LayerGpuGeometry,
    nodes_geometry: LayerGpuGeometry,
    overlays_geometry: LayerGpuGeometry,
    hud_geometry: LayerGpuGeometry,
    hud_layer: SceneLayer,
    hud_text: GuiTextRenderer,
    hud_label: String,
    uniform_dirty: bool,
    frame_perf: GuiRenderPerfCounters,
}

impl GuiRenderer {
    /// Create one renderer bound to a winit window.
    pub(crate) async fn new(window: Arc<Window>, vsync: GuiVsync) -> Result<Self, Box<dyn Error>> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let surface = instance.create_surface(window.clone())?;
        let adapter = request_hardware_adapter(&instance, &surface).await?;
        let caps = surface.get_capabilities(&adapter);
        let format = preferred_surface_format(&caps.formats);
        let present_mode = select_present_mode(&caps.present_modes, vsync);
        let alpha_mode = caps
            .alpha_modes
            .first()
            .copied()
            .ok_or("surface reported no alpha modes")?;
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("gui-device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                },
                None,
            )
            .await?;
        let initial_size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: initial_size.width.max(1),
            height: initial_size.height.max(1),
            present_mode,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let uniform_buffer = setup::create_uniform_buffer(&device, config.width, config.height);
        let (uniform_bind_group_layout, uniform_bind_group) =
            create_uniform_bind_group(&device, &uniform_buffer);
        let shader = setup::create_shader_module(&device);
        let triangles_pipeline = create_pipeline(
            &device,
            &shader,
            &uniform_bind_group_layout,
            config.format,
            wgpu::PrimitiveTopology::TriangleList,
        );
        let lines_pipeline = create_pipeline(
            &device,
            &shader,
            &uniform_bind_group_layout,
            config.format,
            wgpu::PrimitiveTopology::LineList,
        );
        let top_preview =
            TopPreviewRenderer::new(&device, &uniform_bind_group_layout, config.format);
        let static_panel_geometry = LayerGpuGeometry::new(&device, "static-panel", 1024);
        let edges_geometry = LayerGpuGeometry::new(&device, "edges", 2048);
        let nodes_geometry = LayerGpuGeometry::new(&device, "nodes", 8192);
        let overlays_geometry = LayerGpuGeometry::new(&device, "overlays", 2048);
        let hud_geometry = LayerGpuGeometry::new(&device, "hud", 512);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            triangles_pipeline,
            lines_pipeline,
            uniform_buffer,
            uniform_bind_group,
            top_preview,
            static_panel_geometry,
            edges_geometry,
            nodes_geometry,
            overlays_geometry,
            hud_geometry,
            hud_layer: SceneLayer::default(),
            hud_text: GuiTextRenderer::default(),
            hud_label: String::with_capacity(24),
            uniform_dirty: false,
            frame_perf: GuiRenderPerfCounters::default(),
        })
    }

    /// Return configured surface width in physical pixels.
    pub(crate) fn width(&self) -> usize {
        self.config.width as usize
    }

    /// Return configured surface height in physical pixels.
    pub(crate) fn height(&self) -> usize {
        self.config.height as usize
    }

    /// Resize the swapchain/surface to match current window dimensions.
    pub(crate) fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.uniform_dirty = true;
    }

    /// Render one scene frame to the GUI surface.
    ///
    /// `panel_width` defines the left editor pane and is used as a scissor
    /// clip so graph content cannot bleed into the right TOP viewer pane.
    /// `avg_fps` drives the fullscreen-safe HUD counter in the top-right.
    pub(crate) fn render(
        &mut self,
        frame: &SceneFrame,
        top_view: Option<TopViewerFrame<'_>>,
        panel_width: usize,
        avg_fps: f32,
    ) -> Result<(), Box<dyn Error>> {
        self.frame_perf = GuiRenderPerfCounters::default();
        let rebuild = self.rebuild_dirty_layers(frame);
        self.frame_perf.upload_bytes = self
            .frame_perf
            .upload_bytes
            .saturating_add(rebuild.upload_bytes);
        self.frame_perf.alloc_bytes = self
            .frame_perf
            .alloc_bytes
            .saturating_add(rebuild.alloc_bytes);
        let hud_rebuild = self.rebuild_hud_layer(avg_fps);
        self.frame_perf.upload_bytes = self
            .frame_perf
            .upload_bytes
            .saturating_add(hud_rebuild.upload_bytes);
        self.frame_perf.alloc_bytes = self
            .frame_perf
            .alloc_bytes
            .saturating_add(hud_rebuild.alloc_bytes);
        if self.uniform_dirty {
            self.frame_perf.upload_bytes = self
                .frame_perf
                .upload_bytes
                .saturating_add(std::mem::size_of::<ViewportUniform>() as u64);
            self.queue.write_buffer(
                &self.uniform_buffer,
                0,
                bytemuck::bytes_of(&ViewportUniform::new(self.config.width, self.config.height)),
            );
            self.uniform_dirty = false;
        }
        let top_preview_upload_bytes = self.render_surface(
            frame.clear.unwrap_or(Color::argb(0xFF000000)),
            panel_width,
            top_view,
        )?;
        self.frame_perf.upload_bytes = self
            .frame_perf
            .upload_bytes
            .saturating_add(top_preview_upload_bytes);
        self.frame_perf.submit_count = 1;
        Ok(())
    }

    /// Return and reset renderer counters from the most recent frame.
    pub(crate) fn take_perf_counters(&mut self) -> GuiRenderPerfCounters {
        let counters = self.frame_perf;
        self.frame_perf = GuiRenderPerfCounters::default();
        counters
    }

    fn rebuild_dirty_layers(&mut self, frame: &SceneFrame) -> LayerRebuildStats {
        let mut stats = LayerRebuildStats::default();
        if !frame.dirty.any() {
            return stats;
        }
        if frame.dirty.static_panel {
            let layer = self.static_panel_geometry.rebuild(
                &self.device,
                &self.queue,
                &frame.static_panel,
                "static-panel",
            );
            stats.upload_bytes = stats.upload_bytes.saturating_add(layer.upload_bytes);
            stats.alloc_bytes = stats.alloc_bytes.saturating_add(layer.alloc_bytes);
        }
        if frame.dirty.edges {
            let layer =
                self.edges_geometry
                    .rebuild(&self.device, &self.queue, &frame.edges, "edges");
            stats.upload_bytes = stats.upload_bytes.saturating_add(layer.upload_bytes);
            stats.alloc_bytes = stats.alloc_bytes.saturating_add(layer.alloc_bytes);
        }
        if frame.dirty.nodes {
            let layer =
                self.nodes_geometry
                    .rebuild(&self.device, &self.queue, &frame.nodes, "nodes");
            stats.upload_bytes = stats.upload_bytes.saturating_add(layer.upload_bytes);
            stats.alloc_bytes = stats.alloc_bytes.saturating_add(layer.alloc_bytes);
        }
        if frame.dirty.overlays {
            let layer = self.overlays_geometry.rebuild(
                &self.device,
                &self.queue,
                &frame.overlays,
                "overlays",
            );
            stats.upload_bytes = stats.upload_bytes.saturating_add(layer.upload_bytes);
            stats.alloc_bytes = stats.alloc_bytes.saturating_add(layer.alloc_bytes);
        }
        stats
    }

    fn render_surface(
        &mut self,
        clear: Color,
        panel_width: usize,
        top_view: Option<TopViewerFrame<'_>>,
    ) -> Result<u64, Box<dyn Error>> {
        let surface_tex = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                self.surface.get_current_texture()?
            }
            Err(other) => {
                return Err(format!("failed to acquire GUI surface texture: {other}").into())
            }
        };
        let view = surface_tex
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("gui-render-encoder"),
            });

        let top_preview_upload_bytes =
            self.top_preview
                .prepare(&self.device, &self.queue, top_view, &mut encoder);

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gui-render-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear.r as f64,
                            g: clear.g as f64,
                            b: clear.b as f64,
                            a: clear.a as f64,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            let editor_scissor_w = panel_width.min(self.config.width as usize) as u32;
            let editor_scissor_h = self.config.height;

            self.top_preview.draw(&mut pass, &self.uniform_bind_group);

            pass.set_scissor_rect(0, 0, editor_scissor_w, editor_scissor_h);
            self.draw_layer(&mut pass, &self.static_panel_geometry);
            self.draw_layer(&mut pass, &self.edges_geometry);
            self.draw_layer(&mut pass, &self.nodes_geometry);
            self.draw_layer(&mut pass, &self.overlays_geometry);

            pass.set_scissor_rect(0, 0, self.config.width, self.config.height);
            self.draw_layer(&mut pass, &self.hud_geometry);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        surface_tex.present();
        Ok(top_preview_upload_bytes)
    }

    fn draw_layer<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, layer: &'a LayerGpuGeometry) {
        if layer.triangle_count > 0 {
            pass.set_pipeline(&self.triangles_pipeline);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_vertex_buffer(0, layer.triangle_buffer.slice(..));
            pass.draw(0..layer.triangle_count, 0..1);
        }
        if layer.line_count > 0 {
            pass.set_pipeline(&self.lines_pipeline);
            pass.set_bind_group(0, &self.uniform_bind_group, &[]);
            pass.set_vertex_buffer(0, layer.line_buffer.slice(..));
            pass.draw(0..layer.line_count, 0..1);
        }
    }

    fn rebuild_hud_layer(&mut self, avg_fps: f32) -> LayerRebuildStats {
        self.hud_layer.rects.clear();
        self.hud_layer.lines.clear();

        self.hud_label.clear();
        if avg_fps.is_finite() && avg_fps > 0.0 {
            self.hud_label.push_str(&format!("FPS {:.1}", avg_fps));
        } else {
            self.hud_label.push_str("FPS --.-");
        }

        let text_w = self.hud_text.measure_text_width(self.hud_label.as_str(), 1.0);
        let metrics = self.hud_text.metrics_scaled(1.0);
        let box_w = text_w + HUD_PAD_X * 2;
        let box_h = metrics.line_height_px + HUD_PAD_Y * 2;
        let x = self.config.width as i32 - HUD_MARGIN_PX - box_w;
        let y = HUD_MARGIN_PX;
        let rect = super::geometry::Rect::new(x, y, box_w, box_h);
        self.hud_layer
            .rects
            .push(super::scene::ColoredRect { rect, color: HUD_BG });
        push_border_lines(&mut self.hud_layer, rect, HUD_BORDER);
        self.hud_text.push_text(
            &mut self.hud_layer.rects,
            x + HUD_PAD_X,
            y + HUD_PAD_Y,
            self.hud_label.as_str(),
            HUD_TEXT,
        );

        self.hud_geometry
            .rebuild(&self.device, &self.queue, &self.hud_layer, "hud")
    }
}

fn push_border_lines(layer: &mut SceneLayer, rect: super::geometry::Rect, color: Color) {
    let x0 = rect.x;
    let y0 = rect.y;
    let x1 = rect.x + rect.w - 1;
    let y1 = rect.y + rect.h - 1;
    layer.lines.push(super::scene::ColoredLine {
        x0,
        y0,
        x1,
        y1: y0,
        color,
    });
    layer.lines.push(super::scene::ColoredLine {
        x0: x1,
        y0,
        x1,
        y1,
        color,
    });
    layer.lines.push(super::scene::ColoredLine {
        x0: x1,
        y0: y1,
        x1: x0,
        y1,
        color,
    });
    layer.lines.push(super::scene::ColoredLine {
        x0,
        y0: y1,
        x1: x0,
        y1: y0,
        color,
    });
}
