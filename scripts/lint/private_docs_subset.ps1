param(
    [string]$Manifest = "scripts/lint/private_docs_subset_files.txt"
)

$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "../..")
# The manifest is intentionally staged in buckets so CI can ratchet enforcement
# without forcing one large documentation migration in a single change.
$manifestPath = Join-Path $repoRoot $Manifest

if (-not (Test-Path $manifestPath)) {
    throw "[private-docs] missing manifest: $manifestPath"
}

$errors = @()

foreach ($raw in Get-Content -Path $manifestPath) {
    $relPath = $raw.Trim()
    if ([string]::IsNullOrWhiteSpace($relPath) -or $relPath.StartsWith("#")) {
        continue
    }

    $filePath = Join-Path $repoRoot $relPath
    if (-not (Test-Path $filePath)) {
        $errors += "[private-docs] listed file not found: $relPath"
        continue
    }

    $lines = Get-Content -Path $filePath
    $sawDoc = $false
    for ($i = 0; $i -lt $lines.Length; $i++) {
        $line = $lines[$i]
        if ($line -match '^\s*///' -or $line -match '^\s*//!') {
            $sawDoc = $true
            continue
        }
        if ($line -match '^\s*#\[') {
            continue
        }
        if ($line -match '^\s*$') {
            $sawDoc = $false
            continue
        }

        $isItem = $line -match '^\s*pub(\(crate\))?\s+((async|const|unsafe)\s+)*(fn|struct|enum|const|static|mod|trait|type)\b'
        $isField = $line -match '^\s*pub(\(crate\))?\s+[A-Za-z_][A-Za-z0-9_]*\s*:'

        if (($isItem -or $isField) -and -not $sawDoc) {
            $errors += "$relPath:$($i + 1) missing doc comment for '$($line.Trim())'"
        }

        $sawDoc = $false
    }
}

if ($errors.Count -gt 0) {
    $errors | ForEach-Object { Write-Error $_ }
    throw "[private-docs] documentation lint failed"
}

Write-Host "[private-docs] documentation lint passed"
