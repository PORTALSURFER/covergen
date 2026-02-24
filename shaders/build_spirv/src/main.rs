//! Build SPIR-V shader artifacts from rust-gpu shader crates.
//!
//! This helper crate intentionally uses Rust 2021 so it can run with the
//! rust-gpu pinned nightly toolchain independently of the main application.

use std::error::Error;
use std::path::{Path, PathBuf};

use spirv_builder::{MetadataPrintout, SpirvBuilder};

/// Source-to-artifact mapping for runtime shader programs.
struct ShaderArtifact {
    output_file: &'static str,
    crate_path: &'static str,
}

const SHADER_ARTIFACTS: &[ShaderArtifact] = &[
    ShaderArtifact {
        output_file: "fractal_main.spv",
        crate_path: "shaders/fractal_main",
    },
    ShaderArtifact {
        output_file: "graph_ops.spv",
        crate_path: "shaders/graph_ops",
    },
    ShaderArtifact {
        output_file: "graph_decode.spv",
        crate_path: "shaders/graph_decode",
    },
    ShaderArtifact {
        output_file: "retained_post.spv",
        crate_path: "shaders/retained_post",
    },
];

/// Compile all shader artifacts and write them to the output directory.
fn main() -> Result<(), Box<dyn Error>> {
    let repo_root = repo_root_dir();
    let out_dir = spirv_output_dir(&repo_root);
    std::fs::create_dir_all(&out_dir)?;

    for artifact in SHADER_ARTIFACTS {
        compile_rust_gpu_artifact(artifact, &repo_root, &out_dir)?;
    }

    println!(
        "wrote {} shader artifacts to {}",
        SHADER_ARTIFACTS.len(),
        out_dir.display()
    );
    Ok(())
}

/// Resolve repository root from this helper crate location.
fn repo_root_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(".."))
}

/// Resolve the SPIR-V output directory from environment or default path.
fn spirv_output_dir(repo_root: &Path) -> PathBuf {
    if let Ok(path) = std::env::var("COVERGEN_RUST_GPU_SPIRV_DIR") {
        return PathBuf::from(path);
    }
    repo_root.join("target/rust-gpu")
}

/// Compile one rust-gpu shader crate into one SPIR-V artifact file.
fn compile_rust_gpu_artifact(
    artifact: &ShaderArtifact,
    repo_root: &Path,
    out_dir: &Path,
) -> Result<(), Box<dyn Error>> {
    let crate_dir = repo_root.join(artifact.crate_path);
    let compile_result = SpirvBuilder::new(crate_dir.as_path(), "spirv-unknown-vulkan1.1")
        .print_metadata(MetadataPrintout::None)
        .build()?;

    let module_path = compile_result.module.unwrap_single();
    let output_path = out_dir.join(artifact.output_file);
    std::fs::copy(module_path, &output_path)?;
    println!(
        "built (rust-gpu) {} -> {}",
        artifact.crate_path,
        output_path.display()
    );
    Ok(())
}
