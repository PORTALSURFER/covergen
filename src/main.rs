use std::sync::mpsc::channel;
use std::{env, error::Error};

use bytemuck::{Pod, Zeroable};
use image::{ImageBuffer, Rgba};
use wgpu::util::DeviceExt;

const SHADER: &str = r#"
struct Params {
    width: u32,
    height: u32,
    symmetry: u32,
    iterations: u32,
}

@group(0) @binding(0)
var<storage, read_write> out_pixels: array<u32>;

@group(0) @binding(1)
var<uniform> params: Params;

fn pack_color(i: f32, u: f32, v: f32) -> u32 {
    let r = 0.5 + 0.5 * sin(i * 6.28318530718 + u * 8.0);
    let g = 0.5 + 0.5 * sin(i * 5.28318530718 + v * 11.0);
    let b = 0.5 + 0.5 * sin(i * 4.28318530718 + (u + v) * 6.0);
    let rb = u32(clamp(r, 0.0, 1.0) * 255.0 + 0.5);
    let gb = u32(clamp(g, 0.0, 1.0) * 255.0 + 0.5);
    let bb = u32(clamp(b, 0.0, 1.0) * 255.0 + 0.5);
    return (255u << 24u) | (bb << 16u) | (gb << 8u) | rb;
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.width || id.y >= params.height) {
        return;
    }

    let mut px = (f32(id.x) / f32(params.width)) - 0.5;
    let mut py = (f32(id.y) / f32(params.height)) - 0.5;

    if (params.symmetry > 1u) {
        px = abs(px);
        if (params.symmetry > 2u) {
            py = abs(py);
        }
        if (params.symmetry > 3u) {
            if (py > px) {
                let t = px;
                px = py;
                py = t;
            }
        }
    }

    let aspect = f32(params.width) / f32(params.height);
    var zx = (px * aspect) * 2.8;
    var zy = py * 2.8;
    let cx = px * 3.0;
    let cy = py * 3.0;

    var i: u32 = 0u;
    var mag2 = 0.0;
    loop {
        if (i >= params.iterations || mag2 > 4.0) {
            break;
        }
        let x2 = zx * zx - zy * zy + cx;
        let y2 = 2.0 * zx * zy + cy;
        zx = x2;
        zy = y2;
        mag2 = zx * zx + zy * zy;
        i = i + 1u;
    }

    let t = f32(i) / f32(params.iterations);
    out_pixels[id.x + id.y * params.width] = pack_color(t, cx, cy);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    width: u32,
    height: u32,
    symmetry: u32,
    iterations: u32,
}

struct Config {
    width: u32,
    height: u32,
    symmetry: u32,
    iterations: u32,
    output: String,
}

impl Config {
    fn from_env() -> Result<Self, Box<dyn Error>> {
        let mut args = env::args().skip(1).peekable();
        let mut cfg = Config {
            width: 1024,
            height: 1024,
            symmetry: 4,
            iterations: 240,
            output: "fractal.png".to_string(),
        };

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--width" | "-w" => {
                    let value = args
                        .next()
                        .ok_or("missing width value, pass --width <u32>")?;
                    cfg.width = value.parse()?;
                }
                "--height" | "-h" => {
                    let value = args
                        .next()
                        .ok_or("missing height value, pass --height <u32>")?;
                    cfg.height = value.parse()?;
                }
                "--size" => {
                    let value = args
                        .next()
                        .ok_or("missing size value, pass --size <width>x<height>")?;
                    let mut split = value.split('x');
                    let w: u32 = split.next().ok_or("size needs WIDTHxHEIGHT")?.parse()?;
                    let h: u32 = split.next().ok_or("size needs WIDTHxHEIGHT")?.parse()?;
                    cfg.width = w;
                    cfg.height = h;
                }
                "--symmetry" => {
                    let value = args
                        .next()
                        .ok_or("missing symmetry value, pass --symmetry <1-8>")?;
                    cfg.symmetry = value.parse()?;
                }
                "--iterations" => {
                    let value = args
                        .next()
                        .ok_or("missing iterations value, pass --iterations <u32>")?;
                    cfg.iterations = value.parse()?;
                }
                "--output" | "-o" => {
                    cfg.output = args
                        .next()
                        .ok_or("missing output file name, pass --output <path>")?
                        .to_string();
                }
                _ => return Err(format!("unknown argument: {arg}").into()),
            }
        }

        if cfg.width == 0 || cfg.height == 0 {
            return Err("width and height must be greater than zero".into());
        }
        if cfg.symmetry == 0 {
            return Err("symmetry must be at least 1".into());
        }
        if cfg.iterations == 0 {
            return Err("iterations must be at least 1".into());
        }

        Ok(cfg)
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let config = Config::from_env()?;
    pollster::block_on(run(config))?;
    Ok(())
}

async fn run(config: Config) -> Result<(), Box<dyn Error>> {
    let shader = wgpu::ShaderModuleDescriptor {
        label: Some("fractal shader"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    };

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        })
        .await
        .ok_or("no compatible GPU adapter found")?;

    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        )
        .await?;

    let shader_module = device.create_shader_module(shader);
    let params = Params {
        width: config.width,
        height: config.height,
        symmetry: config.symmetry,
        iterations: config.iterations,
    };
    let output_size = (config.width as usize * config.height as usize * std::mem::size_of::<u32>()) as u64;

    let out_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("output storage"),
        size: output_size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });

    let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("uniforms"),
        contents: bytemuck::bytes_of(&params),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });

    let staging_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("staging"),
        size: output_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("bind group layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    });

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("bind group"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: out_buffer.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: uniform_buffer.as_entire_binding(),
            },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("pipeline layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("compute pipeline"),
        layout: Some(&pipeline_layout),
        module: &shader_module,
        entry_point: Some("main"),
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("command encoder"),
    });

    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("compute pass"),
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        let work_x = (config.width + 15) / 16;
        let work_y = (config.height + 15) / 16;
        pass.dispatch_workgroups(work_x, work_y, 1);
    }

    encoder.copy_buffer_to_buffer(&out_buffer, 0, &staging_buffer, 0, output_size);
    queue.submit(Some(encoder.finish()));

    let slice = staging_buffer.slice(..);
    let (tx, rx) = channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        tx.send(result).expect("map callback dropped");
    });
    device.poll(wgpu::Maintain::Wait);
    match rx.recv()? {
        Ok(()) => {}
        Err(err) => return Err(format!("buffer map failed: {err:?}").into()),
    }

    let data = slice.get_mapped_range();
    let image_data = data.to_vec();
    drop(data);
    staging_buffer.unmap();

    let img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::from_raw(config.width, config.height, image_data)
        .ok_or("could not create image buffer from GPU output")?;

    img.save(&config.output)?;
    println!("Generated {}", config.output);
    Ok(())
}
