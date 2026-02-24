<#
.SYNOPSIS
Build and validate rust-gpu SPIR-V shader artifacts on Windows hosts.

.DESCRIPTION
Runs an optional caller-provided rust-gpu build command, validates required
SPIR-V artifacts, prints artifact metadata (size/hash/timestamp), and
optionally enforces the bash validator as a parity check when bash exists.
#>
param(
    [string]$ArtifactsDir = "target/rust-gpu",
    [string]$BuildCommand = $env:COVERGEN_RUST_GPU_BUILD_COMMAND,
    [string]$Toolchain = $env:COVERGEN_RUST_GPU_TOOLCHAIN,
    [switch]$SkipBashValidation
)

$ErrorActionPreference = "Stop"

$required = @(
    "fractal_main.spv",
    "graph_ops.spv",
    "graph_decode.spv",
    "retained_post.spv"
)

if ([string]::IsNullOrWhiteSpace($Toolchain)) {
    $Toolchain = "nightly-2023-05-27"
}

$started = Get-Date
Write-Host "[shader] build+validate started at $($started.ToUniversalTime().ToString('u'))"
Write-Host "[shader] artifacts dir: $ArtifactsDir"
Write-Host "[shader] rustup toolchain: $Toolchain"

$installedToolchains = (& rustup toolchain list) -split "`n" | ForEach-Object {
    ($_ -split "\s+")[0].Trim()
} | Where-Object { $_ -ne "" }
if (-not ($installedToolchains | Where-Object { $_ -eq $Toolchain -or $_ -like "$Toolchain-*" })) {
    throw "missing rustup toolchain '$Toolchain'. install with: rustup toolchain install $Toolchain -c rust-src -c rustc-dev -c llvm-tools-preview"
}

if ([string]::IsNullOrWhiteSpace($BuildCommand)) {
    $BuildCommand = "cargo +$Toolchain run --quiet --manifest-path shaders/build_spirv/Cargo.toml"
    Write-Host "[shader] no build command provided; using default: $BuildCommand"
}

if ([string]::IsNullOrWhiteSpace($env:RUSTGPU_SKIP_TOOLCHAIN_CHECK)) {
    $env:RUSTGPU_SKIP_TOOLCHAIN_CHECK = "1"
}

[Environment]::SetEnvironmentVariable("COVERGEN_RUST_GPU_SPIRV_DIR", $ArtifactsDir)
Write-Host "[shader] running build command: $BuildCommand"
$buildStarted = Get-Date
& ([scriptblock]::Create($BuildCommand))
$buildElapsed = (Get-Date) - $buildStarted
Write-Host ("[shader] build command completed in {0:N2}s" -f $buildElapsed.TotalSeconds)

$validateScript = Join-Path $PSScriptRoot "validate_rust_gpu_artifacts.ps1"
& $validateScript -Root $ArtifactsDir

$rows = @()
foreach ($file in $required) {
    $path = Join-Path $ArtifactsDir $file
    $item = Get-Item -LiteralPath $path
    $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $path).Hash.ToLowerInvariant()
    $rows += [PSCustomObject]@{
        File = $file
        SizeBytes = $item.Length
        LastWriteUtc = $item.LastWriteTimeUtc.ToString("u")
        Sha256 = $hash
    }
}

Write-Host "[shader] artifact inventory:"
$rows | Format-Table -AutoSize | Out-String | Write-Host

if (-not $SkipBashValidation) {
    $bash = Get-Command bash -ErrorAction SilentlyContinue
    if ($null -eq $bash) {
        Write-Host "[shader] bash not found; skipped scripts/shaders/validate_rust_gpu_artifacts.sh parity check"
    }
    else {
        Write-Host "[shader] running bash parity validator"
        & $bash.Source "scripts/shaders/validate_rust_gpu_artifacts.sh" $ArtifactsDir
    }
}

$elapsed = (Get-Date) - $started
Write-Host ("[shader] build+validate completed in {0:N2}s" -f $elapsed.TotalSeconds)
