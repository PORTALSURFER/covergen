//! Shader module loading for strict rust-gpu SPIR-V artifacts.
//!
//! V2 runtime shader programs are loaded exclusively from rust-gpu-generated
//! SPIR-V binaries. Missing or invalid artifacts are treated as hard errors.

use std::borrow::Cow;
use std::error::Error;
use std::path::{Path, PathBuf};

/// Shader programs used by the runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShaderProgram {
    FractalMain,
    GraphOps,
    GraphDecode,
    RetainedPost,
}

const SHADER_SPIRV_DIR_ENV: &str = "COVERGEN_RUST_GPU_SPIRV_DIR";

/// Create a shader module for one program using strict rust-gpu SPIR-V input.
pub(crate) fn create_shader_module(
    device: &wgpu::Device,
    program: ShaderProgram,
) -> Result<wgpu::ShaderModule, Box<dyn Error>> {
    let words = load_spirv_words(program)?;
    Ok(device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(program_label(program)),
        source: wgpu::ShaderSource::SpirV(Cow::Owned(words)),
    }))
}

fn program_label(program: ShaderProgram) -> &'static str {
    match program {
        ShaderProgram::FractalMain => "fractal shader",
        ShaderProgram::GraphOps => "v2 graph ops shader",
        ShaderProgram::GraphDecode => "v2 graph decode shader",
        ShaderProgram::RetainedPost => "retained post shader",
    }
}

fn program_spirv_name(program: ShaderProgram) -> &'static str {
    match program {
        ShaderProgram::FractalMain => "fractal_main.spv",
        ShaderProgram::GraphOps => "graph_ops.spv",
        ShaderProgram::GraphDecode => "graph_decode.spv",
        ShaderProgram::RetainedPost => "retained_post.spv",
    }
}

fn load_spirv_words(program: ShaderProgram) -> Result<Vec<u32>, Box<dyn Error>> {
    let file = program_spirv_name(program);
    let path = spirv_root_dir().join(file);
    let bytes = std::fs::read(&path).map_err(|err| {
        format!(
            "failed to read rust-gpu SPIR-V '{}' at {}: {err}",
            file,
            path.display()
        )
    })?;
    parse_spirv_words(&bytes, &path)
}

fn spirv_root_dir() -> PathBuf {
    if let Ok(path) = std::env::var(SHADER_SPIRV_DIR_ENV) {
        return PathBuf::from(path);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/rust-gpu")
}

fn parse_spirv_words(bytes: &[u8], path: &Path) -> Result<Vec<u32>, Box<dyn Error>> {
    if bytes.len() < 4 || bytes.len() % 4 != 0 {
        return Err(format!(
            "invalid SPIR-V byte length for {}: expected multiple of 4, got {}",
            path.display(),
            bytes.len()
        )
        .into());
    }

    let mut words = Vec::with_capacity(bytes.len() / 4);
    for chunk in bytes.chunks_exact(4) {
        words.push(u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }

    const SPIRV_MAGIC: u32 = 0x0723_0203;
    if words.first().copied() != Some(SPIRV_MAGIC) {
        return Err(format!(
            "invalid SPIR-V magic in {}: expected {SPIRV_MAGIC:#x}",
            path.display()
        )
        .into());
    }

    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::{program_spirv_name, ShaderProgram};

    #[test]
    fn spirv_program_names_match_expected_files() {
        assert_eq!(
            program_spirv_name(ShaderProgram::FractalMain),
            "fractal_main.spv"
        );
        assert_eq!(program_spirv_name(ShaderProgram::GraphOps), "graph_ops.spv");
        assert_eq!(
            program_spirv_name(ShaderProgram::GraphDecode),
            "graph_decode.spv"
        );
        assert_eq!(
            program_spirv_name(ShaderProgram::RetainedPost),
            "retained_post.spv"
        );
    }
}
