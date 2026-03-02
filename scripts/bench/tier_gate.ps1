<#
.SYNOPSIS
Lock or validate benchmark thresholds for one hardware tier on Windows hosts.

.DESCRIPTION
PowerShell equivalent of scripts/bench/tier_gate.sh.
It validates rust-gpu artifacts first, then runs
`cargo run --bin covergen -- bench` with tier-specific threshold
lock/validation arguments.
#>
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [ValidateSet("lock", "validate")]
    [string]$Mode,

    [Parameter(Mandatory = $true, Position = 1)]
    [ValidateSet("desktop_mid", "laptop_integrated")]
    [string]$Tier
)

$ErrorActionPreference = "Stop"

function Get-EnvOrDefault {
    param(
        [string]$Name,
        [string]$DefaultValue
    )
    $value = [Environment]::GetEnvironmentVariable($Name)
    if ([string]::IsNullOrWhiteSpace($value)) {
        return $DefaultValue
    }
    return $value
}

function Test-PlaceholderThresholds {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Path
    )
    if (!(Test-Path -Path $Path)) {
        return $false
    }
    $content = Get-Content -Path $Path -Raw
    return ($content -match "0\.001000" -or $content -match "1000000\.000000")
}

$samples = Get-EnvOrDefault -Name "SAMPLES" -DefaultValue "8"
$animationSamples = Get-EnvOrDefault -Name "ANIMATION_SAMPLES" -DefaultValue "4"
$size = Get-EnvOrDefault -Name "SIZE" -DefaultValue "1024"
$seconds = Get-EnvOrDefault -Name "BENCH_SECONDS" -DefaultValue ""
if ([string]::IsNullOrWhiteSpace($seconds)) {
    $seconds = Get-EnvOrDefault -Name "COVERGEN_SECONDS" -DefaultValue "6"
}
$fps = Get-EnvOrDefault -Name "FPS" -DefaultValue "24"
$preset = Get-EnvOrDefault -Name "PRESET" -DefaultValue "mask-atlas"
$profile = Get-EnvOrDefault -Name "PROFILE" -DefaultValue "performance"
$outputRoot = Get-EnvOrDefault -Name "OUTPUT_ROOT" -DefaultValue "target/bench"
$shaderRoot = Get-EnvOrDefault -Name "COVERGEN_RUST_GPU_SPIRV_DIR" -DefaultValue "target/rust-gpu"

$thresholdFile = "docs/v2/benchmarks/$Tier.thresholds.ini"
$outputDir = Join-Path $outputRoot $Tier

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "../..")
$validateScript = Join-Path $repoRoot "scripts/shaders/validate_rust_gpu_artifacts.ps1"

Write-Host "[bench] $Mode tier=$Tier output=$outputDir thresholds=$thresholdFile"
& $validateScript -Root $shaderRoot

$args = @(
    "run",
    "--quiet",
    "--bin",
    "covergen",
    "--",
    "bench",
    "--tier", $Tier,
    "--samples", $samples,
    "--animation-samples", $animationSamples,
    "--size", $size,
    "--seconds", $seconds,
    "--fps", $fps,
    "--preset", $preset,
    "--profile", $profile,
    "--output-dir", $outputDir,
    "--require-v2-scenarios"
)

if ($Mode -eq "lock") {
    $args += @("--lock-thresholds", $thresholdFile)
}
else {
    if (Test-PlaceholderThresholds -Path $thresholdFile) {
        throw @"
[bench] locked-threshold check failed for $thresholdFile
Detected placeholder threshold values.
Run: scripts/ci_local.ps1 lock $Tier
"@
    }
    $args += @("--thresholds", $thresholdFile)
}

& cargo @args
if ($LASTEXITCODE -ne 0) {
    throw "benchmark threshold gate failed with exit code $LASTEXITCODE"
}
