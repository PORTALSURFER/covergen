$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "../..")
$reportDir = Join-Path $repoRoot "target/ci"
$reportFile = Join-Path $reportDir "structure_drift_report.txt"

$defaultThresholdsFile = Join-Path $repoRoot "docs/v2/benchmarks/structure_drift.thresholds.ini"
$thresholdsFile = if ($env:COVERGEN_STRUCTURE_DRIFT_THRESHOLDS_FILE) {
    $env:COVERGEN_STRUCTURE_DRIFT_THRESHOLDS_FILE
} else {
    $defaultThresholdsFile
}
$gateMode = if ($env:COVERGEN_STRUCTURE_DRIFT_GATE) { $env:COVERGEN_STRUCTURE_DRIFT_GATE } else { "warn" }

$maxFileLines = 400
$maxOversizedFiles = 40
$maxDeadCodeSuppressions = 40
$maxTooManyArgumentsSuppressions = 10

if (Test-Path $thresholdsFile) {
    Get-Content -Path $thresholdsFile | ForEach-Object {
        $line = $_ -replace "[;#].*$", ""
        if (-not ($line -match "=")) { return }
        $parts = $line.Split("=", 2)
        $key = $parts[0].Trim()
        $value = $parts[1].Trim()
        if ([string]::IsNullOrWhiteSpace($key) -or [string]::IsNullOrWhiteSpace($value)) { return }
        switch ($key) {
            "max_file_lines" { $maxFileLines = [int]$value }
            "max_oversized_files" { $maxOversizedFiles = [int]$value }
            "max_dead_code_suppressions" { $maxDeadCodeSuppressions = [int]$value }
            "max_too_many_arguments_suppressions" { $maxTooManyArgumentsSuppressions = [int]$value }
        }
    }
}

if ($gateMode -notin @("off", "warn", "fail")) {
    throw "[structure-drift] invalid COVERGEN_STRUCTURE_DRIFT_GATE='$gateMode' (expected off|warn|fail)"
}

New-Item -ItemType Directory -Path $reportDir -Force | Out-Null

$oversizedEntries = New-Object System.Collections.Generic.List[string]
$rustFiles = Get-ChildItem -Path (Join-Path $repoRoot "src") -Recurse -File -Filter "*.rs" |
    Sort-Object FullName
foreach ($file in $rustFiles) {
    $lineCount = (Get-Content -Path $file.FullName | Measure-Object -Line).Lines
    if ($lineCount -gt $maxFileLines) {
        $relative = $file.FullName.Substring($repoRoot.Path.Length + 1).Replace("\", "/")
        $oversizedEntries.Add(("{0,6} {1}" -f $lineCount, $relative))
    }
}

$deadCodeMatches = Select-String -Path (Join-Path $repoRoot "src/**/*.rs"), (Join-Path $repoRoot "scripts/**/*.sh"), (Join-Path $repoRoot "scripts/**/*.ps1") -Pattern "allow\(dead_code\)" -ErrorAction SilentlyContinue
$deadCodeLines = New-Object System.Collections.Generic.List[string]
if ($deadCodeMatches) {
    foreach ($match in $deadCodeMatches) {
        $relative = $match.Path.Substring($repoRoot.Path.Length + 1).Replace("\", "/")
        $deadCodeLines.Add("${relative}:$($match.LineNumber):$($match.Line.Trim())")
    }
}

$argMatches = Select-String -Path (Join-Path $repoRoot "src/**/*.rs"), (Join-Path $repoRoot "scripts/**/*.sh"), (Join-Path $repoRoot "scripts/**/*.ps1") -Pattern "allow\(clippy::too_many_arguments\)" -ErrorAction SilentlyContinue
$argLines = New-Object System.Collections.Generic.List[string]
if ($argMatches) {
    foreach ($match in $argMatches) {
        $relative = $match.Path.Substring($repoRoot.Path.Length + 1).Replace("\", "/")
        $argLines.Add("${relative}:$($match.LineNumber):$($match.Line.Trim())")
    }
}

$lines = New-Object System.Collections.Generic.List[string]
$lines.Add("[structure-drift] generated: $([DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ'))")
$lines.Add("[structure-drift] gate mode: $gateMode")
$thresholdsRelative = if ($thresholdsFile.StartsWith($repoRoot.Path)) {
    $thresholdsFile.Substring($repoRoot.Path.Length + 1).Replace("\", "/")
} else {
    $thresholdsFile
}
$lines.Add("[structure-drift] thresholds file: $thresholdsRelative")
$lines.Add("[structure-drift] thresholds: max_file_lines=$maxFileLines, max_oversized_files=$maxOversizedFiles, max_dead_code_suppressions=$maxDeadCodeSuppressions, max_too_many_arguments_suppressions=$maxTooManyArgumentsSuppressions")
$lines.Add("")
$lines.Add("[structure-drift] oversized Rust files (>$maxFileLines LOC): $($oversizedEntries.Count)")
foreach ($entry in $oversizedEntries) {
    $lines.Add($entry)
}
$lines.Add("")
$lines.Add("[structure-drift] dead-code lint suppressions: $($deadCodeLines.Count)")
foreach ($entry in $deadCodeLines) {
    $lines.Add($entry)
}
$lines.Add("")
$lines.Add("[structure-drift] too-many-arguments lint suppressions: $($argLines.Count)")
foreach ($entry in $argLines) {
    $lines.Add($entry)
}

$lines | Set-Content -Path $reportFile
$lines | ForEach-Object { Write-Host $_ }
$reportRelative = $reportFile.Substring($repoRoot.Path.Length + 1).Replace("\", "/")
Write-Host "[structure-drift] report written: $reportRelative"

$violations = New-Object System.Collections.Generic.List[string]
if ($oversizedEntries.Count -gt $maxOversizedFiles) {
    $violations.Add("oversized file count $($oversizedEntries.Count) exceeds max $maxOversizedFiles")
}
if ($deadCodeLines.Count -gt $maxDeadCodeSuppressions) {
    $violations.Add("dead-code suppressions $($deadCodeLines.Count) exceeds max $maxDeadCodeSuppressions")
}
if ($argLines.Count -gt $maxTooManyArgumentsSuppressions) {
    $violations.Add("too-many-arguments suppressions $($argLines.Count) exceeds max $maxTooManyArgumentsSuppressions")
}

if ($violations.Count -gt 0) {
    switch ($gateMode) {
        "off" { }
        "warn" {
            Write-Warning "[structure-drift] threshold warnings:"
            foreach ($violation in $violations) {
                Write-Warning "  - $violation"
            }
        }
        "fail" {
            $message = "[structure-drift] threshold gate failed:`n- " + ($violations -join "`n- ")
            throw $message
        }
    }
}
