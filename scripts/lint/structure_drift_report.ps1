$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "../..")
$reportDir = Join-Path $repoRoot "target/ci"
$reportFile = Join-Path $reportDir "structure_drift_report.txt"

New-Item -ItemType Directory -Path $reportDir -Force | Out-Null

$lines = New-Object System.Collections.Generic.List[string]
$lines.Add("[structure-drift] generated: $([DateTime]::UtcNow.ToString('yyyy-MM-ddTHH:mm:ssZ'))")
$lines.Add("")
$lines.Add("[structure-drift] oversized Rust files (>400 LOC)")

$rustFiles = Get-ChildItem -Path (Join-Path $repoRoot "src") -Recurse -File -Filter "*.rs" |
    Sort-Object FullName
foreach ($file in $rustFiles) {
    $lineCount = (Get-Content -Path $file.FullName | Measure-Object -Line).Lines
    if ($lineCount -gt 400) {
        $relative = $file.FullName.Substring($repoRoot.Path.Length + 1).Replace("\", "/")
        $lines.Add(("{0,6} {1}" -f $lineCount, $relative))
    }
}

$lines.Add("")
$lines.Add("[structure-drift] dead-code lint suppressions")
$deadCodeMatches = Select-String -Path (Join-Path $repoRoot "src/**/*.rs"), (Join-Path $repoRoot "scripts/**/*.sh"), (Join-Path $repoRoot "scripts/**/*.ps1") -Pattern "allow\(dead_code\)" -ErrorAction SilentlyContinue
if ($deadCodeMatches) {
    foreach ($match in $deadCodeMatches) {
        $relative = $match.Path.Substring($repoRoot.Path.Length + 1).Replace("\", "/")
        $lines.Add("$relative:$($match.LineNumber):$($match.Line.Trim())")
    }
}

$lines.Add("")
$lines.Add("[structure-drift] too-many-arguments lint suppressions")
$argMatches = Select-String -Path (Join-Path $repoRoot "src/**/*.rs"), (Join-Path $repoRoot "scripts/**/*.sh"), (Join-Path $repoRoot "scripts/**/*.ps1") -Pattern "allow\(clippy::too_many_arguments\)" -ErrorAction SilentlyContinue
if ($argMatches) {
    foreach ($match in $argMatches) {
        $relative = $match.Path.Substring($repoRoot.Path.Length + 1).Replace("\", "/")
        $lines.Add("$relative:$($match.LineNumber):$($match.Line.Trim())")
    }
}

$lines | Set-Content -Path $reportFile
$lines | ForEach-Object { Write-Host $_ }
Write-Host "[structure-drift] report written: $($reportFile.Substring($repoRoot.Path.Length + 1).Replace('\\', '/'))"
