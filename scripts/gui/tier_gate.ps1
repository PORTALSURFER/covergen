<#
.SYNOPSIS
Lock or validate GUI interaction performance thresholds for one hardware tier.

.DESCRIPTION
Runs deterministic GUI benchmark interactions, captures a CSV trace, computes p95
metrics, and either locks or validates threshold files.
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

function Get-P95 {
    param(
        [double[]]$Values
    )
    if ($null -eq $Values -or $Values.Count -eq 0) {
        throw "cannot compute p95 from empty values"
    }
    $sorted = $Values | Sort-Object
    $index = [Math]::Ceiling($sorted.Count * 0.95) - 1
    if ($index -lt 0) {
        $index = 0
    }
    return [double]$sorted[$index]
}

function Read-IniLikeFile {
    param(
        [string]$Path
    )
    $entries = @{}
    foreach ($line in Get-Content -Path $Path) {
        if ([string]::IsNullOrWhiteSpace($line)) {
            continue
        }
        if ($line.TrimStart().StartsWith("#")) {
            continue
        }
        $parts = $line -split "=", 2
        if ($parts.Count -ne 2) {
            continue
        }
        $entries[$parts[0].Trim()] = $parts[1].Trim()
    }
    return $entries
}

$traceFrames = [int](Get-EnvOrDefault -Name "GUI_TRACE_FRAMES" -DefaultValue "420")
$warmupFrames = [int](Get-EnvOrDefault -Name "GUI_WARMUP_FRAMES" -DefaultValue "60")
$targetFps = Get-EnvOrDefault -Name "GUI_TARGET_FPS" -DefaultValue "60"
$size = Get-EnvOrDefault -Name "GUI_SIZE" -DefaultValue "1024"
$seed = Get-EnvOrDefault -Name "GUI_SEED" -DefaultValue "1337"
$msMargin = [double](Get-EnvOrDefault -Name "GUI_MS_THRESHOLD_MARGIN" -DefaultValue "1.20")
$hitMargin = [double](Get-EnvOrDefault -Name "GUI_HIT_THRESHOLD_MARGIN" -DefaultValue "1.20")
$bridgeMargin = [double](Get-EnvOrDefault -Name "GUI_BRIDGE_THRESHOLD_MARGIN" -DefaultValue "$hitMargin")
$outputRoot = Get-EnvOrDefault -Name "OUTPUT_ROOT" -DefaultValue "target/bench"
$shaderRoot = Get-EnvOrDefault -Name "COVERGEN_RUST_GPU_SPIRV_DIR" -DefaultValue "target/rust-gpu"

$outputDir = Join-Path $outputRoot $Tier
$traceFile = Join-Path $outputDir "gui_interaction_trace.csv"
$metricsFile = Join-Path $outputDir "gui_interaction_metrics.ini"
$thresholdFile = "docs/v2/benchmarks/$Tier.gui_interaction.thresholds.ini"

New-Item -ItemType Directory -Force -Path $outputDir | Out-Null

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "../..")
$validateScript = Join-Path $repoRoot "scripts/shaders/validate_rust_gpu_artifacts.ps1"

Write-Host "[gui-bench] $Mode tier=$Tier trace=$traceFile thresholds=$thresholdFile"
& $validateScript -Root $shaderRoot

$args = @(
    "run",
    "--quiet",
    "--bin",
    "covergen",
    "--",
    "gui",
    "--width", $size,
    "--height", $size,
    "--seed", $seed,
    "--gui-target-fps", $targetFps,
    "--gui-vsync", "off",
    "--gui-benchmark-drag",
    "--gui-benchmark-frames", "$traceFrames",
    "--gui-perf-trace", $traceFile
)
& cargo @args
if ($LASTEXITCODE -ne 0) {
    throw "gui interaction benchmark run failed with exit code $LASTEXITCODE"
}

if (!(Test-Path -Path $traceFile)) {
    throw "missing trace file: $traceFile"
}

$rows = Import-Csv -Path $traceFile | Where-Object { [int64]$_.frame -ge $warmupFrames }
if ($rows.Count -eq 0) {
    throw "no samples remained after warmup=$warmupFrames"
}
if (!($rows[0].PSObject.Properties.Name -contains "bridge_intersection_tests")) {
    throw "trace missing required column bridge_intersection_tests: $traceFile"
}

$updateP95 = Get-P95 -Values ($rows | ForEach-Object { [double]$_.update_ms })
$sceneP95 = Get-P95 -Values ($rows | ForEach-Object { [double]$_.scene_ms })
$renderP95 = Get-P95 -Values ($rows | ForEach-Object { [double]$_.render_ms })
$hitP95 = [Math]::Round((Get-P95 -Values ($rows | ForEach-Object { [double]$_.hit_test_scans })), 0)
$bridgeP95 = [Math]::Round((Get-P95 -Values ($rows | ForEach-Object { [double]$_.bridge_intersection_tests })), 0)

$metricLines = @(
    "# covergen gui interaction metrics",
    "version=1",
    "tier=$Tier",
    "trace_file=$traceFile",
    "trace_frames=$traceFrames",
    "warmup_frames=$warmupFrames",
    "sample_count=$($rows.Count)",
    ("update_ms_p95={0:F4}" -f $updateP95),
    ("scene_ms_p95={0:F4}" -f $sceneP95),
    ("render_ms_p95={0:F4}" -f $renderP95),
    "hit_test_scans_p95=$hitP95",
    "bridge_intersection_tests_p95=$bridgeP95"
)
Set-Content -Path $metricsFile -Value $metricLines
Write-Host "[gui-bench] wrote metrics: $metricsFile"

if ($Mode -eq "lock") {
    $thresholdLines = @(
        "# covergen gui interaction thresholds",
        "version=1",
        "tier=$Tier",
        "trace_frames=$traceFrames",
        "warmup_frames=$warmupFrames",
        ("update_ms_p95_max={0:F4}" -f ($updateP95 * $msMargin)),
        ("scene_ms_p95_max={0:F4}" -f ($sceneP95 * $msMargin)),
        ("render_ms_p95_max={0:F4}" -f ($renderP95 * $msMargin)),
        ("hit_test_scans_p95_max={0:F0}" -f ($hitP95 * $hitMargin)),
        ("bridge_intersection_tests_p95_max={0:F0}" -f ($bridgeP95 * $bridgeMargin))
    )
    Set-Content -Path $thresholdFile -Value $thresholdLines
    Write-Host "[gui-bench] locked thresholds: $thresholdFile"
    exit 0
}

if (!(Test-Path -Path $thresholdFile)) {
    throw "missing threshold file: $thresholdFile"
}

$threshold = Read-IniLikeFile -Path $thresholdFile
if (!$threshold.ContainsKey("tier")) {
    throw "threshold file missing tier key: $thresholdFile"
}
if ($threshold["tier"] -ne $Tier) {
    throw "threshold tier mismatch: expected=$Tier found=$($threshold["tier"])"
}

foreach ($required in @("update_ms_p95_max", "scene_ms_p95_max", "render_ms_p95_max", "hit_test_scans_p95_max", "bridge_intersection_tests_p95_max")) {
    if (!$threshold.ContainsKey($required)) {
        throw "threshold file missing required key: $required"
    }
}

$violations = @()
if ($updateP95 -gt [double]$threshold["update_ms_p95_max"]) {
    $violations += ("update_ms_p95={0:F4} exceeds {1}" -f $updateP95, $threshold["update_ms_p95_max"])
}
if ($sceneP95 -gt [double]$threshold["scene_ms_p95_max"]) {
    $violations += ("scene_ms_p95={0:F4} exceeds {1}" -f $sceneP95, $threshold["scene_ms_p95_max"])
}
if ($renderP95 -gt [double]$threshold["render_ms_p95_max"]) {
    $violations += ("render_ms_p95={0:F4} exceeds {1}" -f $renderP95, $threshold["render_ms_p95_max"])
}
if ($hitP95 -gt [double]$threshold["hit_test_scans_p95_max"]) {
    $violations += ("hit_test_scans_p95={0:F0} exceeds {1}" -f $hitP95, $threshold["hit_test_scans_p95_max"])
}
if ($bridgeP95 -gt [double]$threshold["bridge_intersection_tests_p95_max"]) {
    $violations += ("bridge_intersection_tests_p95={0:F0} exceeds {1}" -f $bridgeP95, $threshold["bridge_intersection_tests_p95_max"])
}

if ($violations.Count -gt 0) {
    Write-Host "[gui-bench] threshold validation failed ($($violations.Count) violations):"
    foreach ($line in $violations) {
        Write-Host "  - $line"
    }
    throw "gui interaction threshold validation failed"
}

Write-Host "[gui-bench] threshold validation passed: $thresholdFile"
