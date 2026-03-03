<#
.SYNOPSIS
Run the full local CI gate for one hardware tier on Windows hosts.

.DESCRIPTION
PowerShell equivalent of scripts/ci_local.sh. Executes rust-gpu artifact
validation, formatting/tests, visual regression checks, GPU confidence tests,
then tier threshold lock/validation including deterministic GUI interaction gates.
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
    $global:LASTEXITCODE = 0
    & $Command
    if (-not $?) {
        throw "$Label failed"
    }
    if ($LASTEXITCODE -ne 0) {
        throw "$Label failed with exit code $LASTEXITCODE"
    }
}

function Invoke-CommonCiSteps {
    param(
        [string]$StepsFile
    )

    if (-not (Test-Path $StepsFile)) {
        throw "missing shared CI steps file: $StepsFile"
    }

    foreach ($rawLine in Get-Content -Path $StepsFile) {
        $line = $rawLine.Trim()
        if ([string]::IsNullOrWhiteSpace($line) -or $line.StartsWith("#")) {
            continue
        }
        $parts = $line.Split("|", 3)
        if ($parts.Length -lt 3) {
            throw "invalid ci step line: $line"
        }
        $label = $parts[0].Trim()
        $pwshCommand = $parts[2].Trim()
        Invoke-CheckedCommand -Label $label -Command ([scriptblock]::Create($pwshCommand))
    }
}

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$validateShaderScript = Join-Path $repoRoot "scripts/shaders/validate_rust_gpu_artifacts.ps1"
$buildShaderScript = Join-Path $repoRoot "scripts/shaders/build_rust_gpu_artifacts.ps1"
$tierGateScript = Join-Path $repoRoot "scripts/bench/tier_gate.ps1"
$guiTierGateScript = Join-Path $repoRoot "scripts/gui/tier_gate.ps1"
$handoffScript = Join-Path $repoRoot "scripts/bench/store_handoff_artifacts.ps1"
$commonStepsFile = Join-Path $repoRoot "scripts/lib/ci_local_steps.tsv"

$shaderRoot = [Environment]::GetEnvironmentVariable("COVERGEN_RUST_GPU_SPIRV_DIR")
if ([string]::IsNullOrWhiteSpace($shaderRoot)) {
    $shaderRoot = "target/rust-gpu"
}

Write-Host "[ci_local] ensuring rust-gpu artifacts in $shaderRoot"
try {
    $global:LASTEXITCODE = 0
    & $validateShaderScript -Root $shaderRoot
    if (-not $?) {
        throw "validate_rust_gpu_artifacts failed"
    }
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
Invoke-CommonCiSteps -StepsFile $commonStepsFile
Invoke-CheckedCommand -Label "benchmark thresholds ($Mode) for tier=$Tier" -Command {
    & $tierGateScript -Mode $Mode -Tier $Tier
}
Invoke-CheckedCommand -Label "gui interaction thresholds ($Mode) for tier=$Tier" -Command {
    & $guiTierGateScript -Mode $Mode -Tier $Tier
}

if ($CaptureHandoff) {
    Invoke-CheckedCommand -Label "capturing handoff artifacts for tier=$Tier" -Command {
        & $handoffScript -Tier $Tier -HandoffRoot $HandoffRoot
    }
}

Write-Host "[ci_local] completed mode=$Mode tier=$Tier"
