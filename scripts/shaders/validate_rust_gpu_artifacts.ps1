<#
.SYNOPSIS
Validate required rust-gpu SPIR-V shader artifacts on Windows hosts.

.DESCRIPTION
Checks that required SPIR-V files exist under the artifact root and that
each file has the correct SPIR-V magic bytes (03 02 23 07).
#>
param(
    [Parameter(Position = 0)]
    [string]$Root = "target/rust-gpu"
)

$ErrorActionPreference = "Stop"

$required = @(
    "fractal_main.spv",
    "graph_ops.spv",
    "graph_decode.spv",
    "retained_post.spv"
)

function Get-SpirvMagicHex {
    param([string]$Path)
    $stream = [System.IO.File]::OpenRead($Path)
    try {
        $buffer = New-Object byte[] 4
        $read = $stream.Read($buffer, 0, 4)
        if ($read -ne 4) {
            throw "file is too small to contain SPIR-V magic"
        }
        return ($buffer | ForEach-Object { $_.ToString("x2") }) -join ""
    }
    finally {
        $stream.Dispose()
    }
}

foreach ($file in $required) {
    $path = Join-Path $Root $file
    if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
        throw "missing shader artifact: $path"
    }

    $magic = Get-SpirvMagicHex -Path $path
    if ($magic -ne "03022307") {
        throw "invalid SPIR-V magic in ${path}: got $magic, expected 03022307"
    }
}

Write-Host "rust-gpu shader artifacts look valid in $Root"
