//! Build SPIR-V shader artifacts from WGSL sources.
//!
//! This binary compiles the runtime WGSL shader sources into SPIR-V files
//! under `target/rust-gpu` (or `COVERGEN_RUST_GPU_SPIRV_DIR` when set).

use std::error::Error;
use std::path::{Path, PathBuf};

/// Source-to-artifact mapping for runtime shader programs.
struct ShaderArtifact {
    source_file: &'static str,
    output_file: &'static str,
}

const SHADER_ARTIFACTS: &[ShaderArtifact] = &[
    ShaderArtifact {
        source_file: "shader.wgsl",
        output_file: "fractal_main.spv",
    },
    ShaderArtifact {
        source_file: "gpu_graph_ops.wgsl",
        output_file: "graph_ops.spv",
    },
    ShaderArtifact {
        source_file: "gpu_graph_decode.wgsl",
        output_file: "graph_decode.spv",
    },
    ShaderArtifact {
        source_file: "gpu_retained_post.wgsl",
        output_file: "retained_post.spv",
    },
];

/// Compile all shader artifacts and write them to the output directory.
fn main() -> Result<(), Box<dyn Error>> {
    let out_dir = spirv_output_dir();
    std::fs::create_dir_all(&out_dir)?;

    for artifact in SHADER_ARTIFACTS {
        compile_shader_artifact(artifact, &out_dir)?;
    }

    println!(
        "wrote {} shader artifacts to {}",
        SHADER_ARTIFACTS.len(),
        out_dir.display()
    );
    Ok(())
}

/// Resolve the SPIR-V output directory from environment or default path.
fn spirv_output_dir() -> PathBuf {
    if let Ok(path) = std::env::var("COVERGEN_RUST_GPU_SPIRV_DIR") {
        return PathBuf::from(path);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/rust-gpu")
}

/// Compile one WGSL source file into one SPIR-V artifact file.
fn compile_shader_artifact(
    artifact: &ShaderArtifact,
    out_dir: &Path,
) -> Result<(), Box<dyn Error>> {
    let source_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join(artifact.source_file);
    let source_text = std::fs::read_to_string(&source_path)?;
    let module = naga::front::wgsl::parse_str(&source_text)
        .map_err(|err| format!("failed to parse WGSL '{}': {err}", source_path.display()))?;

    let mut validator = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    );
    let module_info = validator.validate(&module)?;
    let spv_words = naga::back::spv::write_vec(
        &module,
        &module_info,
        &naga::back::spv::Options::default(),
        None,
    )?;

    let output_path = out_dir.join(artifact.output_file);
    std::fs::write(&output_path, words_to_le_bytes(&spv_words))?;
    println!(
        "built {} -> {}",
        source_path.display(),
        output_path.display()
    );
    Ok(())
}

/// Convert SPIR-V words into little-endian bytes for artifact storage.
fn words_to_le_bytes(words: &[u32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(words.len() * std::mem::size_of::<u32>());
    for &word in words {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes
}
