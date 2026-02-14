use std::sync::mpsc::channel;
use std::{
    env,
    error::Error,
    time::{SystemTime, UNIX_EPOCH},
};

use bytemuck::{Pod, Zeroable};
use image::{ImageBuffer, Rgba};
use wgpu::util::DeviceExt;

const SHADER: &str = r#"
struct Params {
    width: u32,
    height: u32,
    symmetry: u32,
    iterations: u32,
    seed: u32,
}

@group(0) @binding(0)
var<storage, read_write> out_pixels: array<u32>;

@group(0) @binding(1)
var<uniform> params: Params;

fn hash01(x: f32, y: f32, seed: u32) -> f32 {
    let s = f32(seed) * 0.00000011920928955;
    return fract(sin(dot(vec2<f32>(x, y), vec2<f32>(12.9898, 78.233)) + s) * 43758.5453123);
}

fn pack_gray(level: f32) -> u32 {
    let c = u32(clamp(level, 0.0, 1.0) * 255.0 + 0.5);
    return (255u << 24u) | (c << 16u) | (c << 8u) | c;
}

fn layer_value(x: f32, y: f32, params: Params, layer: u32) -> f32 {
    let base = f32(params.seed) * 0.00000011920928955;
    let jitter = hash01(x + base, y - base, params.seed + layer);
    let layer_shift = f32(layer) * 0.24 + (jitter - 0.5) * 0.35;
    let angle = layer_shift;
    let radius = 0.48 + 0.08 * f32(layer) + (jitter - 0.5) * 0.1;
    let cx = (radius * x * (3.2 + 0.2 * jitter)) + 0.16 * (sin(angle) + cos(angle * 0.7 + base));
    let cy = (radius * y * (3.0 + 0.2 * (1.0 - jitter))) + 0.16 * (cos(angle) + sin(angle * 0.9 - base));

    let rot_x = x * cos(angle) - y * sin(angle);
    let rot_y = x * sin(angle) + y * cos(angle);
    var zx = rot_x * 2.6;
    var zy = rot_y * 2.6;

    var i: u32 = 0u;
    var mag2 = 0.0;
    var escaped = false;

    loop {
        if (i >= params.iterations) {
            break;
        }
        if (mag2 > 4.0) {
            escaped = true;
            break;
        }

        let x2 = zx * zx - zy * zy + cx;
        let y2 = 2.0 * zx * zy + cy;
        zx = x2;
        zy = y2;
        mag2 = zx * zx + zy * zy;
        i = i + 1u;
    }

    return select(
        0.0,
        1.0 - (f32(i) / f32(params.iterations)),
        escaped,
    );
}

@compute @workgroup_size(16, 16)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.width || id.y >= params.height) {
        return;
    }

    let mut px = (f32(id.x) / f32(params.width)) - 0.5;
    let mut py = (f32(id.y) / f32(params.height)) - 0.5;
    let mut value = 0.0;
    let layer_count = 6u;
    let layer_scale: f32 = 1.0 / f32(layer_count);

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

    let aspect = f32(params.height) / f32(params.width);
    let sx = px * aspect;

    var layer: u32 = 0u;
    loop {
        if (layer >= layer_count) {
            break;
        }
        let layer_brightness = layer_value(sx, py, params, layer);
        let weight = 1.0 - (f32(layer) * 0.11);
        value = value + (layer_brightness * layer_brightness * weight * layer_scale);
        layer = layer + 1u;
    }

    out_pixels[id.x + id.y * params.width] = pack_gray(value);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    width: u32,
    height: u32,
    symmetry: u32,
    iterations: u32,
    seed: u32,
}

struct Config {
    width: u32,
    height: u32,
    symmetry: u32,
    iterations: u32,
    seed: u32,
    output: String,
}

impl Config {
    fn from_env() -> Result<Self, Box<dyn Error>> {
        let mut args = env::args().skip(1);
        let mut cfg = Config {
            width: 1024,
            height: 1024,
            symmetry: 4,
            iterations: 240,
            seed: random_seed(),
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
                "--seed" => {
                    let value = args.next().ok_or("missing seed value, pass --seed <u32>")?;
                    cfg.seed = value.parse()?;
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

fn random_seed() -> u32 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|dur| dur.as_nanos() as u64)
        .unwrap_or(0);
    let mut seed = (nanos as u32) ^ ((nanos >> 32) as u32);
    seed ^= seed << 13;
    seed ^= seed >> 17;
    seed ^= seed << 5;
    seed
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
            seed: config.seed,
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
    println!("Generated {} (seed: {})", config.output, config.seed);
    Ok(())
}
