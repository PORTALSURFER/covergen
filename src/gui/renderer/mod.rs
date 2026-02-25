//! WGPU renderer for the realtime GUI editor.

mod setup;
mod viewer;

use std::error::Error;
use std::sync::Arc;

use winit::window::Window;

use crate::runtime_config::GuiVsync;

use super::scene::{Color, SceneFrame};
use super::top_view::TopViewerFrame;
use crate::gui::geometry::Rect;
use setup::{
    create_pipeline, create_uniform_bind_group, create_vertex_buffer, grow_capacity,
    preferred_surface_format, push_rect_triangles, request_hardware_adapter, select_present_mode,
    Vertex, ViewportUniform,
};

/// GPU renderer state for one GUI window/surface.
#[derive(Debug)]
pub(crate) struct GuiRenderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    triangles_pipeline: wgpu::RenderPipeline,
    lines_pipeline: wgpu::RenderPipeline,
    viewer_pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    viewer_texture_layout: wgpu::BindGroupLayout,
    viewer_sampler: wgpu::Sampler,
    viewer_bind_group: Option<wgpu::BindGroup>,
    viewer_texture: Option<wgpu::Texture>,
    viewer_texture_size: (u32, u32),
    viewer_quad_buffer: wgpu::Buffer,
    viewer_visible: bool,
    triangle_buffer: wgpu::Buffer,
    line_buffer: wgpu::Buffer,
    triangle_capacity: usize,
    line_capacity: usize,
    triangle_vertices: Vec<Vertex>,
    line_vertices: Vec<Vertex>,
    uniform_dirty: bool,
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
        let viewer_texture_layout = viewer::create_texture_bind_group_layout(&device);
        let viewer_sampler = viewer::create_texture_sampler(&device);
        let viewer_shader = viewer::create_shader_module(&device);
        let viewer_pipeline = viewer::create_pipeline(
            &device,
            &viewer_shader,
            &uniform_bind_group_layout,
            &viewer_texture_layout,
            config.format,
        );
        let viewer_quad_buffer = viewer::create_vertex_buffer(&device);
        let triangle_capacity = 8192;
        let line_capacity = 8192;
        let triangle_buffer = create_vertex_buffer(&device, triangle_capacity, "gui-triangle-vb");
        let line_buffer = create_vertex_buffer(&device, line_capacity, "gui-line-vb");
        Ok(Self {
            surface,
            device,
            queue,
            config,
            triangles_pipeline,
            lines_pipeline,
            viewer_pipeline,
            uniform_buffer,
            uniform_bind_group,
            viewer_texture_layout,
            viewer_sampler,
            viewer_bind_group: None,
            viewer_texture: None,
            viewer_texture_size: (0, 0),
            viewer_quad_buffer,
            viewer_visible: false,
            triangle_buffer,
            line_buffer,
            triangle_capacity,
            line_capacity,
            triangle_vertices: Vec::with_capacity(triangle_capacity),
            line_vertices: Vec::with_capacity(line_capacity),
            uniform_dirty: false,
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
    pub(crate) fn render(
        &mut self,
        frame: &SceneFrame,
        top_view: Option<TopViewerFrame<'_>>,
        panel_width: usize,
    ) -> Result<(), Box<dyn Error>> {
        self.rebuild_geometry(frame);
        self.ensure_vertex_capacity(self.triangle_vertices.len(), self.line_vertices.len());
        if self.uniform_dirty {
            self.queue.write_buffer(
                &self.uniform_buffer,
                0,
                bytemuck::bytes_of(&ViewportUniform::new(self.config.width, self.config.height)),
            );
            self.uniform_dirty = false;
        }
        self.queue.write_buffer(
            &self.triangle_buffer,
            0,
            bytemuck::cast_slice(&self.triangle_vertices),
        );
        self.queue.write_buffer(
            &self.line_buffer,
            0,
            bytemuck::cast_slice(&self.line_vertices),
        );
        self.update_viewer_texture(top_view);
        self.render_surface(
            frame.clear.unwrap_or(Color::argb(0xFF000000)),
            panel_width,
        )
    }

    fn render_surface(&mut self, clear: Color, panel_width: usize) -> Result<(), Box<dyn Error>> {
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
            if !self.triangle_vertices.is_empty() {
                self.draw_top_viewer(&mut pass);
                pass.set_scissor_rect(0, 0, editor_scissor_w, editor_scissor_h);
                pass.set_pipeline(&self.triangles_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, self.triangle_buffer.slice(..));
                pass.draw(0..self.triangle_vertices.len() as u32, 0..1);
            } else {
                self.draw_top_viewer(&mut pass);
            }
            if !self.line_vertices.is_empty() {
                pass.set_scissor_rect(0, 0, editor_scissor_w, editor_scissor_h);
                pass.set_pipeline(&self.lines_pipeline);
                pass.set_bind_group(0, &self.uniform_bind_group, &[]);
                pass.set_vertex_buffer(0, self.line_buffer.slice(..));
                pass.draw(0..self.line_vertices.len() as u32, 0..1);
            }
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        surface_tex.present();
        Ok(())
    }

    fn rebuild_geometry(&mut self, frame: &SceneFrame) {
        self.triangle_vertices.clear();
        self.line_vertices.clear();
        let triangle_target = frame.rects.len().saturating_mul(6);
        if triangle_target > self.triangle_vertices.capacity() {
            self.triangle_vertices
                .reserve(triangle_target - self.triangle_vertices.capacity());
        }
        let line_target = frame.lines.len().saturating_mul(2);
        if line_target > self.line_vertices.capacity() {
            self.line_vertices
                .reserve(line_target - self.line_vertices.capacity());
        }
        for rect in &frame.rects {
            push_rect_triangles(&mut self.triangle_vertices, rect.rect, rect.color);
        }
        for line in &frame.lines {
            self.line_vertices
                .push(Vertex::new(line.x0, line.y0, line.color));
            self.line_vertices
                .push(Vertex::new(line.x1, line.y1, line.color));
        }
    }

    fn ensure_vertex_capacity(&mut self, triangles: usize, lines: usize) {
        if triangles > self.triangle_capacity {
            self.triangle_capacity = grow_capacity(triangles);
            self.triangle_buffer =
                create_vertex_buffer(&self.device, self.triangle_capacity, "gui-triangle-vb");
        }
        if lines > self.line_capacity {
            self.line_capacity = grow_capacity(lines);
            self.line_buffer =
                create_vertex_buffer(&self.device, self.line_capacity, "gui-line-vb");
        }
    }

    fn update_viewer_texture(&mut self, top_view: Option<TopViewerFrame<'_>>) {
        let Some(top_view) = top_view else {
            self.viewer_visible = false;
            return;
        };
        if top_view.width == 0 || top_view.height == 0 || top_view.rgba8.is_empty() {
            self.viewer_visible = false;
            return;
        }
        self.ensure_viewer_texture(top_view.width, top_view.height);
        let Some(texture) = self.viewer_texture.as_ref() else {
            self.viewer_visible = false;
            return;
        };
        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            top_view.rgba8,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(top_view.width.saturating_mul(4)),
                rows_per_image: Some(top_view.height),
            },
            wgpu::Extent3d {
                width: top_view.width,
                height: top_view.height,
                depth_or_array_layers: 1,
            },
        );
        let rect = Rect::new(
            top_view.x,
            top_view.y,
            top_view.width as i32,
            top_view.height as i32,
        );
        let quad = viewer::quad_vertices(rect);
        self.queue
            .write_buffer(&self.viewer_quad_buffer, 0, bytemuck::cast_slice(&quad));
        self.viewer_visible = true;
    }

    fn ensure_viewer_texture(&mut self, width: u32, height: u32) {
        if self.viewer_texture_size == (width, height) && self.viewer_bind_group.is_some() {
            return;
        }
        self.viewer_texture_size = (width, height);
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gui-top-viewer-texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = viewer::create_texture_bind_group(
            &self.device,
            &self.viewer_texture_layout,
            &texture_view,
            &self.viewer_sampler,
        );
        self.viewer_texture = Some(texture);
        self.viewer_bind_group = Some(bind_group);
    }

    fn draw_top_viewer<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        if !self.viewer_visible {
            return;
        }
        let Some(bind_group) = self.viewer_bind_group.as_ref() else {
            return;
        };
        pass.set_pipeline(&self.viewer_pipeline);
        pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        pass.set_bind_group(1, bind_group, &[]);
        pass.set_vertex_buffer(0, self.viewer_quad_buffer.slice(..));
        pass.draw(0..6, 0..1);
    }
}
