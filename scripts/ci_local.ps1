<#
.SYNOPSIS
Run the full local CI gate for one hardware tier on Windows hosts.

.DESCRIPTION
PowerShell equivalent of scripts/ci_local.sh. Executes rust-gpu artifact
validation, formatting/tests, visual regression checks, GPU confidence tests,
then tier threshold lock/validation.
#>
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [ValidateSet("validate", "lock")]
    [string]$Mode,

    [Parameter(Mandatory = $true, Position = 1)]
    [ValidateSet("desktop_mid", "laptop_integrated")]
    [string]$Tier,

    [switch]$CaptureHandoff,

    [string]$HandoffRoot = "docs/plans/handoffs"
)

$ErrorActionPreference = "Stop"

function Invoke-CheckedCommand {
    param(
        [string]$Label,
        [scriptblock]$Command
    )
    Write-Host "[ci_local] $Label"
    & $Command
    if ($LASTEXITCODE -ne 0) {
        throw "$Label failed with exit code $LASTEXITCODE"
    }
}

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$validateShaderScript = Join-Path $repoRoot "scripts/shaders/validate_rust_gpu_artifacts.ps1"
$buildShaderScript = Join-Path $repoRoot "scripts/shaders/build_rust_gpu_artifacts.ps1"
$tierGateScript = Join-Path $repoRoot "scripts/bench/tier_gate.ps1"
$handoffScript = Join-Path $repoRoot "scripts/bench/store_handoff_artifacts.ps1"

$shaderRoot = [Environment]::GetEnvironmentVariable("COVERGEN_RUST_GPU_SPIRV_DIR")
if ([string]::IsNullOrWhiteSpace($shaderRoot)) {
    $shaderRoot = "target/rust-gpu"
}

Write-Host "[ci_local] ensuring rust-gpu artifacts in $shaderRoot"
try {
    & $validateShaderScript -Root $shaderRoot
    if ($LASTEXITCODE -ne 0) {
        throw "validate_rust_gpu_artifacts returned $LASTEXITCODE"
    }
    Write-Host "[ci_local] rust-gpu artifacts already valid"
}
catch {
    Write-Host "[ci_local] rust-gpu artifacts missing/invalid, building"
    Invoke-CheckedCommand -Label "building rust-gpu artifacts" -Command {
        & $buildShaderScript -ArtifactsDir $shaderRoot
    }
}
Invoke-CheckedCommand -Label "rustfmt check" -Command { & cargo fmt --check }
Invoke-CheckedCommand -Label "clippy (warnings + private docs denied)" -Command {
    & cargo clippy --all-targets --all-features -- -D warnings -D clippy::missing_docs_in_private_items
}
Invoke-CheckedCommand -Label "full test suite" -Command { & cargo test -q }
Invoke-CheckedCommand -Label "still snapshot regression" -Command {
    & cargo test v2_still_fixed_seed_snapshots_match
}
Invoke-CheckedCommand -Label "animation snapshot regression" -Command {
    & cargo test v2_animation_fixed_seed_sampled_frames_match
}
Invoke-CheckedCommand -Label "animation movie-quality regression" -Command {
    & cargo test v2_animation_movie_quality_metrics_within_bounds
}
Invoke-CheckedCommand -Label "gpu still confidence regression" -Command {
    & cargo test v2_gpu_still_fixed_seed_is_deterministic_when_hardware_available
}
Invoke-CheckedCommand -Label "gpu animation confidence regression" -Command {
    & cargo test v2_gpu_animation_sampled_frames_change_when_hardware_available
}
Invoke-CheckedCommand -Label "benchmark thresholds ($Mode) for tier=$Tier" -Command {
    & $tierGateScript -Mode $Mode -Tier $Tier
}

if ($CaptureHandoff) {
    Invoke-CheckedCommand -Label "capturing handoff artifacts for tier=$Tier" -Command {
        & $handoffScript -Tier $Tier -HandoffRoot $HandoffRoot
    }
}

Write-Host "[ci_local] completed mode=$Mode tier=$Tier"
