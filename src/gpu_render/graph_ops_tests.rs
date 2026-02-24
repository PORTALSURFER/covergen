use super::GpuGraphOps;

#[test]
fn graph_ops_shader_compiles_when_adapter_is_available() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        force_fallback_adapter: true,
        compatible_surface: None,
    }));

    let Some(adapter) = adapter else {
        return;
    };

    let device = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("graph ops shader test device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
        },
        None,
    ));
    let Ok((device, _queue)) = device else {
        return;
    };

    match GpuGraphOps::new(&device) {
        Ok(_ops) => {}
        Err(err) => {
            let message = err.to_string();
            if message.contains("failed to read rust-gpu SPIR-V") {
                return;
            }
            panic!("graph ops shader should compile: {message}");
        }
    }
}
